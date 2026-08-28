# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow"]
# ///
"""
Renames Netrunner Reboot Project card images to the current Proxy Nexus naming convention.

    01001.jpg      ->  noise__hacker_extraordinaire@core.jpg
    01001-alt.jpg  ->  noise__hacker_extraordinaire@alt.jpg
    09001.jpg      ->  sync__everything__everywhere@dad.jpg + ...@dad~back.jpg

The input is a folder of images named by NRDB code, which is what download.py writes.
Copies into an output folder; the source is never modified.

Flip cards arrive as one image holding every face side by side, so those are cut apart
into a file per face. See README.md for the mapping rules and known limitations.

This script owns the catalog and the id normalization; download.py imports both from here.
"""

import os
import re
import json
import shutil
import argparse
import unicodedata
import urllib.request
from collections import namedtuple

from PIL import Image, JpegImagePlugin

API_BASE = 'https://nrdb.reteki.fun/api/2.0/public'

# The browser game's card data, the only place alt art printings are listed.
CLIENT_CARDS_URL = 'https://reteki.fun/data/cards'

ALT_IMAGE_URL = 'https://media.reteki.fun/img/cards/{stem}.png'

CATALOG_CACHE = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), 'netrunner_reboot_catalog_cache.json')

# Used when the catalog response carries no imageUrlTemplate.
FALLBACK_IMAGE_URL = 'https://nrdb.reteki.fun/card_image/large/{code}.jpg'

IMAGE_EXTS = ('.jpg', '.jpeg', '.png')

# Extension per image format. Names are written from the format read out of the
# file, not from the URL it came from: reteki serves the alt arts as JPEG under
# a .png URL, and a .png holding JPEG bytes is a filename that lies.
FORMAT_EXTENSIONS = {'JPEG': '.jpg', 'PNG': '.png'}

# What download.py writes: a card's 5-digit NRDB code, or an alt art's own name.
SOURCE_NAME = re.compile(r'^(?P<code>\d{5})(?:-(?P<variant>[A-Za-z0-9]+))?$')

# The size of a single card face on reteki. A card with more than one face is
# served as one image of these tiled in a grid, which is how a sheet is told
# apart from a plain card.
FACE_SIZE = (1720, 2400)

# Cards whose faces are shrunk to fit a single frame instead of tiled full size,
# as {code: (columns, rows)}. Project Genesis holds all four of its versions in
# one ordinary 1720x2400 image, so neither its size nor anything in the catalog
# marks it as a sheet and it has to be listed. Its faces come out at half the
# resolution of every other card, which rename.py reports.
SCALED_SHEETS = {'54019': (2, 2)}

# Cards whose front media.reteki.fun renders better than nrdb.reteki.fun's sheet
# does, so the sheet contributes only its remaining faces. The two sites serve
# identical bytes for every card that isn't a sheet; only these multi-face
# renders differ, and each was checked against the rules text in the catalog:
#
#   53024 Hype             nrdb.reteki.fun still reads "gain 4[credit]", now 5
#   54019 Project Genesis  nrdb.reteki.fun's is half size and reads trash 2, now 3
#
# nrdb.reteki.fun is the newer render for Caterpillar (51009), where
# media.reteki.fun still shows the pre-errata STR 2, so this is a list rather
# than a preference for one site.
MEDIA_FRONTS = {'53024', '54019'}

# Local name for a front fetched from media.reteki.fun, which serves it under
# the plain card code and would otherwise collide with nrdb.reteki.fun's sheet.
FRONT_VARIANT = 'front'

Catalog = namedtuple('Catalog', 'cards packs image_url_template alt_arts')

# NFKD handles accents; these are the Latin letters it leaves alone. Needed so
# normalize_title() keeps matching deunicode() in the Rust core.
_TRANSLITERATE = str.maketrans({
    'ø': 'o', 'Ø': 'O', 'æ': 'ae', 'Æ': 'AE', 'œ': 'oe', 'Œ': 'OE',
    'ð': 'd', 'Ð': 'D', 'þ': 'th', 'Þ': 'Th', 'ß': 'ss',
    'đ': 'd', 'Đ': 'D', 'ł': 'l', 'Ł': 'L',
})


def deunicode(text):
    """Transliterate to ASCII the way the Rust `deunicode` crate does."""
    decomposed = unicodedata.normalize('NFKD', text.translate(_TRANSLITERATE))
    return ''.join(c for c in decomposed if not unicodedata.combining(c))


def normalize_title(title):
    """Mirrors normalize_title in proxynexus-core/src/card_store.rs.

    Card ids come from this, so the two must stay in sync or the names written
    here won't resolve at collection-build time.
    """
    text = deunicode(title).lower()
    return "".join([c if c.isalnum() else "_" for c in text])


def fetch_json(url):
    print(f"Fetching: {url}")
    req = urllib.request.Request(url, headers={'User-Agent': 'ProxyNexus-ImageMigrator/1.0'})
    with urllib.request.urlopen(req) as response:
        return json.loads(response.read().decode('utf-8'))


def load_catalog():
    """Read the cached reteki catalog, downloading it first if absent."""
    if os.path.exists(CATALOG_CACHE):
        with open(CATALOG_CACHE, 'r', encoding='utf-8') as f:
            data = json.load(f)
        return Catalog(data.get("cards", []), data.get("packs", []),
                       data.get("image_url_template", FALLBACK_IMAGE_URL),
                       data.get("alt_arts", []))

    print("Catalog cache not found. Downloading catalog...")
    cards_response = fetch_json(f"{API_BASE}/cards")
    packs_response = fetch_json(f"{API_BASE}/packs")

    catalog = Catalog(
        cards_response.get("data", []),
        packs_response.get("data", []),
        image_url_template(cards_response),
        alt_arts(fetch_json(CLIENT_CARDS_URL)),
    )

    with open(CATALOG_CACHE, 'w', encoding='utf-8') as f:
        json.dump(catalog._asdict(), f)
    return catalog


def image_url_template(cards_response):
    """The response's own image URL pattern, upgraded to HTTPS.

    Taken from the response rather than hardcoded so a move of the image host is
    picked up by deleting the cache. reteki advertises the pattern over plain
    HTTP but serves the same path over HTTPS.
    """
    template = cards_response.get("imageUrlTemplate") or FALLBACK_IMAGE_URL
    if template.startswith("http://"):
        template = "https://" + template[len("http://"):]
    return template


def alt_arts(client_cards):
    """Alt art printings, as {stem, code, label}, from the browser game's data.

    The NRDB API has no record of these; only the client that draws them does.
    A card's `alt_art` maps a printing label to the image's own name. Both are
    taken from the data rather than derived from the card code, so a label that
    isn't "alt" still resolves.
    """
    arts = []
    for card in client_cards:
        for label, stem in (card.get('alt_art') or {}).items():
            arts.append({'stem': stem, 'code': card['code'], 'label': label})
    return sorted(arts, key=lambda art: art['stem'])


def cards_by_code(cards):
    return {card['code']: card for card in cards}


def alt_arts_by_stem(arts):
    return {art['stem']: art for art in arts}


def output_name(card, printing, part, ext):
    """The Proxy Nexus filename for one face of a card printing.

    No `.bleed`: reteki's faces are the card face alone, with no bleed border.
    """
    return f"{normalize_title(card['title'])}@{printing}{part}{ext}"


def face_grid(size, code=None):
    """(columns, rows) of card faces in an image; (1, 1) for an ordinary card.

    An exact multiple of FACE_SIZE is the only thing measurement treats as a
    sheet, so an image that is merely a different resolution stays a single
    face. Sheets that are shrunk to card size instead are named in
    SCALED_SHEETS, since nothing about them can be measured.
    """
    if code in SCALED_SHEETS:
        return SCALED_SHEETS[code]
    columns, rows = size[0] // FACE_SIZE[0], size[1] // FACE_SIZE[1]
    if (columns * FACE_SIZE[0], rows * FACE_SIZE[1]) == size and columns * rows > 1:
        return columns, rows
    return 1, 1


def sheet_contribution(code, cut, parts):
    """The faces a sheet contributes, dropping the front where media.reteki.fun
    supplies a better one."""
    if code in MEDIA_FRONTS:
        return cut[1:], parts[1:]
    return cut, parts


def part_names(face_count):
    """Part suffix per face, in reading order. Empty string is the front.

    A printing has one front and a back per physical card. Two faces is a flip
    card, so the second is that card's back. More than two is an identity
    printed in several forms, all sharing the one front, so each further face is
    the back of another physical card.
    """
    if face_count <= 1:
        return ['']
    return [''] + ['~back'] + [f'~back{n}' for n in range(2, face_count)]


def faces(img, grid):
    """Each card face in `img`, in reading order.

    Cutting into equal parts covers both layouts: a full-size sheet divides into
    cells of exactly FACE_SIZE, a shrunk one into cells of whatever fraction it
    used.
    """
    columns, rows = grid
    if (columns, rows) == (1, 1):
        return [img]
    width, height = img.size[0] // columns, img.size[1] // rows
    return [img.crop((x * width, y * height, (x + 1) * width, (y + 1) * height))
            for y in range(rows) for x in range(columns)]


def jpeg_encoder_settings(img):
    """Encoder settings that reproduce `img`'s own JPEG encoding.

    Cutting a sheet apart is the one lossy step in this pipeline; reusing the
    source's quantization tables keeps it from also dropping quality.
    """
    settings = {'format': 'JPEG', 'qtables': img.quantization}
    sampling = JpegImagePlugin.get_sampling(img)

    if sampling in (0, 1, 2):
        settings['subsampling'] = sampling
    for key in ('exif', 'icc_profile'):
        if key in img.info:
            settings[key] = img.info[key]
    return settings


def save_face(face, source, path):
    """Write one cut-out face, encoded the way `source` was."""
    settings = (jpeg_encoder_settings(source) if source.format == 'JPEG'
                else {'format': source.format})
    face.save(path, **settings)


def check_collision(seen_filenames, new_filename, source):
    """Register that `source` produced `new_filename` in this run.

    Returns the prior source if a *different* one already produced this same
    destination filename, else None. shutil.copy2 would overwrite silently.
    """
    prior = seen_filenames.get(new_filename)
    seen_filenames[new_filename] = source
    if prior is not None and prior != source:
        return prior
    return None


def image_kind(path):
    """(extension, (width, height)) read from the file's own header, else None.

    Also the guard against an unusable download: an empty or truncated file has
    no readable header and comes back None.
    """
    try:
        with Image.open(path) as img:
            extension = FORMAT_EXTENSIONS.get(img.format or "")
            return (extension, img.size) if extension else None
    except (OSError, ValueError):
        return None


def main():
    parser = argparse.ArgumentParser(
        description="Rename Netrunner Reboot card images to the Proxy Nexus convention.")
    parser.add_argument("source", help="Folder of code-named images (download.py's output)")
    parser.add_argument("-o", "--output", default="output", help="Output folder")
    parser.add_argument("--dry-run", action="store_true",
                        help="Print what would be written, write nothing")
    args = parser.parse_args()

    source = os.path.abspath(args.source)
    if not os.path.isdir(source):
        parser.error(f"not a folder: {source}")

    output = os.path.abspath(args.output)
    if not args.dry_run:
        os.makedirs(output, exist_ok=True)

    catalog = load_catalog()
    by_code = cards_by_code(catalog.cards)
    by_stem = alt_arts_by_stem(catalog.alt_arts)

    seen_filenames = {}
    written = skipped = undersized = 0

    for filename in sorted(os.listdir(source)):
        path = os.path.join(source, filename)
        stem, ext = os.path.splitext(filename)

        if not os.path.isfile(path) or ext.lower() not in IMAGE_EXTS:
            continue

        match = SOURCE_NAME.match(stem)
        if not match:
            print(f"[SKIP] {filename}: not a card code or alt art name")
            skipped += 1
            continue

        variant = match.group('variant')
        code, printing = match.group('code'), None

        if variant == FRONT_VARIANT and code in MEDIA_FRONTS:
            pass  # the card's own front, fetched from media.reteki.fun
        elif variant:
            art = by_stem.get(stem)
            if art is None:
                print(f"[SKIP] {filename}: no alt art with this name in the catalog")
                skipped += 1
                continue
            code, printing = art['code'], art['label']

        card = by_code.get(code)
        if card is None:
            print(f"[SKIP] {filename}: no card with code {code} in the catalog")
            skipped += 1
            continue
        printing = printing or card['pack_code']

        kind = image_kind(path)
        if kind is None:
            print(f"[SKIP] {filename}: unreadable image ({os.path.getsize(path)} bytes)")
            skipped += 1
            continue
        ext = kind[0]

        with Image.open(path) as source_img:
            if variant:
                # Alt arts and media-host fronts are a single card, never a sheet.
                cut, parts = [source_img], ['']
            else:
                cut = faces(source_img, face_grid(source_img.size, code))
                cut, parts = sheet_contribution(code, cut, part_names(len(cut)))

            for face, part in zip(cut, parts):
                new_filename = output_name(card, printing, part, ext)

                prior = check_collision(seen_filenames, new_filename, filename)
                if prior is not None:
                    print(f"[SKIP] {filename}: '{prior}' already produced {new_filename}")
                    skipped += 1
                    continue

                if face.size[0] < FACE_SIZE[0] or face.size[1] < FACE_SIZE[1]:
                    print(f"[WARN] {filename}: {face.size[0]}x{face.size[1]}, below the "
                          f"{FACE_SIZE[0]}x{FACE_SIZE[1]} every other card is served at")
                    undersized += 1

                if args.dry_run:
                    print(f"[DRY]  {filename} -> {new_filename}")
                elif face.size == source_img.size:
                    # A whole card copies byte for byte; only a cut face is
                    # re-encoded. The test is the face's size rather than a
                    # single-face sheet, because a sheet whose front comes from
                    # media.reteki.fun also contributes one face -- and copying
                    # there writes the whole strip out under the back's name.
                    shutil.copy2(path, os.path.join(output, new_filename))
                    print(f"[OK]   {filename} -> {new_filename}")
                else:
                    save_face(face, source_img, os.path.join(output, new_filename))
                    print(f"[OK]   {filename} -> {new_filename}   (cut)")
                written += 1

    verb = "would write" if args.dry_run else "wrote"
    print(f"\nSummary: {verb} {written}, skipped {skipped}, {undersized} below print size.")


if __name__ == "__main__":
    main()
