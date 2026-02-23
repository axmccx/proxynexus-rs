use crate::card_source::CardSource;
use crate::card_store::CardStore;
use crate::models::{CardRequest, Printing};
use std::collections::HashMap;

pub fn list_available_sets() -> Result<String, Box<dyn std::error::Error>> {
    let store = CardStore::new()?;
    let sets = store.get_available_sets()?;
    Ok(sets
        .iter()
        .map(|s| format!("  - {}", s))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn generate_query_output(
    card_source: &impl CardSource,
) -> Result<String, Box<dyn std::error::Error>> {
    let card_requests = card_source.to_card_requests()?;

    let store = CardStore::new()?;
    let available = store.get_available_printings(&card_requests)?;

    format_query_output(&store, &card_requests, &available)
}

fn format_query_output(
    db: &CardStore,
    requests: &[CardRequest],
    available: &HashMap<String, Vec<Printing>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: HashMap<String, u32> = HashMap::new();
    for req in requests {
        if !counts.contains_key(&req.code) {
            order.push(req.code.clone());
        }
        *counts.entry(req.code.clone()).or_insert(0) += 1;
    }

    let mut lines_data: Vec<(String, Vec<String>)> = Vec::new();
    let mut max_base_len = 0;

    for code in &order {
        let printings = match available.get(code) {
            Some(p) => p,
            None => continue,
        };

        let count = counts.get(code).unwrap_or(&1);

        let default_request = CardRequest {
            code: code.clone(),
            variant: None,
            collection: None,
        };
        let default_printing = db.select_printing(&default_request, printings)?;

        let base = format!(
            "{}x {} [{}:{}]",
            count,
            default_printing.card_title,
            default_printing.variant,
            default_printing.collection
        );

        let alternatives: Vec<String> = printings
            .iter()
            .filter(|p| {
                !(p.variant == default_printing.variant
                    && p.collection == default_printing.collection)
            })
            .map(|p| format!("[{}:{}]", p.variant, p.collection))
            .collect();

        if base.len() > max_base_len {
            max_base_len = base.len();
        }

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
