# Call of Cthulhu Image File Renamer

Renames Call of Cthulhu: The Card Game scans to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against the catalog the `coclcg` adapter embeds.

Call of Cthulhu has no card database API. The catalog is built instead from a community collection
spreadsheet on BoardGameGeek, by `build_catalog.py`. Both scripts live here so the two stay in
step: the renamer matches scans against the same file the adapter reads.

## Building the catalog

Only needed when the source data changes.

Download the xlsx from
[Complete Collection Card List](https://boardgamegeek.com/filepage/263938/complete-collection-card-list)
and save its one sheet as CSV, then:

```bash
uv run build_catalog.py "~/Downloads/CoC_card__list_(collection).csv"
```

Writes `coc_cards.json` and `coc_packs.json` into `proxynexus-core/src/games/coclcg/`, where the
adapter embeds them with `include_str!`. 54 packs, 1565 cards, 1583 printings.

### Ids

The card id is a slug of the card's title (`thomas-f-malone`). Where a title is shared, the
subtitle is added (`the-necronomicon-al-azif`, `cthulhu-lord-of-rlyeh`), and where two cards share
both, the faction is (`opening-night-story` and `opening-night-hastur`). The card printed without
a subtitle keeps the bare title, so `Hastur` and `Hastur (Lord of Carcosa)` are `hastur` and
`hastur-lord-of-carcosa`. A suffix goes into the catalog title too, in parentheses, which is the
form Proxy Nexus already reads as a title carrying a distinguishing suffix.

The pack id is a slug of the set name (`secrets-of-arkham`).

### What the spreadsheet needs correcting on

The spreadsheet is the only complete list of the game's cards, and it is hand-made. Everything
below was checked against the scans of the cards themselves, and lives in a named table at the top
of `build_catalog.py`.

| | |
|---|---|
| `TITLE_FIXES` | 24 misspelled titles — `Hasur`, `Theif for Hire`, `Out of Dhe Darkness`. One goes the other way: the card really is printed `Student Archaelogist`, and the spreadsheet is the one that corrects it. |
| `SUBTITLE_FIXES` | Two cards whose subtitle was run into the title: `Beneath the Surface Eureka!` and `The Rays of Dawn Cleansing Light`. |
| `POSITION_FIXES` | 15 wrong card numbers, all in the Core Set: the supports at 141-147 are rotated by one, and the ten story cards are in an order of their own. The number printed on the card ("F 147", in the band under the art) is what the table follows. |
| `RELEASE_DATES` | The seven sets the spreadsheet does not date. |
| `PROMOS` | The 15 promo cards, which the spreadsheet does not list at all. |

A stale entry in any of these is an error rather than a silent no-op: a title or pack it cannot
place stops the run.

### Dates

The spreadsheet dates every set from Summons of the Deep onwards. For the Core Set and the six
Forgotten Lore packs it lists first, its date columns hold the cycle and pack index instead, so
those come from `RELEASE_DATES`.

The Core Set is documented as October 2008. Forgotten Lore is not the 2008 cycle its position in
the spreadsheet suggests: it is the revised edition, reprinted in 2011 as a single set with
continuous numbering — that is the numbering the spreadsheet carries, and the printing the scans
are, their copyright line reading 2011. The six are spread across 2011 to hold their order. Only
the order is documented, so read those dates as "in this order, around here" rather than as exact
days.

The promos pack has no date and sorts last.

## Getting the scans

One archive, from
[this r/callofcthulhulcg post](https://www.reddit.com/r/callofcthulhulcg/comments/b74pjq/call_of_cthulhu_card_scans_600dpi/),
which links to a
[Google Drive folder](https://drive.google.com/drive/folders/1dqooB5JZNNyoNxs__R2TmBYrqXJ8Z9kA).

Download it as-is. Every scan is ~590 dpi, cut to the card and with no bleed border.

```
Call of Cthulhu LCG/
├── 01 - Core Set/
│   ├── 001 - Thomas F. Malone.jpg
│   └── Story Card Reverse.jpg
├── 02 - Forgotten Lore/
│   └── 02 - Kingsport Dreams/021 - The Terrible Old Man.jpg
├── 18 - The Mark of Madness/001 - Alejandro Ruiz.jpg
├── Lore/            the story sheets folded into each pack, as PDFs
├── Miscellaneous/   the card back, and two draft cards
├── Organized Play/  rules and FAQ PDFs
└── Promos/Azathoth.jpg
```

## Running it

Requires [`uv`](https://docs.astral.sh/uv/).

```bash
uv run rename.py "~/Downloads/Call of Cthulhu LCG" --dry-run          # preview
uv run rename.py "~/Downloads/Call of Cthulhu LCG" -o coclcg_out      # apply
```

Scans are copied into the output folder. The source is never modified.

Dry-run first and read the report sections. `Filenames matched to a card by spelling`,
`Filenames matched to a card by number` and `Filenames whose number disagrees with the catalog`
list every file the two sources disagree about, and all three are short enough to check by eye.

Four passes follow, in this order:

```bash
# 1. The corners, which hold the black the cards were scanned against.
uv run ../../corner_infill/corner_infill_dark.py coclcg_out -o coclcg_infilled \
    --background-cutoff 150 --flood-tolerance 60

# 2. The same black, in the few pixels along every edge.
uv run ../../coc_edge_crop/coc_edge_crop.py coclcg_infilled -o coclcg_trimmed

# 3. Story and conspiracy cards, which are printed landscape.
uv run ../agot/rotate_horizontal.py coclcg_trimmed

# 4. The print screen, and the resample to print resolution.
uv run ../../coc_despeckle/coc_despeckle.py coclcg_trimmed \
    -o ~/Pictures/proxynexus_collections/coclcg
```

**The white border stays.** These cards are printed with a band of white between the frame and the
card's edge — about 0.12in of a 2.5in card — and it is part of the card, not scan margin. FFG's own
rulebook, in `Organized Play/Core Rules.pdf`, draws The Bootleg Whiskey Cover-Up with a white ring
3.1% of the card across; the scan of that card measures 3.2%. Cropping it away would print the card
undersized and unlike the retail one.

**Steps 1 and 2 take the black**, which is what the scans were made against. It shows outside the
card's rounded corners, which is step 1, and as a rim a few pixels deep along every edge, which is
step 2 — median 8px, worst 28px. Both matter for the same reason: Proxy Nexus builds a card's bleed
by repeating the outermost pixel outward, so black left at the edge becomes the whole bleed, and
shows against the card's white border on any outward drift in the cut. See
[corner_infill](../../corner_infill/README.md) for the infill options and
[coc_edge_crop](../../coc_edge_crop/README.md) for the trim.

The 15 promos are reported and left alone by step 2 and keep their black rim: their art is printed
to the edge, so nothing distinguishes their border from the ground, and their bleed is meant to be
dark.

**[The despeckle](../../coc_despeckle/README.md)** averages out the ~169 line/inch print screen and
resamples to MPC's 300dpi, which is also where the collection loses two thirds of its size — 1.8GB
at scan resolution against 510MB. It runs last, on infilled and trimmed images.

The resample is deliberate and belongs here rather than downstream: Proxy Nexus only ever scales an
image *up*, so a card left at scan resolution reaches MPC oversized and is resampled by a filter
nobody here chose, which brings some of the print screen back. At 1038px the sheet comes out at
MPC's 300dpi spec with nothing further to scale. The despeckle README has the measurements.

## How a file is resolved

**Pack, from the folder.** The deepest folder between the file and the source root that names a
pack, so a cycle folder above it is walked past. A folder that names no pack exactly is matched by
spelling, which is what carries `05 - At the Mountains of Madness` to `the-mountains-of-madness`.

**Card, from the title, then the number.** Punctuation is dropped from both sides for the exact
match, because the archive punctuates titles inconsistently — an apostrophe is written as `_`, an
exclamation mark is sometimes dropped, an ellipsis is spelled with three periods. That is safe: no
two cards in a pack have titles differing only in punctuation.

Whatever punctuation cannot fix, spelling does, scoped to the one pack the folder names. Nine
filenames are misspelled (`Tch-Tcho Tribe`, `San Giogio in Alga`), and a title that two cards are
equally close to is reported rather than guessed.

The number decides the rest. Where a pack prints several cards under one title — the four
Necronomicons in The Unspeakable Pages — it picks between them, and where the title matches
nothing at all it is what is left: `030 - Reading the Star Signs.jpg` is a scan of Itinerant
Scholar, whose number it carries and whose title it does not.

**Promos.** The promos have a folder of their own and no numbers, so they are matched by title
against the cards the catalog gives a promo printing, at a threshold tight enough that only an
exact-but-for-punctuation title gets through. Every one of them is an alternate art of a card that
is already in a pack, so each becomes a second printing of that card rather than a card of its
own.

**Which scan gets used.** The archive holds one card twice, and the larger scan wins.

**What is left out.** `Lore/`, `Organized Play/` and `Miscellaneous/` hold nothing printed on a
poker card: the story sheets folded into each pack, the rules and FAQ, the card backs, and two
draft-format cards that are in no set. The four card back scans are counted and passed by wherever
they sit; they live in the adapter instead.

## Card backs

Four scans, put through the same passes as the fronts — corner-infilled, edge-trimmed, despeckled,
and the three landscape ones rotated the same way the fronts are — in
`proxynexus-core/src/games/coclcg/backs/`:

| File | From | For |
|---|---|---|
| `card_original.jpg` | `Miscellaneous/Card Reverse.jpg` | every card but the 32 story cards |
| `story-core-set_original.jpg` | `01 - Core Set/Story Card Reverse.jpg` | the Core Set's 10 story cards |
| `story-secrets-of-arkham_original.jpg` | `05 - Secrets of Arkham/Story Card Back.jpg` | that set's 10 story cards |
| `story-the-shifting-sands_original.jpg` | `09 - Ancient Relics/Story Card Back.jpg` | The Shifting Sands' 12 story cards |

Story cards are the only ones not printed with the standard back, and each of the three products
holding them has a back of its own, so each is its own back group. Conspiracies look like a fourth
but are not: they are landscape cards a player shuffles into their own deck, so they carry the
standard back.

`Miscellaneous/Draft - Story Card Reverse.jpg` is the back of the two draft cards, which are in no
set, so it is not carried.

## Known gaps

None. The archive resolves 1583 scans covering every printing of every card in the catalog, the 15
promos included.

The two draft cards in `Miscellaneous/` are the only printed cards the collection does not hold.
They are in no set, so the spreadsheet does not list them and the catalog has nowhere to put them.
