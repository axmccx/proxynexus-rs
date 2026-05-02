use crate::card_source::CardSource;
use crate::card_store::{CardStore, normalize_title};
use crate::db_storage::DbStorage;
use crate::error::Result;
use crate::models::{CardRequest, Printing};
use std::collections::HashMap;

pub async fn list_available_sets(db: &mut DbStorage, game: &str) -> Result<String> {
    let mut store = CardStore::new(db, game.to_string())?;
    let sets = store.get_available_packs().await?;

    let max_name_len = sets
        .iter()
        .map(|(name, _, _)| name.len())
        .max()
        .unwrap_or(0);
    let max_override_len = sets
        .iter()
        .map(|(_, code, _)| code.len() + 4)
        .max()
        .unwrap_or(0);

    let lines: Vec<String> = sets
        .iter()
        .map(|(name, code, meta)| {
            let pack_override = format!("[::{}]", code);
            format!(
                "  - {:name_width$} {:override_width$}    {}",
                name,
                pack_override,
                meta,
                name_width = max_name_len,
                override_width = max_override_len
            )
        })
        .collect();

    Ok(lines.join("\n"))
}

pub async fn generate_query_output(
    card_source: &impl CardSource,
    db: &mut DbStorage,
    game: &str,
) -> Result<String> {
    let mut store = CardStore::new(db, game.to_string())?;
    let card_requests = card_source.to_card_requests(&mut store).await?;

    let available = store.get_available_printings(&card_requests).await?;

    format_query_output(&card_requests, &available)
}

pub async fn resolve_query_printings(
    card_source: &impl CardSource,
    db: &mut DbStorage,
    game: &str,
) -> Result<(Vec<Printing>, HashMap<String, Vec<Printing>>)> {
    let mut store = CardStore::new(db, game.to_string())?;
    let card_requests = card_source.to_card_requests(&mut store).await?;

    let available = store.get_available_printings(&card_requests).await?;
    let printings = store.resolve_printings(&card_requests, &available)?;
    Ok((printings, available))
}

pub fn apply_variant_overrides(
    base: &[Printing],
    available: &HashMap<String, Vec<Printing>>,
    global_overrides: &HashMap<String, String>,
    index_overrides: &HashMap<(String, usize), String>,
) -> Vec<Printing> {
    let mut occurrence_map = HashMap::<String, usize>::new();
    let mut result = Vec::with_capacity(base.len());

    for p in base {
        let title_norm = normalize_title(&p.card_title);
        let occurrence = occurrence_map.entry(title_norm.clone()).or_insert(0);

        let override_str = index_overrides
            .get(&(title_norm.clone(), *occurrence))
            .or_else(|| global_overrides.get(&title_norm));

        let mut resolved = p.clone();
        if let Some(over_str) = override_str
            && let Some(variants) = available.get(&title_norm)
            && let Some(variant_p) = variants.iter().find(|v| {
                let p_display = v.pack_id.as_deref().or(v.variant.as_deref()).unwrap_or("");
                let v_str = format!("{}:{}", p_display, v.collection);
                v_str == *over_str
            })
        {
            resolved = variant_p.clone();
        }
        result.push(resolved);
        *occurrence += 1;
    }
    result
}

fn format_query_output(
    requests: &[CardRequest],
    available: &HashMap<String, Vec<Printing>>,
) -> Result<String> {
    type GroupKey = (String, Option<String>, Option<String>);
    let mut order: Vec<GroupKey> = Vec::new();
    let mut counts: HashMap<GroupKey, u32> = HashMap::new();
    let mut key_to_request: HashMap<GroupKey, CardRequest> = HashMap::new();

    for req in requests {
        let normalized = normalize_title(&req.title);
        let key = (normalized, req.printing.clone(), req.collection.clone());
        if !counts.contains_key(&key) {
            order.push(key.clone());
            key_to_request.insert(key.clone(), req.clone());
        }
        *counts.entry(key).or_insert(0) += 1;
    }

    let mut lines_data: Vec<(String, Vec<String>)> = Vec::new();
    let mut max_base_len = 0;

    for key in &order {
        let req = key_to_request.get(key).unwrap();
        let normalized_title = &key.0;

        let printings = match available.get(normalized_title) {
            Some(p) => p,
            None => continue,
        };

        let resolved_p = CardStore::select_printing(req, printings)?;
        let count = counts.get(key).unwrap_or(&1);

        let printing_display = resolved_p
            .pack_id
            .as_deref()
            .or(resolved_p.variant.as_deref())
            .unwrap_or("official");

        let base = format!(
            "{}x {} [{}:{}]",
            count, resolved_p.card_title, printing_display, resolved_p.collection,
        );

        max_base_len = max_base_len.max(base.len());

        let alternatives = printings
            .iter()
            .filter(|p| {
                p.variant != resolved_p.variant
                    || p.collection != resolved_p.collection
                    || p.pack_id != resolved_p.pack_id
            })
            .map(|p| {
                let p_display = p
                    .pack_id
                    .as_deref()
                    .or(p.variant.as_deref())
                    .unwrap_or("official");
                format!("[{}:{}]", p_display, p.collection)
            })
            .collect();

        lines_data.push((base, alternatives));
    }

    let mut lines: Vec<String> = Vec::new();
    for (base, alternatives) in lines_data {
        if alternatives.is_empty() {
            lines.push(base);
        } else {
            let padded_base = format!("{:width$}", base, width = max_base_len);
            lines.push(format!(
                "{}    # also: {}",
                padded_base,
                alternatives.join(", ")
            ));
        }
    }

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Printing;
    use std::collections::HashMap;

    fn mock_printing(
        code: &str,
        is_official: bool,
        variant: Option<&str>,
        coll: &str,
        pack: Option<&str>,
    ) -> Printing {
        Printing {
            card_title: "Sure Gamble".into(),
            card_id: code.into(),
            is_official,
            variant: variant.map(|v| v.to_string()),
            image_key: format!("{}.jpg", code),
            parts: Vec::new(),
            collection: coll.into(),
            side: "runner".into(),
            pack_id: pack.map(|p| p.to_string()),
            date_release: None,
        }
    }

    #[test]
    fn test_apply_variant_overrides_global() {
        let base_p = mock_printing("sure_gamble", true, None, "ffg-en", Some("core"));
        let alt_p = mock_printing("sure_gamble", false, Some("alt1"), "standard", None);

        let base = vec![base_p.clone(), base_p.clone()];

        let mut available = HashMap::new();
        available.insert("sure_gamble".into(), vec![base_p.clone(), alt_p.clone()]);

        let mut global_overrides = HashMap::new();
        global_overrides.insert("sure_gamble".into(), "alt1:standard".into());

        let result = apply_variant_overrides(&base, &available, &global_overrides, &HashMap::new());
        assert_eq!(result.len(), 2);

        // Both occurrences should be overridden globally
        for r in &result {
            assert_eq!(r.variant, Some("alt1".to_string()));
            assert_eq!(r.collection, "standard");
        }
    }

    #[test]
    fn test_apply_variant_overrides_index() {
        let base_p = mock_printing("sure_gamble", true, None, "ffg-en", Some("core"));
        let alt_p = mock_printing("sure_gamble", false, Some("alt1"), "standard", None);

        let base = vec![base_p.clone(), base_p.clone()];

        let mut available = HashMap::new();
        available.insert("sure_gamble".into(), vec![base_p.clone(), alt_p.clone()]);

        let mut index_overrides = HashMap::new();
        // Override only the second occurrence (index 1)
        index_overrides.insert(("sure_gamble".into(), 1), "alt1:standard".into());

        let result = apply_variant_overrides(&base, &available, &HashMap::new(), &index_overrides);
        assert_eq!(result.len(), 2);

        // index 0: should remain official
        assert_eq!(result[0].variant, None);
        assert_eq!(result[0].collection, "ffg-en");

        // index 1: should be overridden
        assert_eq!(result[1].variant, Some("alt1".to_string()));
        assert_eq!(result[1].collection, "standard");
    }

    #[test]
    fn test_apply_variant_overrides_index_precedence() {
        let base_p = mock_printing("sure_gamble", true, None, "ffg-en", Some("core"));
        let alt_p = mock_printing("sure_gamble", false, Some("alt1"), "standard", None);
        let promo_p = mock_printing("sure_gamble", false, Some("promo"), "special", None);

        let base = vec![base_p.clone(), base_p.clone()];

        let mut available = HashMap::new();
        available.insert(
            "sure_gamble".into(),
            vec![base_p.clone(), alt_p.clone(), promo_p.clone()],
        );

        let mut global_overrides = HashMap::new();
        global_overrides.insert("sure_gamble".into(), "alt1:standard".into());

        let mut index_overrides = HashMap::new();
        index_overrides.insert(("sure_gamble".into(), 1), "promo:special".into());

        let result =
            apply_variant_overrides(&base, &available, &global_overrides, &index_overrides);
        assert_eq!(result.len(), 2);

        // index 0 uses global override
        assert_eq!(result[0].variant, Some("alt1".to_string()));
        assert_eq!(result[0].collection, "standard");

        // index 1 uses index-specific override, which takes precedence
        assert_eq!(result[1].variant, Some("promo".to_string()));
        assert_eq!(result[1].collection, "special");
    }
}
