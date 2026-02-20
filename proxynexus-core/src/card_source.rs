use crate::models::CardRequest;

pub trait CardSource {
    fn to_card_requests(&self) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>>;
}

pub struct Cardlist(pub String);
pub struct SetName(pub String);
pub struct NrdbUrl(pub String);
