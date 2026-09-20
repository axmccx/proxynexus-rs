# Marvel Champions Image File Renamer

Renames Marvel Champions card scans to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against [MarvelCDB](https://marvelcdb.com/). Card ids are MarvelCDB codes and pack ids are
MarvelCDB pack codes, which is what the `marvel-champions` adapter reads.

Two sources, one renamer each, because neither covers the game on its own:

```
rename.py           Marvel Champions/Heros/.../Hawkeye_Mockingbird_Ally_4.tiff   scans, every product
rename_proxies.py   Marvel Champions Proxies/Wasp/13001a_front.png               proxies, hero packs
```

`rename_proxies.py`'s source is the better of the two and its generic backs are the ones the
adapter ships, but it reaches under half the cards. `rename.py`'s covers the rest, and is run with
`--exclude` pointing at the proxy collection so the two never hold the same card. See
[Coverage](#coverage).

## The scan archive

A [Google Drive folder](https://drive.google.com/drive/folders/1FO7FRfJbqGsmAkfePhkzpmEqmW1-VwF2).
Download it as-is; the script walks the whole tree. Most files are ~580 dpi TIFFs cut to the card
with no bleed border, on a light scanner ground that shows outside the rounded corners.

| Folder | Pack |
|---|---|
| `Core Set/` | `core` |
| `Expansion Campaings/<box>/` | the campaign box, from `PRODUCT_DIRS` |
| `Scenario Packs/<pack>/` | the scenario pack, from `PRODUCT_DIRS` |
| `Heros/<alter ego>_<hero>/` | the pack the hero is printed in |
| `Modular Sets/<set>/` | the pack the encounter set is printed in |
| `Aspects/<aspect>/` | any pack; the card decides |

`Heros/` holds the heroes of the campaign boxes as well as the hero packs, so a folder resolves
through its hero rather than a table: to the pack named after the hero where there is one
(`SP!!dr` is the pack SP//dr), otherwise to the pack of the MarvelCDB hero card it names, with the
alter ego deciding between two heroes of one name (`Shuri_Black Panther` against the Core Set's).

`PROMO!!!/`, `Box Art/`, `Ronan The Accuser/` (pages of a PDF, not cards), card backs, reference
cards and decklists are reported and skipped.

### Running it

Requires [`uv`](https://docs.astral.sh/uv/). Run from this folder.

```bash
uv run rename.py "~/Downloads/Marvel Champions" --dry-run          # preview
uv run rename.py "~/Downloads/Marvel Champions" -o mc_renamed \
    --exclude ~/Pictures/proxynexus_collections/marvel-champions/marvel-champions-fears
uv run fix_orientation.py mc_renamed -o mc_faced
uv run ../../ahlcg_edge_trim/ahlcg_edge_trim.py mc_faced -o mc_trimmed
uv run fill_corners.py mc_trimmed \
    -o ~/Pictures/proxynexus_collections/marvel-champions/marvel-champions-dmitri
```

Each step writes a new folder and leaves its input alone. The intermediate folders are ~1GB each
and can be deleted afterwards.

`--packs` defaults to the packs Fantasy Flight lists as out of print (`OUT_OF_PRINT_PACKS`).
`--packs all` builds every pack the archive holds; a comma-separated list builds those.

`--exclude` takes a folder of already-named images and leaves out the cards in it, so the proxy
collection is built first and this one fills what it does not reach. It is repeatable, and reads
names whether or not they are `.bleed`. A card is only left out when that folder has every face of
it, since a front and its back have to end up in the same collection.

The MarvelCDB catalog is downloaded on first run and cached as
`marvel_champions_catalog_cache.json`; `--refresh-catalog` re-fetches it.

Dry-run first and read the report sections. `Packs not asked for` is the long one and is
expected. `Extra scanned copies of one card` is expected too: encounter sets are scanned once per
copy, and some cards are scanned in two folders. `Unmatched` and `Several cards fit the filename`
are the ones that want looking at.

## The proxy drive

A private Google Drive, shared by the person who made it:
`https://drive.google.com/drive/folders/1zG_uVhdVLqWuiPSwKvvQgCp3Ze7-YlsU`. Not public, so it
cannot be fetched without their link.

One PNG per card face, 1590x2200, already named by MarvelCDB code:

```
Wasp/13001a_front.png          Wasp/13001a_back.png          _Shared Backs/Hero Back.png
```

Every image already carries a bleed border, stretched outward from the art rather than mirrored,
by `oriented_bleed.py`, which the drive carries alongside the images. So the images are named
`.bleed` and Proxy Nexus builds no bleed of its own. `card-library.json` records what the drive
holds; nothing here reads it, since the filenames carry the codes.

```bash
uv run rename_proxies.py "~/Downloads/Marvel Champions Proxies" --dry-run
uv run rename_proxies.py "~/Downloads/Marvel Champions Proxies" -o mc_proxies
uv run fix_orientation.py mc_proxies \
    -o ~/Pictures/proxynexus_collections/marvel-champions/marvel-champions-fears
```

The drive stores its landscape cards the other way round from the scan archive -- title band on
the left, read by turning the card anticlockwise -- so 94 of its 95 need the same half turn, and
the collections would otherwise disagree with each other.

The PNGs are 9GB and are saved as JPEG at quality 92, which brings the collection to 1GB. At that
quality the sample measured 42-43dB against the PNG, and quality 95 would cost 37% more disk for
1.5dB. `--quality` overrides it.

There is no matching to do: the code names the card, MarvelCDB gives its pack, and `_back` is the
card's second face. `--packs` defaults to the out-of-print packs, the same list `rename.py` uses
and reads from it; `--packs all` takes the six still-in-print packs the drive also holds.

A card's two sides have to sit in the same collection, so a card the drive has only one side of is
left out and the scan archive supplies both. Four cards are: `04160a`, `04161a` and `04162a`, the
Hydra Campaign's basic upgrades, and `26002` Vision's Intangible, all of which the drive scanned
front-only.

`_Shared Backs/` holds the three generic backs, brighter than the scans the adapter shipped with
and bled to the same 1590x2200. They are the adapter's backs now, in
`proxynexus-core/src/games/marvel_champions/backs/` as `{group}_proxy.bleed.jpg`, re-encoded as
JPEG because a 12MB PNG each is a lot to embed in the binary.

## How the scan archive maps

**The title** is found by looking up every run of the filename's fields against the titles of the
pack, because the title is not in a fixed field: it is the second in
`Captain America_Shield Toss_Event_6` and the first in `Shield Toss_Event`. Where two runs are
titles, the one ending last wins, since a hero or encounter set name is often a card too and is
written first. Misspelt filenames fall through to a fuzzy match over every run, preferring titles
of the type the filename gives (`Protection_Defensive Stage_Upgrade` is Defensive Stance, not the
ally Protector). A title too misspelt for that is placed by its type and number where only one
card fits (`7_Rocky Outpost_Environment`).

**Cards sharing a title** are separated by what else the path and filename say, in this order:
the encounter set a folder or field names (`Brawler_Basic_Coup de Grace_Upgrade`), a pack a field
names (`Leadership_Teamwork_Event_Thor`), `Hero` or `Alter-Ego`, a face letter on a number
(`190a`), the type, the aspect, the stage (`I`, `Stage II`, `1A`, `A1`), the number (the collector
number, or the place in the encounter set), and last the side.

**Sides.** MarvelCDB lists a double-sided card as two linked codes and hides the back, so a scan
of the hidden code is written as the `~back` of the code linking to it. The archive writes sides as
`_Side A`, `_A`, `Side 1B`, `1B` or a letter on the number, and the side it writes outranks the
title it gives: `1B_Basic_Basic Attack Upgrade` is the back, titled Improved Attack Upgrade.

- A face letter is MarvelCDB's code suffix, and outranks a type copied from the other face:
  `Team Assembled_Environment_190a` is 40190a, the player side scheme Assemble the Team.
- Except on identity cards, where the archive letters the faces its own way:
  `Rocket Raccoon_Alter-Ego_29a` is the alter ego, which MarvelCDB codes 16029b.
- Where one face of a pair is marked and the other is not (`4_The Art of Evasion_Main Scheme` beside
  `..._Main Scheme_1A`), or both are marked the same side under different types
  (`7_Upper Manhattan_Environment_Side B` and `..._Side Scheme_Side B`), the second goes on the
  side the first left free. These are reported.

**Shared scans.** `Sinister Experiment_Main Scheme_140a-142a` and
`Attack on Xavier_s_Main Scheme_2A_BAck for cards 126 to 129` are one printed face shared by a run
of cards, and are written once per card in the run.

**MarvelCDB's duplicate main schemes.** Most two-sided main schemes are listed three times: as the
linked pair `01097a`/`01097b`, and again as a plain `01097`. Scans go to the pair, and the
adapter leaves the plain code out of the catalog.

## Finishing the scans

The proxy drive's images need only the first of these; the other two run on `rename.py`'s output.

**[fix_orientation.py](fix_orientation.py)** turns the landscape cards (main schemes, side schemes
and player side schemes) so they all face the same way, in either collection. They are stored
portrait, a quarter turn from how they are printed, and the archive turns them either way: 237 of
its 275 needed a half turn, against 94 of the proxy drive's 95. They end up read by turning the
card clockwise, which is how every landscape card in a Proxy Nexus collection is stored; `--ccw`
does the opposite.

MarvelCDB has no picture of many of these faces, so the scans are compared against each other
rather than against the database. Every one of these cards has a white title band on its top edge,
which a quarter turn puts down one long side. The script builds a template of the scans'
brightness across their width from the scans themselves, and checks each scan against it and its
mirror. On the archive's full 2348-image build it agreed with Tesseract's orientation detection on
all 264 scans Tesseract was confident about but one, `40093` The Senator's Support, where
Tesseract was wrong. Tesseract is not used, because it was unsure about the other 106. Portrait
cards are all stored upright and are left alone.

**[ahlcg_edge_trim.py](../../ahlcg_edge_trim/README.md)**, a shared utility, removes a strip of
scanner ground running along a straight edge. Here it is the card's pale cut edge, 1-4px on 176 of
the 1257 scans, which the bleed would otherwise be built from. It runs before the corners are
filled, because it can move the edge the corners are measured from.

**[fill_corners.py](fill_corners.py)** fills the scanner ground outside each rounded corner, which
Proxy Nexus would otherwise repeat into the bleed.
[corner_infill_arc.py](../../corner_infill/README.md) was tried first and leaves most of these
corners showing: the ground here ranges from light grey to brown, darkens into a shadow at the
card's edge, and is close in colour to the grey dotted border many cards carry. So this one masks
by shape: everything outside a circle of 85px touching both edges, a little more than the card's
own 3mm corner at the archive's ~580 dpi, is inpainted from the card beside it. Where art runs
into a corner, the wedge filled is the part a rounded corner cuts off when printed.

## Coverage

Two collections, from the two sources, with no card in both:

| | `marvel-champions-fears` | `marvel-champions-dmitri` |
|---|---|---|
| Source | the proxy drive | the scan archive |
| Built | first | second, with `--exclude` |
| Images | 1107 | 1261 |
| Size | 1004MB | 991MB |
| Bleed | in the image | built by Proxy Nexus |
| Of the 2523 out-of-print images | 1106 | 1261 |

Together they hold 2367 of the 2523, and one face beyond the catalog: the back of Wasp's `13001c`
giant form, which MarvelCDB does not list. No card is in both, and neither collection holds a card
that is missing its second face.

The proxy drive holds almost nothing of the campaign boxes other than The Rise of Red Skull
(165 of 182) and The Green Goblin (57 of 57): 4 images of The Mad Titan's Shadow, 6 of Mutant
Genesis, 2 each of Sinister Motives and NeXt Evolution, none of The Galaxy's Most Wanted, The Hood
or MoJo Mania. It also holds 184 images of six packs still in print, which neither collection
takes.

**What is missing.** 156 of the 2523, with the proxy drive filling what it can:

- 154 are reprints whose scan is filed under another printing in the archive, mostly basic and
  aspect cards a hero pack reprints (`21017` Moxie in The Mad Titan's Shadow is scanned under
  Ant-Man), and which the proxy drive does not carry either.
- 2 the archive does not hold at all, `09027` Physical Toll and `35027` Past Demons, come from the
  proxy drive, as do the 19 whose original printing only the drive scanned (`03032` Followed).

The adapter gives a reprint its original's title, so with a collection holding the other
printing's scan loaded, Proxy Nexus offers that scan for these cards too.

## Reporting what is where

**[report_coverage.py](report_coverage.py)** reads the finished collections and writes what each
holds and what neither does, for sharing with whoever made a source:

```bash
uv run report_coverage.py ~/Pictures/proxynexus_collections/marvel-champions/* -o ~/Downloads
```

It writes `marvel-champions-collections.md`, a summary with a row per out-of-print set and the
sets left out for still being in print; `marvel-champions-collection-differences.csv`, the same
per-set counts; and a `<collection>-missing.csv` per collection, a row per card face that
collection lacks, saying whether the other one has it.

## Tests

```bash
uv run --with pytest --with Pillow --with numpy --with opencv-python --no-project \
    pytest utils/image_file_renamers/marvel_champions/tests/ -v
```

Covers filename parsing, title matching, the order cards sharing a title are separated in, sides,
shared scans and hero folders for `rename.py`; which way round a scan is, against synthetic
profiles, for `fix_orientation.py`; the corner mask's shape for `fill_corners.py`; the output
names and what is skipped for `rename_proxies.py`; and which faces are counted for
`report_coverage.py`. No network calls.

## Known limitations

`Nadia Van Dyne_Wasp.tiff` writes no type or side and is reported rather than guessed.
