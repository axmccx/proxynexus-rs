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
        | Some("contract")
        | Some("treasure") => "player",
        Some("quest") | Some("campaign") | Some("nightmare-setup") | Some("setup") => "quest",
        _ => "encounter",
    }
}

#[cfg(test)]
mod tests {
    use super::side_from_type_code;

    #[test]
    fn player_type_codes_map_to_player_side() {
        assert_eq!(side_from_type_code(Some("hero")), "player");
        assert_eq!(side_from_type_code(Some("treasure")), "player");
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
}
