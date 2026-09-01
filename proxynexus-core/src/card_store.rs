use crate::card_source::{CardSource, Cardlist, SetName};
use crate::db_storage::{DbStorage, build_in_clause, quote_sql_string};
use crate::error::{ProxyNexusError, Result};
use crate::file_naming::back_index;
use crate::models::{CardRequest, CardSide, Decklist, Printing, ResolvedCardRequests};
use gluesql::FromGlueRow;
use gluesql::core::row_conversion::SelectExt;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::string::String;
use tracing::warn;

#[derive(FromGlueRow)]
struct PackRow {
    pack_name: String,
    pack_id: String,
    coll_name: Option<String>,
    coll_count: i64,
    date_release: Option<String>,
}

#[derive(FromGlueRow)]
struct PackVersionRow {
    pack_id: String,
    version_id: String,
    card_id: String,
}

#[derive(FromGlueRow)]
struct PrintedCardRow {
    card_id: String,
}

#[derive(FromGlueRow)]
struct CardNameRow {
    id: String,
    title: String,
    pack_id: String,
    title_normalized: String,
    position: Option<i64>,
}

#[derive(FromGlueRow)]
struct CardRequestRow {
    id: String,
    title: String,
    quantity: i64,
    pack_id: String,
    position: Option<i64>,
}

#[derive(FromGlueRow)]
struct CardTitleRow {
    title: String,
}

#[derive(FromGlueRow)]
struct AvailablePrintingRow {
    title: String,
    id: String,
    is_official: bool,
    variant: Option<String>,
    file_path: String,
    side: String,
    name: String,
    back_group: String,
    pack_id: Option<String>,
    has_bleed: bool,
    date_release: Option<String>,
    position: Option<i64>,
}

#[derive(Hash, PartialEq, Eq, Debug)]
struct PrintingGroupKey {
    normalized_title: String,
    card_id: String,
    variant: Option<String>,
    collection_name: String,
    pack_id: Option<String>,
    position: Option<i64>,
}

/// A pack, and what the loaded collections can print of it.
///
/// `printable` counts cards, not images. A card prints from any printing of
/// itself, so a pack holding none of its own images is still printable in full
/// when the sets around it carry the same cards -- which is most of the revised
/// line. Availability has to be asked as "can these cards print", not "does
/// this pack own images".
#[derive(Clone, Debug, PartialEq)]
pub struct AvailablePack {
    pub name: String,
    pub id: String,
    pub date_release: Option<String>,
    /// Cards the pack prints.
    pub total: i64,
    /// Of those, the ones some collection holds an image of.
    pub printable: i64,
    /// `"12 in nr-nsg"` per collection, counting only the pack's own images.
    pub collections: Vec<String>,
}

pub fn normalize_title(title: &str) -> String {
    deunicode::deunicode(title)
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

pub fn clean_card_name(name: &str) -> &str {
    name.trim_end_matches(|c: char| c.is_whitespace() || "●○•·-–—".contains(c))
}

impl CardSource for Cardlist {
    async fn to_card_requests(&self, store: &mut CardStore<'_>) -> Result<ResolvedCardRequests> {
        let result = store.parse_cardlist_into_card_requests(&self.0).await?;

        if !result.not_found.is_empty() {
            warn!(
                "{} card(s) not found in catalog: {:?}",
                result.not_found.len(),
                result.not_found
            );
            warn!("Consider running 'proxynexus catalog update'");
        }

        Ok(result)
    }
}

impl CardSource for SetName {
    async fn to_card_requests(&self, store: &mut CardStore<'_>) -> Result<ResolvedCardRequests> {
        store
            .get_card_requests_from_set_name(&self.0)
            .await
            .map(|r| ResolvedCardRequests {
                requests: r,
                not_found: Vec::new(),
            })
    }
}

pub struct CardStore<'a> {
    db: &'a mut DbStorage,
    pub active_game_id: String,
}

type CardOverride<'a> = (&'a str, Option<String>, Option<i64>, Option<String>);

impl<'a> CardStore<'a> {
    pub fn new(db: &'a mut DbStorage, active_game_id: String) -> Result<Self> {
        Ok(Self { db, active_game_id })
    }

    pub async fn get_all_card_names(&mut self) -> Result<Vec<String>> {
        let query = format!(
            "SELECT DISTINCT c.title
            FROM cards c
            INNER JOIN printings p ON c.id = p.card_id
            WHERE c.game_id = {}
            ORDER BY c.title",
            quote_sql_string(&self.active_game_id)
        );
        let payloads = self.db.execute(&query).await?;
        let mut names = Vec::new();

        if let Some(payload) = payloads.into_iter().next() {
            names = payload
                .rows_as::<CardTitleRow>()?
                .into_iter()
                .map(|row| row.title)
                .collect();
        }

        Ok(names)
    }

    async fn parse_cardlist_into_card_requests(
        &mut self,
        text: &str,
    ) -> Result<ResolvedCardRequests> {
        type CardlistEntry<'a> = (&'a str, u32, Option<String>, Option<i64>, Option<String>);
        let mut entries: Vec<CardlistEntry> = Vec::new();

        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            let (qty, rest) = Self::parse_quantity(line);
            let (name, printing_pref, position_pref, collection_pref) =
                Self::parse_overrides(rest)?;

            let name = clean_card_name(name);
            entries.push((name, qty, printing_pref, position_pref, collection_pref));
        }

        if entries.is_empty() {
            return Ok(ResolvedCardRequests::default());
        }

        let titles: Vec<&str> = entries.iter().map(|(name, ..)| *name).collect();
        let (resolved_cards, not_found) = self.resolve_names_to_cards(&titles).await?;

        let mut requests = Vec::new();

        for (name, qty, printing, position, collection) in entries {
            if let Some((code, title, resolved_pack_code)) = resolved_cards.get(name) {
                requests.extend(std::iter::repeat_n(
                    CardRequest {
                        title: title.clone(),
                        id: code.clone(),
                        printing: printing
                            .clone()
                            .or_else(|| Some(resolved_pack_code.clone())),
                        collection: collection.clone(),
                        position,
                    },
                    qty as usize,
                ));
            }
        }

        Ok(ResolvedCardRequests {
            requests,
            not_found,
        })
    }

    pub fn parse_quantity(line: &str) -> (u32, &str) {
        if let Some((qty_str, card_name)) = line
            .split_once("x ")
            .filter(|(qty_str, _)| qty_str.chars().all(|c| c.is_ascii_digit()))
        {
            let qty: u32 = qty_str.parse().unwrap_or(1);
            (qty, card_name.trim())
        } else if let Some((prefix, rest)) = line.split_once(' ') {
            if prefix.chars().all(|c| c.is_ascii_digit()) {
                let qty: u32 = prefix.parse().unwrap_or(1);
                (qty, rest.trim())
            } else {
                (1, line)
            }
        } else {
            (1, line)
        }
    }

    pub fn parse_overrides(text: &str) -> Result<CardOverride<'_>> {
        if let Some(bracket_start) = text.find('[') {
            let name = text[..bracket_start].trim();
            let bracket_end = text.find(']').ok_or_else(|| {
                ProxyNexusError::Internal("Unclosed bracket in card override".into())
            })?;

            let inner = &text[bracket_start + 1..bracket_end];
            if inner.trim().is_empty() {
                return Err(ProxyNexusError::Internal("Empty override brackets".into()));
            }

            let parts: Vec<Option<String>> = inner
                .split(':')
                .map(|s| {
                    let cleaned = s.trim().to_lowercase();
                    if cleaned.is_empty() {
                        None
                    } else {
                        Some(cleaned)
                    }
                })
                .collect();

            let (printing, position, collection) = match parts.as_slice() {
                [printing] => (printing.clone(), None, None),
                [printing, position] => (
                    printing.clone(),
                    position.as_deref().and_then(|p| p.parse::<i64>().ok()),
                    None,
                ),
                [printing, position, collection] => (
                    printing.clone(),
                    position.as_deref().and_then(|p| p.parse::<i64>().ok()),
                    collection.clone(),
                ),
                _ => {
                    return Err(ProxyNexusError::Internal(format!(
                        "Card override '{}' has too many ':'-separated parts",
                        inner
                    )));
                }
            };

            Ok((name, printing, position, collection))
        } else {
            Ok((text.trim(), None, None, None))
        }
    }

    async fn resolve_names_to_cards(
        &mut self,
        names: &[&str],
    ) -> Result<(HashMap<String, (String, String, String)>, Vec<String>)> {
        if names.is_empty() {
            return Ok((HashMap::new(), Vec::new()));
        }

        let normalized_name_map: HashMap<&str, String> = names
            .iter()
            .map(|&name| (name, normalize_title(name)))
            .collect();

        let unique_normalized_name: HashSet<&str> =
            normalized_name_map.values().map(|s| s.as_str()).collect();
        let in_clause = build_in_clause(unique_normalized_name);

        let query = format!(
            "SELECT
                c.api_id as id,
                c.title,
                p.api_id as pack_id,
                c.title_normalized,
                v.position
             FROM cards c
             JOIN card_versions v ON c.id = v.card_id
             JOIN packs p ON v.pack_id = p.id
             WHERE c.title_normalized IN ({})
               AND c.game_id = {}
             ORDER BY
                 CASE WHEN p.date_release IS NULL THEN 1 ELSE 0 END,
                 p.date_release DESC,
                 c.api_id",
            in_clause,
            quote_sql_string(&self.active_game_id)
        );

        let payloads = self.db.execute(&query).await?;
        let mut resolved_map: HashMap<String, (String, String, String)> = HashMap::new();

        if let Some(payload) = payloads.into_iter().next() {
            let name_rows = payload.rows_as::<CardNameRow>()?;
            for row in name_rows {
                resolved_map.entry(row.title_normalized).or_insert((
                    row.id,
                    row.title,
                    row.pack_id,
                ));
            }
        }

        let mut title_to_card: HashMap<String, (String, String, String)> = HashMap::new();
        let mut not_found = Vec::new();

        for (&title, normalized) in &normalized_name_map {
            if let Some(card_data) = resolved_map.get(normalized) {
                title_to_card.insert(title.to_string(), card_data.clone());
            } else {
                not_found.push(title.to_string());
            }
        }

        Ok((title_to_card, not_found))
    }

    pub async fn get_available_packs(&mut self) -> Result<Vec<AvailablePack>> {
        let own_images_q = format!(
            "SELECT
                p.name as pack_name,
                p.id as pack_id,
                col.name AS coll_name,
                COUNT(pr.id) as coll_count,
                p.date_release
            FROM packs p
            JOIN card_versions v ON p.id = v.pack_id
            LEFT JOIN printings pr ON pr.version_id = v.id
            LEFT JOIN collections col ON pr.collection_id = col.id
            WHERE p.game_id = {}
            GROUP BY p.id, col.id",
            quote_sql_string(&self.active_game_id)
        );

        let versions_q = format!(
            "SELECT p.id as pack_id, v.id as version_id, v.card_id as card_id
             FROM packs p
             JOIN card_versions v ON v.pack_id = p.id
             WHERE p.game_id = {}",
            quote_sql_string(&self.active_game_id)
        );
        let printed_q = format!(
            "SELECT pr.card_id as card_id
             FROM printings pr
             JOIN cards c ON pr.card_id = c.id
             WHERE c.game_id = {}",
            quote_sql_string(&self.active_game_id)
        );

        let printed: HashSet<String> = match self.db.execute(&printed_q).await?.into_iter().next() {
            Some(payload) => payload
                .rows_as::<PrintedCardRow>()?
                .into_iter()
                .map(|row| row.card_id)
                .collect(),
            None => HashSet::new(),
        };

        let mut totals: HashMap<String, (i64, i64)> = HashMap::new();
        let mut seen_versions: HashSet<String> = HashSet::new();
        if let Some(payload) = self.db.execute(&versions_q).await?.into_iter().next() {
            for row in payload.rows_as::<PackVersionRow>()? {
                if !seen_versions.insert(row.version_id) {
                    continue;
                }
                let entry = totals.entry(row.pack_id).or_insert((0, 0));
                entry.0 += 1;
                if printed.contains(&row.card_id) {
                    entry.1 += 1;
                }
            }
        }

        let payloads = self.db.execute(&own_images_q).await?;

        struct PackGroup {
            id: String,
            name: String,
            date_release: Option<String>,
            collections: Vec<String>,
        }

        let mut pack_data: HashMap<String, PackGroup> = HashMap::new();

        if let Some(payload) = payloads.into_iter().next() {
            let pack_rows = payload.rows_as::<PackRow>()?;

            for row in pack_rows {
                let date_release = row.date_release;

                let entry = pack_data
                    .entry(row.pack_id.clone())
                    .or_insert_with(|| PackGroup {
                        id: row.pack_id.clone(),
                        name: row.pack_name,
                        date_release,
                        collections: Vec::new(),
                    });

                if let Some(name) = row.coll_name
                    && row.coll_count > 0
                {
                    entry
                        .collections
                        .push(format!("{} in {}", row.coll_count, name));
                }
            }
        }

        let mut sorted_packs: Vec<_> = pack_data.into_values().collect();
        sorted_packs.sort_by(|a, b| {
            a.date_release
                .is_none()
                .cmp(&b.date_release.is_none())
                .then_with(|| b.date_release.cmp(&a.date_release))
                .then_with(|| a.name.cmp(&b.name))
        });

        Ok(sorted_packs
            .into_iter()
            .map(|mut pack| {
                pack.collections.sort();
                let (total, printable) = totals.get(&pack.id).copied().unwrap_or((0, 0));
                AvailablePack {
                    name: pack.name,
                    id: pack.id,
                    date_release: pack.date_release,
                    total,
                    printable,
                    collections: pack.collections,
                }
            })
            .collect())
    }

    async fn get_card_requests_from_set_name(
        &mut self,
        set_name: &str,
    ) -> Result<Vec<CardRequest>> {
        let query = format!(
            "SELECT c.api_id as id, c.title, v.quantity, p.api_id as pack_id, v.position
             FROM cards c
             JOIN card_versions v ON c.id = v.card_id
             JOIN packs p ON v.pack_id = p.id
             WHERE LOWER(p.name) = {}
               AND c.game_id = {}
             ORDER BY v.position, c.id",
            quote_sql_string(&set_name.to_lowercase()),
            quote_sql_string(&self.active_game_id)
        );

        let payloads = self.db.execute(&query).await?;
        let mut results = Vec::new();

        if let Some(payload) = payloads.into_iter().next() {
            let request_rows = payload.rows_as::<CardRequestRow>()?;

            for row in request_rows {
                results.extend(std::iter::repeat_n(
                    CardRequest {
                        title: row.title,
                        id: row.id,
                        printing: Some(row.pack_id),
                        collection: None,
                        position: row.position,
                    },
                    row.quantity as usize,
                ));
            }
        }

        if results.is_empty() {
            return Err(ProxyNexusError::Internal(format!(
                "No cards found for set '{}'",
                set_name
            )));
        }

        Ok(results)
    }

    pub async fn resolve_decklist_to_requests(
        &mut self,
        decklist: &Decklist,
    ) -> Result<ResolvedCardRequests> {
        if decklist.cards.is_empty() {
            return Ok(ResolvedCardRequests::default());
        }

        let card_ids: HashSet<&String> = decklist.cards.iter().map(|e| &e.card_id).collect();
        let packs: HashSet<&String> = decklist
            .cards
            .iter()
            .filter_map(|e| e.pack_id.as_ref())
            .collect();
        let in_clause = build_in_clause(card_ids);
        let pack_clause = if packs.is_empty() {
            String::new()
        } else {
            format!(" OR p.api_id IN ({})", build_in_clause(packs))
        };

        let query = format!(
            "SELECT
                c.api_id as id,
                c.title,
                p.api_id as pack_id,
                c.title_normalized,
                v.position
             FROM cards c
             JOIN card_versions v ON c.id = v.card_id
             JOIN packs p ON v.pack_id = p.id
             WHERE (c.api_id IN ({0}) OR c.title_normalized IN ({0}){1})
               AND c.game_id = {2}
             ORDER BY c.api_id",
            in_clause,
            pack_clause,
            quote_sql_string(&self.active_game_id)
        );

        let payloads = self.db.execute(&query).await?;
        let mut resolved_by_id = HashMap::new();
        let mut resolved_by_pack_position = HashMap::new();
        let mut rows_by_pack: HashMap<String, Vec<(String, String, String)>> = HashMap::new();
        let mut resolved_by_norm = HashMap::new();

        if let Some(payload) = payloads.into_iter().next() {
            for row in payload.rows_as::<CardNameRow>()? {
                let card = (row.id.clone(), row.title.clone());
                resolved_by_id.entry(row.id.clone()).or_insert(card.clone());
                if let Some(pos) = row.position {
                    resolved_by_pack_position
                        .entry((row.pack_id.clone(), pos))
                        .or_insert(card.clone());
                }
                rows_by_pack.entry(row.pack_id.clone()).or_default().push((
                    row.id.clone(),
                    row.title.clone(),
                    row.title_normalized.clone(),
                ));
                resolved_by_norm
                    .entry(row.title_normalized.clone())
                    .or_insert(card);
            }
        }

        // Matches a deck's plain name against stored titles, handling the unique suffix.
        // More than one match falls back to position.
        let name_in_pack = |pack: &String, name: &str| -> Option<(String, String)> {
            let boundary = format!("{}__", name);
            let mut matched: Option<(String, String)> = None;
            for (id, title, title_normalized) in rows_by_pack.get(pack)? {
                if title_normalized == name || title_normalized.starts_with(&boundary) {
                    match &matched {
                        Some((prev_id, _)) if prev_id != id => return None,
                        _ => matched = Some((id.clone(), title.clone())),
                    }
                }
            }
            matched
        };

        let mut requests = Vec::new();
        let mut not_found = Vec::new();
        for entry in &decklist.cards {
            let by_exact_id = resolved_by_id.get(&entry.card_id).cloned();
            let by_unique_name_in_pack = entry
                .pack_id
                .as_ref()
                .and_then(|pack| name_in_pack(pack, &entry.card_id));
            let by_pack_and_position =
                entry
                    .pack_id
                    .as_ref()
                    .zip(entry.position)
                    .and_then(|(pack, pos)| {
                        resolved_by_pack_position.get(&(pack.clone(), pos)).cloned()
                    });
            let by_normalized_title = resolved_by_norm.get(&entry.card_id).cloned();

            let matched = by_exact_id
                .or(by_unique_name_in_pack)
                .or(by_pack_and_position)
                .or(by_normalized_title);

            if let Some((real_id, title)) = matched {
                requests.extend(std::iter::repeat_n(
                    CardRequest {
                        title,
                        id: real_id,
                        printing: entry.pack_id.clone(),
                        collection: None,
                        position: entry.position,
                    },
                    entry.quantity as usize,
                ));
            } else {
                warn!(
                    "Card ID '{}' from decklist not found in local catalog",
                    entry.card_id
                );
                not_found.push(entry.card_id.clone());
            }
        }

        Ok(ResolvedCardRequests {
            requests,
            not_found,
        })
    }

    pub async fn get_available_printings(
        &mut self,
        card_requests: &[CardRequest],
    ) -> Result<HashMap<String, Vec<Printing>>> {
        if card_requests.is_empty() {
            return Ok(HashMap::new());
        }

        let unique_titles: HashSet<String> = card_requests
            .iter()
            .map(|r| normalize_title(&r.title))
            .collect();

        let in_clause = build_in_clause(&unique_titles);

        let query = format!(
            "SELECT
                c.title,
                c.api_id as id,
                (p.version_id IS NOT NULL) AS is_official,
                p.variant,
                p.file_path,
                p.side,
                col.name,
                c.back_group,
                pks.api_id as pack_id,
                p.has_bleed,
                pks.date_release,
                v.position
             FROM printings p
             JOIN cards c ON p.card_id = c.id
             JOIN collections col ON p.collection_id = col.id
             LEFT JOIN card_versions v ON p.version_id = v.id
             LEFT JOIN packs pks ON v.pack_id = pks.id
             WHERE c.title_normalized IN ({})
               AND c.game_id = {}",
            in_clause,
            quote_sql_string(&self.active_game_id)
        );

        let payloads = self.db.execute(&query).await?;
        let mut resolved_printings: HashMap<String, Vec<Printing>> = HashMap::new();

        if let Some(payload) = payloads.into_iter().next() {
            let printing_rows = payload.rows_as::<AvailablePrintingRow>()?;
            resolved_printings = Self::assemble_printings(printing_rows);
        }

        if resolved_printings.is_empty() && !card_requests.is_empty() {
            return Err(ProxyNexusError::Internal(
                "No printings found in your collections for any requested cards.".into(),
            ));
        }

        let mut missing_titles = HashSet::new();
        for req in card_requests {
            let norm = normalize_title(&req.title);
            if !resolved_printings.contains_key(&norm) && missing_titles.insert(norm) {
                warn!(
                    "No printings found for '{}' in your collections.",
                    req.title
                );
            }
        }

        Ok(resolved_printings)
    }

    fn assemble_side(rows: Vec<AvailablePrintingRow>) -> CardSide {
        let mut side = CardSide::default();

        for row in rows {
            if row.has_bleed {
                side.bleed_image_key = Some(row.file_path);
            } else {
                side.image_key = Some(row.file_path);
            }
        }

        side
    }

    fn assemble_printings(rows: Vec<AvailablePrintingRow>) -> HashMap<String, Vec<Printing>> {
        let mut resolved_printings: HashMap<String, Vec<Printing>> = HashMap::new();
        let mut groups: HashMap<PrintingGroupKey, Vec<AvailablePrintingRow>> = HashMap::new();

        for row in rows {
            let normalized = normalize_title(&row.title);
            let key = PrintingGroupKey {
                normalized_title: normalized,
                card_id: row.id.clone(),
                variant: row.variant.clone(),
                collection_name: row.name.clone(),
                pack_id: row.pack_id.clone(),
                position: row.position,
            };
            groups.entry(key).or_default().push(row);
        }

        for (key, rows) in groups {
            let first_row = &rows[0];
            let card_title = first_row.title.clone();
            let is_official = first_row.is_official;
            let back_group = first_row.back_group.clone();
            let date_release = first_row.date_release.clone();

            let mut by_side: HashMap<String, Vec<AvailablePrintingRow>> = HashMap::new();
            for row in rows {
                by_side.entry(row.side.clone()).or_default().push(row);
            }

            let front = by_side
                .remove("front")
                .map(Self::assemble_side)
                .unwrap_or_default();

            let backs: Vec<CardSide> = by_side
                .into_iter()
                .filter_map(|(label, rows)| back_index(&label).map(|index| (index, rows)))
                .collect::<BTreeMap<_, _>>()
                .into_values()
                .map(Self::assemble_side)
                .collect();

            let printing = Printing {
                card_title,
                card_id: key.card_id,
                is_official,
                variant: key.variant,
                front,
                backs,
                collection: key.collection_name,
                back_group,
                pack_id: key.pack_id,
                date_release,
                position: key.position,
            };

            resolved_printings
                .entry(key.normalized_title)
                .or_default()
                .push(printing);
        }

        // Sorted so the earliest printing is the default choice, with ties
        // broken deterministically: card_id separates same-title cards
        // sharing a pack, position separates two printings of one card in
        // one pack, variant separates alt arts of one card.
        for printings in resolved_printings.values_mut() {
            printings.sort_by_key(|p| {
                (
                    p.date_release.is_none(),
                    p.date_release.clone(),
                    p.card_id.clone(),
                    p.position,
                    p.variant.clone(),
                )
            });
        }

        resolved_printings
    }

    pub fn resolve_printings(
        &self,
        requests: &[CardRequest],
        available: &HashMap<String, Vec<Printing>>,
    ) -> Result<Vec<Printing>> {
        let mut result = Vec::new();

        for request in requests {
            let normalized = normalize_title(&request.title);

            if let Some(printings) = available.get(&normalized) {
                match Self::select_printing(request, printings) {
                    Ok(printing) => result.push(printing),
                    Err(e) => {
                        warn!("{}", e);
                        if let Some(fallback) = printings.first() {
                            warn!(
                                "  Using: {} from {}",
                                fallback.variant.as_deref().unwrap_or("official"),
                                fallback.collection
                            );
                            result.push(fallback.clone());
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    pub fn select_printing(request: &CardRequest, printings: &[Printing]) -> Result<Printing> {
        let mut candidates: Vec<&Printing> = printings.iter().collect();

        // Ensure the best match (fewest misses) is at index 0
        candidates.sort_by_key(|p| {
            let printing_miss = request.printing.is_some()
                && request.printing != p.pack_id
                && request.printing != p.variant;

            let collection_miss =
                request.collection.is_some() && request.collection.as_ref() != Some(&p.collection);

            // Only relevant when a pack prints one card twice.
            let position_miss = request.position.is_some() && request.position != p.position;

            // Printings are looked up by title, which isn't always unique per
            // card. Ranked below pack/collection because
            // resolve_decklist_to_requests can pair a resolved id with a
            // pack_id from elsewhere, and that pack must still win.
            let id_miss = p.card_id != request.id;

            (
                printing_miss,
                collection_miss,
                position_miss,
                id_miss,
                !p.is_official,
                p.date_release.is_none(),
                p.date_release.clone(),
            )
        });

        candidates.into_iter().next().cloned().ok_or_else(|| {
            ProxyNexusError::Internal(format!(
                "No matching printing found for '{}'",
                request.title
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Printing;

    fn row(side: &str, file_path: &str) -> AvailablePrintingRow {
        AvailablePrintingRow {
            title: "Jinteki Biotech".into(),
            id: "jinteki_biotech".into(),
            is_official: true,
            variant: None,
            file_path: file_path.into(),
            side: side.into(),
            name: "nr-ffg".into(),
            back_group: "corp".into(),
            pack_id: Some("the_valley".into()),
            has_bleed: false,
            date_release: None,
            position: None,
        }
    }

    fn only_printing(rows: Vec<AvailablePrintingRow>) -> Printing {
        let mut assembled = CardStore::assemble_printings(rows);
        assert_eq!(assembled.len(), 1);
        let mut printings = assembled.drain().next().unwrap().1;
        assert_eq!(printings.len(), 1);
        printings.remove(0)
    }

    #[test]
    fn backs_are_ordered_by_index_not_by_the_order_rows_arrive() {
        let printing = only_printing(vec![
            row("back3", "c.jpg"),
            row("front", "f.jpg"),
            row("back", "a.jpg"),
            row("back2", "b.jpg"),
        ]);

        assert_eq!(printing.front.image_key.as_deref(), Some("f.jpg"));
        assert_eq!(
            printing
                .backs
                .iter()
                .map(|back| back.image_key.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec!["a.jpg", "b.jpg", "c.jpg"]
        );
    }

    #[test]
    fn a_printing_with_no_back_rows_has_no_backs() {
        let printing = only_printing(vec![row("front", "f.jpg")]);

        assert_eq!(printing.front.image_key.as_deref(), Some("f.jpg"));
        assert!(printing.backs.is_empty());
    }

    #[test]
    fn the_bled_and_unbled_scans_of_one_side_land_on_the_same_side() {
        let mut bled = row("front", "f.bleed.jpg");
        bled.has_bleed = true;
        let printing = only_printing(vec![row("front", "f.jpg"), bled]);

        assert_eq!(printing.front.image_key.as_deref(), Some("f.jpg"));
        assert_eq!(
            printing.front.bleed_image_key.as_deref(),
            Some("f.bleed.jpg")
        );
    }

    fn mock_printing(
        code: &str,
        is_official: bool,
        variant: Option<&str>,
        coll: &str,
        pack: Option<&str>,
        date: Option<&str>,
    ) -> Printing {
        Printing {
            card_title: "Mocked Printing".into(),
            card_id: code.into(),
            is_official,
            variant: variant.map(|s| s.to_string()),
            front: CardSide {
                image_key: Some(format!("{}.jpg", code)),
                bleed_image_key: None,
            },
            backs: Vec::new(),
            collection: coll.into(),
            back_group: "runner".into(),
            pack_id: pack.map(|p| p.to_string()),
            date_release: date.map(|s| s.to_string()),
            position: None,
        }
    }

    #[test]
    fn test_select_printing_distinguishes_same_title_different_card_id() {
        // The Nîn-in-Eilph prints three "Through the Marsh" cards: same title,
        // same pack, same collection, different card ids and different B-sides.
        let p1 = mock_printing(
            "through_the_marsh_no_end_in_sight_nie",
            true,
            None,
            "lotrlcg-enhanced",
            Some("the_nin_in_eilph"),
            Some("2014-11-20"),
        );
        let p2 = mock_printing(
            "through_the_marsh_a_weary_passage_nie",
            true,
            None,
            "lotrlcg-enhanced",
            Some("the_nin_in_eilph"),
            Some("2014-11-20"),
        );
        let p3 = mock_printing(
            "through_the_marsh_a_forgotten_land_nie",
            true,
            None,
            "lotrlcg-enhanced",
            Some("the_nin_in_eilph"),
            Some("2014-11-20"),
        );
        let available = vec![p1, p2, p3];

        for want in [
            "through_the_marsh_no_end_in_sight_nie",
            "through_the_marsh_a_weary_passage_nie",
            "through_the_marsh_a_forgotten_land_nie",
        ] {
            let req = CardRequest {
                title: "Through the Marsh".into(),
                id: want.into(),
                printing: Some("the_nin_in_eilph".into()),
                collection: None,
                position: None,
            };
            assert_eq!(
                CardStore::select_printing(&req, &available)
                    .unwrap()
                    .card_id,
                want,
                "asked for {want}, got a different card"
            );
        }
    }

    #[test]
    fn test_explicit_pack_request_outranks_card_id_match() {
        // Only lotrlcg gives each pack's printing its own card id, and its
        // decklist path can pair an id resolved from one pack with a pack_id
        // naming another (the resolved_by_norm fallback). The pack the caller
        // asked for must still win, or asking for a reprint silently returns
        // the original.
        let core = mock_printing(
            "faramir_core",
            true,
            None,
            "lotrlcg-enhanced",
            Some("core_set"),
            Some("2011-01-01"),
        );
        let starter = mock_printing(
            "faramir_tples",
            true,
            None,
            "lotrlcg-enhanced",
            Some("two_player_limited_edition_starter"),
            Some("2013-01-01"),
        );
        let available = vec![core, starter];

        let req = CardRequest {
            title: "Faramir".into(),
            id: "faramir_tples".into(),
            printing: Some("core_set".into()),
            collection: None,
            position: None,
        };
        assert_eq!(
            CardStore::select_printing(&req, &available)
                .unwrap()
                .card_id,
            "faramir_core",
            "the requested pack must beat the card id carried by the request"
        );
    }

    #[test]
    fn test_select_printing_prioritization() {
        let p1 = mock_printing(
            "sure_gamble",
            true,
            None,
            "ffg-en",
            Some("core"),
            Some("2012-12-01"),
        );
        let p2 = mock_printing(
            "sure_gamble",
            false,
            Some("alt1"),
            "ffg-en",
            None,
            Some("2012-12-01"),
        );
        let p3 = mock_printing(
            "magnum_opus",
            true,
            None,
            "ffg-en",
            Some("revised_core"),
            Some("2017-01-01"),
        );
        let p_collection = mock_printing(
            "sure_gamble",
            true,
            None,
            "nsg-en",
            Some("system_gateway"),
            Some("2017-01-01"),
        );

        let available = vec![p1.clone(), p2.clone(), p3.clone(), p_collection.clone()];

        // Exact variant match
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "sure_gamble".into(),
            printing: Some("alt1".into()),
            collection: None,
            position: None,
        };
        assert_eq!(
            CardStore::select_printing(&req, &available)
                .unwrap()
                .variant,
            Some("alt1".to_string())
        );

        // Exact collection match
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "sure_gamble".into(),
            printing: None,
            collection: Some("nsg-en".into()),
            position: None,
        };
        assert_eq!(
            CardStore::select_printing(&req, &available)
                .unwrap()
                .collection,
            "nsg-en"
        );

        // Exact pack match
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "sure_gamble".into(),
            printing: Some("system_gateway".to_string()),
            collection: None,
            position: None,
        };
        assert_eq!(
            CardStore::select_printing(&req, &available)
                .unwrap()
                .pack_id,
            Some("system_gateway".to_string())
        );

        // Variant Fallback: If requested variant is missing, pick the official art
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "sure_gamble".into(),
            printing: Some("missing_variant".into()),
            collection: None,
            position: None,
        };
        let result = CardStore::select_printing(&req, &available).unwrap();
        assert_eq!(result.variant, None);
        assert_eq!(result.pack_id, Some("core".to_string())); // Oldest first

        // Default to the earliest official
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "sure_gamble".into(),
            printing: None,
            collection: None,
            position: None,
        };
        let result = CardStore::select_printing(&req, &available).unwrap();
        assert_eq!(result.variant, None);
        assert_eq!(result.pack_id, Some("core".to_string()));

        // Printing match beats collection match
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "sure_gamble".into(),
            printing: Some("core".into()),
            collection: Some("nsg-en".into()),
            position: None,
        };
        let result = CardStore::select_printing(&req, &available).unwrap();
        assert_eq!(result.collection, "ffg-en");
        assert_eq!(result.pack_id, Some("core".to_string()));
    }

    #[test]
    fn test_select_printing_fallback_logic() {
        let p1 = mock_printing("1", true, None, "c1", Some("p1"), Some("2020-01-01"));
        let p2 = mock_printing("1", false, Some("alt1"), "c1", None, Some("2021-01-01"));
        let p3 = mock_printing("1", false, Some("promo"), "c2", None, Some("2022-01-01"));
        let available = vec![p1.clone(), p2.clone(), p3.clone()];

        // 1. Missing variant fallback to official version
        let req1 = CardRequest {
            title: "Test".into(),
            id: "1".into(),
            printing: Some("missing_variant".into()),
            collection: None,
            position: None,
        };
        let result1 = CardStore::select_printing(&req1, &available).unwrap();
        assert!(result1.is_official);
        assert_eq!(result1.pack_id, Some("p1".to_string()));

        // 2. Collection override match
        let p4 = mock_printing("1", true, None, "c2", Some("p4"), Some("2023-01-01"));
        let available2 = vec![p1.clone(), p4.clone()];
        let req3 = CardRequest {
            title: "Test".into(),
            id: "1".into(),
            printing: None,
            collection: Some("c2".into()),
            position: None,
        };
        let result3 = CardStore::select_printing(&req3, &available2).unwrap();
        assert_eq!(result3.collection, "c2");
    }

    #[test]
    fn test_select_printing_by_position() {
        let gandalf_4 = Printing {
            card_title: "Gandalf".into(),
            card_id: "gandalf_core".into(),
            is_official: true,
            variant: None,
            front: CardSide {
                image_key: Some("gandalf_1_tples.jpg".into()),
                bleed_image_key: None,
            },
            backs: Vec::new(),
            collection: "enhanced".into(),
            back_group: "player".into(),
            pack_id: Some("two_player_limited_edition_starter".into()),
            date_release: Some("2013-01-01".into()),
            position: Some(4),
        };
        let gandalf_37 = Printing {
            front: CardSide {
                image_key: Some("gandalf_2_tples.jpg".into()),
                bleed_image_key: None,
            },
            position: Some(37),
            ..gandalf_4.clone()
        };
        let available = vec![gandalf_4.clone(), gandalf_37.clone()];

        let req_37 = CardRequest {
            title: "Gandalf".into(),
            id: "gandalf_core".into(),
            printing: Some("two_player_limited_edition_starter".into()),
            collection: None,
            position: Some(37),
        };
        assert_eq!(
            CardStore::select_printing(&req_37, &available)
                .unwrap()
                .front
                .image_key
                .as_deref(),
            Some("gandalf_2_tples.jpg")
        );

        let req_none = CardRequest {
            title: "Gandalf".into(),
            id: "gandalf_core".into(),
            printing: Some("two_player_limited_edition_starter".into()),
            collection: None,
            position: None,
        };
        assert_eq!(
            CardStore::select_printing(&req_none, &available)
                .unwrap()
                .front
                .image_key
                .as_deref(),
            Some("gandalf_1_tples.jpg")
        );
    }

    #[test]
    fn test_clean_card_name() {
        // valid trailing characters remain
        assert_eq!(clean_card_name("Snare!"), "Snare!");
        assert_eq!(clean_card_name("Eli 1.0"), "Eli 1.0");
        assert_eq!(
            clean_card_name("The World is Yours*"),
            "The World is Yours*"
        );
        assert_eq!(clean_card_name("Masterwork (v37)"), "Masterwork (v37)");
        assert_eq!(
            clean_card_name("\"Freedom Through Equality\""),
            "\"Freedom Through Equality\""
        );
        assert_eq!(
            clean_card_name("Title (with parens)"),
            "Title (with parens)"
        );
        assert_eq!(clean_card_name("It Likes Riddles?"), "It Likes Riddles?");
        assert_eq!(
            clean_card_name("Roast 'Em or Boil 'Em?"),
            "Roast 'Em or Boil 'Em?"
        );
        assert_eq!(clean_card_name("Sweet Dreams…"), "Sweet Dreams…");

        // invalid trailing characters get stripped
        assert_eq!(clean_card_name("Hedge Fund ●"), "Hedge Fund");
        assert_eq!(clean_card_name("Sure Gamble -"), "Sure Gamble");
        assert_eq!(clean_card_name("Paperclip ●●●"), "Paperclip");
        assert_eq!(clean_card_name("Card Name ! ●"), "Card Name !");
        assert_eq!(clean_card_name("Card Name ●●●"), "Card Name");
        assert_eq!(clean_card_name("Rest by Night? ●"), "Rest by Night?");
    }

    #[test]
    fn test_parse_quantity() {
        assert_eq!(
            CardStore::parse_quantity("3x Sure Gamble"),
            (3, "Sure Gamble")
        );
        assert_eq!(
            CardStore::parse_quantity("3 Sure Gamble"),
            (3, "Sure Gamble")
        );
        assert_eq!(CardStore::parse_quantity("Sure Gamble"), (1, "Sure Gamble"));
        assert_eq!(
            CardStore::parse_quantity("10x Hedge Fund"),
            (10, "Hedge Fund")
        );
    }

    #[test]
    fn test_parse_overrides() {
        // Printing only
        let (name, p, pos, c) = CardStore::parse_overrides("Sure Gamble [alt]").unwrap();
        assert_eq!(name, "Sure Gamble");
        assert_eq!(p, Some("alt".into()));
        assert_eq!(pos, None);
        assert_eq!(c, None);

        // Two segments: the second slot is always position, never collection.
        let (_, p, pos, c) = CardStore::parse_overrides("Gandalf [tples:37]").unwrap();
        assert_eq!(p, Some("tples".into()));
        assert_eq!(pos, Some(37));
        assert_eq!(c, None);

        // A non-numeric second segment is still read as position (so it
        // parses to None); it is never reinterpreted as a collection.
        let (_, p, pos, c) = CardStore::parse_overrides("Sure Gamble [alt:ffg-en]").unwrap();
        assert_eq!(p, Some("alt".into()));
        assert_eq!(pos, None);
        assert_eq!(c, None);

        // Three segments: printing, position, collection.
        let (_, p, pos, c) = CardStore::parse_overrides("Gandalf [tples:37:enhanced]").unwrap();
        assert_eq!(p, Some("tples".into()));
        assert_eq!(pos, Some(37));
        assert_eq!(c, Some("enhanced".into()));

        // Naming a collection without pinning a position needs the empty
        // middle slot.
        let (_, p, pos, c) =
            CardStore::parse_overrides("Sure Gamble [core_set::enhanced]").unwrap();
        assert_eq!(p, Some("core_set".into()));
        assert_eq!(pos, None);
        assert_eq!(c, Some("enhanced".into()));

        // Case normalization in overrides
        let (_, p, _, _) = CardStore::parse_overrides("Card [ALT]").unwrap();
        assert_eq!(p, Some("alt".into()));
    }

    #[test]
    fn test_parse_overrides_rejects_too_many_segments() {
        assert!(CardStore::parse_overrides("Card [a:b:c:d]").is_err());
    }

    #[tokio::test]
    async fn cardlist_position_override_resolves_through_the_full_pipeline() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        db.initialize_schema().await.unwrap();
        db.execute("INSERT INTO packs (id, api_id, name, game_id) VALUES ('pack_tples', 'tples', 'Two-Player Limited Edition Starter', 'lotrlcg')").await.unwrap();
        db.execute("INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('lotrlcg_gandalf_core', 'gandalf_core', 'lotrlcg', 'Gandalf', 'gandalf')").await.unwrap();
        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ('v_gandalf_4', 'lotrlcg_gandalf_core', 'pack_tples', 1, 4)").await.unwrap();

        let mut store = CardStore::new(&mut db, "lotrlcg".to_string()).unwrap();

        // The trailing "# comment" must not swallow the position segment --
        // position uses ":" now, so it no longer collides with "#" comments.
        let result = store
            .parse_cardlist_into_card_requests("1x Gandalf [tples:37:enhanced] # my copy")
            .await
            .unwrap();

        assert_eq!(result.requests.len(), 1);
        assert_eq!(result.requests[0].printing, Some("tples".to_string()));
        assert_eq!(result.requests[0].position, Some(37));
    }

    #[tokio::test]
    async fn a_title_ending_in_punctuation_resolves_from_a_cardlist() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        db.initialize_schema().await.unwrap();
        db.execute("INSERT INTO packs (id, api_id, name, game_id) VALUES ('p_ohauh', 'the_hobbit_over_hill_and_under_hill', 'The Hobbit: Over Hill and Under Hill', 'lotrlcg')").await.unwrap();
        db.execute(&format!(
            "INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('c_riddles', 'riddles', 'lotrlcg', 'It Likes Riddles?', '{}')",
            normalize_title("It Likes Riddles?")
        )).await.unwrap();
        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ('v_riddles', 'c_riddles', 'p_ohauh', 1, 1)").await.unwrap();

        let mut store = CardStore::new(&mut db, "lotrlcg".to_string()).unwrap();

        let result = store
            .parse_cardlist_into_card_requests(
                "1x It Likes Riddles? [the_hobbit_over_hill_and_under_hill]",
            )
            .await
            .unwrap();

        assert!(result.not_found.is_empty(), "{:?}", result.not_found);
        assert_eq!(result.requests.len(), 1);
        assert_eq!(result.requests[0].title, "It Likes Riddles?");
    }

    /// One card printed in two packs, with an image only in the older one --
    /// the shape of the whole revised line.
    async fn two_packs_one_image() -> (tempfile::TempDir, DbStorage) {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        db.initialize_schema().await.unwrap();
        for q in [
            "INSERT INTO packs (id, api_id, name, game_id, date_release) VALUES ('p_core', 'core_set', 'Core Set', 'lotrlcg', '2011-04-20')",
            "INSERT INTO packs (id, api_id, name, game_id, date_release) VALUES ('p_rev', 'revised_core_set', 'Revised Core Set', 'lotrlcg', '2022-01-01')",
            "INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('lotrlcg_aragorn_core', 'aragorn_core', 'lotrlcg', 'Aragorn', 'aragorn')",
            "INSERT INTO card_versions (id, card_id, pack_id, quantity, position, api_id) VALUES ('v_core', 'lotrlcg_aragorn_core', 'p_core', 1, 1, 'aragorn_core')",
            "INSERT INTO card_versions (id, card_id, pack_id, quantity, position, api_id) VALUES ('v_rev', 'lotrlcg_aragorn_core', 'p_rev', 1, 1, 'aragorn_revcore')",
            "INSERT INTO collections (id, name, game_id, added_date) VALUES (1, 'enhanced', 'lotrlcg', '2024-01-01')",
            "INSERT INTO printings (id, collection_id, card_id, version_id, file_path, side) VALUES (1, 1, 'lotrlcg_aragorn_core', 'v_core', 'a.jpg', 'front')",
        ] {
            db.execute(q).await.unwrap();
        }
        (temp_dir, db)
    }

    #[tokio::test]
    async fn a_pack_with_no_images_of_its_own_is_still_printable_from_another_pack() {
        let (_dir, mut db) = two_packs_one_image().await;
        let mut store = CardStore::new(&mut db, "lotrlcg".to_string()).unwrap();

        let packs = store.get_available_packs().await.unwrap();
        let revised = packs.iter().find(|p| p.id == "p_rev").unwrap();

        // Revised Core owns no image, but its Aragorn is the Core Set Aragorn,
        // so the card prints and the pack must be offered.
        assert!(revised.collections.is_empty());
        assert_eq!((revised.total, revised.printable), (1, 1));
    }

    #[tokio::test]
    async fn a_pack_owning_its_image_reports_it_as_its_own() {
        let (_dir, mut db) = two_packs_one_image().await;
        let mut store = CardStore::new(&mut db, "lotrlcg".to_string()).unwrap();

        let packs = store.get_available_packs().await.unwrap();
        let core = packs.iter().find(|p| p.id == "p_core").unwrap();

        assert_eq!(core.collections, vec!["1 in enhanced".to_string()]);
        assert_eq!((core.total, core.printable), (1, 1));
    }

    #[tokio::test]
    async fn a_pack_whose_cards_have_no_image_anywhere_is_not_printable() {
        let (_dir, mut db) = two_packs_one_image().await;
        for q in [
            "INSERT INTO packs (id, api_id, name, game_id) VALUES ('p_dom', 'the_dark_of_mirkwood', 'The Dark of Mirkwood', 'lotrlcg')",
            "INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('lotrlcg_obsidian_arrows', 'obsidian_arrows', 'lotrlcg', 'Obsidian Arrows', 'obsidian_arrows')",
            "INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ('v_dom', 'lotrlcg_obsidian_arrows', 'p_dom', 1, 26)",
        ] {
            db.execute(q).await.unwrap();
        }
        let mut store = CardStore::new(&mut db, "lotrlcg".to_string()).unwrap();

        let packs = store.get_available_packs().await.unwrap();
        let dom = packs.iter().find(|p| p.id == "p_dom").unwrap();

        assert_eq!((dom.total, dom.printable), (1, 0));
    }

    #[test]
    fn test_normalize_title() {
        assert_eq!(normalize_title("Sure Gamble"), "sure_gamble");
        assert_eq!(normalize_title("Snare!"), "snare_");
        assert_eq!(normalize_title("Café"), "cafe");
        assert_eq!(normalize_title("piñata"), "pinata");
    }

    #[test]
    fn test_assemble_printings_includes_all_packs() {
        let row1 = AvailablePrintingRow {
            title: "Fine Katana".into(),
            id: "fine-katana".into(),
            is_official: true,
            variant: None,
            file_path: "l5r/collection/fine-katana@core.jpg".into(),
            side: "front".into(),
            name: "collection".into(),
            back_group: "test".into(),
            pack_id: Some("core".into()),
            date_release: Some("2017-10-05".into()),
            has_bleed: false,
            position: None,
        };
        let row2 = AvailablePrintingRow {
            title: "Fine Katana".into(),
            id: "fine-katana".into(),
            is_official: true,
            variant: None,
            file_path: "l5r/collection/fine-katana@emerald-core-set.jpg".into(),
            side: "front".into(),
            name: "collection".into(),
            back_group: "test".into(),
            pack_id: Some("emerald-core-set".into()),
            date_release: Some("2021-10-21".into()),
            has_bleed: false,
            position: None,
        };

        let result = CardStore::assemble_printings(vec![row1, row2]);
        let printings = result.get("fine_katana").unwrap();

        assert_eq!(printings.len(), 2);
        let pack_ids: Vec<_> = printings
            .iter()
            .map(|p| p.pack_id.as_deref().unwrap())
            .collect();
        assert!(pack_ids.contains(&"core"));
        assert!(pack_ids.contains(&"emerald-core-set"));
    }

    #[test]
    fn test_assemble_printings_distinguishes_positions_in_one_pack() {
        // The Two-Player Limited Edition Starter prints Gandalf twice: same
        // title, id, pack, and collection, different scans at positions 4 and 37.
        let row_4 = AvailablePrintingRow {
            title: "Gandalf".into(),
            id: "gandalf_core".into(),
            is_official: true,
            variant: None,
            file_path: "lotrlcg/enhanced/gandalf_1_tples@tples.jpg".into(),
            side: "front".into(),
            name: "enhanced".into(),
            back_group: "player".into(),
            pack_id: Some("two_player_limited_edition_starter".into()),
            date_release: Some("2013-01-01".into()),
            has_bleed: false,
            position: Some(4),
        };
        let row_37 = AvailablePrintingRow {
            title: "Gandalf".into(),
            id: "gandalf_core".into(),
            is_official: true,
            variant: None,
            file_path: "lotrlcg/enhanced/gandalf_2_tples@tples.jpg".into(),
            side: "front".into(),
            name: "enhanced".into(),
            back_group: "player".into(),
            pack_id: Some("two_player_limited_edition_starter".into()),
            date_release: Some("2013-01-01".into()),
            has_bleed: false,
            position: Some(37),
        };

        let result = CardStore::assemble_printings(vec![row_4, row_37]);
        let printings = result.get("gandalf").unwrap();

        assert_eq!(printings.len(), 2);
        let positions: Vec<_> = printings.iter().map(|p| p.position).collect();
        assert_eq!(positions, vec![Some(4), Some(37)]);
    }

    fn get_mock_available_printings() -> HashMap<String, Vec<Printing>> {
        let mut available = HashMap::new();
        let p1 = mock_printing(
            "sure_gamble",
            true,
            None,
            "ffg-en",
            Some("core"),
            Some("2012-12-01"),
        );
        let p2 = mock_printing(
            "sure_gamble",
            false,
            Some("alt1"),
            "standard",
            Some("core"),
            Some("2012-12-01"),
        );
        let p3 = mock_printing(
            "sure_gamble",
            true,
            None,
            "alt-arts",
            Some("revised"),
            Some("2017-01-01"),
        );
        let p_collection = mock_printing(
            "sure_gamble",
            true,
            None,
            "alt-arts",
            Some("revised"),
            Some("2017-01-01"),
        );
        available.insert("sure_gamble".to_string(), vec![p1, p2, p3, p_collection]);
        available
    }

    #[test]
    fn test_resolve_printings() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        let store = CardStore::new(&mut db, "netrunner".to_string()).unwrap();

        let mut available = get_mock_available_printings();
        available.insert(
            "snare_".to_string(),
            vec![mock_printing(
                "snare_",
                true,
                None,
                "ffg-en",
                Some("core"),
                Some("2012-12-01"),
            )],
        );

        let req1 = CardRequest {
            title: "Sure Gamble".into(),
            id: "sure_gamble".into(),
            printing: None,
            collection: None,
            position: None,
        };
        let req2 = CardRequest {
            title: "Missing Card".into(),
            id: "missing_card".into(),
            printing: None,
            collection: None,
            position: None,
        };
        let req3 = CardRequest {
            title: "Snare!".into(),
            id: "snare_".into(),
            printing: None,
            collection: None,
            position: None,
        };

        let result = store
            .resolve_printings(&[req1, req2, req3], &available)
            .unwrap();

        // Only 2 printings resolved, missing card was skipped safely
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].card_id, "sure_gamble");
        assert_eq!(result[1].card_id, "snare_");
    }

    #[tokio::test]
    async fn test_resolve_decklist_to_requests() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        db.initialize_schema().await.unwrap();

        // Seed some cards
        db.execute("INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('netrunner_sure_gamble', 'sure_gamble', 'netrunner', 'Sure Gamble', 'sure_gamble')").await.unwrap();
        db.execute("INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('netrunner_snare_', 'snare_', 'netrunner', 'Snare!', 'snare_')").await.unwrap();

        db.execute("INSERT INTO packs (id, api_id, name, game_id) VALUES ('pack_core', 'core', 'Core Set', 'netrunner')").await.unwrap();
        db.execute("INSERT INTO packs (id, api_id, name, game_id) VALUES ('pack_none', 'none', 'None', 'netrunner')").await.unwrap();

        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity) VALUES ('v1', 'netrunner_sure_gamble', 'pack_core', 3)").await.unwrap();
        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity) VALUES ('v2', 'netrunner_snare_', 'pack_none', 3)").await.unwrap();

        let mut store = CardStore::new(&mut db, "netrunner".to_string()).unwrap();

        let decklist = Decklist {
            cards: vec![
                crate::models::DecklistEntry {
                    card_id: "sure_gamble".to_string(),
                    pack_id: Some("core".to_string()),
                    quantity: 3,
                    position: None,
                },
                crate::models::DecklistEntry {
                    card_id: "snare_".to_string(),
                    pack_id: None,
                    quantity: 1,
                    position: None,
                },
            ],
        };

        let result = store.resolve_decklist_to_requests(&decklist).await.unwrap();
        let requests = result.requests;

        assert_eq!(requests.len(), 4);

        // Check Sure Gamble requests
        let sure_gamble_reqs: Vec<_> = requests.iter().filter(|r| r.id == "sure_gamble").collect();
        assert_eq!(sure_gamble_reqs.len(), 3);
        assert_eq!(sure_gamble_reqs[0].printing, Some("core".to_string()));
        assert_eq!(sure_gamble_reqs[0].title, "Sure Gamble");

        // Check Snare! request
        let snare_reqs: Vec<_> = requests.iter().filter(|r| r.id == "snare_").collect();
        assert_eq!(snare_reqs.len(), 1);
        assert_eq!(snare_reqs[0].printing, None);
        assert_eq!(snare_reqs[0].title, "Snare!");
    }

    async fn seed_lotrlcg_db(db: &mut DbStorage) {
        db.initialize_schema().await.unwrap();
        db.execute("INSERT INTO packs (id, api_id, name, game_id) VALUES ('pack_voi', 'voi', 'The Voice of Isengard', 'lotrlcg')").await.unwrap();

        // Two cards sharing the plain title "Gríma" -- titles aren't
        // disambiguated yet, that lands in a later commit.
        db.execute("INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('lotrlcg_grima_hero_voi', 'grima_hero_voi', 'lotrlcg', 'Gríma', 'grima')").await.unwrap();
        db.execute("INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('lotrlcg_grima_objective_ally_voi', 'grima_objective_ally_voi', 'lotrlcg', 'Gríma', 'grima')").await.unwrap();
        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ('v_grima_hero', 'lotrlcg_grima_hero_voi', 'pack_voi', 3, 2)").await.unwrap();
        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ('v_grima_ally', 'lotrlcg_grima_objective_ally_voi', 'pack_voi', 3, 16)").await.unwrap();

        // Thorin Stonehelm (actual position 36) and Aragorn (position 1) in
        // the same pack -- RingsDB code 22001 names Thorin but claims #1.
        db.execute("INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('lotrlcg_thorin_stonehelm', 'thorin_stonehelm', 'lotrlcg', 'Thorin Stonehelm', 'thorin_stonehelm')").await.unwrap();
        db.execute("INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('lotrlcg_aragorn_tples', 'aragorn_tples', 'lotrlcg', 'Aragorn', 'aragorn')").await.unwrap();
        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ('v_thorin', 'lotrlcg_thorin_stonehelm', 'pack_voi', 3, 36)").await.unwrap();
        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ('v_aragorn', 'lotrlcg_aragorn_tples', 'pack_voi', 1, 1)").await.unwrap();

        // Gildor Inglorion, already stored under its disambiguated title, and
        // Eyes in the Dark at the position RingsDB code 22081 lies about
        // (#81, an encounter treachery).
        db.execute("INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('lotrlcg_gildor_inglorion_tples', 'gildor_inglorion_tples', 'lotrlcg', 'Gildor Inglorion (TPLES)', 'gildor_inglorion__tples_')").await.unwrap();
        db.execute("INSERT INTO cards (id, api_id, game_id, title, title_normalized) VALUES ('lotrlcg_eyes_in_the_dark', 'eyes_in_the_dark', 'lotrlcg', 'Eyes in the Dark', 'eyes_in_the_dark')").await.unwrap();
        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ('v_gildor', 'lotrlcg_gildor_inglorion_tples', 'pack_voi', 1, 5)").await.unwrap();
        db.execute("INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ('v_eyes', 'lotrlcg_eyes_in_the_dark', 'pack_voi', 1, 81)").await.unwrap();
    }

    fn grima_entry(position: Option<i64>) -> Decklist {
        Decklist {
            cards: vec![crate::models::DecklistEntry {
                card_id: "grima".to_string(),
                pack_id: Some("voi".to_string()),
                quantity: 1,
                position,
            }],
        }
    }

    #[tokio::test]
    async fn two_cards_sharing_a_title_resolve_by_position() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        seed_lotrlcg_db(&mut db).await;
        let mut store = CardStore::new(&mut db, "lotrlcg".to_string()).unwrap();

        let hero = store
            .resolve_decklist_to_requests(&grima_entry(Some(2)))
            .await
            .unwrap();
        assert_eq!(hero.requests[0].id, "grima_hero_voi");

        let ally = store
            .resolve_decklist_to_requests(&grima_entry(Some(16)))
            .await
            .unwrap();
        assert_eq!(ally.requests[0].id, "grima_objective_ally_voi");
    }

    #[tokio::test]
    async fn a_shared_title_with_no_position_still_resolves_deterministically() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        seed_lotrlcg_db(&mut db).await;
        let mut store = CardStore::new(&mut db, "lotrlcg".to_string()).unwrap();

        let result = store
            .resolve_decklist_to_requests(&grima_entry(None))
            .await
            .unwrap();
        assert_eq!(result.requests[0].id, "grima_hero_voi");
    }

    #[tokio::test]
    async fn a_lying_position_does_not_override_a_unique_name_match_in_the_pack() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        seed_lotrlcg_db(&mut db).await;
        let mut store = CardStore::new(&mut db, "lotrlcg".to_string()).unwrap();

        // RingsDB code 22001: names Thorin Stonehelm, but claims position 1,
        // which in this pack actually belongs to Aragorn.
        let decklist = Decklist {
            cards: vec![crate::models::DecklistEntry {
                card_id: "thorin_stonehelm".to_string(),
                pack_id: Some("voi".to_string()),
                quantity: 1,
                position: Some(1),
            }],
        };

        let result = store.resolve_decklist_to_requests(&decklist).await.unwrap();
        assert_eq!(result.requests[0].id, "thorin_stonehelm");
    }

    #[tokio::test]
    async fn a_lying_position_does_not_override_a_disambiguated_title_match() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        seed_lotrlcg_db(&mut db).await;
        let mut store = CardStore::new(&mut db, "lotrlcg".to_string()).unwrap();

        // RingsDB code 22081: names Gildor Inglorion, but claims position 81,
        // which in this pack belongs to Eyes in the Dark, an encounter card.
        // The stored title is disambiguated ("Gildor Inglorion (TPLES)"), so
        // the deck's plain name only matches its disambiguating suffix.
        let decklist = Decklist {
            cards: vec![crate::models::DecklistEntry {
                card_id: "gildor_inglorion".to_string(),
                pack_id: Some("voi".to_string()),
                quantity: 1,
                position: Some(81),
            }],
        };

        let result = store.resolve_decklist_to_requests(&decklist).await.unwrap();
        assert_eq!(result.requests[0].id, "gildor_inglorion_tples");
    }
}
