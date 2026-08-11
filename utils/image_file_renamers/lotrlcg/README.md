# LotR LCG Image File Renamer

Renames The Lord of the Rings LCG card scans to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against the [Hall of Beorn](https://hallofbeorn.com/) card exports.

There are two different rename scripts:

```
rename.py        official FFG cards, 2011-2021 print run, plus the Nightmare decks
rename_alep.py   ALeP fan-made expansions
```

Scans are copied into output folders. The sources are never modified.

`rename.py` writes two folders rather than one: `lotrlcg-enhanced/` for the normal packs and
`lotrlcg-nightmare/` for the Nightmare decks. `rename_alep.py` writes `lotrlcg-alep/`.

`rename_alep.py` loads `rename.py` for the four helpers they share (`log`, `fetch_json`,
`clean_for_match`, `normalize_title`), so it is not standalone.

## Getting the official scans

Four archives feed `rename.py`. Download all four into one folder, keeping the names below exactly.

| Folder | Size | Source | Has bleed |
|---|---|---|---|
| `Enhanced Proxies` | 16 G | [Google Drive](https://drive.google.com/drive/folders/1jEy_yvRaPXGylPfilxhweQjffo9AZQsQ) | yes |
| `Lotr Lcg Nightmare 600 dpi Scans` | 3.1 G | [Google Drive](https://drive.google.com/drive/folders/10pz9RF2e1QrJXqsmmLWPkO1Hu1auQuxB), from [this r/lotrlcg post](https://www.reddit.com/r/lotrlcg/comments/1mf1ncw/here_are_all_the_high_resolution_nightmare_scans/) | yes |
| `Lord of the Rings LCG` | 16 G | [archive.org](https://archive.org/download/the-lord-of-thering-lcg-collection/Lord%20of%20the%20Rings%20LCG/) | yes |
| `Lord of the Rings LCG RAW` | 4.6 G | [Google Drive](https://drive.google.com/drive/folders/1rRKAU5DcQoYqFafdgKBwMNRnTrtl0c3c), from [this r/lotrlcg post](https://www.reddit.com/r/lotrlcg/comments/fw69iq/lord_of_the_rings_the_card_game_600dpi/) | no |

`Enhanced Proxies` is the primary source for the normal packs: FFG scans that have been sharpened
and given a bleed border. It holds no Nightmare cards, so the Nightmare decks come from
`Lotr Lcg Nightmare 600 dpi Scans`. Both are incomplete, and `Lord of the Rings LCG` fills the gaps. 
`Lord of the Rings LCG RAW` is the untouched scan set the others derive from, with no bleed.

Download them as they come. All four are laid out as cycle folders (`01 - Core Set`,
`02 - Shadows of Mirkwood`, …) with `Player`/`Encounter`/`Quest`/`Nightmare` subfolders, and the
script resolves a pack from the folder name. Rulesheets, card backs, print templates and the
`Reworked_Cards_(WorkInProgress)` folder sit alongside the cards and are ignored, because they
resolve to no pack or no card.

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
uv run rename_alep.py ~/Downloads/GenericPNG --dry-run
uv run rename_alep.py ~/Downloads/GenericPNG -o ~/Pictures/lotr/lotrlcg-alep
```

The first argument is the folder holding the four archives; `-o` is where the output folders are
created, defaulting to the current directory.

Dry-run first and read the `[SKIP]` lines. An unexpected skip usually means a filename the matcher
doesn't understand rather than a card with no scan.

The Hall of Beorn catalog is cached next to the script as `lotrlcg_catalog_cache.json`, downloaded
on first run from the `PlayerCards`, `EncounterCards` and `QuestCards` exports. Delete it to pick up
catalog changes. `rename_alep.py` has no cache and fetches `hallofbeorn.com/Export/ALeP` and
`ringsdb.com/api/public/cards/` on every run.

Both scripts re-encode to JPEG at quality 90.

## How it maps

**Card ids** come from the Hall of Beorn `Slug` field, transliterated to ASCII, lowercased, with
every non-alphanumeric character replaced by `_`. The slug already carries a pack disambiguator
(`Lost-Soul-of-Lorien-TDMN`), which is what keeps reprints of the same title apart.

**Packs** come from the folder the scan sits in, resolved through a cascade: the folder name, then
the same name with `Nightmare` appended when `Nightmare` appears anywhere in the path, then the
parent folder, then the grandparent, then a last-resort Nightmare guess. This looks like defensive
dead code and is not — the four archives disagree about how deeply cards are nested, and each step
covers a real case. Folder names carry a leading index (`03 - Khazad-dûm`) which is stripped first.

**Matching** a filename to a card is fuzzier. Filenames are shaped `001 - Aragorn`,
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

**`.bleed`** is declared per archive in `SOURCE_FOLDERS` rather than measured off the image. The
other renamers read image dimensions instead, which does not work here: the trimmed scans in
`Lord of the Rings LCG RAW` and the bled quest cards in `Lord of the Rings LCG` overlap in aspect
ratio, so no threshold separates them.

**A card is only taken once.** `processed_cards` keys on `(card id, pack, is_back)`, so the first
archive to supply a printing wins and the later ones are skipped silently. On the archives above
that yields 3546 files from `Enhanced Proxies`, 646 from `Lotr Lcg Nightmare 600 dpi Scans`, 400
from `Lord of the Rings LCG` and 9 from `Lord of the Rings LCG RAW` — 3891 in `lotrlcg-enhanced`
and 710 in `lotrlcg-nightmare`.

**Two special cases are hardcoded.** Na'asiyah and Captain Sahír in The Grey Havens are single
physical cards with a different card on each face, so writing one face also writes the other's
`~back`. The Nîn-in-Eilph Nightmare quest fronts are named after the wrong stage in the archive and
are intercepted by filename.

## Tests

```bash
uv run --with pytest --with unidecode --with pillow --no-project \
  pytest utils/image_file_renamers/lotrlcg/tests/ -v
```

Covers name normalization, filename parsing, back-face detection, the pack-scoping property of
`PACK_TITLE_FIXES`, per-folder orphaned-back validation, and the ALeP title resolver. No file I/O,
network calls or image fixtures, and nothing depends on the catalog cache, so the suite passes on a
fresh clone.

## Known limitations

- **1238 catalog printings across 24 packs have no scan at all.** These are FFG's 2022-onwards
  revised line, which the archives predate: Revised Core Set, the three Saga reprints (The
  Fellowship of the Ring, The Two Towers, The Return of the King), the Campaign and Hero Expansions,
  the four Starter Decks, The Dark of Mirkwood, eight Preorder Promotions and Intruders in Chetwood
  Nightmare. A further 24 printings are missing from 13 packs that are otherwise covered — 7 from the
  fan-made The Hunt for the Dreadnaught, 5 from Escape from Khazad-dûm, one or two each from the
  rest. Hall of Beorn does publish image URLs, so a `download_missing.py` in the style of the AGOT
  one is possible, but nothing here does it.
- **269 files are skipped per run.** Most are not cards — scenario introduction pages
  (`000 - Introduction Part 3.jpg`), rules inserts, difficulty-mode selectors for the fan-made Hunt
  for the Dreadnaught pack, and `Heading.jpg` section dividers. They resolve to a pack but to no card
  in it. Read the list when re-running against a different archive; a genuine card in there is a
  matcher bug.
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
  front and back paths, and has to stay that way: a card's two faces must resolve to the same
  target id, so per-path copies can drift apart and break the `~back` pairing.
- **`migrate.log` and `migration_audit_log.csv` are always written under `lotrlcg-enhanced/`**, never
  under `lotrlcg-nightmare/`, regardless of which folders a run actually touched.
- **The typo tables are hand-maintained.** They encode specific mismatches between these filenames
  and this catalog. Pointing the script at a differently-named archive will need new entries, and a
  plausible-looking tidy-up can silently break a working match.
