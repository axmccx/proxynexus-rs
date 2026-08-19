pub mod adapter;
pub mod api;
#[cfg(not(target_arch = "wasm32"))]
pub mod identity;
pub mod models;

pub fn side_from_type_code(type_code: Option<&str>) -> &'static str {
    match type_code {
        Some("hero")
        | Some("ally")
        | Some("attachment")
        | Some("event")
        | Some("player-side-quest")
        | Some("player-objective")
        | Some("contract")
        | Some("treasure") => "player",
        Some("quest") | Some("campaign") | Some("nightmare-setup") | Some("setup") => "quest",
        _ => "encounter",
    }
}

pub fn canonical_pack_name(ringsdb_name: &str) -> String {
    let cleaned = ringsdb_name.replace("ALeP - ", "").replace(".English", "");
    match cleaned.as_str() {
        "Over Hill and Under Hill" | "On the Doorstep" => format!("The Hobbit: {}", cleaned),
        _ => cleaned,
    }
}

#[cfg(test)]
mod tests {
    use super::{canonical_pack_name, side_from_type_code};

    #[test]
    fn player_type_codes_map_to_player_side() {
        assert_eq!(side_from_type_code(Some("hero")), "player");
        assert_eq!(side_from_type_code(Some("treasure")), "player");
        assert_eq!(side_from_type_code(Some("player-objective")), "player");
    }

    #[test]
    fn quest_type_codes_map_to_quest_side() {
        assert_eq!(side_from_type_code(Some("quest")), "quest");
        assert_eq!(side_from_type_code(Some("nightmare-setup")), "quest");
    }

    #[test]
    fn unknown_or_absent_type_codes_map_to_encounter_side() {
        assert_eq!(side_from_type_code(Some("enemy")), "encounter");
        assert_eq!(side_from_type_code(None), "encounter");
    }

    #[test]
    fn hobbit_saga_boxes_gain_the_hall_of_beorn_prefix() {
        assert_eq!(
            canonical_pack_name("Over Hill and Under Hill"),
            "The Hobbit: Over Hill and Under Hill"
        );
        assert_eq!(
            canonical_pack_name("On the Doorstep"),
            "The Hobbit: On the Doorstep"
        );
    }

    #[test]
    fn alep_and_english_suffix_are_stripped() {
        assert_eq!(canonical_pack_name("ALeP - Redhorn Gate"), "Redhorn Gate");
        assert_eq!(canonical_pack_name("Redhorn Gate.English"), "Redhorn Gate");
    }

    #[test]
    fn other_pack_names_pass_through_unchanged() {
        assert_eq!(
            canonical_pack_name("The Voice of Isengard"),
            "The Voice of Isengard"
        );
    }
}
