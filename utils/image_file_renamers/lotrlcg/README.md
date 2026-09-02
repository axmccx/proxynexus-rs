# LotR LCG Image File Renamer

Renames The Lord of the Rings LCG card scans to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against the [Hall of Beorn](https://hallofbeorn.com/) card exports.

There are four different rename scripts, and one script that checks the result:

```
rename.py             the 2011-2021 print run
rename_nightmare.py   the Nightmare decks
rename_alep.py        ALeP fan-made expansions
rename_ffg_pdfs.py    FFG's print-and-play campaign and replacement cards
audit_coverage.py     which sets the output folders can print, and what is missing
```

Scans are copied into output folders. The sources are never modified.

`rename.py` writes `lotrlcg-enhanced/`, `rename_nightmare.py` writes `lotrlcg-nightmare/`,
`rename_alep.py` writes `lotrlcg-alep/`, and `rename_ffg_pdfs.py` writes `lotrlcg-ffg-pdf/`.
One collection is not renamer output at all: see *Hand-filled gaps* below.

`rename_nightmare.py`, `rename_alep.py` and `rename_ffg_pdfs.py` all load `rename.py` for the
helpers they share (`log`, `set_log_file`, `fetch_json`, `clean_for_match`, `normalize_title`,
`load_catalog`, `find_orphaned_backs`), so none of the three is standalone.

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

## Getting the FFG print-and-play cards

`rename_ffg_pdfs.py` does not take scans. It takes the output of
[`utils/lotr_ffg_pdf_slicer/`](../../lotr_ffg_pdf_slicer/README.md), which extracts card images from
the print-and-play PDFs FFG publishes on its product support pages. Download the six PDFs into one
folder, run the slicer over that folder with no `-o`, and it leaves one folder of images per PDF
beside them:

```
lotrlcg ffg pdf/
├── mec108_angmar_campaign_cards_wprint_permission.pdf
├── mec108_angmar_campaign_cards_wprint_permission/
│   ├── cards/          # 001 - 156.png, 001 - 156~back.png, ...
│   └── manifest.csv
└── …
```

That whole folder is what this script takes. It reads each `manifest.csv` for the pages and
collector numbers, and reads the PDF beside it for the page text.

These PDFs carry the campaign and replacement cards from FFG's 2022-onwards revised line, which the
scan archives predate. Between them they complete three card sets:

| Card set | Printings covered | From |
|---|---|---|
| Angmar Awakened Campaign Expansion | 39 of 39 | `mec108_angmar_campaign_cards_wprint_permission.pdf` |
| Dream-chaser Campaign Expansion | 79 of 79 | `mec111_dreamchaser_campaign_cards_web2.pdf` |
| Ered Mithrin Campaign Expansion | 40 of 40 | `mec115_ered_mithrin_camp_box_-_campaign_cards.pdf` |
| The Dark of Mirkwood | 11 of 51 | `dark_of_mirkwood_campaign_cards.pdf` |
| Revised Core Set | 10 of 138 | `core_set_campaign_cards.pdf` |
| Defenders of Gondor (Starter Deck) | 3 of 34 | `mec104-mec105_replacement_cards.pdf` |
| Elves of Lórien (Starter Deck) | 2 of 31 | `mec104-mec105_replacement_cards.pdf` |

The last four PDFs print only a subset of their set — the campaign cards a box adds to an earlier
pack, and five errata'd cards — so the rest of those sets still has no image.

## Hand-filled gaps

`lotrlcg-gap-fills/` is not renamer output. Nothing regenerates it; if it is lost it has to be
rebuilt by hand from the steps here. It holds the seven cards `audit_coverage.py` reported as
`blank` — no image anywhere in the other four collections, and no other printing of the same card
to fall back on.

**Four downloaded.** FFG printed The Dark of Mirkwood's #15, #23, #26 and #37 only in the retail
scenario pack. They are in none of the three scan archives, and the print-and-play PDF carries only
the campaign layer, so nothing in this repo can produce them. Taken from
[lotr.cardgame.tools](https://lotr.cardgame.tools) at 1468x2080, front only, no bleed:

| Card | Source | Written as |
|---|---|---|
| #15 Abandoned Camp | [MEC102-15](https://images.cardgame.tools/lotr/MEC102-15-front-Location-Abandoned_Camp-The_Oath_Campaign.jpg) | `abandoned_camp_tdom@the_dark_of_mirkwood.jpg` |
| #23 Wild Wargs | [MEC102-23](https://images.cardgame.tools/lotr/MEC102-23-front-Enemy-Wild_Wargs-The_Goblins_Campaign.jpg) | `wild_wargs_tdom@the_dark_of_mirkwood.jpg` |
| #26 Obsidian Arrows | [MEC102-26](https://images.cardgame.tools/lotr/MEC102-26-front-Treachery-Obsidian_Arrows-The_Goblins_Campaign.jpg) | `obsidian_arrows_tdom@the_dark_of_mirkwood.jpg` |
| #37 Crumbling Stairs | [MEC102-37](https://images.cardgame.tools/lotr/MEC102-37-front-Location-Crumbling_Stairs-The_Caves_of_Nibin_Dum_Campaign.jpg) | `crumbling_stairs_tdom@the_dark_of_mirkwood.jpg` |

That site composites cards onto black, so their rounded corners arrive filled with black rather
than the white a flatbed leaves, and [`corner_infill.py`](../../corner_infill/README.md) does
nothing to them because it only detects white. Run `corner_infill_dark.py` instead, which floods
outward from each image corner and inpaints what it reaches; each of these filled 0.22-0.26% of its
pixels, which is four corner arcs.

These four are also the only images in the collection that still carry the print screen. They are
renders of the offset-printed card rather than scans of a proxy, and the rosette is on them at
3.5px across the 1468x2080 they download at — worst in the shadows, where near-solid ink leaves the
paper showing through as bright dots. Everything else here comes from the Enhanced Proxies, which
were denoised and sharpened before they were published. So
[`lotr_despeckle.py`](../../lotr_despeckle/README.md) runs on them and on nothing else, notching the
screen out of the spectrum — it is periodic and the picture is not — and denoising what that
leaves. It leaves
the resolution alone: these arrive at about 592dpi and the rest of the collection sits at about
578dpi, so they already match, and downscaling only these to MPC's cut line would leave them the
one set of cards at half everyone else's resolution.

Then rename by hand, taking the ids from `printing_card_ids` rather than guessing them. No
`.bleed` — these are cut to the card.

```bash
uv run ../../corner_infill/corner_infill_dark.py ~/Downloads/cardgame-tools-lotr --debug
uv run ../../lotr_despeckle/lotr_despeckle.py ~/Downloads/cardgame-tools-lotr-infilled \
    -o ~/Downloads/cardgame-tools-lotr-despeckled
```

**Three copied from another printing.** The Fellowship of the Ring is the 2022 Saga reprint of The
Black Riders and The Road Darkens, and three of its cards have no scan. Each was copied out of
`lotrlcg-enhanced` and renamed onto the Fellowship printing, so all three carry the older set's
frame and icon:

| Card | Copied from | Written as |
|---|---|---|
| #69 Race to Rivendell | `race_to_rivendell_tbr@the_black_riders` (front and `~back`) | `race_to_rivendell_tfotr@the_fellowship_of_the_ring` |
| #77 Stricken Dumb | `striken_dumb_tbr@the_black_riders` | `stricken_dumb_tfotr@the_fellowship_of_the_ring` |
| #151 The Argonath | `the_argonauth_rd@the_road_darkens` | `the_argonath_tfotr@the_fellowship_of_the_ring` |

Stricken Dumb and The Argonath are the same card either side — comparing their identity strings
with the title field dropped, the two halves match exactly, so only Hall of Beorn's spelling
separated them. It stores `Striken Dumb` in The Black Riders and `The Argonauth` in The Road
Darkens; `normalize_title` feeds the identity, so one letter split each card into two ids and
nothing filled the gap. The copies are the right artwork.

**Race to Rivendell is not.** The two printings are genuinely different cards: same title, type and
stats, but the back's rules text diverges where FFG reworded the stage for the Saga release. These
two files carry the 2011 wording under the Fellowship id. Replace them if a scan of the Fellowship
printing turns up.

`.bleed` is kept on all four copies because the sources carry a bleed border, and omitted on the
four downloads because they do not.

## Checking coverage

`audit_coverage.py` reads the collection folders and reports, per card set, what would come out if
you asked Proxy Nexus to print it. Every card lands in one of five states:

| State | Meaning |
|---|---|
| `own` | this printing has an image of its own |
| `filled` | another printing of the same card supplies it — prints correctly, in the other set's frame |
| `twin` | the second Hall of Beorn entry for a flip card whose other face prints |
| `wrong` | what resolves is a different card |
| `blank` | nothing resolves; the card does not print |

A set is complete when every card is `own`, `filled` or `twin`. **None of the three is a gap.**
`filled` is right because two printings that share an identity are the same card — this is what
lets an incomplete set print in full from the sets around it. `twin` is right because Hall of Beorn
lists a card that flips twice: Eithiliant and Eithiliant-Upgraded are one piece of cardboard, and
the renamers write the second face as the first's `~back`, so nothing will ever carry the second
id and nothing should.

```bash
uv run audit_coverage.py ~/Pictures/proxynexus_collections/lotrlcg
uv run audit_coverage.py ~/Pictures/proxynexus_collections/lotrlcg --include ~/Downloads/lotrlcg-ffg-pdf
uv run audit_coverage.py ~/Pictures/proxynexus_collections/lotrlcg --set "The Grey Havens"
uv run audit_coverage.py ~/Pictures/proxynexus_collections/lotrlcg --csv coverage.csv --all
```

The first argument is the folder holding the `lotrlcg-*` collection folders. `--include` adds one
that is not in there yet, so a collection can be measured before it is added. `--exclude` drops one
by name; folders ending `_bak`, `-bak`, `.bak` or `~` are skipped by default, because a backup beside the
collections would otherwise be scanned as one and cover cards the real collection is missing.
`--set` prints every card of one set. `--csv` writes a row per card.

Pack release dates come from RingsDB, the adapter's own source, cached beside the script as
`lotrlcg_ringsdb_pack_dates.json`; `--refresh-dates` re-fetches. They are not cosmetic: the
earliest printing's slug becomes the card id, so a wrong or missing date moves ids and silently
changes the answer. 845 of the 5327 printings depend on them.

**Nothing in it is fuzzy.** Two printings are the same card when
[`identity.rs`](../../../proxynexus-core/src/games/lotrlcg/identity.rs) says their identity strings
match — title, card type, sphere, and both faces' stats, rules text and subtitle, compared exactly.
The script mirrors that function, `card_titles` beside it, `file_naming.rs::parse_filename`,
`collection_manager.rs::resolve_card_and_version` and `card_store.rs::select_printing`. Those five
are the contract, and `tests/test_lotrlcg_audit_coverage.py` ports the Rust's own test cases so a
change on that side that the script has not followed fails here.

The identity grouping is why coverage cannot be counted per printing. Hall of Beorn lists 5327
printings of 4180 cards; the other 1147 are reprints that collapse onto an earlier printing's slug.
Counting a reprint with no image of its own as missing overstates the gap several-fold.

## Running it

Requires [`uv`](https://docs.astral.sh/uv/).

```bash
uv run rename.py ~/Downloads/lotrlcg --dry-run              # preview
uv run rename.py ~/Downloads/lotrlcg -o ~/Pictures/lotr     # apply

uv run rename_nightmare.py "~/Downloads/LOTR LCG Nightmare Cards - Remastered" --dry-run
uv run rename_nightmare.py "~/Downloads/LOTR LCG Nightmare Cards - Remastered" -o ~/Pictures/lotr

uv run rename_alep.py ~/Downloads/GenericPNG --dry-run
uv run rename_alep.py ~/Downloads/GenericPNG -o ~/Pictures/lotr

uv run rename_ffg_pdfs.py "~/Downloads/lotrlcg ffg pdf" --dry-run
uv run rename_ffg_pdfs.py "~/Downloads/lotrlcg ffg pdf" -o ~/Pictures/lotr
```

The first argument is the source folder, `-o` is where the output folder is created, defaulting to
the current directory.

Dry-run first and read the `[SKIP]` lines. An unexpected skip usually means a filename the matcher
doesn't understand rather than a card with no scan. `rename_nightmare.py` reports no skips at all
against an intact archive.

The Hall of Beorn catalog is cached next to the script as `lotrlcg_catalog_cache.json`, downloaded
on first run from the `PlayerCards`, `EncounterCards` and `QuestCards` exports, and shared by
`rename.py`, `rename_nightmare.py` and `rename_ffg_pdfs.py`. Delete it to pick up catalog changes. `rename_alep.py` has no
cache and fetches `hallofbeorn.com/Export/ALeP` and `ringsdb.com/api/public/cards/` on every run.

All four scripts re-encode to JPEG at quality 90.

## How it maps

**Card ids** come from the Hall of Beorn `Slug` field, transliterated to ASCII, lowercased, with
every non-alphanumeric character replaced by `_`. The slug already carries a pack disambiguator
(`Lost-Soul-of-Lorien-TDMN`), which is what keeps reprints of the same title apart.

**Packs** come from the folder the scan sits in, resolved through a cascade: the folder name, then
the same name with `Nightmare` appended when `Nightmare` appears anywhere in the path, then the
parent folder, then the grandparent, then a last-resort Nightmare guess. This looks like defensive
dead code and is not — the archives nest cards at different depths, and each step covers a real
case. Folder names carry a leading index (`03 - Khazad-dûm`) which is stripped first.
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

A few entries are not spellings at all. A Shadow in the East names both `072 - Gollum (Hero).jpg`
and `073 - Gollum (Enemy).jpg` after Gollum, but 072 is the Sméagol hero; the table sends it to
Sméagol. Spelling those two "correctly" would point both scans at one card and lose the other.

**Read the `[SKIP]` lines against the catalog, not just for volume.** Three of these entries were
added after an audit found scans the matcher could not place: `065 - 1A - The Leading Fish.jpg`
(Heirs of Númenor prints "Leaping") and the two Gollum files above. Four faces, two card sets
completed. Of the 128 distinct skipped names, 47 are `_1` copies of a card already written and 63
resemble no card in their pack, so the signal is thin but it is there.

**`rename_nightmare.py` needs no typo tables; it matches on position.** Each scenario folder's
`Card list.txt` lists every card as `Qty | Type | Front | Back`, and the leading number is the one
printed on the card, which matches Hall of Beorn's `Number`. Position therefore resolves the cards
Hall of Beorn stores misspelled — `Writing Tentacle`, `Gobline Trapper`, `Swarming Mosquitos`,
`Lost Soul of L≤rien`, `Rob and Bob` — along with the Nightmare Mode cards, whose catalog titles
carry a ` Nightmare` suffix the manifest omits.

Title overrides position only when the two resolve to different cards *and* the title is unique
within its pack. That is what puts Intruders in Chetwood #5 and #6 in the printed order: Hall of
Beorn has the two transposed, and the printed cards read 5 = Outskirts of Archet, 6 = Greenway Path.
A title that collides inside its pack is dropped from the lookup, so Flight from Moria's three cards
titled "Search for an Exit" stay on position. A title override that displaces a matching position
logs an `[INFO]` line; where position and title resolve differently but the title is ambiguous or
absent, position is kept and nothing is logged.

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

**`rename_ffg_pdfs.py` resolves on the collector number**, like the Nightmare script and for the
same reason: the number is printed on the card, and the slicer reads it off the page. Nothing is
inferred from a filename or a title. The card set comes from `PACKS`, keyed by PDF, and
`mec104-mec105_replacement_cards.pdf` maps its collector numbers instead of naming one set, because
it reprints five cards across two Starter Decks and its pages are not grouped by set.

**Hall of Beorn's Ered Mithrin numbering runs one ahead of the cards.** The card printed 164 is
Journey Up the Anduin, which the catalog has at 165, and the offset holds across the whole set.
`NUMBER_OFFSETS` carries it, and is applied to the printed number to reach the catalog's.

**Every match is scored against the page text.** The share of a catalog card's wording — title,
subtitle, rules text, traits, keywords — that appears on the page runs from 0.84 to 1.00 when the
card is right. A card below `MATCH_FLOOR` is logged, but no per-card floor separates right from
wrong on its own: two similar cards in one set share enough wording that a card matched to its
neighbour still scores up to 0.94. The median over a whole PDF does separate them — 0.97 or better
when the set lines up, 0.43 or worse when it is off by one — so that median is the real guard, and
it is what would catch a `NUMBER_OFFSETS` entry going stale.

**A number carrying two catalog slugs is one physical card, and the two slugs are handled
differently depending on whether they name the same card.** Which slug is which face is settled by
scoring both against both pages and giving each the page it fits better, rather than by reading
`Upgraded` out of the slug: two of the pairs are Sea Serpent Menacing/Enraged and Hard/Expert Mode,
where neither slug says which side it is. A pair that scores identically on both pages is skipped
rather than guessed at. Then:

- **Same title** — an upgradable card. FFG prints the Upgraded side on the back of the base side,
  so it is one card with two faces, and only the base slug is written, with the back file as its
  `~back`. Writing the second slug too would file the same two images under a second id and print
  the card twice. This matches what `rename.py` does for the flip cards in The Hunt for the
  Dreadnaught, and covers nineteen Dream-chaser ships and upgrades.
- **Different titles** — two distinct cards sharing one piece of cardboard, so each is written in
  its own right and backed by the other, because a decklist naming either has to resolve. This is
  the treatment `rename.py` gives Na'asiyah and Captain Sahír, and it covers Angmar's Protect the
  Innocent / Arnor Ravaged at 158a and 158b, and Dream-chaser's Hard Mode / Expert Mode.

**A number carrying one slug may still have a back**, where Hall of Beorn folds an upgraded face
into the card rather than listing it separately. It is written as that card's `~back`.

## Tests

```bash
uv run --with pytest --with unidecode --with pillow --with pymupdf --no-project \
  pytest utils/image_file_renamers/lotrlcg/tests/ -v
```

Covers name normalization, filename parsing, back-face detection, the pack-scoping property of
`PACK_TITLE_FIXES`, per-folder orphaned-back validation, the ALeP title resolver, for the Nightmare
script: manifest parsing, card-reference splitting, copy-suffix and scan-key handling, the
position-versus-title resolution rules, pack resolution, and the bleed trim landing on the MPC
frame, and for the FFG PDF script: the wording overlap, the catalog lookup's handling of a repeated
slug and of two slugs on one number, card set resolution for both single-set and multi-set PDFs,
face assignment including the tie it refuses to guess at, and the consistency of the declared
tables. No file I/O, network calls or image fixtures, and nothing depends on the catalog cache, so
the suite passes on a fresh clone.

## Known limitations

- **One card in the whole game has no image: The Grey Havens #84 Navigation.** It is in none of the
  three scan archives — the pack's only skipped filenames are its eight `Heading` section dividers —
  and no other printing shares its identity. Everything else either has its own image or prints
  from another printing of the same card. Getting there took `rename_ffg_pdfs.py` for FFG's revised
  line, three `PACK_TITLE_FIXES` entries for scans the matcher could not place, and the seven
  hand-filled cards above. Hall of Beorn publishes an image URL for it, so a `download_missing.py`
  in the style of the AGOT one would fetch it, but only at 423x600. The Nightmare line is complete:
  637 printings across all 72 packs. Run `audit_coverage.py` for the current position rather than
  trusting this paragraph.
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
- **Four Dream-chaser cards are not in the Hall of Beorn card set.** The PDF prints The Havens
  Burn (23), the Dream-chaser location (24), the Stormcaller ship-enemy (121) and The Shattered
  Monument (129), each with an upgraded back. The Dream-chaser Campaign Expansion export has no
  entry at those numbers, so `rename_ffg_pdfs.py` logs a `[SKIP]` and writes nothing for them. The
  images are in the slicer's output; only the catalog entry is missing.
- **`ENCODING_FIXES` repairs mangled non-ASCII in ALeP filenames** (`Th_odwyn` → `Theodwyn`,
  `Nazg l` → `Nazgul`) as a fallback when the live catalog has no match. One table serves both the
  front and back paths, and has to stay that way: a card's two faces must resolve to the same target
  id, so per-path copies can drift apart and break the `~back` pairing.
- **The typo tables are hand-maintained.** They encode specific mismatches between `rename.py`'s
  filenames and this catalog. Pointing it at a differently-named archive will need new entries, and a
  plausible-looking tidy-up can silently break a working match.
