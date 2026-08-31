# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
Renames Warhammer 40k Conquest card scans to the current Proxy Nexus naming
convention.

Two archives circulate, laid out differently, and either or both can be passed
in a single run:

  pack archive     Planetfall Cycle/Decree of Ruin/bleed_Acid Maw.png
  faction archive  Dark Eldar/Event/Raid.jpg

A file is matched to a card by its name, and to a pack by its folder in the
pack archive or by the card's own entry in the faction archive. Where both
archives hold a card, the sharper scan wins. Copies into an output folder; the
sources are never modified.

See README.md for the mapping rules and known limitations.
"""

import os
import re
import json
import shutil
import difflib
import argparse
import unicodedata
import struct
from collections import defaultdict

CATALOG_DIR = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)),
                 '..', '..', '..', 'proxynexus-core', 'src', 'games', 'whconquest')
)

IMAGE_EXTS = ('.png', '.jpg', '.jpeg')

# A poker card is 2.5x3.5in, and MPC's bleed adds 0.11in on every side.
CARD_WIDTH_INCHES = 2.5
BLEED_WIDTH_INCHES = 2.72

# MPC's minimum print size. Every scan carrying a bleed border is exactly this,
# so the image itself tells us whether one is already baked in.
BLEED_SIZE = (816, 1110)

# `bleed_Sicarius_s Chosen (3).png`: an optional prefix, then the card name
# with `_` standing in for an apostrophe, then an optional copy number.
NAME_PREFIX = 'bleed_'
COPY_SUFFIX = re.compile(r'^(.*?)\s*\((\d+)\)$')

# Filenames are hand-typed and hold a fair number of spelling mistakes
# ("Guass Flayer", "Lemon Russ Conqueror"). In the pack archive, matching is
# scoped to the one pack the folder names, which keeps a loose threshold safe --
# the worst real misspelling scores 0.875 and the nearest wrong card in the same
# pack is far below that.
CARD_CUTOFF = 0.85
CARD_MARGIN = 0.05

# Folder names are hand-typed too ("Descendants of Isha" for The Descendants of
# Isha), but there are only ~30 of them, so this can be strict.
PACK_CUTOFF = 0.8

# Warlords are the only double-sided cards. The pack archive numbers the two
# sides as copies 1 and 2; the faction archive names the reverse outright.
DOUBLE_SIDED_TYPE = 'Warlord'
BLOODIED_SUFFIX = '_bloodied'

# Faction-archive folders that hold no printable card: blank templates and
# loyalty icons for making custom cards, punchboard tokens, and phase reference
# sheets.
FACTION_SKIP_DIRS = {'blanked', 'tokens', 'misc'}

# The faction archive carries fan-made reworkings of cards beside the real
# ones, under the same name with a suffix. Those are not the printed card.
FACTION_VARIANT_SUFFIX = re.compile(r'_apoka$', re.IGNORECASE)

# A card name has to be matched against the whole catalog in the faction
# archive, with no pack folder to narrow it down, so the threshold there is
# tight enough that only an exact-but-for-punctuation name gets through.
FACTION_CUTOFF = 0.93

# One card, the warlord '"Subject: O-X62113"', is printed with a Greek omega
# that every source transcribes differently: the catalog data flattens it to a
# bare "O", the faction archive spells it "Omega", and the pack archive
# mistypes it as "Q". Spelling gets the pack archive there because its match is
# scoped to one pack, but not the faction archive, so the reading is spelled
# out here. No other card in the catalog carries a symbol.
NAME_ALIASES = {'subjectomegax62113': 'subject-o-x62113'}


def slug(text):
    """Lowercase hyphenated id, matching build_catalog.py."""
    text = unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode()
    text = text.replace("'", '')
    return re.sub(r'[^a-z0-9]+', '-', text.lower()).strip('-')


def squash(text):
    """Letters and digits only. The archives punctuate names differently and
    inconsistently -- the faction archive spells both a space and an apostrophe
    as `_`, so `Straken_s_Cunning` and `23rd_Mechanised_Battalion` need opposite
    readings of the same character -- and dropping punctuation from both sides
    sidesteps the ambiguity. It stays unique: no two cards in the catalog have
    names that differ only in punctuation."""
    text = unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode()
    return re.sub(r'[^a-z0-9]+', '', text.lower())


def card_index(cards):
    """Card lookups for one candidate set: exact by squashed name, and by id
    for the fuzzy fallback."""
    return {'by_id': cards, 'by_squash': {squash(card['name']): card for card in cards.values()}}


def load_catalog():
    with open(os.path.join(CATALOG_DIR, 'whc_cards.json'), encoding='utf-8') as handle:
        cards = json.load(handle)
    with open(os.path.join(CATALOG_DIR, 'whc_packs.json'), encoding='utf-8') as handle:
        packs = json.load(handle)

    by_pack = defaultdict(dict)
    for card in cards:
        by_pack[card['pack_code']][card['unique_id']] = card

    pack_ids = {}
    for pack in packs:
        pack_ids[pack['code']] = pack['code']
        pack_ids[slug(pack['name'])] = pack['code']

    return by_pack, pack_ids


def image_size(path):
    """Dimensions of a PNG or JPEG, without decoding the pixels."""
    with open(path, 'rb') as handle:
        head = handle.read(24)
        if head[:8] == b'\x89PNG\r\n\x1a\n':
            return struct.unpack('>II', head[16:24])

        handle.seek(2)
        while True:
            marker = handle.read(2)
            if len(marker) < 2 or marker[0] != 0xFF:
                return None
            length = struct.unpack('>H', handle.read(2))[0]
            if 0xC0 <= marker[1] <= 0xCF and marker[1] not in (0xC4, 0xC8, 0xCC):
                height, width = struct.unpack('>HH', handle.read(5)[1:])
                return width, height
            handle.seek(length - 2, os.SEEK_CUR)


def card_dpi(width, bleed):
    """Resolution across the card itself, so a bleed scan and a bleedless one
    can be compared. Everything outside the card is discarded before printing,
    and both archives' bleeds turn out to be mirrored edges rather than real
    ones, so only the card area is worth measuring."""
    inches = BLEED_WIDTH_INCHES if bleed else CARD_WIDTH_INCHES
    return width / inches


def describe(path):
    """The facts about a scan that decide whether it is the one to use."""
    size = image_size(path)
    width = size[0] if size else 0
    bleed = size == BLEED_SIZE
    return {
        'path': path,
        'ext': os.path.splitext(path)[1].lower(),
        'bleed': bleed,
        'dpi': card_dpi(width, bleed),
        'lossless': path.lower().endswith('.png'),
    }


def split_copy_index(base_name):
    """`bleed_Brood Warriors (3)` -> ("Brood Warriors", 3). A name with no copy
    number is the first copy."""
    name = base_name.removeprefix(NAME_PREFIX)
    match = COPY_SUFFIX.match(name)
    if match:
        return match.group(1), int(match.group(2))
    return name, 1


def resolve_pack(path, source_root, pack_ids):
    """The deepest folder between the file and the source root that names a
    pack. The folders in between -- `Warlords`, `Planets`, a warlord's own
    folder -- name no pack and are walked past."""
    relative = os.path.relpath(os.path.dirname(path), source_root)
    parts = [] if relative == '.' else relative.split(os.sep)

    for part in reversed(parts):
        key = slug(part)
        if key in pack_ids:
            return pack_ids[key], None
        close = difflib.get_close_matches(key, list(pack_ids), n=1, cutoff=PACK_CUTOFF)
        if close:
            return pack_ids[close[0]], (part, close[0])

    return None, None


def resolve_card(name, cards, cutoff=CARD_CUTOFF):
    """The card the filename names. Punctuation is dropped for the exact match,
    then spelling carries the rest -- the source filenames are full of
    misspellings that no amount of normalising will fix."""
    key = squash(name)
    if key in cards['by_squash']:
        return cards['by_squash'][key], None, None
    if NAME_ALIASES.get(key) in cards['by_id']:
        return cards['by_id'][NAME_ALIASES[key]], None, None

    key = slug(name)
    scored = sorted(
        ((difflib.SequenceMatcher(None, key, candidate).ratio(), candidate)
         for candidate in cards['by_id']),
        reverse=True,
    )
    if not scored or scored[0][0] < cutoff:
        return None, None, f'no card named {key!r}'
    if len(scored) > 1 and scored[0][0] - scored[1][0] < CARD_MARGIN:
        return None, None, f'{key!r} is as close to {scored[0][1]!r} as to {scored[1][1]!r}'

    return cards['by_id'][scored[0][1]], (key, scored[0][1], round(scored[0][0], 3)), None


def collect_pack_archive(source_root, by_pack, pack_ids):
    """Resolves every scan in an archive laid out one folder per pack."""
    packs = {pack_id: card_index(cards) for pack_id, cards in by_pack.items()}
    groups = defaultdict(list)
    unresolved, pack_guesses, card_guesses = [], {}, []

    for root, dirs, files in os.walk(source_root):
        dirs.sort()
        for filename in sorted(files):
            if not filename.lower().endswith(IMAGE_EXTS):
                continue

            path = os.path.join(root, filename)
            name, index = split_copy_index(os.path.splitext(filename)[0])

            pack_id, guess = resolve_pack(path, source_root, pack_ids)
            if pack_id is None:
                unresolved.append((path, 'no pack folder'))
                continue
            if guess:
                pack_guesses[guess[0]] = guess[1]

            card, guess, reason = resolve_card(name, packs[pack_id])
            if card is None:
                unresolved.append((path, reason))
                continue
            if guess:
                card_guesses.append((filename, guess[1], guess[2]))

            groups[(pack_id, card['unique_id'])].append(
                dict(describe(path), index=index, card=card))

    return groups, unresolved, pack_guesses, card_guesses


def collect_faction_archive(source_root, catalog):
    """Resolves every scan in an archive laid out by faction and card type. No
    folder names the pack, so the card's own catalog entry supplies it -- which
    works because card names are unique across the whole catalog. The archive
    also holds a good deal of fan-made content that is in no pack; those files
    match no card and are counted rather than reported one by one."""
    groups = defaultdict(list)
    foreign, card_guesses = 0, []

    for root, dirs, files in os.walk(source_root):
        dirs[:] = sorted(d for d in dirs if d.lower() not in FACTION_SKIP_DIRS)
        for filename in sorted(files):
            if not filename.lower().endswith(IMAGE_EXTS):
                continue

            base_name = os.path.splitext(filename)[0]
            if FACTION_VARIANT_SUFFIX.search(base_name):
                foreign += 1
                continue

            bloodied = base_name.lower().endswith(BLOODIED_SUFFIX)
            if bloodied:
                base_name = base_name[:-len(BLOODIED_SUFFIX)]

            card, guess, _ = resolve_card(base_name, catalog, cutoff=FACTION_CUTOFF)
            if card is None:
                foreign += 1
                continue
            if guess:
                card_guesses.append((filename, guess[1], guess[2]))

            path = os.path.join(root, filename)
            groups[(card['pack_code'], card['unique_id'])].append(
                dict(describe(path), index=2 if bloodied else 1, card=card))

    return groups, foreign, card_guesses


def merge(*collected):
    """Pools the scans every archive found for each card."""
    groups = defaultdict(list)
    for archive in collected:
        for key, scans in archive.items():
            groups[key].extend(scans)
    return groups


def sharpest(scans):
    """The scan to print. Resolution across the card decides it; a bleed
    already in the file and a lossless format break ties, in that order,
    because each only saves a step rather than adding detail."""
    return max(scans, key=lambda scan: (scan['dpi'], scan['bleed'], scan['lossless']))


def plan(groups):
    """Names one scan per side of each card. The first copy is the front; on a
    warlord the second is the bloodied reverse. Everything else the archives
    hold for that card is a further scan of the same face, and is passed over."""
    renames, passed_over, half_warlords = [], [], []

    for (pack_id, card_id), scans in sorted(groups.items()):
        warlord = scans[0]['card']['type'] == DOUBLE_SIDED_TYPE
        by_side = defaultdict(list)
        for scan in scans:
            by_side[scan['index'] if warlord else 1].append(scan)

        if warlord and len(by_side) < 2:
            half_warlords.append(card_id)

        for side_index, side_scans in sorted(by_side.items()):
            chosen = sharpest(side_scans)
            side = '~back' if side_index == 2 else ''
            bleed = '.bleed' if chosen['bleed'] else ''
            renames.append(
                (chosen['path'], f'{card_id}@{pack_id}{side}{bleed}{chosen["ext"]}'))
            passed_over.extend(
                scan['path'] for scan in sorted(side_scans, key=lambda s: s['path'])
                if scan is not chosen)

    return renames, passed_over, half_warlords


def report_gaps(groups, by_pack):
    """Cards in a pack that some scan reached, but that no scan matched."""
    seen = defaultdict(set)
    for pack_id, card_id in groups:
        seen[pack_id].add(card_id)

    gaps = []
    for pack_id, card_ids in sorted(seen.items()):
        missing = sorted(set(by_pack[pack_id]) - card_ids)
        if missing:
            gaps.append((pack_id, len(by_pack[pack_id]), missing))
    return gaps


def main():
    parser = argparse.ArgumentParser(
        description='Renames Warhammer 40k Conquest card scans to the Proxy Nexus naming '
                    'convention. Copies into an output folder; the sources are never modified.')
    parser.add_argument('--pack-archive', metavar='FOLDER',
                        help='An archive laid out one folder per pack')
    parser.add_argument('--faction-archive', metavar='FOLDER',
                        help='An archive laid out by faction and card type')
    parser.add_argument('-o', '--output', default='whconquest_renamed', help='Destination folder')
    parser.add_argument('--dry-run', action='store_true', help='Preview without copying')
    args = parser.parse_args()

    if not args.pack_archive and not args.faction_archive:
        parser.error('pass --pack-archive, --faction-archive, or both')

    output_folder = os.path.abspath(args.output)
    by_pack, pack_ids = load_catalog()
    by_id = {card_id: card for cards in by_pack.values() for card_id, card in cards.items()}
    print(f'--- Catalog: {len(by_id)} cards in {len(by_pack)} packs ---')

    from_packs, unresolved, pack_guesses, card_guesses = defaultdict(list), [], {}, []
    from_factions, foreign = defaultdict(list), 0

    if args.pack_archive:
        root = os.path.abspath(args.pack_archive)
        print(f'\n--- Pack archive: {root} ---')
        from_packs, unresolved, pack_guesses, card_guesses = collect_pack_archive(
            root, by_pack, pack_ids)
        print(f'  {sum(len(s) for s in from_packs.values())} scans matched')

    if args.faction_archive:
        root = os.path.abspath(args.faction_archive)
        print(f'\n--- Faction archive: {root} ---')
        from_factions, foreign, guesses = collect_faction_archive(root, card_index(by_id))
        card_guesses.extend(guesses)
        print(f'  {sum(len(s) for s in from_factions.values())} scans matched, '
              f'{foreign} files matched no card in the catalog')

    groups = merge(from_packs, from_factions)
    renames, passed_over, half_warlords = plan(groups)

    print(f"\n--- Naming {'(DRY RUN) ' if args.dry_run else ''}---")
    if not args.dry_run:
        os.makedirs(output_folder, exist_ok=True)

    copied, written = 0, {}
    for source, new_name in renames:
        if new_name in written:
            print(f'[WARN] {source} -> {new_name} collides with {written[new_name]}')
        written[new_name] = source

        print(f"{'[DRY] ' if args.dry_run else '[OK]  '} {os.path.basename(source)} -> {new_name}")
        if not args.dry_run:
            try:
                shutil.copy2(source, os.path.join(output_folder, new_name))
            except OSError as error:
                print(f'[ERR]  {source}: {error}')
                continue
        copied += 1

    print(f'\nSummary: {copied} processed, {len(passed_over)} further scans passed over, '
          f'{len(unresolved)} unresolved.')

    chosen_dpi = defaultdict(int)
    for source, _ in renames:
        scan = describe(source)
        chosen_dpi[f"~{round(scan['dpi'] / 25) * 25} dpi"] += 1
    print('Resolution of the scans chosen: '
          + ', '.join(f'{count} at {label}' for label, count in sorted(chosen_dpi.items())))

    if pack_guesses:
        print(f'\n--- Folders matched to a pack by spelling ({len(pack_guesses)}) ---')
        for folder, pack_id in sorted(pack_guesses.items()):
            print(f'  {folder} -> {pack_id}')

    if card_guesses:
        print(f'\n--- Filenames matched to a card by spelling ({len(card_guesses)}) ---')
        for filename, card_id, score in card_guesses:
            print(f'  {filename} -> {card_id} ({score})')

    if half_warlords:
        print(f'\n--- Warlords missing their bloodied side ({len(half_warlords)}) ---')
        print('These print with the generic card back instead of their own reverse.')
        for card_id in half_warlords:
            print(f'  {card_id}')

    if unresolved:
        print(f'\n--- Unresolved ({len(unresolved)}) ---')
        for path, reason in unresolved:
            print(f'  {path} ({reason})')

    gaps = report_gaps(groups, by_pack)
    if gaps:
        print('\n--- Cards with no scan ---')
        for pack_id, total, missing in gaps:
            print(f'  {pack_id} ({len(missing)} of {total}): {", ".join(missing)}')


if __name__ == '__main__':
    main()
