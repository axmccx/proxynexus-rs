# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow"]
# ///
"""
Renames Arkham Horror LCG card scans to the current Proxy Nexus naming
convention, resolving them against ArkhamDB.

The archive is laid out one folder per product, with scenario and encounter-set
folders under it, and every file is named after its card:

    The path to Carcosa/Mythos/2_The Last King/5_Constance Dumaine_Enemy.tif

Filenames are hand-typed and their fields are not in a fixed order, so a card is
found by looking for any catalog title inside the whole filename rather than by
reading a title out of one field. Scans are converted to JPEG and copied into an
output folder; the sources are never modified.

See README.md for the mapping rules and known limitations.
"""

import os
import re
import json
import difflib
import argparse
import unicodedata
import urllib.request
from collections import defaultdict

from PIL import Image

CATALOG_CACHE = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                             'ahlcg_catalog_cache.json')

CARDS_URL = 'https://arkhamdb.com/api/public/cards/?encounter=1'
PACKS_URL = 'https://arkhamdb.com/api/public/packs/'

SOURCE_EXTS = ('.tif', '.tiff', '.png', '.jpg', '.jpeg')

# Top-level folder -> ArkhamDB pack code. Everything in Chapter 1 is one pack per
# product; a campaign split across a deluxe box and its mythos packs would need a
# mapping a folder deeper.
PACK_DIRS = {
    'core set': 'core',
    'the path to carcosa': 'ptc',
    'return to the circle undone': 'rttcu',
    'film fatale': 'film_fatale',
}

# The archive's "Chapter 1" is its own division of the game and is not the
# Chapter 1 card pool this collection is built for: it holds Film Fatale, which
# that pool does not, and none of the other standalone scenarios, which it does.
# Only the three products in both are kept. `--packs` overrides.
CHAPTER1_PACKS = {'core', 'ptc', 'rttcu'}

# The full 22-card Major Arcana, a separate FFG product ArkhamDB does not carry.
# Seven of its names collide with Return to the Circle Undone's tarot player
# assets ("The Fool" against "The Fool - 0"), so it is skipped rather than
# matched loosely.
SKIP_DIRS = {'tarot'}

# Archive titles naming a card ArkhamDB lists under a different name. Both are
# Path to Carcosa enemies the database renamed; the scans are the 2017 printing
# and carry the collector number their code does, 95 and 96.
TITLE_FIXES = {
    ('ptc', 'maniac'): '03095',
    ('ptc', 'youngpsychopath'): '03096',
}

# Where a pack prints one card at two levels, the archive holds a single scan of
# it and the filename says nothing about which level that is. Guessing the lower
# is wrong four times out of five here, so the level is read off the collector
# number printed on the scan and recorded. Anything not covered is reported
# rather than picked.
LEVEL_FIXES = {
    ('core', 'beatcop'): '01028',
    ('core', 'magnifyingglass'): '01040',
    ('core', 'leodeluca'): '01054',
    ('core', 'blindinglight'): '01069',
    ('core', 'lucky'): '01080',
}

# `_Side A` / `_Side B`, the archive's usual way of writing a double-sided card.
SIDE = re.compile(r'_Side\s*([AB])$', re.I)

# Acts and agendas write their side into the label instead: `Act 1A_Trapped` and
# `Act 1B_The Door on the Floor` are the two faces of one card, and each face
# carries its own title.
ACT = re.compile(r'^(act|agenda)\s*(\d+)\s*([ab])(?![a-z0-9])', re.I)

# `_L3`, the level of an upgradable player card. Not part of the title.
LEVEL = re.compile(r'_L\d$', re.I)

# A leading `12_`, `5-7_`, `16 to 18_` or `9. `: the card's place in its folder.
# It counts copies as often as it counts distinct cards, so it is not a card
# number -- but it does order the folder, which is what tells two cards of the
# same name apart.
POSITION = re.compile(r'^(\d+)(?:\s*(?:-|to)\s*(\d+))?[_.]\s*')

# Mini investigator cards. ArkhamDB has no code for them and they are not poker
# sized, so they are reported and left.
MINI = re.compile(r'(^|_)mini(_|$)', re.I)

# Scans of a card back rather than a card face. These live in the adapter.
BACK_FILES = {'tarotback', 'back', 'invistigatorback', 'mythosback'}

# One scan serving as the back of several cards at once: either the positions it
# names, as `4-5_Location Backs.tif` does, or every card of the title it names,
# as `Location_Arkham Woods_Back.tif` does for all six Arkham Woods.
SHARED_BACK = re.compile(r'(back\s*for\s*locations|[_\s]backs?$)', re.I)

# A title has to be at least this long before a substring hit counts, so a short
# one cannot match a fragment of an unrelated filename.
MIN_SUBSTRING = 6

# Scoped to a single pack, so a loose threshold is safe. The archive misspells
# freely: `Obscuring For`, `Handman_s Brook`, `Brazier Enchntment`.
FUZZY_CUTOFF = 0.82
FUZZY_MARGIN = 0.04

# Types printed landscape. Their scans are stored upright in a portrait frame,
# rotated a quarter turn -- but not all the same way, so they are reported for a
# later orientation pass rather than turned here on a guess.
LANDSCAPE_TYPES = {'investigator', 'act', 'agenda'}

TYPE_WORDS = {
    'asset': 'asset', 'event': 'event', 'skill': 'skill', 'enemy': 'enemy',
    'treachery': 'treachery', 'trachery': 'treachery',
    'location': 'location', 'lacation': 'location',
    'act': 'act', 'agenda': 'agenda', 'story': 'story', 'setup': 'scenario',
}


def squash(text):
    """Letters and digits only.

    The archive writes an apostrophe as `_`, so `Skids O_toole` has to collapse
    to the same thing as `"Skids" O'Toole`, and separators have to vanish with it
    for a title to be findable inside a filename.
    """
    text = unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode()
    return re.sub(r'[^a-z0-9]+', '', text.lower())


def fetch_json(url):
    request = urllib.request.Request(url, headers={'User-Agent': 'proxynexus-rs'})
    with urllib.request.urlopen(request) as response:
        return json.load(response)


def load_catalog(refresh=False):
    """Read the cached ArkhamDB catalog, downloading it first if absent."""
    if refresh or not os.path.exists(CATALOG_CACHE):
        print('Downloading ArkhamDB catalog...')
        data = {'cards': fetch_json(CARDS_URL), 'packs': fetch_json(PACKS_URL)}
        with open(CATALOG_CACHE, 'w', encoding='utf-8') as handle:
            json.dump(data, handle)
    with open(CATALOG_CACHE, encoding='utf-8') as handle:
        data = json.load(handle)
    return data['cards'], data['packs']


class Catalog:
    """The cards of one pack, indexed the ways a filename addresses them."""

    def __init__(self, cards, pack_code):
        self.code = pack_code
        self.cards = [c for c in cards if c['pack_code'] == pack_code]
        self.by_code = {c['code']: c for c in self.cards}
        self.by_title = defaultdict(list)
        self.by_back_title = defaultdict(list)
        for card in self.cards:
            self.by_title[squash(card['name'])].append(card)
            if card.get('back_name'):
                self.by_back_title[squash(card['back_name'])].append(card)
        for index in (self.by_title, self.by_back_title):
            for group in index.values():
                group.sort(key=lambda c: c['code'])
        # A card whose back is printed as a card of its own -- "Constance
        # Dumaine" backed by "Engram's Oath" -- gives ArkhamDB two codes. The
        # back is flagged hidden, and a scan pair is one card, so a match on the
        # back resolves to the front it is linked from.
        self.front_of = {c['linked_to_code']: c['code']
                         for c in self.cards if c.get('linked_to_code')}


def parse_name(stem):
    """Split a filename stem into the parts that identify a card.

    Returns a dict holding the body a title is searched for in, the side, the
    folder position or position range, and the act/agenda label if there is one.
    """
    info = {'mini': bool(MINI.search(stem)), 'side': None, 'act': None,
            'first': None, 'last': None}
    match = SIDE.search(stem)
    if match:
        info['side'] = 'front' if match.group(1).upper() == 'A' else 'back'
        stem = stem[:match.start()]
    stem = LEVEL.sub('', stem)
    match = POSITION.match(stem)
    if match:
        info['first'] = int(match.group(1))
        info['last'] = int(match.group(2) or match.group(1))
        stem = stem[match.end():]
    match = ACT.match(stem)
    if match:
        info['act'] = (match.group(1).lower(), int(match.group(2)))
        # Only where the filename does not say the side outright. Return to the
        # Circle Undone writes both, and writes them disagreeing: its
        # `2_Act 4a_A Circle Unbroken_Side B` is the back, whatever the `4a`
        # says, and reading the label over the suffix loses every such back.
        if info['side'] is None:
            info['side'] = 'front' if match.group(3).upper() == 'A' else 'back'
    info['body'] = stem
    return info


def type_of(stem):
    """The card type a filename names, as an ArkhamDB type_code."""
    for field in reversed(stem.split('_')):
        code = TYPE_WORDS.get(field.strip().lower())
        if code:
            return code
    return None


def title_runs(stem):
    """Contiguous runs of the stem's fields, longest first.

    The title sits in a different field from one filename to the next, and is
    sometimes split across two by an apostrophe written as `_`, so every run is
    offered rather than one field guessed at.
    """
    fields = [f for f in stem.split('_') if f.strip()]
    runs = []
    for size in range(len(fields), 0, -1):
        for start in range(len(fields) - size + 1):
            runs.append(' '.join(fields[start:start + size]))
    return runs


def candidates(stem, catalog):
    """Every card in the pack the filename could name. Returns (cards, how)."""
    haystack = squash(stem)

    for (pack, title), code in TITLE_FIXES.items():
        if pack == catalog.code and title in haystack:
            return [catalog.by_code[code]], 'a name ArkhamDB has since changed'

    hits = [(haystack.rindex(title) + len(title), len(title), cards)
            for title, cards in catalog.by_title.items()
            if len(title) >= MIN_SUBSTRING and title in haystack]
    if hits:
        # The title ending last in the filename wins. A scenario or encounter
        # set is often a card in its own right and its name is written ahead of
        # the card's, so `16_The Devourer Below_Umordhoth_Enemy` names Umordhoth.
        # Two titles ending together are one inside the other, and the longer is
        # meant: `Basic Weakness_Silver Twilight Acolyte_Enemy` is not Acolyte.
        hits.sort(key=lambda hit: (hit[0], hit[1]), reverse=True)
        return narrow(hits[0][2], stem, catalog), 'title'

    for run in title_runs(stem):
        squashed = squash(run)
        if not squashed:
            continue
        scored = sorted(((difflib.SequenceMatcher(None, squashed, title).ratio(), title)
                         for title in catalog.by_title), reverse=True)
        if not scored or scored[0][0] < FUZZY_CUTOFF:
            continue
        if len(scored) > 1 and scored[0][0] - scored[1][0] < FUZZY_MARGIN:
            return [], f'ambiguous between {scored[0][1]!r} and {scored[1][1]!r}'
        return narrow(catalog.by_title[scored[0][1]], stem, catalog), 'spelling'

    return [], 'no title matched'


def back_cards(stem, catalog):
    """Cards whose printed back is the one this filename names.

    A back is often titled in its own right rather than after the card it
    belongs to, and several cards can share one: `Lobby Doorway` is the back of
    all three Curtain Call locations behind it, while `A City Aflame` is the
    back of exactly one of the three copies of The Stranger. Matching on the
    back title places both without a rule for either.
    """
    haystack = squash(stem)
    hits = [(haystack.rindex(title) + len(title), len(title), cards)
            for title, cards in catalog.by_back_title.items()
            if len(title) >= MIN_SUBSTRING and title in haystack]
    if hits:
        hits.sort(key=lambda hit: (hit[0], hit[1]), reverse=True)
        return hits[0][2]
    for run in title_runs(stem):
        squashed = squash(run)
        if not squashed:
            continue
        scored = sorted(((difflib.SequenceMatcher(None, squashed, title).ratio(), title)
                         for title in catalog.by_back_title), reverse=True)
        # Tighter than the front-title pass: a back title is only reached for
        # after the front titles have all missed, so there is no folder scoping
        # left to make a loose threshold safe.
        if scored and scored[0][0] >= 0.9:
            return catalog.by_back_title[scored[0][1]]
    return []


def narrow(cards, stem, catalog):
    """Cut a same-name group down using what else the filename says.

    Carcosa prints four Whispers in Your Head, told apart only by a subtitle, and
    the archive writes that subtitle into the filename. Type does the same job
    for Constance Dumaine, who is both an enemy and an asset in the pack. Two
    levels of one player card have neither, and are covered by LEVEL_FIXES.
    """
    if len(cards) < 2:
        return cards
    haystack = squash(stem)
    named = [c for c in cards if c.get('subname') and squash(c['subname']) in haystack]
    if named:
        return named
    wanted = type_of(stem)
    if wanted:
        typed = [c for c in cards if c.get('type_code') == wanted]
        if typed:
            cards = typed
    if len(cards) > 1:
        fixed = LEVEL_FIXES.get((catalog.code, squash(cards[0]['name'])))
        if fixed:
            return [catalog.by_code[fixed]]
    return cards


def levels_differ(cards):
    """Whether a same-name group is one card printed at several levels."""
    return len({c.get('xp') for c in cards}) > 1


def group_key(info):
    """What pairs a card's two scans together.

    An act's faces share its label, a numbered card's share its position, and
    anything else shares the body of its filename -- which is the whole stem
    with `_Side A` taken off, so `Location_Attic` and `Location_Attic_Side B`
    meet there.
    """
    if info['act']:
        return ('act',) + info['act']
    if info['first'] is not None:
        return ('pos', info['first'])
    return ('name', squash(info['body']))


def resolve_folder(entries, catalog):
    """Assign cards to one folder's files.

    Where several groups name the same card, as the two Heretics in Return to
    the Wages of Sin do, the folder's order decides which is which: nothing else
    in the filenames separates them.
    """
    # A back that names itself places itself, however many cards share it.
    named_backs, entries = [], list(entries)
    for entry in list(entries):
        if entry['side'] != 'back' or candidates(entry['body'], catalog)[0]:
            continue
        cards = back_cards(entry['body'], catalog)
        if cards:
            named_backs.append((entry, cards))
            entries.remove(entry)

    shared_backs = [e for e in entries
                    if SHARED_BACK.search(e['stem'])
                    or (e['side'] == 'back' and e['last'] != e['first'])]
    entries = [e for e in entries if e not in shared_backs]

    groups = defaultdict(list)
    for entry in entries:
        groups[group_key(entry)].append(entry)

    claims, problems = defaultdict(list), []
    for key, group in sorted(groups.items(), key=lambda kv: sort_key(kv[0])):
        cards, how = [], 'no title matched'
        # A group's front usually names the card, but a shared position can put
        # an unnamed file first, so every member is tried before giving up.
        for entry in sorted(group, key=lambda e: e['side'] == 'back'):
            cards, how = candidates(entry['body'], catalog)
            if cards:
                break
        if not cards:
            problems.extend((e, how) for e in group)
            continue
        claims[tuple(c['code'] for c in cards)].append((key, group, how))

    resolved = [(entry, card, 'a back naming itself')
                for entry, cards in named_backs for card in cards]
    extras, unlevelled = [], []
    for codes, staked in claims.items():
        cards = sorted((catalog.by_code[c] for c in codes), key=lambda c: c['code'])
        staked.sort(key=lambda item: sort_key(item[0]))
        if len(staked) < len(cards) and levels_differ(cards):
            for _key, group, _how in staked:
                for entry in group:
                    unlevelled.append((entry, cards))
        # One scan standing in for a run of cards: Curtain Call prints three
        # copies of The Stranger sharing a front, and the archive scans that
        # front once under the range `5-7`. Their backs differ and are placed by
        # back title, above.
        ranged = [g for g in staked if any(e['last'] != e['first'] for e in g[1])]
        backs_only = [g for g in staked if all(e['side'] == 'back' for e in g[1])]
        if len(staked) == 1 and (ranged or backs_only) and len(cards) > 1:
            _key, group, how = staked[0]
            for entry in group:
                resolved.extend((entry, card, how) for card in cards)
            continue
        for index, (_key, group, how) in enumerate(staked):
            if index >= len(cards):
                extras.extend((e, cards[-1]['name']) for e in group)
                continue
            for entry in group:
                resolved.append((entry, cards[index], how))
    return resolved, problems, extras, unlevelled, shared_backs


def sort_key(key):
    """Order group keys so numbered ones run in folder order."""
    kind = key[0]
    if kind == 'pos':
        return (0, key[1], '')
    if kind == 'act':
        return (1, key[2], key[1])
    return (2, 0, key[1])


def convert(src, dst, quality):
    """Write the scan as JPEG. The TIFFs are RGB here but need not be."""
    with Image.open(src) as image:
        if image.mode != 'RGB':
            image = image.convert('RGB')
        image.save(dst, 'JPEG', quality=quality, optimize=True, subsampling=0)


def collect(source, reports, wanted):
    """Walk the archive, grouping files by the folder they sit in."""
    folders = defaultdict(list)
    for root, _dirs, files in os.walk(source):
        for name in sorted(files):
            if not name.lower().endswith(SOURCE_EXTS):
                continue
            path = os.path.join(root, name)
            rel = os.path.relpath(path, source)
            parts = rel.split(os.sep)
            pack = PACK_DIRS.get(parts[0].lower())
            if pack is None:
                reports['Folders naming no pack'].append(rel)
                continue
            if pack not in wanted:
                reports['Products outside the Chapter 1 card pool'].append(rel)
                continue
            if any(part.lower() in SKIP_DIRS for part in parts[:-1]):
                reports['The Major Arcana tarot deck, which ArkhamDB does not carry'].append(rel)
                continue
            stem = os.path.splitext(name)[0]
            if squash(stem) in BACK_FILES:
                reports['Card backs, which live in the adapter'].append(rel)
                continue
            info = parse_name(stem)
            if info['mini']:
                reports['Mini investigator cards, which ArkhamDB has no code for'].append(rel)
                continue
            info.update(rel=rel, path=path, stem=stem)
            folders[(pack, os.path.dirname(rel))].append(info)
    return folders


def main():
    parser = argparse.ArgumentParser(
        description='Rename Arkham Horror LCG scans to the Proxy Nexus convention.')
    parser.add_argument('input', help="The archive's Chapter 1 folder.")
    parser.add_argument('-o', '--output', default='ahlcg_out', help='Output directory.')
    parser.add_argument('--quality', type=int, default=92, help='JPEG quality (default 92).')
    parser.add_argument('--dry-run', action='store_true', help='Report without writing.')
    parser.add_argument('--refresh-catalog', action='store_true',
                        help='Re-download the ArkhamDB catalog.')
    parser.add_argument('--packs', default=','.join(sorted(CHAPTER1_PACKS)),
                        help='Pack codes to keep, comma separated. Defaults to the three the '
                             'archive shares with the Chapter 1 card pool; pass "all" for every '
                             'product the archive holds.')
    args = parser.parse_args()

    source = os.path.abspath(os.path.expanduser(args.input))
    wanted = (set(PACK_DIRS.values()) if args.packs == 'all'
              else {p.strip() for p in args.packs.split(',') if p.strip()})
    unknown = wanted - set(PACK_DIRS.values())
    if unknown:
        parser.error(f"--packs names no product in the archive: {', '.join(sorted(unknown))}")
    cards, _packs = load_catalog(args.refresh_catalog)
    catalogs = {code: Catalog(cards, code) for code in wanted}

    reports = defaultdict(list)
    folders = collect(source, reports, wanted)
    written, how_counts = {}, defaultdict(int)

    for (pack, _folder), entries in sorted(folders.items()):
        catalog = catalogs[pack]
        resolved, problems, extras, unlevelled, shared_backs = resolve_folder(entries, catalog)
        for entry, reason in problems:
            reports[f'Unmatched: {reason}'].append(entry['rel'])
        for entry, choices in unlevelled:
            levels = ', '.join(f"{c['code']} (xp {c.get('xp')})" for c in choices)
            reports['Level not decidable from the filename; add a LEVEL_FIXES entry'].append(
                f"{entry['rel']}  [{levels}]")
        for entry, name in extras:
            reports['Extra scanned copies of one card'].append(f"{entry['rel']}  ({name})")

        at_position, of_title = {}, defaultdict(list)
        for entry, card, how in resolved:
            # A hidden back resolves to the front it is linked from, and is that
            # front's `~back` whatever the filename called its side.
            code = catalog.front_of.get(card['code'], card['code'])
            suffix = '~back' if (code != card['code'] or entry['side'] == 'back') else ''
            name = f'{code}@{pack}{suffix}.jpg'
            if not suffix:
                of_title[squash(card['name'])].append(code)
                if entry['first'] is not None:
                    at_position[entry['first']] = code
            if name in written:
                reports['Extra scanned copies of one card'].append(
                    f"{entry['rel']}  ->  {name}")
                continue
            written[name] = entry['path']
            how_counts[how] += 1
            if card['type_code'] in LANDSCAPE_TYPES:
                reports['Landscape cards, stored a quarter turn out'].append(name)

        # A shared back names the positions it covers, and those positions were
        # just resolved to cards.
        for entry in shared_backs:
            if entry['first'] is not None:
                codes = [at_position[p] for p in range(entry['first'], entry['last'] + 1)
                         if p in at_position]
            else:
                cards_named, _how = candidates(SHARED_BACK.sub('', entry['body']), catalog)
                codes = of_title.get(squash(cards_named[0]['name']), []) if cards_named else []
            hit = False
            for code in codes:
                name = f'{code}@{pack}~back.jpg'
                if name not in written:
                    written[name] = entry['path']
                    how_counts['a back shared by several cards'] += 1
                hit = True
            if not hit:
                reports['Unmatched: shared back covering no resolved card'].append(entry['rel'])

    print(f'{len(written)} files resolved')
    for how, count in sorted(how_counts.items()):
        print(f'  {count:5}  matched by {how}')
    for heading in sorted(reports):
        print(f'\n{heading} ({len(reports[heading])}):')
        for line in reports[heading]:
            print(f'  {line}')

    if args.dry_run:
        return
    os.makedirs(args.output, exist_ok=True)
    for name, path in sorted(written.items()):
        convert(path, os.path.join(args.output, name), args.quality)
    print(f'\nwrote {len(written)} files to {args.output}')


if __name__ == '__main__':
    main()
