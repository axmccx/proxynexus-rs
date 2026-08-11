# AGOT Image File Renamer

Renames A Game of Thrones LCG card scans to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against the [ThronesDB](https://thronesdb.com/api/) catalog.

Two source archives are supported, and the script picks a strategy per pack folder:

```
Official FFG   00_core/A Clash of Kings.jpg      numbered folder + card name
agot.cards     R_R_TIFF_ENG/08_Iron Mines.tif    pack-code folder + indexed name
```

Scans are copied into an output folder. The source is never modified. Both archives can be passed
to a single run.

Two supporting scripts cover the parts that aren't renaming: `download_missing.py` fills gaps from
ThronesDB, and `rotate_horizontal.py` turns landscape scans portrait.

## Getting the scans

### Official FFG cards

From
[this r/AgameofthronesLCG post](https://www.reddit.com/r/AgameofthronesLCG/comments/1gwqjan/card_scans_proxies_now_available/),
which links to a
[Google Drive folder](https://drive.google.com/drive/folders/1d-zC--0uJtkcejqYOrDI5HcZbKxvfT5Q?usp=sharing).

Download it as-is; the 45 pack folders are named `00_core` … `44_dote` and the renamer reads the
pack code from the suffix. `02_trtw/Syrio Forel.jpg` is the archive's only corrupt file. The
renamer skips it, so the card never appears in the output, and `download_missing.py` then fills it
along with the other missing cards.

### Community made cards

Twenty sets, 530 cards. Extract each into its own directory inside one parent folder, named after
the set's pack code — e.g. `NCbT_TIFF_ENG`, `WK_TIFF_ENG`. The renamer reads the prefix to tell
reprints apart.

The first eight are on the [Card Files](https://agot.cards/2021/08/19/card-files/) index under
*Printing Service → ENG*. That page stopped being updated in 2023; every set since links its own
files from an `announcing-<set>` post, and the downloads below are the **TIFF Files** ones.

| Released | Set | Folder | Cards | Download |
|---|---|---|---|---|
| 2020-10 | Redesigns | `R_R_TIFF_ENG` | 60 | [TIFF](https://drive.google.com/file/d/1MRwaz5HwONGEyTRAXKBJYBinlR7uqU__/view?usp=sharing) |
| 2020-12 | Forgotten Heroes | `FH_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1B2f4pJmgB26gUobhnrRIPT3s77GDeFE2/view?usp=sharing) |
| 2021-02 | Jade Sea | `JS_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1bH2dn6pRouWbvp7LASAl6tkT_Y-LqJ7v/view) |
| 2021-06 | Hear My Words | `HMW_TIFF_ENG` | 60 | [TIFF](https://drive.google.com/file/d/1Mtpi80GFbXb6rc7C___xZsW2WOSgpqJE/view) |
| 2021-11 | For the Realm | `FTR_TIFF_ENG` | 30 | [TIFF](https://drive.google.com/file/d/1-MWuKj-O40t3A9wrdXlN03RmOyX82V6a/view?usp=sharing) |
| 2022-04 | Bran the Builder | `BTB_TIFF_ENG` | 30 | [TIFF](https://drive.google.com/file/d/1_rnP7yieVqEs3wAt-_CBg2aLIIJMXAGK/view?usp=sharing) |
| 2023-01 | As High as Honor | `AHAH_TIFF_ENG` | 40 | [TIFF](https://drive.google.com/file/d/10A83MALdfJsAQ0b5a5H5RaQj2C5fKgGp/view?usp=sharing) |
| 2023-06 | The Spoils of War | `TSOW_TIFF_ENG` | 30 | [TIFF](https://drive.google.com/file/d/17Pbayk69EJi4WrXQy2FkVc6xznTpZD33/view) |
| 2024-01 | A True Telling | `ATT_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1v0o0bwMPVHsBK-_LVKLOJDRAAHK9k71H/view) |
| 2024-03 | Children of Summer | `CoS_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1_H8RmhbBF605hXTyyyG_k3EK1nXJ3m59/view?usp=sharing) |
| 2024-05 | Fire Upon the Grass | `FUtG_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/11PX0iec90jhZaRJSn9WSpRaGsDXvDa-U/view?usp=sharing) |
| 2024-06 | Ten Thousand Ships | `TTS_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1Bfnk8cuaQgfiby1uvbIxtWBQMToPGo33/view?usp=sharing) |
| 2024-08 | The Iron Chronicle | `TIC_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1HlU_5LmkF4aLPletHRDnbLf1jUrV8gcb/view?usp=sharing) |
| 2024-10 | Winter's Kings | `WK_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1YfBJII0NHv3bAIFgnvv9eK9IKsN3sP4Q/view?usp=sharing) |
| 2025-05 | When All Is Darkest | `WAID_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1dwOoN8Hz1VsZAYI3L8RPx27xrC-QVzvX/view?usp=sharing) |
| 2025-07 | Whispers of War | `WoW_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/11Tqn_-VG-EpLYnbnlJVwLJSJKmO0r2RC/view?usp=sharing) |
| 2025-08 | Mountain and Vale | `MaV_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1puvqPakAsFyKaRyTMM8lFZcBv_pXak02/view?usp=sharing) |
| 2025-10 | Lord of the Waters | `LotW_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1z7GOsbrVczXRNfo7j33KcfDfArTq8-er/view?usp=sharing) |
| 2025-12 | Justice for Elia | `JfE_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1GicEtS_6Q9KVD6s9hbvilWPB1StJFYTz/view?usp=sharing) |
| 2026-02 | No Crown but Truth | `NCbT_TIFF_ENG` | 20 | [TIFF](https://drive.google.com/file/d/1wPdBqwz_XgtqwoNUlg369wvQSsLQqZbx/view?usp=sharing) |

Each set ships a `00_CARD-BACK.tif`, and the older ones a `NOTES_ENG.pdf`. Both are ignored.

## Running it

Requires [`uv`](https://docs.astral.sh/uv/).

```bash
uv run rename.py ~/"Downloads/A Game of Thrones LCG 2nd Edition" --dry-run   # preview
uv run rename.py ~/"Downloads/A Game of Thrones LCG 2nd Edition" -o agot-2nd
uv run rename.py ~/Downloads/agot_community_raw -o agot-community
```

Dry-run first and read the `[SKIP]` lines. An unexpected skip usually means a filename the matcher
doesn't understand rather than a card with no scan.

The ThronesDB catalog is cached next to the script as `agot_catalog_cache.json`, downloaded on
first run. Delete it to pick up catalog changes.

## Filling gaps from ThronesDB

The FFG archive is complete, so you can use `download_missing.py` to download those missing cards from Thronedb. 
It reads a renamed output folder, works out which catalog cards are absent, and downloads them under the same convention:

```bash
uv run download_missing.py agot-2nd --dry-run
uv run download_missing.py agot-2nd
```

Unfortunately the ThronesDB images are roughly 300×419, much lower quality than the 492×699 of the
FFG scans.

## Rotating the landscape scans

Plot cards are printed landscape and have to be stored portrait. Run `rotate_horizontal.py` on the
renamer's **output**, never a source archive, because it modifies files in place:

```bash
uv run rotate_horizontal.py agot-2nd --dry-run
uv run rotate_horizontal.py agot-2nd
```

JPEGs are re-encoded with the quantization tables read off the source, and rotating a TIFF is
lossless, so running this *before* the conversion below costs the pipeline exactly one lossy encode.

## Converting the community cards from TIFFs to JPEG

The community cards are TIFF, and the collection builder only accepts PNG and JPEG. The FFG archive is
already JPEG/PNG, so nothing to do for those.

```bash
cd agot-community
for img in *.tif; do
    magick "${img}[0]" -sampling-factor 1x1,1x1,1x1 -quality 95 "${img%.*}.jpg"
done
```

Sets from 2024 on ship two-page TIFFs, so the `[0]` is needed to keep only the first. The first is
the finished card at 822×1122; the second is a design layer at a different size and offset. Without
it you get pairs like `melisandre__ncbt_@NCbT-0.jpg` and `melisandre__ncbt_@NCbT-1.jpg`, which
would break the image file naming convention.

## How it maps

**Card ids** come from the ThronesDB `label` field rather than `name`, transliterated to ASCII,
lowercased, with every non-alphanumeric character replaced by `_`. `label` carries a pack
disambiguator for functionally different reprints — `Mace Tyrell (TSoW)` against `Mace Tyrell (R)` —
that `name` does not. This mirrors `normalize_title` in `proxynexus-core/src/card_store.rs`; if the
two drift, generated ids silently stop resolving at collection-build time.

**Matching** a filename to a card is separate and fuzzier. `clean_for_match()` strips punctuation,
underscores, case and a leading `"the"`, then applies a typo table. There are two tables, passed in
rather than reached for as globals: `FFG_TYPO_FIXES` covers the DotE pack's three misspellings
(`Rhaelgal`, `FlameMadeFlesh`, `MatthisRowan`) and `COMMUNITY_TYPO_FIXES` the seventeen
abbreviations in the community scans. Merging them would let a fix for one archive change how the other
resolves. Community filenames first go through `strip_community_filename()`, which drops a leading index
(`01_`) and a trailing language code (`_ENG`).

**Pack codes** come from the folder name — the suffix after the first `_` for FFG (`23_tak` →
`TAK`), the prefix before it for the community sets (`TSOW_TIFF_ENG` → `TSoW`). Both resolve
case-insensitively, because folders are upper-cased on disk while ThronesDB codes are mixed-case.
For the community sets the folder is only a hint used to disambiguate reprints; the pack ultimately comes
from the matched card.

**`.bleed`** is added when an image is 822×1122 — a 2.5×3.5in card plus a 1/8in bleed at 300 dpi,
which is what the community scans are. Compared orientation-independently so the landscape plots aren't
misread as trimmed. The FFG scans are 492×699 and get no suffix, so the image decides rather than a
hardcoded per-archive rule.

## Tests

```bash
uv run --with pytest --no-project pytest utils/image_file_renamers/agot/tests/ -v
```

Covers id normalization, both typo tables, archive dispatch, pack resolution, header parsing and
gap detection. Stdlib only — no file I/O beyond temp fixtures, no network, and nothing depends on
the catalog cache, so the suite passes on a fresh clone.

## Known limitations

- **The FFG archive is missing 18 cards** ThronesDB lists. Seventeen are the quoted-title *Song*
  cards — mostly events (`"The Bear and the Maiden Fair"`, `"Lord Renly's Ride"`) plus the
  `"The Rains of Castamere"` agenda. The eighteenth is Core's `Melisandre`; the GtR `Melisandre` is
  a different card and is present. `download_missing.py` fetches all of them at ThronesDB's lower
  resolution.
- **483 cards are in packs no archive covers.** Tower of Joy (281), Old and the New (120,
  unreleased), The Things We Do For Love (36), Valyrian Draft Set (21), Redesigns II (16) and three
  variant packs exist only as ThronesDB images. They are out of scope for `download_missing.py`,
  which only considers packs already present.
- **The catalog moves under you.** ThronesDB added an `R2` reprint set, which forced a pack suffix
  onto nine labels that previously had none — `goldengrove@FtR` became `goldengrove__ftr_@FtR`,
  `clever_feint@MoD` became `clever_feint__mod_@MoD`. The new names are correct for the current
  catalog. Expect more of this, and diff against a known-good collection rather than assuming either
  side is right.
- **The typo tables are hand-maintained** and the leading-`"the"` stripping is empirical, not
  principled. Both encode specific mismatches between these filenames and the catalog. Pointing this
  at a differently-named archive will need new entries, and a plausible-looking tidy-up can silently
  break a working match.
- **Collision detection warns, it does not prevent.** `shutil.copy2` still overwrites; the `[WARN]`
  line only means the overwrite is no longer silent.
- **`rotate_horizontal.py` has no undo.** It writes through a temp file so a failed encode cannot
  truncate the original, but a completed rotate is not reversible. Point it at the renamer's output.
- **Collections built before this tooling carry two defects a rebuild fixes.** In `agot-2nd` all 128
  plot cards sat at quality 75 / 4:2:0 against quality 100 everywhere else, from an earlier version
  of `rotate_horizontal.py` that saved with Pillow's defaults. In `agot-community` three conversion
  passes left 240 files at quality 90, 220 at quality 95 4:4:4 and 70 at quality 95 4:2:0. Neither
  is fixable in place; both go away when the steps above are run from the raw archives.
