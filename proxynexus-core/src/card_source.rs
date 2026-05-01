use crate::card_store::CardStore;
use crate::error::Result;
use crate::games::get_decklist_adapter;
use crate::models::{CardRequest, Decklist};
use async_trait::async_trait;

pub trait CardSource {
    #![allow(async_fn_in_trait)]
    async fn to_card_requests(&self, store: &mut CardStore<'_>) -> Result<Vec<CardRequest>>;
}

pub struct Cardlist(pub String);
pub struct SetName(pub String);
pub struct DecklistUrl(pub String);

impl CardSource for DecklistUrl {
    async fn to_card_requests(&self, store: &mut CardStore<'_>) -> Result<Vec<CardRequest>> {
        let adapter = get_decklist_adapter(&store.active_game_id);
        let decklist = adapter.fetch(&self.0).await?;
        store.resolve_decklist_to_requests(&decklist).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait DecklistProvider {
    async fn fetch(&self, url: &str) -> Result<Decklist>;
}
