# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
Renames AGOT card scans to the current Proxy Nexus naming convention.

Handles both source archives, picking a strategy per pack directory:

  Official FFG   00_core/A Clash of Kings.jpg      -> a_clash_of_kings@Core.jpg
  agot.cards     R_R_TIFF_ENG/08_Iron Mines.tif    -> iron_mines__r_@R.bleed.tif

Copies into an output folder; the source is never modified.

See README.md for where to download the sources, the mapping rules and known
limitations.
"""

import os
import re
import json
import argparse
import unicodedata
import urllib.request
import shutil

CATALOG_CACHE = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'agot_catalog_cache.json')

IMAGE_EXTS = ('.tif', '.tiff', '.jpg', '.jpeg', '.png')

# FFG pack folders are numbered (`00_core`); agot.cards folders are not
# (`AHAH_TIFF_ENG`). This is what tells the two archives apart.
FFG_DIR = re.compile(r'^\d+_(?P<code>.+)$')

# Fan-made pack codes, used to filter the catalog down before building the
# community lookup.
COMMUNITY_PACKS = ["R", "FH", "JS", "HMW", "FtR", "BtB", "AHaH", "TSoW", "WoW",
                   "ATT", "ChoS", "FUtG", "JfE", "LotW", "MaV", "NCbT", "THBIB",
                   "TIC", "TTS", "WAID", "WK"]

# A superseded draft of the same cards as `R_R_TIFF_ENG`, under abbreviated
# filenames that mostly don't match the catalog.
SKIP_DIRS = {'redesigns_tiff_eng'}

# A 2.5x3.5in card plus a 1/8in bleed on every edge, at 300dpi.
BLEED_SIZE = (822, 1122)

# Language suffixes stripped from fan-pack filenames before matching.
LANGUAGE_SUFFIXES = ["_ENG", "_SPA", "_ITA", "_GER", "_CZE"]

# NFKD handles accents; these are the Latin letters it leaves alone. Needed so
# normalize_title() keeps matching deunicode() in the Rust core.
_TRANSLITERATE = str.maketrans({
    'ø': 'o', 'Ø': 'O', 'æ': 'ae', 'Æ': 'AE', 'œ': 'oe', 'Œ': 'OE',
    'ð': 'd', 'Ð': 'D', 'þ': 'th', 'Þ': 'Th', 'ß': 'ss',
    'đ': 'd', 'Đ': 'D', 'ł': 'l', 'Ł': 'L',
})

# Upstream name/label mismatches, one table per archive: merging them would let
# a fix for one change how the other resolves.
FFG_TYPO_FIXES = {
    "matthisrowan": "mathisrowan",
    "flamemadeflesh": "firemadeflesh",
    "rhaelgal": "rhaegal",
}

COMMUNITY_TYPO_FIXES = {
    "starm_s_end": "storm_s_end",
    "stannis_s": "stannis",
    "godry": "godrythegiantslayer",
    "rickard": "rickardkarstark",
    "vargo": "vargohoat",
    "bonifer": "boniferthegood",
    "lefthand": "lefthandlucascodd",
    "supportoftib": "supportfromtheironbank",
    "pointy": "pointyend",
    "haldon": "haldonhalfmaester",
    "reznak": "reznakmoreznak",
    "orton": "ortonmerryweather",
    "seremmom": "seremmoncuy",
    "serharys": "serharysswyft",
    "seraddam": "seraddammarbrand",
    "mellario": "mellarioofnorvos",
    "robarroyce": "serrobarroyce",
}


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


def clean_for_match(text, typo_fixes):
    """Deep cleaning for fuzzy matching (no spaces, no punctuation, no leading 'the')."""
    text = deunicode(text).lower()
    # Underscores stand in for apostrophes in a lot of scan filenames.
    text = text.replace("_", "")
    norm = "".join([c for c in text if c.isalnum()])
    if norm.startswith("the"):
        norm = norm[3:]
    return typo_fixes.get(norm, norm)


def fetch_json(url):
    print(f"Fetching: {url}")
    req = urllib.request.Request(url, headers={'User-Agent': 'ProxyNexus-ImageMigrator/1.0'})
    with urllib.request.urlopen(req) as response:
        return json.loads(response.read().decode('utf-8'))


def load_catalog():
    """Read the cached ThronesDB catalog, downloading it first if absent."""
    if os.path.exists(CATALOG_CACHE):
        with open(CATALOG_CACHE, 'r', encoding='utf-8') as f:
            data = json.load(f)
            return data.get("cards", []), data.get("packs", [])

    print("Catalog cache not found. Downloading catalog...")
    cards = fetch_json("https://thronesdb.com/api/public/cards/")
    packs = fetch_json("https://thronesdb.com/api/public/packs/")
    with open(CATALOG_CACHE, 'w', encoding='utf-8') as f:
        json.dump({"cards": cards, "packs": packs}, f)
    return cards, packs


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


def image_size(path):
    """(width, height) for a TIFF, JPEG or PNG, else None.

    Hand-rolled rather than using Pillow so the script stays dependency-free,
    and reads only the header rather than decoding the image.

    Also the guard against unusable scans: an empty or truncated file has no
    readable header and comes back None.
    """
    with open(path, 'rb') as f:
        head = f.read(8)

        if head[:8] == b'\x89PNG\r\n\x1a\n':
            f.seek(16)  # past the IHDR chunk length and type
            return int.from_bytes(f.read(4), 'big'), int.from_bytes(f.read(4), 'big')

        if head[:4] in (b'II\x2a\x00', b'MM\x00\x2a'):
            return _tiff_size(f, 'little' if head[:2] == b'II' else 'big', head)

        if head[:3] == b'\xff\xd8\xff':
            f.seek(2)
            return _jpeg_size(f)

    return None


def _tiff_size(f, order, head):
    """Read ImageWidth (tag 256) and ImageLength (tag 257) from a TIFF's first IFD."""
    f.seek(int.from_bytes(head[4:8], order))
    entry_count = int.from_bytes(f.read(2), order)
    width = height = None
    for _ in range(entry_count):
        entry = f.read(12)
        if len(entry) < 12:
            break
        tag = int.from_bytes(entry[0:2], order)
        if tag not in (256, 257):
            continue
        field_type = int.from_bytes(entry[2:4], order)
        # SHORT values occupy the first 2 bytes of the value field, LONG all 4.
        size = 2 if field_type == 3 else 4
        value = int.from_bytes(entry[8:8 + size], order)
        if tag == 256:
            width = value
        else:
            height = value
        if width is not None and height is not None:
            return width, height
    return None


def _jpeg_size(f):
    """Read (width, height) from a JPEG's SOF marker."""
    while True:
        byte = f.read(1)
        while byte and byte != b'\xff':
            byte = f.read(1)
        marker = f.read(1)
        while marker == b'\xff':
            marker = f.read(1)
        if not marker:
            return None
        # SOF0-SOF15 carry the frame dimensions; DHT/DAC/RSTn do not.
        if 0xC0 <= marker[0] <= 0xCF and marker[0] not in (0xC4, 0xC8, 0xCC):
            f.read(3)
            height = int.from_bytes(f.read(2), 'big')
            width = int.from_bytes(f.read(2), 'big')
            return width, height
        length = int.from_bytes(f.read(2), 'big')
        if length < 2:
            return None
        f.seek(length - 2, 1)


def has_bleed(size):
    """True if the scan carries a print bleed border.

    Orientation-independent, or landscape plot scans would read as un-bled.
    """
    return size is not None and sorted(size) == sorted(BLEED_SIZE)


def strip_community_filename(base_name):
    """Extract the card name part of a raw fan-pack filename stem: strips a
    leading index/underscore run and a trailing language code.
    e.g. "01_Gendry" -> "Gendry", "1_Bonifer_ENG" -> "Bonifer"
    """
    raw_name = base_name
    for lang in LANGUAGE_SUFFIXES:
        if raw_name.endswith(lang):
            raw_name = raw_name[:-len(lang)]

    raw_name = re.sub(r'^[\d_]+', '', raw_name).replace("_", " ").strip()
    return raw_name


def resolve_pack_guess(faction_dir, pack_code_map=None):
    """Map a fan-pack folder name to its ThronesDB pack code.

    Folder prefixes are upper-cased on disk (`TSOW_TIFF_ENG`) while ThronesDB
    codes are mixed-case (`TSoW`), so the lookup must be case-insensitive; a
    direct comparison silently files reprints under the wrong pack.

    `pack_code_map` maps a lowercased code to its canonical spelling.
    """
    pack_guess = faction_dir.split("_")[0]
    # Folders use the natural abbreviation for Children of Summer; ThronesDB
    # codes it ChoS because CoS belongs to the official pack City of Secrets.
    if pack_guess.lower() == "cos":
        pack_guess = "ChoS"
    if pack_code_map:
        return pack_code_map.get(pack_guess.lower(), pack_guess)
    return pack_guess


def classify_dir(name):
    """Which archive a pack directory belongs to: 'ffg' or 'community'."""
    return 'ffg' if FFG_DIR.match(name) else 'community'


def resolve_ffg(base_name, pack_lookup, pack_code):
    """Resolve an official FFG filename. Returns (card, pack_code, reason)."""
    card = pack_lookup.get(clean_for_match(base_name, FFG_TYPO_FIXES))

    # The DotE pack numbers its scans, e.g. '039.LysaArryn'.
    if not card and '.' in base_name:
        name_part = base_name.split('.', 1)[1]
        card = pack_lookup.get(clean_for_match(name_part, FFG_TYPO_FIXES))

    if not card:
        return None, None, f"Card '{base_name}' not found in pack {pack_code}"
    return card, pack_code, None


def resolve_community(base_name, faction_dir, lookup, pack_code_map):
    """Resolve an agot.cards filename. Returns (card, pack_code, reason)."""
    raw_name = strip_community_filename(base_name)
    matches = lookup.get(clean_for_match(raw_name, COMMUNITY_TYPO_FIXES))
    if not matches:
        return None, None, f"Card '{raw_name}' not found in community sets"

    pack_guess = resolve_pack_guess(faction_dir, pack_code_map)
    card = next((m for m in matches if m['pack_code'] == pack_guess), None)
    if card is None:
        card = matches[0]
        if len(matches) > 1:
            packs_seen = ", ".join(m['pack_code'] for m in matches)
            print(f"[WARN] {faction_dir}/{base_name}: '{raw_name}' is in {packs_seen} "
                  f"but the folder suggests {pack_guess}; using {card['pack_code']}")
    return card, card['pack_code'], None


def pack_directories(input_folder):
    """Yield (path, style) for each pack directory under `input_folder`.

    Falls back to treating `input_folder` itself as a pack directory, so you
    can point the renamer at a single pack.
    """
    subdirs = sorted(d for d in os.listdir(input_folder)
                     if os.path.isdir(os.path.join(input_folder, d))
                     and d.lower() not in SKIP_DIRS)
    if not subdirs:
        yield input_folder, classify_dir(os.path.basename(input_folder))
        return
    for name in subdirs:
        yield os.path.join(input_folder, name), classify_dir(name)


OUTPUT_NAME = re.compile(r'^(?P<id>.+)@(?P<pack>[^.@]+?)(?:\.bleed)?\.(?P<ext>[A-Za-z]+)$')


def format_summary(copied, skipped):
    return f"\nSummary: {copied} processed, {skipped} skipped."


def main():
    parser = argparse.ArgumentParser(
        description="Rename AGOT scans to the Proxy Nexus convention. Copies; never modifies the source.")
    parser.add_argument("inputs", nargs="+",
                        help="One or more folders of scans (official FFG and/or agot.cards)")
    parser.add_argument("-o", "--output", default="agot_renamed", help="Destination folder")
    parser.add_argument("--dry-run", action="store_true", help="Preview without copying")
    args = parser.parse_args()

    output_folder: str = os.path.abspath(args.output)

    cards, packs = load_catalog()
    pack_code_map = {p['code'].lower(): p['code'] for p in packs}

    # FFG resolves within a known pack; fan sets resolve by name across every
    # community pack, because the folder only hints at which one.
    by_pack = {}
    for c in cards:
        by_pack.setdefault(c['pack_code'], {})[clean_for_match(c['name'], FFG_TYPO_FIXES)] = c

    community = {}
    for c in cards:
        if c['pack_code'] in COMMUNITY_PACKS:
            community.setdefault(clean_for_match(c['name'], COMMUNITY_TYPO_FIXES), []).append(c)

    print(f"\n--- Scanning {'(DRY RUN) ' if args.dry_run else ''}---")
    if not args.dry_run:
        os.makedirs(output_folder, exist_ok=True)

    copied = 0
    skipped = 0
    missing_packs = set()
    seen_filenames = {}

    for raw_input in args.inputs:
        input_folder: str = os.path.abspath(raw_input)

        for pack_dir, style in pack_directories(input_folder):
            pack_name = os.path.basename(pack_dir)

            ffg_pack_code = None
            if style == 'ffg':
                match = FFG_DIR.match(pack_name)
                assert match, "classify_dir said 'ffg'"
                ffg_pack_code = pack_code_map.get(match.group('code').lower())
                if not ffg_pack_code:
                    if pack_name not in missing_packs:
                        print(f"[WARN] No ThronesDB pack code for folder: {pack_name}")
                        missing_packs.add(pack_name)
                    continue

            for root, dirs, files in os.walk(pack_dir):
                dirs[:] = [d for d in dirs if d.lower() not in SKIP_DIRS]
                faction_dir = os.path.basename(root)

                for filename in sorted(files):
                    if not filename.lower().endswith(IMAGE_EXTS):
                        continue

                    old_path = os.path.join(root, filename)
                    source_label = f"{faction_dir}/{filename}"

                    size = image_size(old_path)
                    if size is None:
                        print(f"[SKIP] {source_label} (empty or truncated -- "
                              f"re-download this card from thronesdb.com)")
                        skipped += 1
                        continue

                    base_name = os.path.splitext(filename)[0]
                    if style == 'ffg':
                        card, pack_code, reason = resolve_ffg(
                            base_name, by_pack.get(ffg_pack_code, {}), ffg_pack_code)
                    else:
                        card, pack_code, reason = resolve_community(
                            base_name, faction_dir, community, pack_code_map)

                    if card is None:
                        print(f"[SKIP] {source_label} ({reason})")
                        skipped += 1
                        continue

                    ext = os.path.splitext(filename)[1].lower()
                    bleed = ".bleed" if has_bleed(size) else ""
                    new_filename = f"{normalize_title(card['label'])}@{pack_code}{bleed}{ext}"
                    new_path = os.path.join(output_folder, new_filename)

                    # Report same-run collisions and pre-existing files alike.
                    prior_source = check_collision(seen_filenames, new_filename, source_label)
                    if prior_source:
                        print(f"[WARN] Collision: {new_filename} from {source_label} overwrites "
                              f"output already produced by {prior_source} in this run")
                    elif not args.dry_run and os.path.exists(new_path):
                        print(f"[WARN] Collision: {new_filename} from {source_label} overwrites "
                              f"a pre-existing file in {output_folder}")

                    print(f"{'[DRY] ' if args.dry_run else '[OK]  '} {source_label} -> {new_filename}")

                    if not args.dry_run:
                        try:
                            shutil.copy2(old_path, new_path)
                        except OSError as e:
                            print(f"[ERR]  {filename}: {e}")
                            continue
                    copied += 1

    print(format_summary(copied, skipped))


if __name__ == "__main__":
    main()
