# Arkham Horror LCG Image File Renamer

Renames Arkham Horror: The Card Game images to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against [ArkhamDB](https://arkhamdb.com/). Card ids are ArkhamDB codes and pack ids are ArkhamDB
pack codes, which is what the `ahlcg` adapter reads.

Two source archives, one script each, because neither covers the game on its own:

```
rename.py       Core Set/.../1_The Gathering/Act 1A_Trapped.tif    Google Drive, three products
rename_tts.py   SCED-downloads/.../CairoBazaar.d2de0e.json         SCED, everything else
```

The Google Drive images are ~580 dpi against SCED's 750x1050, so they are used wherever they
reach and SCED fills the rest. `--exclude` is how SCED is told what to skip. Neither source is
modified.

The collection being built is the **Chapter 1** card pool: both core sets, the nine campaigns
through The Feast of Hemlock Vale, the five investigator starter decks, the ten standalone
scenarios and the five Return to boxes — 70 ArkhamDB packs, 4622 cards. `CHAPTER1_PACKS` in
`rename_tts.py` lists them.

## Getting the images

### The Google Drive source

A [Google Drive folder](https://drive.google.com/drive/folders/1P0PEHsMVdyPQ2KMI33w6NO1muPaFsZec),
split into Chapter 1 and Chapter 2. Download **Chapter 1** as-is; the script walks the whole tree.
Chapter 2 is not handled here — it holds the 2021 starter decks and a Core Set reprint under a
different layout.

Every file is a ~580 dpi TIFF cut to the card with no bleed border. The folder holds four products,
one ArkhamDB pack each:

| Folder | Pack |
|---|---|
| `Core Set/` | `core` |
| `The path to Carcosa/` | `ptc` |
| `Return to The Circle Undone/` | `rttcu` |
| `Film Fatale/` | `film_fatale` |

The first three are in the Chapter 1 pool and are what `--packs` defaults to. Film Fatale is not,
and is reported and skipped; `--packs all` builds it too.

Beware that the archive's own "Chapter 1" is a different division from the card pool of that name:
it holds Film Fatale, which the pool does not, and none of the other standalones, which it does.

### The SCED source

[SCED](https://github.com/argonui/SCED) carries a picture of every card in the game, and records
the ArkhamDB id each one belongs to. Two clones, side by side in one folder:

```bash
mkdir ~/ah-images && cd ~/ah-images
git clone https://github.com/Chr1Z93/SCED.git
git clone https://github.com/Chr1Z93/SCED-downloads.git
```

`SCED` holds the player cards the mod always has loaded, `SCED-downloads` every campaign and
scenario. Together about 650MB, and no images at all — only JSON describing where they live. The
pictures themselves are sheets on Steam's CDN, downloaded on the first run into `sced_sheet_cache/`
and reused after that. Expect ~3GB; the cache is gitignored and can be deleted when you are done.

## Running it

Requires [`uv`](https://docs.astral.sh/uv/). Run everything from this folder, and run the Google
Drive source first, because SCED needs its output to know what to skip.

```bash
# 1. Google Drive -> ahlcg-hq
uv run rename.py "~/Downloads/Arkaham Horror LCG/Chapter 1" --dry-run      # preview
uv run rename.py "~/Downloads/Arkaham Horror LCG/Chapter 1" -o hq_renamed
uv run fix_orientation.py hq_renamed -o hq_faced
uv run ../../corner_infill/corner_infill_arc.py hq_faced \
    -o ~/Pictures/proxynexus_collections/ahlcg/ahlcg-hq

# 2. SCED, filling what Google Drive did not reach -> ahlcg-tts
uv run rename_tts.py ~/ah-images --exclude hq_renamed --dry-run            # preview
uv run rename_tts.py ~/ah-images --exclude hq_renamed -o tts_cut
uv run fix_orientation.py tts_cut -o tts_faced
uv run ../../corner_infill/corner_infill_arc.py tts_faced \
    -o ~/Pictures/proxynexus_collections/ahlcg/ahlcg-tts
```

`--exclude` takes any folder of already-named files, so the finished `ahlcg-hq` works there just as
well as `hq_renamed`.

Dry-run first and read the report sections. `Extra scanned copies of one card` is the long one and
is expected — the Google Drive archive scans every physical copy, so an encounter set with two
Crypt Chills is two files. `Unmatched` and `Cards SCED has no English object for` are the ones
that want looking at; both are empty on the Chapter 1 build.

The ArkhamDB catalog is downloaded on first run and cached as `ahlcg_catalog_cache.json`;
`--refresh-catalog` re-fetches it.

**[fix_orientation.py](fix_orientation.py)** makes the landscape cards — investigators, acts and
agendas — face the same way. They are stored portrait, as
[AGOT's plots are](../agot/rotate_horizontal.py), but neither source stores them facing
consistently, so some acts in a campaign would print upside down relative to the rest. Each is
compared against ArkhamDB's own picture rather than turned on a rule. It turns 49 of the 491
Google Drive images and 1 of the 5916 SCED ones: SCED is generated and already consistent, the
Google Drive archive is hand-made and is not.

**[corner_infill_arc.py](../../corner_infill/README.md)** removes the ground outside each card's
rounded corners. It matters because Proxy Nexus builds bleed by repeating the outermost pixel, so
a black wedge left in a corner becomes the whole bleed. Nearly all the work is on the SCED images —
3263 of 5916 have a corner to fill, against 3 of 491 from Google Drive, which were cut to the card
and so hold the card's own border colour there.

## Finishing the face check

`rename_tts.py` settles which of a location's two pictures is the front by comparing both against
ArkhamDB (see [How it maps](#how-it-maps)). ArkhamDB rate limits hard, so a run will usually stop
partway, settle what it can, and report the rest under `Cards left as SCED had them, no reference
yet`. Run it again to pick up where it stopped:

```bash
uv run rename_tts.py ~/ah-images --exclude hq_renamed \
    -o ~/Pictures/proxynexus_collections/ahlcg/ahlcg-tts --settle-only
```

`--settle-only` skips the cutting, reads the files as they stand and only renames, so it is safe to
repeat and safe to point at a finished collection. Repeat until nothing is left waiting. A card
still waiting prints the same either way round; only which side shows first differs.

## How it maps

**`rename.py`** takes the pack from the top-level folder, then looks for every catalog title in
that pack inside the whole filename — the fields are not in a fixed order, and the title is the
second in `5_Constance Dumaine_Enemy` but the third in `2_Chilling Cold_Crypt Chill_Treachery`.
The title ending last wins, so `16_The Devourer Below_Umordhoth_Enemy` is Umôrdhoth rather than the
scenario. Punctuation is dropped from both sides, because the archive writes an apostrophe as `_`.
28 misspelled filenames fall through to a fuzzy match, scoped to the one pack.

Cards sharing a name are separated by the subtitle or type the filename gives, and failing that by
their order in the folder. Two levels of one player card have neither, so `LEVEL_FIXES` records the
level read off the collector number on the scan. `TITLE_FIXES` holds two cards ArkhamDB has since
renamed.

Sides are `_Side A` / `_Side B`, except acts and agendas, which write the side into the label and
give each face its own title. A back that names itself is placed by matching ArkhamDB's
`back_name`, and a back shared by several cards is written out once per card it backs.

**`rename_tts.py`** takes the ArkhamDB id from the `.gmnotes` file beside each card object, and the
picture by cutting the sheet slot the object names. Nothing is matched on a filename. That places
4506 of the 4622 cards outright; SCED names an object after the face it puts on the table, which is
not always the code ArkhamDB indexes it under, so four fallbacks follow:

| | | |
|---|---|---|
| The hidden half ArkhamDB links to | 25 | `03325b` Songs That the Hyades Shall Sing, hiding `03325` Shores of Hali |
| The face suffix ArkhamDB adds | 58 | `04128a` At the Station against SCED's `04128` |
| The per-copy suffix SCED adds | 4 | One `10512` Alkaline Rail against SCED's `10512a` and `10512b`, one per printed copy |
| The card ArkhamDB calls it a duplicate of | 29 | A reprint with the same art: `60307` Switchblade is cut from `01044` |

Translations are dropped before any of that, and an official product beats a fan rework of it. Two
cards cut from one object are reported — for a duplicate that is the point, and anything else is a
fallback having reached too far.

**Which of a card's two pictures is the front** is not something SCED records, so it was
measured against ArkhamDB over 112 double-sided cards. Acts, agendas, investigators, scenarios and
stories were the right way round 53 times out of 53. Locations were not: they are laid on the table
unrevealed side up and often, though not always, authored that way. So locations, and only
locations, are checked per card and swapped where they are reversed. `--check-faces` takes other
types, `all`, or `none`.

## Tests

```bash
uv run --with pytest --with Pillow --no-project pytest utils/image_file_renamers/ahlcg/tests/ -v
```

Covers filename parsing, title matching and side pairing for Google Drive; id resolution, sheet
geometry, back detection and the rate-limit backoff for SCED. No network calls.

## Known limitations

**Six Core Set cards are not in the Google Drive source**, and all six come from SCED instead.
Each class folder holds 13 files against the Core Set's 14, and drops whichever level of a
duplicated name it did not scan: `01018` Beat Cop, `01030` Magnifying Glass, `01048` Leo De Luca,
`01066` Blinding Light, `01084` Lucky!. `98004` Roland Banks is a novella promo ArkhamDB files
under `core` that is not in the box.

**Six cards get a front but no back**, three from each source. In SCED, `02250` Whateley Ruins,
`06015a` Dream-Gate and `10578` Weed-Choked Beach sit in decks given the generic card back where
ArkhamDB has a second face. In the Google Drive source, `54032` The 9th Ward, `54033` Library of
Ebla and `54060` Winding Gulf sit at positions the shared-back scan covering their neighbours does
not reach.

**A sheet can be gone** from Steam's CDN. Where SCED holds the card more than once that is
recoverable, and a dead sheet sends its cards to the next copy; one Chapter 1 card needs it,
`07115` Skeptic. A card whose every copy is gone would be reported, and none currently is.

**What the Google Drive source holds that is left out:** 77 extra scanned copies of cards already
placed, the 23-card Major Arcana tarot deck ArkhamDB does not carry, 22 mini investigator cards
ArkhamDB has no code for, and 2 card backs, which belong in the adapter rather than a collection.

ArkhamDB also lists `01000` Random Basic Weakness under `core`. It is a placeholder flagged hidden,
no such card is printed, and nothing tries to give it an image.
