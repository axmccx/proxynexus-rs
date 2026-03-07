use crate::card_source::CardSource;
use crate::card_store::{CardStore, normalize_title};
use crate::db_storage::DbStorage;
use crate::models::{CardRequest, Printing};
use std::collections::HashMap;

pub async fn list_available_sets(db: &mut DbStorage) -> Result<String, Box<dyn std::error::Error>> {
    let mut store = CardStore::new(db)?;
    let sets = store.get_available_packs().await?;

    let max_name_len = sets.iter().map(|(name, _)| name.len()).max().unwrap_or(0);

    let lines: Vec<String> = sets
        .iter()
        .map(|(name, meta)| format!("  - {:width$}    {}", name, meta, width = max_name_len))
        .collect();

    Ok(lines.join("\n"))
}

pub async fn generate_query_output(
    card_source: &impl CardSource,
    db: &mut DbStorage,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut store = CardStore::new(db)?;
    let card_requests = card_source.to_card_requests(&mut store).await?;

    let available = store.get_available_printings(&card_requests).await?;

    format_query_output(&card_requests, &available)
}

fn format_query_output(
    requests: &[CardRequest],
    available: &HashMap<String, Vec<Printing>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: HashMap<String, u32> = HashMap::new();
    for req in requests {
        let normalized = normalize_title(&req.title);
        if !counts.contains_key(&normalized) {
            order.push(normalized.clone());
        }
        *counts.entry(normalized).or_insert(0) += 1;
    }

    let mut lines_data: Vec<(String, Vec<String>)> = Vec::new();
    let mut max_base_len = 0;

    for normalized_title in &order {
        let printings = match available.get(normalized_title) {
            Some(p) => p,
            None => continue,
        };

        let first = &printings[0];
        let default_request = CardRequest {
            title: first.card_title.clone(),
            code: first.card_code.clone(),
            variant: None,
            collection: None,
            pack_code: None,
        };

        let default_p = CardStore::select_printing(&default_request, printings)?;
        let count = counts.get(normalized_title).unwrap_or(&1);

        let base = format!(
            "{}x {} [{}:{}:{}]",
            count,
            default_p.card_title,
            default_p.variant,
            default_p.collection,
            default_p.pack_code,
        );

        max_base_len = max_base_len.max(base.len());

        let alternatives = printings
            .iter()
            .filter(|p| p.variant != default_p.variant || p.collection != default_p.collection)
            .map(|p| format!("[{}:{}:{}]", p.variant, p.collection, p.pack_code))
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
