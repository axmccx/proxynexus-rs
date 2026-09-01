# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
Renames Call of Cthulhu card scans to the current Proxy Nexus naming
convention.

The archive is laid out one folder per pack, sometimes under a cycle folder,
and every file is numbered and named after its card:

    04 - The Dreamlands/02 - In Memory of Day/023 - Daybreak!.jpg

A file is matched to a card by its title, falling back to its number, and to a
pack by its folder. Copies into an output folder; the sources are never
modified.

See README.md for the mapping rules and known limitations.
"""

import os
import re
import json
import shutil
import difflib
import argparse
import unicodedata
from collections import defaultdict

CATALOG_DIR = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)),
                 '..', '..', '..', 'proxynexus-core', 'src', 'games', 'coclcg')
)

IMAGE_EXTS = ('.png', '.jpg', '.jpeg')

# `023 - Daybreak!.jpg`, or `031 - Visitor from the Spheres(1).jpg` for the one
# card the archive holds twice.
CARD_FILE = re.compile(r'^(\d+)\s*-\s*(.*?)\s*(?:\((\d+)\))?$')

# A folder name carries the cycle or pack index: `02 - Kingsport Dreams`.
FOLDER_INDEX = re.compile(r'^\d+\s*-\s*')

# The alternate-art promos are in their own folder and belong to no set, so
# they are matched against the whole catalog and named for the promos pack.
PROMOS_DIR = 'promos'
PROMOS_PACK = 'promos'

# Folders holding nothing that is printed on a card: the rules and FAQ PDFs,
# the story sheets that come folded in each pack, and the card backs and draft
# cards.
SKIP_DIRS = {'lore', 'organized play', 'miscellaneous'}

# Scans of a card back rather than a card face. They live in the adapter
# instead, under proxynexus-core/src/games/coclcg/backs/.
BACK_FILES = {'card reverse', 'story card back', 'story card reverse'}

# Filenames are hand-typed and hold a fair number of spelling mistakes
# ("Tch-Tcho Tribe", "Student Archaelogist"). Matching is scoped to the one
# pack the folder names, which keeps a loose threshold safe.
CARD_CUTOFF = 0.85
CARD_MARGIN = 0.05

# Folder names are hand-typed too ("At the Mountains of Madness" for The
# Mountains of Madness), but there are only ~60 of them, so this can be strict.
PACK_CUTOFF = 0.8

# A promo has to be matched against the whole catalog, with no pack folder to
# narrow it down, so the threshold there is tight enough that only an
# exact-but-for-punctuation title gets through.
PROMO_CUTOFF = 0.95


def slug(text):
    """Lowercase hyphenated id, matching build_catalog.py."""
    text = unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode()
    text = text.replace("'", '')
    return re.sub(r'[^a-z0-9]+', '-', text.lower()).strip('-')


def squash(text):
    """Letters and digits only. The archive punctuates titles differently from
    the catalog and inconsistently within itself -- an apostrophe is written as
    `_`, an exclamation mark is sometimes dropped, an ellipsis is spelled with
    three periods -- and dropping punctuation from both sides sidesteps all of
    it. Nothing is lost: no two cards in a pack have titles that differ only in
    punctuation."""
    text = unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode()
    return re.sub(r'[^a-z0-9]+', '', text.lower())


def load_catalog():
    with open(os.path.join(CATALOG_DIR, 'coc_cards.json'), encoding='utf-8') as handle:
        cards = json.load(handle)
    with open(os.path.join(CATALOG_DIR, 'coc_packs.json'), encoding='utf-8') as handle:
        packs = json.load(handle)

    by_pack = defaultdict(lambda: {'by_squash': defaultdict(list), 'by_number': {}, 'ids': set()})
    for card in cards:
        for version in card['versions']:
            pack = by_pack[version['pack_code']]
            pack['by_squash'][squash(title_only(card))].append(card)
            pack['ids'].add(card['unique_id'])
            if version['number'] is not None:
                pack['by_number'][version['number']] = card

    pack_ids = {}
    for pack in packs:
        pack_ids[pack['code']] = pack['code']
        pack_ids[slug(pack['name'])] = pack['code']

    return cards, by_pack, pack_ids


def title_only(card):
    """The card's title without the suffix `build_catalog.py` adds to tell two
    cards of the same name apart. The archive names files by title alone."""
    return re.sub(r'\s*\([^()]*\)$', '', card['name'])


def resolve_pack(path, source_root, pack_ids):
    """The deepest folder between the file and the source root that names a
    pack, so a cycle folder above it is walked past."""
    relative = os.path.relpath(os.path.dirname(path), source_root)
    parts = [] if relative == '.' else relative.split(os.sep)

    for part in reversed(parts):
        key = slug(FOLDER_INDEX.sub('', part))
        if key in pack_ids:
            return pack_ids[key], None
        close = difflib.get_close_matches(key, list(pack_ids), n=1, cutoff=PACK_CUTOFF)
        if close:
            return pack_ids[close[0]], (part, pack_ids[close[0]])

    return None, None


def match_by_spelling(key, candidates, cutoff):
    """The closest title, where one is clearly closer than the rest."""
    scored = sorted(
        ((difflib.SequenceMatcher(None, key, candidate).ratio(), candidate)
         for candidate in candidates),
        reverse=True,
    )
    if not scored or scored[0][0] < cutoff:
        return None, None
    if len(scored) > 1 and scored[0][0] - scored[1][0] < CARD_MARGIN:
        return None, f'{key!r} is as close to {scored[0][1]!r} as to {scored[1][1]!r}'
    return scored[0], None


def resolve_card(pack, number, title):
    """The card a filename names, and how it was reached.

    The title decides it. Where a pack prints several cards of one title --
    the four Necronomicons in The Unspeakable Pages -- the number picks between
    them, and where the title matches nothing the number is what is left: the
    archive has two files whose title is not the card's at all.
    """
    key = squash(title)

    candidates = pack['by_squash'].get(key, [])
    if len(candidates) == 1:
        return candidates[0], None, None
    if len(candidates) > 1:
        numbered = pack['by_number'].get(number)
        if numbered in candidates:
            return numbered, None, None
        return None, None, f'{title!r} names {len(candidates)} cards in this pack'

    spelled, reason = match_by_spelling(key, pack['by_squash'], CARD_CUTOFF)
    if spelled:
        score, candidate = spelled
        if len(pack['by_squash'][candidate]) == 1:
            card = pack['by_squash'][candidate][0]
            return card, ('spelling', card['unique_id'], round(score, 3)), None

    numbered = pack['by_number'].get(number)
    if numbered:
        return numbered, ('number', numbered['unique_id'], title), None

    return None, None, reason or f'no card titled {title!r}'


def resolve_promo(by_squash, title):
    """A promo, matched against every card the catalog gives a promo printing.
    Every promo scan in the archive is an alternate art of a card that is
    already in a pack."""
    key = squash(title)
    candidates = by_squash.get(key, [])
    if len(candidates) == 1:
        return candidates[0], None, None
    if len(candidates) > 1:
        return None, None, f'{title!r} names {len(candidates)} cards with a promo printing'

    spelled, reason = match_by_spelling(key, by_squash, PROMO_CUTOFF)
    if spelled and len(by_squash[spelled[1]]) == 1:
        card = by_squash[spelled[1]][0]
        return card, ('spelling', card['unique_id'], round(spelled[0], 3)), None

    return None, None, reason or f'no card with a promo printing is titled {title!r}'


def image_size(path):
    """Dimensions of a PNG or JPEG, without decoding the pixels."""
    import struct

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


def collect(source_root, catalog, by_pack, pack_ids):
    """Resolves every scan in the archive to a card and a pack."""
    groups = defaultdict(list)
    unresolved, pack_guesses = [], {}
    card_guesses, number_matches, wrong_numbers = [], [], []
    backs, skipped = 0, 0

    promo_index = defaultdict(list)
    for card in catalog:
        if any(version['pack_code'] == PROMOS_PACK for version in card['versions']):
            promo_index[squash(title_only(card))].append(card)

    for root, dirs, files in os.walk(source_root):
        dirs[:] = sorted(d for d in dirs if d.lower() not in SKIP_DIRS)
        promos = os.path.basename(root).lower() == PROMOS_DIR

        for filename in sorted(files):
            if not filename.lower().endswith(IMAGE_EXTS):
                skipped += 1
                continue

            path = os.path.join(root, filename)
            stem = os.path.splitext(filename)[0]
            if stem.lower() in BACK_FILES:
                backs += 1
                continue

            if promos:
                card, guess, reason = resolve_promo(promo_index, stem)
                pack_id, number = PROMOS_PACK, None
            else:
                match = CARD_FILE.match(stem)
                if not match:
                    unresolved.append((path, 'not named `number - title`'))
                    continue
                number, title = int(match.group(1)), match.group(2)

                pack_id, pack_guess = resolve_pack(path, source_root, pack_ids)
                if pack_id is None:
                    unresolved.append((path, 'no pack folder'))
                    continue
                if pack_guess:
                    pack_guesses[pack_guess[0]] = pack_guess[1]

                card, guess, reason = resolve_card(by_pack[pack_id], number, title)

            if card is None:
                unresolved.append((path, reason))
                continue
            relative = os.path.relpath(path, source_root)
            printed = next((version['number'] for version in card['versions']
                            if version['pack_code'] == pack_id), None)

            if guess and guess[0] == 'spelling':
                card_guesses.append((relative, guess[1], guess[2]))
            elif guess:
                number_matches.append((relative, guess[1], guess[2]))
            elif number is not None and printed != number:
                wrong_numbers.append((relative, card['unique_id'], printed))

            groups[(pack_id, card['unique_id'])].append(path)

    return (groups, unresolved, pack_guesses, card_guesses, number_matches,
            wrong_numbers, backs, skipped)


def plan(groups):
    """Names one scan per card. Where the archive holds a card twice the larger
    scan wins, and the other is passed over."""
    renames, passed_over = [], []

    for (pack_id, card_id), paths in sorted(groups.items()):
        chosen = max(paths, key=lambda path: (image_size(path) or (0, 0))[0])
        extension = os.path.splitext(chosen)[1].lower()
        renames.append((chosen, f'{card_id}@{pack_id}{extension}'))
        passed_over.extend(path for path in sorted(paths) if path != chosen)

    return renames, passed_over


def report_gaps(groups, by_pack):
    """Cards in a pack that some scan reached, but that no scan matched."""
    seen = defaultdict(set)
    for pack_id, card_id in groups:
        seen[pack_id].add(card_id)

    gaps = []
    for pack_id, card_ids in sorted(seen.items()):
        missing = sorted(by_pack[pack_id]['ids'] - card_ids)
        if missing:
            gaps.append((pack_id, len(by_pack[pack_id]['ids']), missing))
    return gaps


def main():
    parser = argparse.ArgumentParser(
        description='Renames Call of Cthulhu card scans to the Proxy Nexus naming convention. '
                    'Copies into an output folder; the source is never modified.')
    parser.add_argument('archive', help='The scan archive, one folder per pack')
    parser.add_argument('-o', '--output', default='coclcg_renamed', help='Destination folder')
    parser.add_argument('--dry-run', action='store_true', help='Preview without copying')
    args = parser.parse_args()

    source_root = os.path.abspath(args.archive)
    output_folder = os.path.abspath(args.output)

    catalog, by_pack, pack_ids = load_catalog()
    print(f'--- Catalog: {len(catalog)} cards in {len(by_pack)} packs ---')

    print(f'\n--- Archive: {source_root} ---')
    (groups, unresolved, pack_guesses, card_guesses, number_matches,
     wrong_numbers, backs, skipped) = collect(source_root, catalog, by_pack, pack_ids)
    print(f'  {sum(len(paths) for paths in groups.values())} scans matched, '
          f'{backs} card backs and {skipped} other files passed by')

    renames, passed_over = plan(groups)

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

    if pack_guesses:
        print(f'\n--- Folders matched to a pack by spelling ({len(pack_guesses)}) ---')
        for folder, pack_id in sorted(pack_guesses.items()):
            print(f'  {folder} -> {pack_id}')

    if card_guesses:
        print(f'\n--- Filenames matched to a card by spelling ({len(card_guesses)}) ---')
        for filename, card_id, score in card_guesses:
            print(f'  {filename} -> {card_id} ({score})')

    if number_matches:
        print(f'\n--- Filenames matched to a card by number ({len(number_matches)}) ---')
        print('The title in the filename names no card in the pack, so its number decided it.')
        for filename, card_id, title in number_matches:
            print(f'  {filename} -> {card_id} (filename says {title!r})')

    if wrong_numbers:
        print(f'\n--- Filenames whose number disagrees with the catalog ({len(wrong_numbers)}) ---')
        print('The title is what was matched on. Either the file is misnumbered or the catalog')
        print('is, and the second case wants a POSITION_FIXES entry in build_catalog.py.')
        for filename, card_id, printed in wrong_numbers:
            print(f'  {filename} -> {card_id} (catalog has {printed})')

    if passed_over:
        print(f'\n--- Further scans of a card already named ({len(passed_over)}) ---')
        for path in passed_over:
            print(f'  {os.path.relpath(path, source_root)}')

    if unresolved:
        print(f'\n--- Unresolved ({len(unresolved)}) ---')
        for path, reason in unresolved:
            print(f'  {os.path.relpath(path, source_root)} ({reason})')

    gaps = report_gaps(groups, by_pack)
    if gaps:
        print('\n--- Cards with no scan ---')
        for pack_id, total, missing in gaps:
            print(f'  {pack_id} ({len(missing)} of {total}): {", ".join(missing)}')


if __name__ == '__main__':
    main()
