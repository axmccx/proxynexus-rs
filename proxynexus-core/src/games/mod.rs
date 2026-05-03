pub mod l5r;
pub mod netrunner;
use crate::card_source::DecklistProvider;
use crate::games::netrunner::adapter::NetrunnerAdapter;

pub fn get_decklist_adapter(game_id: &str) -> Option<Box<dyn DecklistProvider>> {
    match game_id {
        "netrunner" => Some(Box::new(NetrunnerAdapter::new())),
        _ => None,
    }
}
