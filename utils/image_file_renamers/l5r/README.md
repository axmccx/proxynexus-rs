# L5R Image File Renamer

Renames Legend of the Five Rings card scans to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against the [EmeraldDB](https://www.emeralddb.org/) catalog.

Two source archives are supported, and the script picks a strategy per file:

```
Emerald Legacy   TTM5.jpg                                  set abbreviation + card number
FFG 600dpi       Phoenix_D_9_Kaito Temple Protector.tiff   fuzzy card-name match
```

Scans are copied into an output folder. The source is never modified. Both archives can be passed
to a single run.

## Getting the scans

### FFG cards (600 dpi scans)

From [this r/l5r post](https://www.reddit.com/r/l5r/comments/1abxjb8/legend_of_the_five_rings_600_dpi_scans/),
which links to a
[Google Drive folder](https://drive.google.com/drive/folders/1uQLUUx5_-zkGIUq3ftNXoiyjBnUjf9WR).

Download it as-is. The script walks the whole tree, so `Enhanced/`, `Promo Cards/` and the cycle
subfolders can stay exactly as they come. `Support Files/` holds print templates rather than cards
and is ignored automatically.

### Emerald Legacy cards

Sets are found at [emeraldlegacy.org/products](https://emeraldlegacy.org/products/).
Open a set's "More information" page and, under **Single cards**, download:

1. **Single cards, bleed cut** if it exists, otherwise
2. **Single cards, regular cut**

Bleed cut is preferred because Proxy Nexus will use an existing bleed border rather than generating
one, which is ideal for MPC. For PDFs, if a bleed is present, it is automatically cropped out. 

| Set | Product page | Download |
|---|---|---|
| Emerald Core Set | [emerald-core-set](https://emeraldlegacy.org/products/emerald-core-set/) | [Drive](https://drive.google.com/drive/folders/1FCg7PEt7GW6i81uBG7eYXtZ7-0XVmjU7?usp=sharing) |
| Ancient Secrets | [ancient-secrets](https://emeraldlegacy.org/products/ancient-secrets/) | [Drive](https://drive.google.com/drive/folders/1sGwyDjsyufG3HMZ231UCoogyGuEaHdfG) |
| Restoration of Balance | [restoration-of-balance](https://emeraldlegacy.org/products/restoration-of-balance/) | [Drive](https://drive.google.com/drive/folders/1H8ca9QJ81uwW-3J7m28BvIQgmiteY8Vg) |
| Through the Mists | [through-the-mists](https://emeraldlegacy.org/products/through-the-mists/) | [Drive](https://drive.google.com/drive/folders/1LQOzyXOR8Kb2HErn0NUHT0AMP9sfU7YH) |
| Under the Empress' Eyes | [under-the-empress-eyes](https://emeraldlegacy.org/products/under-the-empress-eyes/) | [Drive](https://drive.google.com/drive/folders/1vbBVMWtlHJTVfIDJwok6_dSAgpvJAbzn?usp=sharing) |

Under the Empress' Eyes has no bleed-cut download, so it is the one set taken at regular cut.

Extract all five into a single folder. The files are prefixed per set (`ecs001.jpg`, `ANS_1.jpg`,
`RoB01.jpg`, `TTM5.jpg`, `uee001.jpg`) so they don't collide. Any `archive/` folder in the download
holds superseded scans and is skipped automatically.

## Running it

Requires [`uv`](https://docs.astral.sh/uv/).

```bash
uv run rename.py ~/Downloads/emeraldlegacy ~/Downloads/l5r-600dpi --dry-run       # preview
uv run rename.py ~/Downloads/emeraldlegacy ~/Downloads/l5r-600dpi -o l5r_renamed  # apply
```

Pass both source folders in one run, or one at a time.

Dry-run first and read the `[SKIP]` lines. An unexpected skip usually means a filename the matcher
doesn't understand rather than a card with no scan.

The EmeraldDB catalog is cached next to the script as `l5r_catalog_cache.json`, downloaded on first
run.

## Converting the FFG scans to JPEG

The FFG archive is TIFF, and the collection builder only accepts PNG and JPEG. So after renaming,
convert the output folder:

```bash
cd l5r_renamed
for img in *.tif*; do
    magick "${img}[0]" -sampling-factor 2x2,1x1,1x1 -quality 95 "${img%.*}.jpg"
done
```

Some TIFF files produce two JPEGs when converted, so the `[0]` is needed to keep only the first.
The first is the full-detail scan, the second a flattened copy of the same card. Without it you get
pairs like `abandoning-honor@promo-0.jpg` and `abandoning-honor@promo-1.jpg`, which would break the 
image file naming convention.

The Emerald Legacy files are already JPEG, so nothing to do for those.

## How it maps

**Emerald Legacy** filenames are a set abbreviation plus the card number, which is the card's
position in the set. `PACK_ABBR` maps the abbreviation to an EmeraldDB pack id, then the position
gives the card.

Several cards share a name — "Agasha Sumiko" is printed twice under the ids `agasha-sumiko` and
`agasha-sumiko-2`. A name therefore maps to every card carrying it, and the position number in the
filename picks the exact printing. Where it can't, the oldest is used and the file is listed under
"Ambiguous printings".

**FFG** filenames put the card name last but vary what precedes it — `Phoenix_D_9_Kaito Temple
Protector`, `127_Neutral_D_SuddenTempest`, `Children of the Empire_Neutral_C_80_Stay Your Hand`.
The name is taken as everything after the last metadata marker (`C`, `D`, `PROVINCE`, `STRONGHOLD`
or a bare number), then fuzzy-matched against the catalog with punctuation stripped and a
hand-maintained `TYPO_FIXES` table applied.

Some FFG scans put **two different cards on one physical card** and name both files after the A
side, distinguished only by a side marker — `Side B` on the role cards, `B side` on the Shadowlands
warlords. These are separate cards, not front/back faces: side A of a role card is Keeper of Air and
side B is Seeker of Air; side A of a warlord is the cooperative version and side B the challenge
version. The B side is swapped to the matching card id rather than given a `~back` suffix.

Files under `Promo Cards/` are alt-art reprints of cards that also appear in their original packs,
so they get the custom printing label `promo` instead of a pack id.

`.bleed` is added when an image is exactly 816×1110 — MPC's minimum print size, and what Emerald
Legacy's bleed-cut downloads are. Regular-cut scans are smaller, so the image itself decides rather
than a hardcoded per-set list.

## Tests

```bash
uv run --with pytest --no-project pytest utils/image_file_renamers/l5r/tests/ -v
```

Covers name normalization, the typo table, filename splitting and pack resolution. No file I/O,
network calls or image fixtures.

## Known limitations

- **Promo positions can't disambiguate reprints.** Files under `Promo Cards/` carry the promo's own
  card number, not the position in the pack the card was originally printed in. When a name maps to
  several cards (`togashi-kazue` and `togashi-kazue-2` are both "Togashi Kazue"), that number
  matches neither, so the oldest printing is chosen and the file is listed under "Ambiguous
  printings". 1 file on the current archive.
- **The card name is trusted over the position.** Two promo filenames disagree with themselves:
  `The Core Set_Scorpion_C_185_Forged Edict` names a card that sits at position 184, and
  `The Core Set_Neutral_Province_41_Feast or Famine` names one that isn't in the Core Set at all.
  Checking the scans showed the *name* right and the position wrong in both cases, so the name wins.
- **One filename has no card name.** `Unicorn_Stronghold.tiff` is Shiro Shinjo and is mapped through
  `FILENAME_FIXES`. Every other `*_Stronghold_*` scan names its card properly.
- **Position matching is loose.** A printing's position is matched against *every* digit run in the
  filename, so a stray number anywhere can select the wrong printing. This was tuned against the
  real archives rather than designed.
- **Card backs and tokens are skipped.** Generic backs (`C back.tiff`, `P Back.tiff`) and unnumbered
  tokens (Emerald Legacy's `TTM51.jpg`, a generic Soldier follower) aren't catalog cards and have no
  id to map to. 7 such files in the FFG archive, 2 in Emerald Legacy.
- **TIFF passes straight through.** FFG scans are `.tif`/`.tiff` and keep that extension, since the
  script only renames and never re-encodes. The collection builder accepts only PNG and JPEG, so
  they need converting first — see [Converting the FFG scans to JPEG](#converting-the-ffg-scans-to-jpeg).
- **The typo table is hand-maintained.** It maps misspellings in the original scan filenames onto
  real card names. Pointing this at a differently-named archive will need new entries.
