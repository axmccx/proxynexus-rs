use crate::card_store::normalize_title;
use crate::games::lotrlcg::models::{HobCard, HobCardFace};
use std::collections::{HashMap, HashSet};

/// Maps each printing's slug to its card's id: the slug of that card's
/// earliest printing (undated packs count as printed last).
pub fn printing_card_ids(
    cards: &[HobCard],
    pack_dates: &HashMap<String, String>,
) -> HashMap<String, String> {
    const UNDATED: &str = "9999-99-99";

    let mut earliest: HashMap<String, (String, String, String)> = HashMap::new();

    for c in cards {
        let pack = normalize_title(&c.card_set);
        let date = pack_dates
            .get(&pack)
            .cloned()
            .unwrap_or(UNDATED.to_string());
        let slug = normalize_title(&c.slug);
        let candidate = (date, pack, slug);
        earliest
            .entry(card_identity(c))
            .and_modify(|best| {
                if candidate < *best {
                    *best = candidate.clone();
                }
            })
            .or_insert(candidate);
    }

    cards
        .iter()
        .map(|c| {
            let slug = normalize_title(&c.slug);
            let id = earliest
                .get(&card_identity(c))
                .map(|(_, _, s)| s.clone())
                .unwrap_or_else(|| slug.clone());
            (slug, id)
        })
        .collect()
}

/// Maps each card id to its `cards.title` value: the raw title, or the title
/// plus its earliest slug's suffix (`Aragorn (Core)`) when others share it.
pub fn card_titles(
    cards: &[HobCard],
    card_ids: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut raw_slug: HashMap<String, &str> = HashMap::new();
    let mut title_of: HashMap<&str, &str> = HashMap::new();

    for c in cards {
        let slug = normalize_title(&c.slug);
        raw_slug.entry(slug.clone()).or_insert(&c.slug);
        if let Some(id) = card_ids.get(&slug) {
            title_of.entry(id.as_str()).or_insert(&c.title);
        }
    }

    let mut cards_per_title: HashMap<String, HashSet<&str>> = HashMap::new();
    for (id, title) in &title_of {
        cards_per_title
            .entry(normalize_title(title))
            .or_default()
            .insert(id);
    }

    title_of
        .iter()
        .map(|(id, title)| {
            let shared = cards_per_title
                .get(&normalize_title(title))
                .is_some_and(|ids| ids.len() > 1);
            let card_title = match shared
                .then(|| raw_slug.get(*id).and_then(|raw| slug_suffix(raw, title)))
                .flatten()
            {
                Some(suffix) => format!("{} ({})", title, suffix),
                None => title.to_string(),
            };
            (id.to_string(), card_title)
        })
        .collect()
}

fn card_identity(c: &HobCard) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        normalize_title(&c.title),
        c.card_type,
        c.sphere.as_deref().unwrap_or(""),
        face_key(c.front.as_ref()),
        face_key(c.back.as_ref()),
    )
}

fn face_key(face: Option<&HobCardFace>) -> String {
    let Some(f) = face else {
        return String::new();
    };
    let stats = f
        .stats
        .as_ref()
        .map(|s| {
            [
                &s.threat,
                &s.threat_cost,
                &s.resource_cost,
                &s.willpower,
                &s.attack,
                &s.defense,
                &s.hit_points,
                &s.quest_points,
                &s.engagement_cost,
                &s.stage_number,
            ]
            .iter()
            .map(|v| v.as_deref().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(",")
        })
        .unwrap_or_default();
    let text = f
        .text
        .as_ref()
        .map(|t| normalized_rules_text(t))
        .unwrap_or_default();
    format!("{}|{}|{}", stats, text, f.subtitle.as_deref().unwrap_or(""))
}

fn normalized_rules_text(paragraphs: &[String]) -> String {
    let mut out = String::new();
    let mut pending_separator = false;
    for ch in deunicode::deunicode(&paragraphs.join(" "))
        .to_lowercase()
        .chars()
    {
        if ch.is_alphanumeric() {
            if pending_separator && !out.is_empty() {
                out.push('_');
            }
            out.push(ch);
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }
    out
}

fn slug_suffix(slug: &str, title: &str) -> Option<String> {
    let wanted = normalize_title(title);
    let mut end = None;
    for (i, _) in slug.char_indices().skip(1) {
        if normalize_title(&slug[..i]) == wanted {
            end = Some(i);
            break;
        }
    }
    if end.is_none() && normalize_title(slug) == wanted {
        return None;
    }
    let rest = slug[end?..].trim_start_matches('-').replace('-', " ");
    (!rest.is_empty()).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Card(HobCard);

    impl Card {
        fn new(title: &str, slug: &str, pack: &str) -> Self {
            Self(HobCard {
                title: title.to_string(),
                slug: slug.to_string(),
                card_set: pack.to_string(),
                number: 1,
                quantity: None,
                front: None,
                back: None,
                card_type: "Hero".to_string(),
                sphere: None,
            })
        }

        fn card_type(mut self, t: &str) -> Self {
            self.0.card_type = t.to_string();
            self
        }

        fn sphere(mut self, s: &str) -> Self {
            self.0.sphere = Some(s.to_string());
            self
        }

        fn front_text(mut self, text: &str) -> Self {
            self.0.front = Some(face_with_text(text));
            self
        }

        fn back_text(mut self, text: &str) -> Self {
            self.0.back = Some(face_with_text(text));
            self
        }

        fn build(self) -> HobCard {
            self.0
        }
    }

    fn face_with_text(text: &str) -> HobCardFace {
        HobCardFace {
            image_path: None,
            stats: None,
            text: Some(vec![text.to_string()]),
            subtitle: None,
        }
    }

    fn dates(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(pack, date)| (normalize_title(pack), date.to_string()))
            .collect()
    }

    fn id_of(ids: &HashMap<String, String>, slug: &str) -> String {
        ids.get(&normalize_title(slug))
            .unwrap_or_else(|| panic!("slug {} missing from printing_card_ids", slug))
            .clone()
    }

    #[test]
    fn reprints_with_identical_identity_collapse_to_the_earliest_printing() {
        let cards = vec![
            Card::new("Aragorn", "Aragorn-Core", "Core Set")
                .sphere("Leadership")
                .front_text("Sentinel.")
                .build(),
            Card::new("Aragorn", "Aragorn-RevCore", "Revised Core Set")
                .sphere("Leadership")
                .front_text("Sentinel.")
                .build(),
        ];
        let ids = printing_card_ids(
            &cards,
            &dates(&[
                ("Core Set", "2011-04-20"),
                ("Revised Core Set", "2022-01-01"),
            ]),
        );

        assert_eq!(id_of(&ids, "Aragorn-Core"), normalize_title("Aragorn-Core"));
        assert_eq!(
            id_of(&ids, "Aragorn-RevCore"),
            normalize_title("Aragorn-Core")
        );
    }

    #[test]
    fn different_card_type_or_sphere_keeps_a_shared_title_separate() {
        let types = vec![
            Card::new("Gríma", "Grima-Hero", "Pack")
                .card_type("Hero")
                .build(),
            Card::new("Gríma", "Grima-Objective-Ally", "Pack")
                .card_type("Objective_Ally")
                .build(),
        ];
        let type_ids = printing_card_ids(&types, &dates(&[]));
        assert_ne!(
            id_of(&type_ids, "Grima-Hero"),
            id_of(&type_ids, "Grima-Objective-Ally")
        );

        let spheres = vec![
            Card::new("Faramir", "Faramir-A", "Pack")
                .sphere("Lore")
                .build(),
            Card::new("Faramir", "Faramir-B", "Pack")
                .sphere("Leadership")
                .build(),
        ];
        let sphere_ids = printing_card_ids(&spheres, &dates(&[]));
        assert_ne!(
            id_of(&sphere_ids, "Faramir-A"),
            id_of(&sphere_ids, "Faramir-B")
        );
    }

    #[test]
    fn different_front_or_back_text_keeps_a_shared_title_separate() {
        let cards = vec![
            Card::new("Armor Plating", "Armor-Plating", "Pack")
                .front_text("Attach to a hero.")
                .build(),
            Card::new("Armor Plating", "Armor-Plating-Upgraded", "Pack")
                .front_text("Attach to a hero. Gains +1 defense.")
                .build(),
        ];
        let ids = printing_card_ids(&cards, &dates(&[]));
        assert_ne!(
            id_of(&ids, "Armor-Plating"),
            id_of(&ids, "Armor-Plating-Upgraded")
        );

        let stages = vec![
            Card::new("Stage", "Stage-1", "Pack")
                .back_text("2A setup.")
                .build(),
            Card::new("Stage", "Stage-2", "Pack")
                .back_text("2B setup.")
                .build(),
        ];
        let stage_ids = printing_card_ids(&stages, &dates(&[]));
        assert_ne!(id_of(&stage_ids, "Stage-1"), id_of(&stage_ids, "Stage-2"));
    }

    #[test]
    fn cosmetic_text_differences_do_not_split_identity() {
        let cards = vec![
            Card::new("Dúnedain Lookout", "Lookout-A", "Pack")
                .front_text("Response: after Dúnedain Lookout enters play,\ndraw a card.")
                .build(),
            Card::new("Dúnedain Lookout", "Lookout-B", "Pack")
                .front_text("RESPONSE: After Dunedain Lookout enters play, draw a card.  ")
                .build(),
            Card::new("Thing", "Thing-A", "Pack")
                .front_text("It`s here.")
                .build(),
            Card::new("Thing", "Thing-B", "Pack")
                .front_text("It’s here.")
                .build(),
        ];
        let ids = printing_card_ids(&cards, &dates(&[]));

        assert_eq!(id_of(&ids, "Lookout-A"), id_of(&ids, "Lookout-B"));
        assert_eq!(id_of(&ids, "Thing-A"), id_of(&ids, "Thing-B"));
    }

    #[test]
    fn undated_packs_are_treated_as_printed_last() {
        let cards = vec![
            Card::new("Aragorn", "Zz-Undated-Printing", "Unknown Pack").build(),
            Card::new("Aragorn", "Aa-Dated-Printing", "Known Pack").build(),
        ];
        let ids = printing_card_ids(&cards, &dates(&[("Known Pack", "2015-01-01")]));

        // Alphabetically "Aa..." sorts first, but only "Known Pack" has a
        // date, so it must win as the earliest printing regardless of slug ordering.
        assert_eq!(
            id_of(&ids, "Zz-Undated-Printing"),
            normalize_title("Aa-Dated-Printing")
        );
    }

    #[test]
    fn grouping_is_order_independent() {
        let mut cards = vec![
            Card::new("Aragorn", "Aragorn-Core", "Core Set").build(),
            Card::new("Aragorn", "Aragorn-RevCore", "Revised Core Set").build(),
            Card::new("Gríma", "Grima-Hero", "Pack")
                .card_type("Hero")
                .build(),
        ];
        let pack_dates = dates(&[
            ("Core Set", "2011-04-20"),
            ("Revised Core Set", "2022-01-01"),
        ]);
        let forward = printing_card_ids(&cards, &pack_dates);

        cards.reverse();
        let backward = printing_card_ids(&cards, &pack_dates);

        assert_eq!(forward, backward);
    }

    #[test]
    fn a_title_naming_one_card_stays_plain() {
        let cards = vec![Card::new("Valor", "Valor-RevCore", "Pack").build()];
        let ids = printing_card_ids(&cards, &dates(&[]));
        let titles = card_titles(&cards, &ids);

        assert_eq!(titles[&id_of(&ids, "Valor-RevCore")], "Valor");
    }

    #[test]
    fn a_title_naming_several_cards_gets_an_earliest_slug_suffix() {
        let cards = vec![
            Card::new("Gríma", "Grima-Hero-VoI", "Pack")
                .card_type("Hero")
                .build(),
            Card::new("Gríma", "Grima-Objective-Ally-VoI", "Pack")
                .card_type("Objective_Ally")
                .build(),
        ];
        let ids = printing_card_ids(&cards, &dates(&[]));
        let titles = card_titles(&cards, &ids);

        assert_eq!(titles[&id_of(&ids, "Grima-Hero-VoI")], "Gríma (Hero VoI)");
        assert_eq!(
            titles[&id_of(&ids, "Grima-Objective-Ally-VoI")],
            "Gríma (Objective Ally VoI)"
        );
    }

    #[test]
    fn titles_are_unique_raw_and_normalized() {
        // Three genuinely different heroes sharing the title "Aragorn" --
        // distinguished by front text, the way the real cards differ by stats.
        let cards = vec![
            Card::new("Aragorn", "Aragorn-Core", "Core Set")
                .front_text("Sentinel.")
                .build(),
            Card::new("Aragorn", "Aragorn-TLR", "The Lost Realm")
                .front_text("Fierce.")
                .build(),
            Card::new("Aragorn", "Aragorn-TWitW", "The Wilds")
                .front_text("Renowned.")
                .build(),
        ];
        let pack_dates = dates(&[
            ("Core Set", "2011-04-20"),
            ("The Lost Realm", "2015-01-01"),
            ("The Wilds", "2016-01-01"),
        ]);
        let ids = printing_card_ids(&cards, &pack_dates);
        let titles = card_titles(&cards, &ids);

        assert_eq!(titles.len(), 3);
        let raw: HashSet<_> = titles.values().collect();
        assert_eq!(raw.len(), 3, "titles are not unique raw");
        let normalized: HashSet<_> = titles.values().map(|t| normalize_title(t)).collect();
        assert_eq!(
            normalized.len(),
            3,
            "titles are not unique after normalize_title"
        );
    }
}
