# Adding a New Game to Proxy Nexus

Proxy Nexus was originally built for making Netrunner proxies, but it supports any card game that uses the same poker-sized cards. 
Adding a new game simply involves implementing an adapter that provides catalog data (sets, cards, printings) and optionally fetches decklists. 
Once the adapter is registered, new collections of card images can be added for that new game!

### The Catalog
The "Catalog" is the source of truth for all official card data. 
It defines the mapping between abstract cards and their physical printings. 
Proxy Nexus relies on the catalog to search for cards/sets and match them to image files.

When the Desktop app or CLI runs for the first time, it iterates through all registered game adapters and calls their `fetch_catalog()` method. 
The returned data is then saved to the local database. 
Learn more about how the web app uses this database in the README.md, under `## Updating the Web App's Collections`.

## 1. Create the Module Structure
Inside `proxynexus-core/src/games/`, create a new directory for the game. E.g.:
```
proxynexus-core/src/games/new_game/
├── mod.rs       # Module exports
├── adapter.rs   # Implements CatalogProvider, and optionally DecklistProvider
├── api.rs       # (Optional) Functions to fetch data from the game's API
└── models.rs    # (Optional) Serde structs for parsing the API responses
```

## 2. Implement the Adapter
Your game needs an adapter struct that implements the base `GameAdapterInfo` trait, 
as well as `CatalogProvider` and optionally `DecklistProvider`.

**WASM Compatibility Tip:** Proxy Nexus compiles to `wasm32-unknown-unknown` for the web interface.
`CatalogProvider` is native-only; `DecklistProvider` is the one that also runs in a browser.
Consider using the `crate::games::fetch_json(url)` helper for making API requests,
as it already handles the conditionally compiled code (`reqwest` for native, `gloo_net::http` for WASM)
and error handling for you.

### `GameAdapterInfo`
Found in `proxynexus-core/src/games/mod.rs`. It provides basic metadata for the game.

```rust
use crate::games::GameAdapterInfo;

impl GameAdapterInfo for NewGameAdapter {
    fn game_id(&self) -> &'static str {
        "new-game" // A short, unique game id. Must match the folder name, with `_` written as `-`.
    }

    fn game_name(&self) -> &'static str {
        "New Game" // Display name
    }
}
```

### `CatalogProvider`
Found in `proxynexus-core/src/catalog.rs`. It provides Proxy Nexus with a standardized representation of the game's catalog.
The `catalog` module is only compiled for native builds, so the impl, and every import that only it uses,
must be gated with `#[cfg(not(target_arch = "wasm32"))]`.

```rust
// DecklistProvider compiles to wasm and uses these two, so they stay ungated.
// Gate them as well if your adapter does not implement it.
use async_trait::async_trait;
use crate::error::Result;

#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Catalog, CatalogProvider};

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for NewGameAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        // 1. Fetch data from the game's API or load a JSON file
        // 2. Map the data to Proxy Nexus's `Pack`, `Card`, and `CardVersion` structs
        // 3. Return the `Catalog`
    }
}
```
**Important:**
*   When parsing **Cards**, use `crate::card_store::normalize_title` to populate the `title_normalized` field, 
which is required for the card search.
*   **CardVersion** defines an official physical printing of a card. The `card_id` and `pack_id` you provide 
here **must** match the IDs used in the image file naming convention (e.g., `{card_id}@{pack_id}.jpg`), 
otherwise the file will not be linked in the catalog.

### `DecklistProvider` (Optional)
Found in `proxynexus-core/src/card_source.rs`. It handles parsing decklist URLs from popular deckbuilding sites into 
a list of required cards. If your game does not support fetching decklists via URLs, you can skip implementing
this trait entirely. The UI only shows the Decklist URL tab only for games that `get_decklist_adapter` returns an adapter.

```rust
use async_trait::async_trait;
use crate::card_source::DecklistProvider;
use crate::models::{Decklist, DecklistEntry};

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl DecklistProvider for NewGameAdapter {
    async fn fetch(&self, url: &str) -> Result<Decklist> {
        // 1. Parse the URL
        // 2. Fetch the decklist from the external API
        // 3. Map the response into `DecklistEntry` objects.
    }
}
```

**About `DecklistEntry`:**
When building a `DecklistEntry`, the `card_id` and `quantity` are required. 
However, some deckbuilding APIs may not provide a `pack_id`, so it is an `Option<String>`. 
If omitted (`None`), Proxy Nexus will try to find the best available printing in the user's local collection.
`position` pins one printing when a single pack prints the same card twice. Every current adapter sets it to `None`,
since deckbuilding APIs don't report it.

### Card Backs (Optional)
In order for double-sided PDFs and MPC zip files to include the standard card backs, include scans of them in
`proxynexus-core/src/games/new_game/backs/`, named `{back_group}_{label}[.bleed].{ext}`:

- **`back_group`**: Must be a value your adapter puts in `Card::back_group`. The name is split on its first `_`,
  so the group cannot contain one, while the label can. A name with no `_` at all fails the build.
- **`label`**: Free form label to identify the set of back images. If there are multiple sets of back images, 
  then the user will see a dropdown allowing them to choose which one they want. Use `proxy` for the set that should
  be the default; without it the first label alphabetically is used.
- **`.bleed`** means the back image already has a bleed border. Without the suffix a bleed is generated instead.

Both `build.rs` scripts scan `src/games/*/backs/` and pick up the back image files automatically.
`proxynexus-core/build.rs` generates the lookup table, and embeds the images for native builds.
`proxynexus-gui/build.rs` copies the same files into `public/card_backs/`, which the web build fetches at runtime.

Both derive the game id from the folder name, replacing `_` with `-`. A folder that does not match
what `game_id()` returns leaves the game with no backs.

## 3. Register the Adapter
Once your adapter is written, export your module from `proxynexus-core/src/games/mod.rs` (`pub mod new_game;`),
then register the adapter:

**A. Register for Catalog Syncing (`proxynexus-core/src/catalog.rs`)**
In `CatalogManager::new`, add your adapter to the `adapters` vector:
```rust
use crate::games::netrunner::adapter::NetrunnerAdapter;
use crate::games::new_game::adapter::NewGameAdapter; // 1. Import your adapter

impl<'a> CatalogManager<'a> {
    pub fn new(db: &'a mut DbStorage) -> Self {
        let adapters: Vec<Box<dyn CatalogProvider>> = vec![
            Box::new(NetrunnerAdapter::new()),
            Box::new(NewGameAdapter::new()), // 2. Register your game
        ];
        Self { db, adapters }
    }
    // ...
}
```

**B. Register for Decklist Parsing (Optional, `proxynexus-core/src/games/mod.rs`)**
If your adapter implements `DecklistProvider`, add it to `get_decklist_adapter`, otherwise no change is needed here.
```rust
use crate::card_source::DecklistProvider;
use crate::games::netrunner::adapter::NetrunnerAdapter;
use crate::games::new_game::adapter::NewGameAdapter; // 1. Import your adapter

pub fn get_decklist_adapter(game_id: &str) -> Option<Box<dyn DecklistProvider>> {
    match game_id {
        "netrunner" => Some(Box::new(NetrunnerAdapter::new())),
        "new-game" => Some(Box::new(NewGameAdapter::new())), // 2. Register your game
        _ => None,
    }
}
```

## 4. Build a Card Collection
For the application to generate proxies, users need local image collections. 
The `proxynexus-cli` is used to create `.pnx` collection files from directories of raw card images.

To test your new game, you will need to gather some card images in a folder 
and name them according to the **Image File Naming Convention** (e.g., `{card_id}@{pack_id}.jpg`). 

For full details on the naming convention and the CLI commands required to build and load a collection, 
please refer to the main [README.md](../../../README.md) under the **Local Setup** and **Image File Naming Convention** sections.
