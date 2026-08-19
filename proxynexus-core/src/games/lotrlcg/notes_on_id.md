# lotrlcg card identity

lotrlcg is the only game where the catalog derives card identity itself instead of taking it from the API. 
This documents identity, card ids, titles, how printings are addressed in the catalog and in image files, and why.

## Why identity has to be derived

At time of writing this doc, every other game's deckbuilding API provides a card identifier. 
Netrunner and l5r's APIs assign each card its own ID directly. agot and netrunner-reboot's APIs list one entry
per printing, but each entry's name field (ThronesDB's `label`, reteki's `title`) is already unique per card, 
so the adapter just normalizes it.

Hall of Beorn treats each card like its own printing, identified by a `Slug`, seem in the cards URL.
Card titles don't uniquely identify a card. For example, four different Aragorn cards with different stats are all titled `Aragorn`.
However, using the slug as the card id would make every reprint a separate card.

RingsDB supplies pack release dates and a card's position within its pack, but it only contains player cards, so its 
primary use is for loading decklist URLs. 

## Identity

Two Hall of Beorn printings are the same card when these fields match:

```
(normalized title, CardType, Sphere, Front{Stats, Text, Subtitle}, Back{Stats, Text, Subtitle})
```

- `CardType` separates a hero from an objective ally sharing a title and pack.
- `Sphere` separates two cards that differ only by sphere.
- Front/back stats and text separate everything else, including two cards with an identical front
  but a different back.

Rules text is compared as normalized words: reprints reflow, recapitalize, and use different
apostrophe characters for the same text.

Implemented in `proxynexus-core/src/games/lotrlcg/identity.rs`.

## Card id

The normalized slug of the card's earliest printing, using the pack release date obtained from RingsDB
(Hall of Beorn doesn't provide pack release dates). All printings of the Core Set Aragorn get id `aragorn_core`.
Undated packs sort after dated ones, then by pack name. Using a real slug as the id makes them unique.

## Title

Stored in `cards.title`, the same column every other game's adapter fills. 
When multiple different cards share the same title, each unique card gets the remainder of its own id slug appended.

```
Aragorn (Core)   Aragorn (TLR)      # shared title, each card gets its own suffix
Valor                                # title used by one card, no need for a suffix
```

## `card_versions.api_id`

A card id is derived and can shift when the export is corrected. A printing's slug is Hall of Beorn's permanent name for it. 
The catalog stores both:

- `card_versions.api_id`: the printing's id in its source API. lotrlcg stores the normalized slug, while every other game leaves it `NULL`.
- One version per printing, keyed by `(slug, pack)` instead of `(card id, pack)`. 
This gives the Two-Player Limited Edition Starter's two Gandalf scans two versions instead of one.
- A version's database id: `{game}_{api_id}` when `api_id` is set, else `{game}_{card}_{game}_{pack}`.
- Image files keep the existing `{id}@{pack}` convention. For lotrlcg, `{id}` is the printing's
  slug (`aragorn_revcore@revised_core_set.bleed.jpg`). The importer matches it against `card_versions.api_id` first, 
then falls back to card id + pack.

One importer handles both cases: lotrlcg files name printings, every other game's files name cards.

## Decklist resolution

RingsDB decklist slots are keyed by RingsDB's own numeric code. 
The adapter uses that code only to look up the card's name in RingsDB's card list. 
The resolver gets a normalized version of that name, not the code and not a catalog card id.

A title alone isn't enough, since one title can cover several cards. Resolution tries each of
these in order, and stops at the first match:

1. **Exact id.** Compares the normalized name against the catalog's card id. Other games match here, 
  since their decklist entries carry the catalog's card id, but rarely matches for lotrlcg, 
  since a catalog card id is built from a Hall of Beorn slug.
2. **Name in pack.** Looks only at cards printed in the entry's pack, and matches the deck's card name against a 
  card's stored title either directly or with a label suffix attached. 
  Only counts if exactly one card in that pack matches. This resolves most lotrlcg decklist entries.
3. **Pack and position.** The entry's pack and RingsDB position match a specific printing. 
  RingsDB positions can be wrong: two known codes name the right pack but carry another printing's position.
4. **Title.** Matches the deck's name against a stored title across the whole catalog, with no
   pack and no label suffix.

Implemented in `proxynexus-core/src/card_store.rs::resolve_decklist_to_requests`, shared by every
game's decklist import.

## Rejected alternatives

- Renaming image files to each card's id: required a Python mirror of the identity logic to drive
  the renamer, and renaming every existing file. Dropped for the current design, which leaves
  filenames alone and matches them against `card_versions.api_id`.
- Parsing slugs (title + qualifier + pack tag): Hall of Beorn's qualifiers aren't consistent
  enough to parse.
- A hand-maintained mapping of ambiguous titles: hundreds of entries, growing every pack.
