# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
Renames L5R card scans to the current Proxy Nexus naming convention.

Handles both source archives, picking a strategy per file:

  Emerald Legacy   TTM5.jpg                                -> set abbreviation + card number
  FFG 600dpi       Phoenix_D_9_Kaito Temple Protector.tiff -> fuzzy card-name match

Copies into an output folder; the source is never modified.

See README.md for where to download the sources, the mapping rules and known
limitations.
"""

import os
import re
import json
import shutil
import string
import argparse
import urllib.request

CATALOG_CACHE = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'l5r_catalog_cache.json')

# Emerald Legacy filenames are a set abbreviation plus the card number, e.g.
# ANS_1.jpg, TTM5.jpg, RoB01.jpg, ecs002.jpg, uee001.jpg. A trailing _N marks
# a duplicate copy of the same card.
EL_FILENAME = re.compile(r'^([A-Za-z]+)_?(\d+)(?:_(\d+))?$')

# Some FFG scans put two *different* cards on one physical card and name both
# files after the A-side card, distinguished only by a side marker. The B side
# is a separate card, not a back face:
#   role cards            Keeper of X (A) / Seeker of X (B)
#   Shadowlands warlords  cooperative (A) / challenge (B)
# Two spellings appear -- "Side B" on the role cards and imperial favor,
# "B side" on the warlords. Missing either makes one card overwrite the other.
SIDE_B = re.compile(r'(?<![a-z])(?:side[\s_]*b|b[\s_]*side)(?![a-z])')

PACK_ABBR = {
    'ans': 'ancient-secrets',
    'rob': 'restoration-of-balance',
    'ttm': 'through-the-mists',
    'uee': 'under-the-empress-eyes',
    'ecs': 'emerald-core-set',
}

# MPC's minimum print size. Emerald Legacy's "bleed cut" downloads are exactly
# this and their "regular cut" ones are smaller, so the image itself tells us
# whether a bleed border is already baked in.
BLEED_SIZE = (816, 1110)

# Emerald Legacy ships an `archive` folder of superseded scans alongside the
# current ones.
SKIP_DIRS = {'archive'}

# Alt-art reprints. They're the same cards as their original packs, so they get
# a custom printing label instead of a pack id.
PROMO_DIR = 'promo cards'
PROMO_PRINTING = 'promo'

IMAGE_EXTS = ('.tif', '.tiff', '.png', '.jpg', '.jpeg')

# One scan is missing its card name altogether: `Unicorn_Stronghold.tiff` is
# Shiro Shinjo. Kept separate from TYPO_FIXES because nothing is misspelled --
# the name simply isn't in the filename -- and keyed on the whole base name so
# it can't affect the other `*_Stronghold_*` scans, which are all named
# properly. Verified against the scan itself.
FILENAME_FIXES = {
    "unicorn_stronghold": "Shiro Shinjo",
}

# Misspellings in the original scan filenames, mapped onto the real card names.
# Keys are the output of normalize_name(), so they carry no spaces or
# punctuation.
TYPO_FIXES = {
    "defentthewall": "defendthewall",
    "hirumajojimbo": "hirumayojimbo",
    "secretcashe": "secretcache",
    "callinginfavours": "callinginfavors",
    "alchemicallabratory": "alchemicallaboratory",
    "strstegizing": "strategizing",
    "unguestionedheritage": "unquestionedheritage",
    "contempaltivewisdom": "contemplativewisdom",
    "theperfetgift": "theperfectgift",
    "politicarival": "politicalrival",
    "nobelsacrifice": "noblesacrifice",
    "forgetedict": "forgededict",
    "anassumingyojimbo": "unassumingyojimbo",
    "meditationonthetao": "meditationsonthetao",
    "coolomen": "goodomen",
    "assassiation": "assassination",
    "magnifecentkimono": "magnificentkimono",
    "meddingmediatoe": "meddlingmediator",
    "streghtinnumbers": "strengthinnumbers",
    "tattoedwanderer": "tattooedwanderer",
    "matraoffire": "mantraoffire",
    "paragonofcrace": "paragonofgrace",
    "thefireofjustice": "thefiresofjustice",
    "bayushiaramona": "bayushiaramoro",
    "arnamentartisan": "armamentartisan",
    "distinguisheddjo": "distinguisheddojo",
    "embrancedeath": "embracedeath",
    "thestreghtofthemountain": "thestrengthofthemountain",
    "ringbinding": "ringofbinding",
    "dragontatto": "dragontattoo",
    "mirumotumasashige": "mirumotomasashige",
    "akodaokaede": "akodokaede",
    "derectingthebattle": "directingthebattle",
    "fusuipidciple": "fusuidisciple",
    "fusuididciple": "fusuidisciple",
    "kunilabratory": "kunilaboratory",
    "ragingbattlegroung": "ragingbattleground",
    "teachingoftheelements": "teachingsoftheelements",
    "talantedperformer": "talentedperformer",
    "endlessplainskirmisher": "endlessplainsskirmisher",
    "conningconfidant": "cunningconfidant",
    "consumedbythefivefires": "consumedbyfivefires",
    "sealofthepheonix": "sealofthephoenix",
    "northenrnwallsensei": "northernwallsensei",
    "bloodofonnatangu": "bloodofonnotangu",
    "kangodistrict": "kanjodistrict",
    "utakimediator": "utakumediator",
    "matraofwater": "mantraofwater",
    "suppurtofthescorpion": "supportofthescorpion",
    "hidatomanatsu": "hidatomonatsu",
    "orratefan": "ornatefan",
    "purifiedapprentice": "purifierapprentice",
    "honoredveteran": "honoredveterans",
    "militaryfaithful": "militantfaithful",
    "offeringtothekami": "offeringstothekami",
    "criminalcontracts": "criminalcontacts",
    "honoednodachi": "honednodachi",
    "keepersofthesecretnames": "keeperofsecretnames",
    "tarjujiai": "taryujiai",
    "scorpionsupportofthescorpion": "supportofthescorpion",
    "cranesupportofthecrane": "supportofthecrane",
    "lionsupportofthelion": "supportofthelion",
    "crabsupportofthecrab": "supportofthecrab",
    "phoenixsupportofthephoenix": "supportofthephoenix",
    "unicornsupportoftheunicorn": "supportoftheunicorn",
    "dragonsupportofthedragon": "supportofthedragon",
    "crabkuniyori": "kuniyori",
    "craneprovincemagistratestation": "magistratestation",
    "unicornthewesternwind": "thewesternwind",
}


def fetch_json(url):
    print(f"Fetching: {url}")
    req = urllib.request.Request(url, headers={'User-Agent': 'ProxyNexus-ImageFileRenamer/1.0'})
    with urllib.request.urlopen(req) as response:
        return json.loads(response.read().decode('utf-8'))


def load_catalog():
    """Read the cached EmeraldDB catalog, downloading it first if absent."""
    if os.path.exists(CATALOG_CACHE):
        with open(CATALOG_CACHE, 'r', encoding='utf-8') as f:
            data = json.load(f)
            return data.get("packs", []), data.get("cards", [])

    print("Catalog cache not found. Downloading catalog...")
    packs = fetch_json("https://www.emeralddb.org/api/packs")
    cards = fetch_json("https://www.emeralddb.org/api/cards")
    with open(CATALOG_CACHE, 'w', encoding='utf-8') as f:
        json.dump({"packs": packs, "cards": cards}, f)
    return packs, cards


def normalize_name(name):
    """Lowercase and strip punctuation for fuzzy matching, then apply TYPO_FIXES."""
    name = re.sub(r' x \d+', '', name, flags=re.IGNORECASE)
    name = name.replace("’", "'").replace("“", '"').replace("”", '"').replace("–", "-").replace("—", "-")
    norm = name.translate(str.maketrans('', '', string.punctuation)).lower().replace(" ", "")
    return TYPO_FIXES.get(norm, norm)


def split_prefix_and_name(base_name):
    """Split an FFG filename into (card_name, numbers_in_name, is_side_b).

    FFG scans put the card name last but vary what comes before it
    (`Phoenix_D_9_Kaito Temple Protector`, `127_Neutral_D_SuddenTempest`,
    `Children of the Empire_Neutral_C_80_Stay Your Hand`). Scanning for the
    last metadata marker -- C, D, PROVINCE, STRONGHOLD or a bare number --
    finds the name in every layout.
    """
    parts = base_name.split('_')

    last_delim_idx = -1
    for i, part in enumerate(parts):
        if part.upper() in ('C', 'D', 'PROVINCE', 'STRONGHOLD') or part.isdigit():
            last_delim_idx = i

    if last_delim_idx != -1 and last_delim_idx < len(parts) - 1:
        card_name = "_".join(parts[last_delim_idx + 1:])
    else:
        card_name = parts[-1]

    # Every digit run in the whole filename, used later to pick a printing.
    numbers_in_name = re.findall(r'\d+', base_name)

    prefix = "_".join(parts[:-1]).lower()
    is_side_b = bool(SIDE_B.search(prefix))

    return card_name, numbers_in_name, is_side_b


def side_b_card_id(card_id):
    """The B side of a double-card FFG scan, or None if it isn't one."""
    if card_id.startswith("keeper-of-"):
        return "seeker-of-" + card_id[len("keeper-of-"):]
    if card_id.endswith("-coop"):
        return card_id[:-len("-coop")] + "-challenge"
    return None


def build_indexes(packs, cards):
    """Build the lookups the two strategies resolve against."""
    # Several cards can share a name (Agasha Sumiko is printed twice under two
    # ids), so a name maps to every card that carries it and the printing is
    # picked later from the filename's position number.
    by_name = {}
    for c in cards:
        by_name.setdefault(normalize_name(c['name']), []).append(c)
    for c in cards:
        by_name.setdefault(normalize_name(c['id'].replace("-", " ")), [c])

    by_id = {c['id']: c for c in cards}

    by_pack_position = {}
    for c in cards:
        for v in c.get('versions', []):
            by_pack_position.setdefault((v.get('pack_id'), str(v.get('position'))), c)

    # Undated packs sort last, so a dated printing always wins a tie.
    pack_dates = {p['id']: (p.get('released_at') or "2099-01-01") for p in packs}

    return by_name, by_id, by_pack_position, pack_dates


def jpeg_size(path):
    """(width, height) from a JPEG's SOF marker, or None if it isn't a JPEG.

    Hand-rolled rather than using Pillow so the script stays dependency-free;
    the frame header is all we need and it sits near the front of the file.
    """
    with open(path, 'rb') as f:
        if f.read(2) != b'\xff\xd8':
            return None
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


def has_bleed(path):
    """Emerald Legacy's bleed-cut scans are exactly MPC's minimum print size."""
    try:
        return jpeg_size(path) == BLEED_SIZE
    except OSError:
        return False


def resolve_emerald_legacy(base_name, by_pack_position):
    """Resolve an Emerald Legacy filename.

    Returns (card, pack_id, reason). A card of None with a reason of None means
    the name isn't Emerald Legacy's shape at all, so the caller should try the
    FFG strategy instead.
    """
    match = EL_FILENAME.match(base_name)
    if not match:
        return None, None, None

    abbr, position, copy_num = match.groups()
    pack_id = PACK_ABBR.get(abbr.lower())
    if not pack_id:
        return None, None, f"unknown set abbreviation '{abbr}'"

    if copy_num:
        return None, None, f"duplicate copy of {abbr}{position}"

    card = by_pack_position.get((pack_id, str(int(position))))
    if not card:
        return None, None, f"no card at {pack_id} position {int(position)}"

    return card, pack_id, None


def side_variant(card, is_side_b, by_id):
    """Swap in the other card of a two-cards-on-one-physical-card scan."""
    if is_side_b:
        alt = side_b_card_id(card['id'])
    elif card['id'].endswith('-challenge'):
        alt = card['id'][:-len('-challenge')] + '-coop'
    else:
        alt = None
    return by_id.get(alt, card) if alt else card


def resolve_ffg(base_name, by_name, by_id, pack_dates):
    """Resolve an FFG filename by card name.

    Returns (card, pack_id, tie, reason).
    """
    card_name, numbers_in_name, is_side_b = split_prefix_and_name(base_name)
    card_name = FILENAME_FIXES.get(base_name.lower(), card_name)

    candidates = by_name.get(normalize_name(card_name))
    if not candidates:
        return None, None, None, f"card name '{card_name}' not found"

    # Both sides of a double card are named after the A side, so swap in the
    # side the filename actually refers to.
    seen, swapped = set(), []
    for c in candidates:
        c = side_variant(c, is_side_b, by_id)
        if c['id'] not in seen:
            seen.add(c['id'])
            swapped.append(c)

    # Every (card, printing) pair this name could mean.
    pairs = [(c, v) for c in swapped for v in c.get('versions', [])]
    if not pairs:
        return None, None, None, "card has no printings"

    if len(pairs) == 1:
        card, version = pairs[0]
        return card, version['pack_id'], None, None

    # A position number in the filename identifies the exact printing.
    matched = [(c, v) for c, v in pairs if str(v.get('position', '')) in numbers_in_name]
    if len(matched) == 1:
        card, version = matched[0]
        return card, version['pack_id'], None, None

    card, version = min(pairs, key=lambda cv: pack_dates.get(cv[1]['pack_id'], "2099-01-01"))
    tie = {
        "card": card['name'],
        "options": sorted({v['pack_id'] for _, v in pairs}),
        "chosen": version['pack_id'],
    }
    return card, version['pack_id'], tie, None


def main():
    parser = argparse.ArgumentParser(
        description="Renames L5R card scans to the Proxy Nexus naming convention. "
                    "Copies into an output folder; the source is never modified."
    )
    parser.add_argument("inputs", type=str, nargs="+",
                        help="One or more folders of scans (Emerald Legacy and/or FFG 600dpi)")
    parser.add_argument("-o", "--output", default="l5r_renamed", help="Destination folder")
    parser.add_argument("--dry-run", action="store_true", help="Preview without copying")
    args = parser.parse_args()

    input_folders = [os.path.abspath(p) for p in args.inputs]
    output_folder: str = os.path.abspath(args.output)

    print("--- Verifying/Fetching Catalog ---")
    packs, cards = load_catalog()
    by_name, by_id, by_pack_position, pack_dates = build_indexes(packs, cards)

    print(f"\n--- Scanning {'(DRY RUN) ' if args.dry_run else ''}---")
    if not args.dry_run:
        os.makedirs(output_folder, exist_ok=True)

    copied, skipped, ties = 0, 0, []
    written = {}

    for input_folder in input_folders:
        print(f"\nScanning: {input_folder}")

        for root, dirs, files in os.walk(input_folder):
            dirs[:] = [d for d in dirs if d.lower() not in SKIP_DIRS]
            files.sort()

            for filename in files:
                if not filename.lower().endswith(IMAGE_EXTS):
                    continue

                base_name, ext = os.path.splitext(filename)
                source = os.path.join(root, filename)
                tie = None

                # Emerald Legacy names are unambiguous, so try them first;
                # anything that isn't one is an FFG scan, matched on card name.
                card, pack_id, reason = resolve_emerald_legacy(base_name, by_pack_position)
                if card is None and reason is None:
                    card, pack_id, tie, reason = resolve_ffg(base_name, by_name, by_id, pack_dates)

                if card is None:
                    print(f"[SKIP] {filename} ({reason})")
                    skipped += 1
                    continue

                if tie:
                    ties.append(dict(tie, filename=filename))

                bleed = ".bleed" if has_bleed(source) else ""
                if PROMO_DIR in root.lower():
                    pack_id = PROMO_PRINTING
                new_name = f"{card['id']}@{pack_id}{bleed}{ext.lower()}"

                if new_name in written:
                    print(f"[WARN] {filename} -> {new_name} collides with {written[new_name]}")
                written[new_name] = filename

                print(f"{'[DRY] ' if args.dry_run else '[OK]  '} {filename} -> {new_name}")

                if not args.dry_run:
                    try:
                        shutil.copy2(source, os.path.join(output_folder, new_name))
                    except Exception as e:
                        print(f"[ERR]  {filename}: {e}")
                        continue
                copied += 1

    print(f"\nSummary: {copied} processed, {skipped} skipped.")

    if ties:
        print(f"\n--- Ambiguous printings ({len(ties)}) ---")
        print("The filename didn't say which printing, so the oldest was used.")
        for t in ties:
            print(f"  {t['filename']} -> {t['chosen']}  options: {t['options']}")


if __name__ == "__main__":
    main()
