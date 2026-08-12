# Netrunner Reboot Project Image Downloader

The [Netrunner Reboot Project](https://nrdb.reteki.fun/) is a rebalance of FFG-era Netrunner. It runs
its own fork of NetrunnerDB, which hosts both the catalog and 1720x2400 images of every card, and
a jnet clone that contains an additional 164 alternate arts.

Two scripts:

```
download.py   fetches every card image, named by NRDB code   01001.jpg
rename.py     renames those to the Proxy Nexus convention    noise__hacker_extraordinaire@core.jpg
```

## Getting the images

`download.py` reads the catalog from [reteki's NRDB API](https://nrdb.reteki.fun/api/doc) and pulls one image per card, 
plus the alt arts.

```bash
uv run download.py ~/Downloads/netrunner-reboot-raw --dry-run   # preview
uv run download.py ~/Downloads/netrunner-reboot-raw
```

Images already fetched are skipped.

Every file is named for the format actually served rather than the URL's extension, because the alt
arts come back as JPEG from a `.png` path.

## Running the rename

```bash
uv run rename.py ~/Downloads/netrunner-reboot-raw --dry-run                    # preview
uv run rename.py ~/Downloads/netrunner-reboot-raw -o ~/Downloads/netrunner-reboot
```

Images are copied into the output folder; the raw folder is never modified. Dry-run first and read
the `[SKIP]` lines — a skip means a filename the script doesn't recognise or a code the catalog
doesn't have, not a card that legitimately has no image.

Both scripts share one catalog, cached next to them as `netrunner_reboot_catalog_cache.json` and
downloaded on first run. Delete it to pick up catalog changes.

## How it maps

Reteki runs the NetrunnerDB v2 API, which has no slug field, so ids are derived from the card title
by `normalize_title`, mirroring `normalize_title` in `proxynexus-core/src/card_store.rs`. The pack
is the card's `pack_code`.

```
01001  Noise: Hacker Extraordinaire   core   ->  noise__hacker_extraordinaire@core.jpg
```

No title in the catalog normalizes to the same id as another, so nothing needs disambiguating by
position or label.

No `.bleed` suffix. The images are the card face alone with no bleed border, so Proxy Nexus
generates one at MPC time.

## Alt arts

164 cards have an alternate art. `nrdb.reteki.fun`'s API has no record of them — only the browser
game's [card data](https://reteki.fun/data/cards) does, under an `alt_art` map, and the images are
served from `media.reteki.fun` rather than from the database:

```
{"code": "01001", "alt_art": {"alt": "01001-alt"}}

https://media.reteki.fun/img/cards/01001-alt.png  ->  noise__hacker_extraordinaire@alt.jpg
```

The map's key is the printing label and its value is the image's own name; both are read from the
data rather than derived from the card code, so a card with a second alt, or one whose label isn't
`alt`, needs no code change. In a card list the label picks the printing:

```
3x Sure Gamble [alt]
```

Alt arts land in the same output folder as everything else. A pack printing and an alt printing of
one card are different filenames, so they coexist in a single collection.

## Where the images come from

Three reteki sites are involved, and it matters which is which:

| site | what it is | used for |
|---|---|---|
| `nrdb.reteki.fun` | the card database, a NetrunnerDB fork | the catalog and every card image |
| `reteki.fun` | the browser game, a jinteki.net fork | the alt art list at `/data/cards` |
| `media.reteki.fun` | the image server the browser game loads from | the alt art images |

`media.reteki.fun` also serves the ordinary card images, and they are byte for byte identical to
`nrdb.reteki.fun`'s for **956 of the 972 cards**. The 16 that differ are exactly the multi-face
cards: `nrdb.reteki.fun` renders every face into one sheet, `media.reteki.fun` renders the front
alone.

Those two renders were made at different times, and neither site is uniformly fresher. Checked
against the rules text in the catalog:

| card | `nrdb.reteki.fun` | `media.reteki.fun` | catalog |
|---|---|---|---|
| Caterpillar (`51009`) | **STR 1** | STR 2 | "Front face (Caterpillar - STR 1" |
| Hype (`53024`) | gain 4 | **gain 5** | "draw 3 cards and gain 5[credit]" |
| Project Genesis (`54019`) | trash 2, half size | **trash 3** | `trash_cost: 3` |

So `nrdb.reteki.fun` is the source for cards — it is the only one with the back faces — and
`MEDIA_FRONTS` names the cards whose front is taken from `media.reteki.fun` instead, with the sheet
contributing only its remaining faces. The other 13 sheets differ between the two by nothing but
encoding, and two of them (SYNC, Jinteki Biotech) by a re-typeset that leaves every number
unchanged.

A front fetched this way is stored as `{code}-front` locally, because `media.reteki.fun` serves it
under the plain card code and it would otherwise overwrite `nrdb.reteki.fun`'s sheet.

## Flip cards

Reteki serves a card's faces as a single image with the faces laid out side by side — 3440x2400 for
a two-faced card, 3440x4800 for Jinteki Biotech's four forms. The
[naming convention](../../../README.md#image-file-naming-convention) wants one file per face, so
`rename.py` cuts sheets apart on the 1720x2400 grid:

```
09001.jpg  3440x2400  ->  sync__everything__everywhere@dad.jpg
                          sync__everything__everywhere@dad~back.jpg
08012.jpg  3440x4800  ->  jinteki_biotech__life_imagined@val.jpg
                          jinteki_biotech__life_imagined@val~face2.jpg  (and ~face3, ~face4)
```

Two faces are the front and back of one physical card, so the second is `~back`. Beyond two is an
identity printed in several forms, which has no front-and-back relationship to express, so those are
filed as `~face2` onwards.

Only an exact multiple of 1720x2400 counts as a sheet. Cutting is the one lossy step in the
pipeline — the faces are re-encoded with the source's own quantization tables and chroma
subsampling, and every other card is copied byte for byte.

15 of the 972 card images are sheets by measurement: one in Data and Destiny, one in The Valley,
and 13 in the Reboot-original packs, where flip cards are a recurring design. No alt art is one.

### Sheets that measure as ordinary cards

Project Genesis (`54019`) breaks the rule. Its four versions are **shrunk to fit one 1720x2400
frame** rather than tiled at full size, so nothing about the image or the catalog entry says it is
a sheet. It is listed by code in `SCALED_SHEETS`, and the cut divides the image into equal parts
rather than stepping through it in FACE_SIZE cells:

```
54019-front.jpg  1720x2400  ->  project_genesis@rb5.jpg          Project Genesis  1720x2400
54019.jpg        1720x2400  ->  project_genesis@rb5~face2.jpg    Acheron           860x1200
                                project_genesis@rb5~face3.jpg    Cocytus
                                project_genesis@rb5~face4.jpg    Phlegethon
```

Only the front escapes the resolution loss, because `media.reteki.fun` has it as a whole card — see
[Where the images come from](#where-the-images-come-from). The three alternates come out at 860x1200,
half the resolution of every other card and the best available: no separate image for them exists on
any of the three sites. They are reported as `[WARN] below print size`.

It is the only card that does this. Every image in the collection was reviewed as contact sheets to
be sure, which is the only check that finds this shape; a seam detector does not separate it from
ordinary card art, and the catalog text is the only other tell ("secretly choose any one version").
Add to `SCALED_SHEETS` if another turns up.

The client data lists a card the NRDB API doesn't, `53031` "Consolidation" — it is the back face of
Iris Capital's flip card, which this pipeline already writes as
`iris_capital__trading_tomorrow@rb4~back.jpg`.

## Tests

```bash
uv run --with pytest --with pillow --no-project pytest utils/image_file_renamers/netrunner_reboot/ -v
```

Covers name normalization, sheet detection, face cutting order, part naming, alt art resolution and
format sniffing. No network calls and no image fixtures — the images the tests need are built in
memory.

## Known limitations

- **9 cards have no image.** The draft identities `00005`–`00013`. `download.py` lists them under
  `[404]` at the end of a run. They are draft-format only and outside the Reboot card pool.
- **One image is unusable for print.** Zenith Thoughtworks: Changing Minds (`00014`, draft) is
  served at 508x709 against everything else's 1720x2400. It is renamed like any other card and
  reported as `[WARN] below print size`.
- **Shrunk sheets can only be found by eye.** `SCALED_SHEETS` is a hand-maintained list, because a
  card-sized image holding several small faces is indistinguishable from a card by measurement. A
  new one would pass through as a single card and nothing would flag it.
- **`MEDIA_FRONTS` is a snapshot of which site is stale.** Both are live. If `nrdb.reteki.fun`
  re-renders Hype or Project Genesis, or `media.reteki.fun` catches up, the entries stop being an
  improvement and become a second opinion. Re-check them against the catalog's rules text before
  trusting a rebuild.
- **The alt art list comes from the browser game, not the database.** Nothing in the NRDB API
  mentions alt arts, so `download.py` reads a second source. If the client's data moves, alt arts
  silently stop being fetched while card images carry on working.
- **A retitled card changes its id.** Ids come from card titles, and Reboot retitles cards between
  releases. Renaming into a folder that already holds an older run leaves the old name behind as a
  duplicate rather than replacing it.
