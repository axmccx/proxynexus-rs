use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub name_extra: Option<String>,
    pub side: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub versions: Vec<CardVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CardVersion {
    pub card_id: String,
    pub pack_id: String,
    pub image_url: Option<String>,
    pub position: Option<String>,
    pub quantity: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pack {
    pub id: String,
    pub name: String,
    pub released_at: Option<String>,
    pub cycle_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_card() {
        let json = r#"[{
            "id": "a-bad-death",
            "name": "A Bad Death",
            "name_extra": null,
            "faction": "crab",
            "side": "conflict",
            "type": "event",
            "is_unique": false,
            "text": "<b>Reaction:</b> Some text.",
            "traits": null,
            "cost": "0",
            "deck_limit": 3,
            "influence_cost": 2,
            "elements": [],
            "strength": null,
            "glory": null,
            "versions": [{
                "card_id": "a-bad-death",
                "pack_id": "shadows-of-doubt",
                "flavor": "",
                "illustrator": "Steve Argyle",
                "image_url": "https://emerald-legacy.github.io/emeralddb-images/shadows-of-doubt/sod004.jpg",
                "position": "4",
                "quantity": 3,
                "rotated": false
            }]
        }, {
            "id": "a-fate-worse-than-death-2",
            "name": "A Fate Worse Than Death",
            "name_extra": "2",
            "faction": "scorpion",
            "side": "conflict",
            "type": "event",
            "is_unique": false,
            "text": "Some text.",
            "traits": [],
            "cost": "4",
            "deck_limit": 3,
            "influence_cost": 3,
            "elements": [],
            "strength": null,
            "glory": null,
            "versions": [{
                "card_id": "a-fate-worse-than-death-2",
                "pack_id": "emerald-core-set",
                "flavor": "",
                "illustrator": "Thulsa Doom",
                "image_url": null,
                "position": "168",
                "quantity": 3,
                "rotated": false
            }]
        }]"#;

        let cards: Vec<Card> = serde_json::from_str(json).unwrap();
        assert_eq!(cards.len(), 2);

        assert_eq!(cards[0].id, "a-bad-death");
        assert_eq!(cards[0].name, "A Bad Death");
        assert_eq!(cards[0].name_extra, None);
        assert_eq!(cards[0].side, "conflict");
        assert_eq!(cards[0].type_, "event");
        assert_eq!(cards[0].versions.len(), 1);
        assert_eq!(cards[0].versions[0].pack_id, "shadows-of-doubt");
        assert_eq!(
            cards[0].versions[0].image_url.as_deref(),
            Some("https://emerald-legacy.github.io/emeralddb-images/shadows-of-doubt/sod004.jpg")
        );
        assert_eq!(cards[0].versions[0].quantity, 3);
        assert_eq!(cards[0].versions[0].position.as_deref(), Some("4"));

        assert_eq!(cards[1].name_extra, Some("2".into()));
        assert_eq!(cards[1].versions[0].image_url, None);
        assert_eq!(cards[1].versions[0].position.as_deref(), Some("168"));
    }

    #[test]
    fn deserialize_pack() {
        let json = r#"[{
            "id": "disciples-of-the-void",
            "name": "Disciples of the Void",
            "position": 1,
            "size": 28,
            "released_at": "2018-04-05T00:00:00.000Z",
            "publisher_id": "L5C08",
            "cycle_id": "clan-packs",
            "rotated": false
        }, {
            "id": "some-unreleased-pack",
            "name": "Unreleased",
            "position": 99,
            "size": 10,
            "released_at": null,
            "publisher_id": null,
            "cycle_id": "emerald-legacy",
            "rotated": false
        }]"#;

        let packs: Vec<Pack> = serde_json::from_str(json).unwrap();
        assert_eq!(packs.len(), 2);

        assert_eq!(packs[0].id, "disciples-of-the-void");
        assert_eq!(packs[0].name, "Disciples of the Void");
        assert_eq!(
            packs[0].released_at.as_deref(),
            Some("2018-04-05T00:00:00.000Z")
        );
        assert_eq!(packs[0].cycle_id, "clan-packs");

        assert_eq!(packs[1].released_at, None);
    }
}
