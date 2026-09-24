# Marvel Champions Image File Renamer

Renames Marvel Champions proxy images to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against [MarvelCDB](https://marvelcdb.com/). Card ids are MarvelCDB codes and pack ids are
MarvelCDB pack codes, which is what the `marvel-champions` adapter reads.

One source, one collection: `marvel-champions-oop`, the cards Fantasy Flight lists as out of
print.

## The source

A private Google Drive, shared on Discord by the person who made it:
`https://drive.google.com/drive/folders/1zG_uVhdVLqWuiPSwKvvQgCp3Ze7-YlsU`. Not public, so it
cannot be fetched without their link. 12GB, a folder per product.

One PNG per card face, 1590x2200, already named by MarvelCDB code:

```
Wasp/13001a_front.png                                   a card's two faces
Wasp/13001a_back.png
_Alternate Art/08013_front Stealth Strike (Gambit).png  an alt art, and the product it came in
_Shared Backs/Hero Back.png                             the generic backs
```

Every image already carries a bleed border, stretched outward from the art rather than mirrored,
by `oriented_bleed.py`, which the drive carries alongside the images. So the images are named
`.bleed` and Proxy Nexus builds no bleed of its own. `card-library.json` records what the drive
holds; nothing here reads it, since the filenames carry the codes.

## Running it

Requires [`uv`](https://docs.astral.sh/uv/). Run from this folder.

```bash
uv run rename_proxies.py "~/Downloads/Marvel Champions Proxies" --dry-run     # preview
uv run rename_proxies.py "~/Downloads/Marvel Champions Proxies" -o mc_proxies
uv run fix_orientation.py mc_proxies \
    -o ~/Pictures/proxynexus_collections/marvel-champions/marvel-champions-oop
```

The intermediate folder is ~2GB and can be deleted afterwards. The MarvelCDB catalog is downloaded
on first run and cached as `marvel_champions_catalog_cache.json`; `--refresh-catalog` re-fetches
it.

`--packs` defaults to the out-of-print packs (`OUT_OF_PRINT_PACKS`); `--packs all` takes every pack
the drive holds, and a comma-separated list takes those.

## How it maps

There is no matching to do: the code names the card, MarvelCDB gives its pack, and `_back` is the
card's second face, which for a double-sided card is the face MarvelCDB hides behind the code that
links to it.

**Alternate arts** are written as a printing of their own, named after the product they came in:
`08013_front Stealth Strike (Gambit).png` becomes `08013@alt_gambit.bleed.jpg`. The `alt_` keeps
them clear of the pack codes, several of which a product shares a name with. 32 of the drive's 56
are alt arts of out-of-print cards; the rest belong to packs still in print and are dropped with
them.

**A card the drive has only one side of** is kept and reported: `04160a`, `04161a` and `04162a`,
the Hydra Campaign's basic upgrades, and `26002` Vision's Intangible have a front and no back, so
they print with the generic back. A back with no front would be left out instead, since a
collection will not import one.

**The images are saved as JPEG at quality 92**, which takes the collection from 21GB to 2.2GB. At
that quality the sample measured 42-43dB against the PNG, and quality 95 would cost 37% more disk
for 1.5dB. `--quality` overrides it.

**[fix_orientation.py](fix_orientation.py)** turns the landscape cards (main schemes, side schemes
and player side schemes) so they all face the same way. They are stored portrait, a quarter turn
from how they are printed, and the drive turns them the other way from the convention: 337 of its
376 need a half turn. They end up read by turning the card clockwise, which is how every landscape
card in a Proxy Nexus collection is stored; `--ccw` does the opposite.

MarvelCDB has no picture of many of these faces, so the images are compared against each other
rather than against the database. Every one of these cards has a white title band on its top edge,
which a quarter turn puts down one long side. The script builds a template of the images'
brightness across their width from the images themselves, and checks each against it and its
mirror. Portrait cards are all stored upright and are left alone.

**`_Shared Backs/`** holds the generic backs, in a standard and a Classic set. They are not part of
a collection: the standard three are the adapter's, in
`proxynexus-core/src/games/marvel_champions/backs/` as `{group}_proxy.bleed.jpg`, re-encoded as
JPEG because a 12MB PNG each is a lot to embed in the binary.

## Coverage

| | |
|---|---|
| Images | 2384 |
| Of the 2583 out-of-print images | 2351 |
| Alternate arts | 32, as printings of their own |
| Size | 2.2GB |
| Bleed | in the image |

Plus one face beyond the catalog, the back of Wasp's `13001c` giant form, which MarvelCDB does not
list. No card is missing its front.

The 38 out-of-print sets include The Wrecking Crew, which Fantasy Flight marks out of print. The
Once and Future Kang is not among them: it left Asmodee's distribution catalogue in September
2026, which is usually the first step, but Fantasy Flight has not marked it, so the drive's 58
faces for it are skipped.

**What is missing.** 232 of the 2583, and 227 of those are reprints: MarvelCDB gives a reprint its
own code, and the drive draws the card under the code it reprints instead. The adapter gives a
reprint its original's title, so Proxy Nexus offers the original's image for them anyway. The
other 5 are cards the drive does not have.

## Reporting what is where

**[report_coverage.py](report_coverage.py)** reads the finished collection and writes what it
holds and what it does not, for sharing with whoever made the source:

```bash
uv run report_coverage.py ~/Pictures/proxynexus_collections/marvel-champions/marvel-champions-oop \
    -o ~/Downloads
```

It writes `marvel-champions-collections.md`, a summary with a row per out-of-print set and the
sets left out for still being in print; `marvel-champions-collection-differences.csv`, the same
per-set counts; and `<collection>-missing.csv`, a row per card face the collection lacks.

## Tests

```bash
uv run --with pytest --with Pillow --with numpy --no-project \
    pytest utils/image_file_renamers/marvel_champions/tests/ -v
```

Covers the output names, alternate arts and what is skipped for `rename_proxies.py`; which way
round an image is, against synthetic profiles, for `fix_orientation.py`; and which faces are
counted for `report_coverage.py`. No network calls.
