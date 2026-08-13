# LotR LCG Image File Renamer

Renames The Lord of the Rings LCG card scans to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against the [Hall of Beorn](https://hallofbeorn.com/) card exports.

There are three different rename scripts:

```
rename.py             the 2011-2021 print run
rename_nightmare.py   the Nightmare decks
rename_alep.py        ALeP fan-made expansions
```

Scans are copied into output folders. The sources are never modified.

`rename.py` writes `lotrlcg-enhanced/`, `rename_nightmare.py` writes `lotrlcg-nightmare/`, and
`rename_alep.py` writes `lotrlcg-alep/`.

`rename_nightmare.py` and `rename_alep.py` both load `rename.py` for the helpers they share (`log`,
`fetch_json`, `clean_for_match`, `normalize_title`, `load_catalog`, `find_orphaned_backs`), so
neither is standalone.

## Getting the scans

Download each archive as it comes, keeping the folder names below exactly. Only the Nightmare
archive needs picking apart — see below.

| Folder | Script | Size | Source | Has bleed |
|---|---|---|---|---|
| `Enhanced Proxies` | `rename.py` | 16 G | [Google Drive](https://drive.google.com/drive/folders/1jEy_yvRaPXGylPfilxhweQjffo9AZQsQ) | yes |
| `Lord of the Rings LCG` | `rename.py` | 16 G | [archive.org](https://archive.org/download/the-lord-of-thering-lcg-collection/Lord%20of%20the%20Rings%20LCG/) | yes |
| `Lord of the Rings LCG RAW` | `rename.py` | 4.6 G | [Google Drive](https://drive.google.com/drive/folders/1rRKAU5DcQoYqFafdgKBwMNRnTrtl0c3c), from [this r/lotrlcg post](https://www.reddit.com/r/lotrlcg/comments/fw69iq/lord_of_the_rings_the_card_game_600dpi/) | no |
| `LOTR LCG Nightmare Cards - Remastered` | `rename_nightmare.py` | 6.4 G of 19 G | [Google Drive](https://drive.google.com/drive/folders/1d_jQs4Hno0K8e5W1pb8mif5VNFuVzCse) | yes |

`rename.py` takes the three archives in one folder. `Enhanced Proxies` is its primary source,
sharpened and given a bleed border; it is incomplete, and `Lord of the Rings LCG` fills the gaps.
`Lord of the Rings LCG RAW` is the untouched scan set the others derive from, with no bleed. All
three are laid out as cycle folders (`01 - Core Set`, `02 - Shadows of Mirkwood`, …) with
`Player`/`Encounter`/`Quest`/`Nightmare` subfolders, and the script resolves a pack from the folder
name. Rulesheets, card backs, print templates and the `Reworked_Cards_(WorkInProgress)` folder sit
alongside the cards and are ignored, because they resolve to no pack or no card. Any folder
resolving to a Nightmare pack is skipped, including the `Nightmare/` subfolders inside
`Lord of the Rings LCG`.

`rename_nightmare.py` takes the Nightmare archive itself, and only needs the eight numbered cycle
folders from it:

```
LOTR LCG Nightmare Cards - Remastered/
├── 01 - Core Set - Shadows of Mirkwood - Nightmare/   needed, 808 M
├── 02 - Khazad-dûm - Dwarrowdelf - Nightmare/         needed, 848 M
├── …                                                  (03 - 06, one per cycle)
├── 07 - The Hobbit/                                   needed, 530 M
├── 08 - The Lord of the Rings/                        needed, 1021 M
├── - Print at Home (A4, 3mm bleed)/                   skip, 1.9 G
├── - Print at Home (A4, no bleed)/                    skip, 1.7 G
├── - Print at Home (US Letter, 3mm bleed)/            skip, 1.9 G
├── - Print at Home (US Letter, no bleed)/             skip, 1.7 G
└── - Professional Printer (3mm Bleed)/                skip, 5.4 G
```

The cycle folders hold the card scans as JPEGs with a 5mm bleed edge, one folder per scenario
across all 72 Nightmare packs, each with a `Card list.txt` manifest. The sagas in `07` and `08`
nest one level deeper, a folder per product then a folder per scenario.

The five `- ` folders are print-ready PDFs of the same cards — 12.6 G of the 19 G total — and
nothing here reads them. The three loose `… details.txt` files are printing instructions. Anything
without a manifest is skipped, so leaving them in place is harmless; not downloading them saves the
bulk of the archive.

## Getting the ALeP scans

ALeP is [A Long-extended Party](https://alongextendedparty.com/), the fan group that has continued
the card pool since FFG stopped. From their
[printing guides and downloads](https://alongextendedparty.com/printing-guides-and-downloads/) page,
take the **RGB Image Archives [bleed margins, 800 dpi]** link, which points to a
[MediaFire folder](https://www.mediafire.com/folder/w2k5kqnfbxu50/GenericPNG).

That folder ships each pack in several languages. Download only the ones suffixed `.English` and
extract them into a single folder:

```
GenericPNG/
├── ALeP - The Aldburg Plot.English/
│   ├── front/          # 001-1-Card Title-1o.png
│   └── back_official/  # 001-1-Card Title-2o.png
├── ALeP - Blood in the Isen.English/
└── …
```

These already carry a bleed margin, so every ALeP output is named `.bleed`.

## Running it

Requires [`uv`](https://docs.astral.sh/uv/).

```bash
uv run rename.py ~/Downloads/lotrlcg --dry-run              # preview
uv run rename.py ~/Downloads/lotrlcg -o ~/Pictures/lotr     # apply

uv run rename_nightmare.py "~/Downloads/LOTR LCG Nightmare Cards - Remastered" --dry-run
uv run rename_nightmare.py "~/Downloads/LOTR LCG Nightmare Cards - Remastered" -o ~/Pictures/lotr

uv run rename_alep.py ~/Downloads/GenericPNG --dry-run
uv run rename_alep.py ~/Downloads/GenericPNG -o ~/Pictures/lotr/lotrlcg-alep
```

The first argument is the source folder, `-o` is where the output folder is created, defaulting to
the current directory.

Dry-run first and read the `[SKIP]` lines. An unexpected skip usually means a filename the matcher
doesn't understand rather than a card with no scan. `rename_nightmare.py` reports no skips at all
against an intact archive.

The Hall of Beorn catalog is cached next to the script as `lotrlcg_catalog_cache.json`, downloaded
on first run from the `PlayerCards`, `EncounterCards` and `QuestCards` exports, and shared by
`rename.py` and `rename_nightmare.py`. Delete it to pick up catalog changes. `rename_alep.py` has no
cache and fetches `hallofbeorn.com/Export/ALeP` and `ringsdb.com/api/public/cards/` on every run.

All three scripts re-encode to JPEG at quality 90.

## How it maps

**Card ids** come from the Hall of Beorn `Slug` field, transliterated to ASCII, lowercased, with
every non-alphanumeric character replaced by `_`. The slug already carries a pack disambiguator
(`Lost-Soul-of-Lorien-TDMN`), which is what keeps reprints of the same title apart.

**Packs** come from the folder the scan sits in, resolved through a cascade: the folder name, then
the same name with `Nightmare` appended when `Nightmare` appears anywhere in the path, then the
parent folder, then the grandparent, then a last-resort Nightmare guess. This looks like defensive
dead code and is not — the archives disagree about how deeply cards are nested, and each step covers
a real case. Folder names carry a leading index (`03 - Khazad-dûm`) which is stripped first.
`rename_nightmare.py` resolves the scenario folder, then its parent for the sagas, which nest one
level deeper.

**Matching** a filename to a card is fuzzier in `rename.py`. Filenames are shaped `001 - Aragorn`,
`047a - A Perilous Voyage`, or `011 - 1B - The Hunt Begins`, and `parse_filename()` splits them into
a position, a title and a back-face flag. A trailing `B`/`D`/`F`/`H` on the position, a `(side b)`
marker or the word `reverse` all mark a back face, which gets the `~back` part suffix.
`clean_for_match()` then strips punctuation, case and a leading `"the"` before comparing to the
catalog, and two typo tables are applied on top.

**The typo tables run in two layers.** A blanket list of replacements handles misspellings that are
unambiguous across the whole archive, and `PACK_TITLE_FIXES` is keyed by `(pack, title)` for the
rest. The scoping matters: the same wording is often correct in one pack and wrong in another, so a
global replace silently loses a card. Both tables map filenames onto Hall of Beorn's spelling, which
means several entries map a *correctly* spelled filename onto an upstream typo.

**`rename_nightmare.py` matches on position and needs neither.** Each scenario folder's
`Card list.txt` lists every card as `Qty | Type | Front | Back`, and the leading number is the one
printed on the card, which agrees with Hall of Beorn's `Number`. That resolves the cards Hall of
Beorn stores misspelled — `Writing Tentacle`, `Gobline Trapper`, `Swarming Mosquitos`,
`Lost Soul of L≤rien`, `Rob and Bob` — along with the Nightmare Mode cards, whose catalog titles
carry a ` Nightmare` suffix the manifest omits. Title wins only where it disagrees *and* resolves
unambiguously, which is what puts Intruders in Chetwood #5 and #6 the right way round: Hall of Beorn
has those two transposed, and the printed cards read 5 = Outskirts of Archet, 6 = Greenway Path. A
title that collides inside its pack is dropped from the lookup, so Flight from Moria's three cards
titled "Search for an Exit" stay on position. Both paths log an `[INFO]` line.

**Back faces come from the manifest** in `rename_nightmare.py`, not from the filename: `Back` is
either `encounter back`, meaning single-sided, or the file to write as `~back`. Where that file is
another numbered card in the pack, both faces are catalog cards and each is written as the other's
`~back` — The Drowned Ruins pairs Jagged Cavern with Overgrown Passage this way. A `2B` back belongs
to the front's own card and does not become a front of its own.

**Nightmare scans are trimmed and turned upright.** Their bleed is 5mm, so `trim_box()` takes the
excess off: 94 px from the sides and 91 px from the top and bottom of a 3432x4680 scan, leaving
3244x4498 with about 3mm of bleed all round. The target is the cut rather than the frame. MPC cuts
a `.bleed` upload at 744/816 of its width and 1038/1110 of its height, and `crop_bleed_border` takes
those same fractions off before `pdf.rs` stretches what is left onto a fixed 178.54 x 249.09 pt
card, so the card has to sit at exactly those fractions — 4.41% of the width and 3.24% of the
height. Bleed beyond that prints as a dark margin. The sides lose more than the top and bottom
because the fractions differ per axis while the source bleed is 5mm all round. The 35 sideways quest
scans are rotated clockwise first, since every other collection is portrait and so is the MPC frame.

**`.bleed`** is declared per archive in `SOURCE_FOLDERS` rather than measured off the image. The
other renamers read image dimensions instead, which does not work here: the trimmed scans in
`Lord of the Rings LCG RAW` and the bled quest cards in `Lord of the Rings LCG` overlap in aspect
ratio, so no threshold separates them.

**A card is only taken once.** `processed_cards` keys on `(card id, pack, is_back)`, so the first
archive to supply a printing wins and the later ones are skipped silently.

**One special case is hardcoded.** Na'asiyah and Captain Sahír in The Grey Havens are single
physical cards with a different card on each face, so writing one face also writes the other's
`~back`.

## Tests

```bash
uv run --with pytest --with unidecode --with pillow --no-project \
  pytest utils/image_file_renamers/lotrlcg/tests/ -v
```

Covers name normalization, filename parsing, back-face detection, the pack-scoping property of
`PACK_TITLE_FIXES`, per-folder orphaned-back validation, the ALeP title resolver, and for the
Nightmare script: manifest parsing, card-reference splitting, copy-suffix and scan-key handling, the
position-versus-title resolution rules, pack resolution, and the bleed trim landing on the MPC
frame. No file I/O, network calls or image fixtures, and nothing depends on the catalog cache, so
the suite passes on a fresh clone.

## Known limitations

- **Catalog printings across 23 packs have no scan at all.** These are FFG's 2022-onwards revised
  line, which the archives predate: Revised Core Set, the three Saga reprints (The Fellowship of the
  Ring, The Two Towers, The Return of the King), the Campaign and Hero Expansions, the four Starter
  Decks, The Dark of Mirkwood and eight Preorder Promotions. A further 24 printings are missing from
  13 packs that are otherwise covered — 7 from the fan-made The Hunt for the Dreadnaught, 5 from
  Escape from Khazad-dûm, one or two each from the rest. Hall of Beorn does publish image URLs, so a
  `download_missing.py` in the style of the AGOT one is possible, but nothing here does it. The
  Nightmare line is complete: 637 printings across all 72 packs.
- **Non-card files are skipped per `rename.py` run.** Scenario introduction pages
  (`000 - Introduction Part 3.jpg`), rules inserts, difficulty-mode selectors for the fan-made Hunt
  for the Dreadnaught pack, and `Heading.jpg` section dividers. They resolve to a pack but to no card
  in it. Read the list when re-running against a different archive; a genuine card in there is a
  matcher bug.
- **Only the first copy of a Nightmare card is kept.** The archive scans each copy of a card
  separately, carrying the encounter-set number printed on that copy, but the naming convention has
  one image per printing and part, so copies 2..n are dropped and Proxy Nexus prints copy 1 that many
  times. `--all-copies` writes them all for inspection; the names it produces are not indexable.
  Keeping them needs a copy slot in the naming convention and in printing resolution.
- **A card with two different backs keeps the first.** The Drowned Ruins prints Jagged Cavern and
  Submerged Crawlway on two cards each, backed with Overgrown Passage on one and Sharp Precipice on
  the other. One image per printing and part means the second pairing is dropped with a `[WARN]`.
- **An unresolvable collision maps one front onto every matching card.** When several cards in a
  pack share a title and the position number doesn't pick one, the front image is written for all of
  them rather than guessing. This over-produces images for ambiguous titles, which is safer than
  silently dropping a card. Back faces take the first match and log a `[WARN]`.
- **ALeP name collisions are resolved by sort order, last one wins.** 15 files collide onto 12
  names. Most are the pack-title and "Community Scenario" cover cards, which carry a `0.x` prefix and
  are overwritten by the real numbered card; one pair is an original and an errata'd printing of
  Brand son of Bain, where the errata wins. The right image surviving is a property of how these
  files happen to sort, not a rule the script enforces — check the `[WARN]` lines after any
  re-download.
- **`GENERIC_BACK_FILE_SIZES` dedupes shared ALeP card backs by exact byte size**
  (`{1670115, 1828547, 1675019, 1693758}`). A re-export or re-compression upstream would silently
  stop matching and start emitting spurious unique backs.
- **`ENCODING_FIXES` repairs mangled non-ASCII in ALeP filenames** (`Th_odwyn` → `Theodwyn`,
  `Nazg l` → `Nazgul`) as a fallback when the live catalog has no match. One table serves both the
  front and back paths, and has to stay that way: a card's two faces must resolve to the same target
  id, so per-path copies can drift apart and break the `~back` pairing.
- **The typo tables are hand-maintained.** They encode specific mismatches between `rename.py`'s
  filenames and this catalog. Pointing it at a differently-named archive will need new entries, and a
  plausible-looking tidy-up can silently break a working match.
