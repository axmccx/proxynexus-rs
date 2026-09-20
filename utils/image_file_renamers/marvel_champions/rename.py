# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow"]
# ///
"""
Renames Marvel Champions card scans to the current Proxy Nexus naming
convention, resolving them against MarvelCDB.

The archive is laid out by product, then by villain, encounter set or hero, and
every file is named after its card with its fields in no fixed order:

    Expansion Campaings/Red Skull/Absorbing Man/4B_None Shall Pass_Main Scheme_1B.tiff
    Heros/Steve Rogers_Captain America/Leadership_Hawkeye_Ally_12.tiff

Filenames are hand-typed, so a card is found by looking for a catalog title in
any run of the filename's fields, then narrowed by the type, stage, side and
number the filename also carries. Scans are converted to JPEG and copied into an
output folder; the sources are never modified.

See README.md for the mapping rules and known limitations.
"""

import argparse
import difflib
import json
import os
import re
import unicodedata
import urllib.request
from collections import Counter, defaultdict

CATALOG_CACHE = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                             'marvel_champions_catalog_cache.json')

CARDS_URL = 'https://marvelcdb.com/api/public/cards/?encounter=1'
PACKS_URL = 'https://marvelcdb.com/api/public/packs/'

SOURCE_EXTS = ('.tif', '.tiff', '.png', '.jpg', '.jpeg')

# `Expansion Campaings/<folder>` and `Scenario Packs/<folder>` -> MarvelCDB pack.
PRODUCT_DIRS = {
    ('expansioncampaings', 'redskull'): 'trors',
    ('expansioncampaings', 'galaxysmostwanted'): 'gmw',
    ('expansioncampaings', 'themadtitansshadow'): 'mts',
    ('expansioncampaings', 'sinistermotives'): 'sm',
    ('expansioncampaings', 'mutantgenesis'): 'mut_gen',
    ('expansioncampaings', 'nextevolution'): 'next_evol',
    ('expansioncampaings', 'ageofapocalypse'): 'aoa',
    ('expansioncampaings', 'agendsofshield'): 'aos',
    ('expansioncampaings', 'civilwar'): 'cw',
    ('expansioncampaings', 'fearnoevil'): 'fne',
    ('scenariopacks', 'greengoblin'): 'gob',
    ('scenariopacks', 'wreckingcrew'): 'twc',
    ('scenariopacks', 'kang'): 'toafk',
    ('scenariopacks', 'thehood'): 'hood',
    ('scenariopacks', 'mojomania'): 'mojo',
    ('scenariopacks', 'synthzoidsmackdown'): 'synthezoid',
    ('scenariopacks', 'trickstertakeover'): 'tt',
}

# Packs Fantasy Flight lists as out of print, which is what `--packs` defaults
# to. The Wrecking Crew or The Once and Future Kang is also out of print, but
# which one is unconfirmed, so neither is listed.
OUT_OF_PRINT_PACKS = {
    'trors', 'gmw', 'mts', 'sm', 'mut_gen', 'next_evol',
    'gob', 'hood', 'mojo',
    'cap', 'thor', 'bkw', 'drs', 'hlk', 'wsp', 'qsv', 'scw', 'stld', 'gam', 'drax',
    'nebu', 'warm', 'valk', 'vision', 'nova', 'ironheart', 'spiderham', 'spdr',
    'cyclops', 'phoenix', 'wolv', 'storm', 'gambit', 'rogue', 'psylocke', 'x23',
    'deadpool',
}

# Top-level folders that hold no card MarvelCDB lists as its own printing.
SKIPPED_DIRS = {
    'boxart': 'Box art',
    'promo': 'Promo cards, which MarvelCDB does not list',
    'ronantheaccuser': 'Ronan printed as PDF pages rather than cards',
}

# Scans that are not a card face: card backs, which live in the adapter,
# reference cards and decklists.
NOT_A_CARD = re.compile(
    r'(^|_)(hero|encounter|villain|player|means|motive|oppurtunity|opportunity)\s*back$'
    r'|reference|ref\s*c[ar]{2}d|deck\s*(list|little)|deck$|full-art', re.IGNORECASE)

TYPE_WORDS = {
    'hero': 'hero', 'alterego': 'alter_ego', 'aterego': 'alter_ego',
    'ally': 'ally', 'event': 'event', 'support': 'support', 'resource': 'resource',
    'upgrade': 'upgrade', 'upgarde': 'upgrade',
    'villain': 'villain', 'leader': 'leader', 'leade': 'leader',
    'minion': 'minion', 'mainscheme': 'main_scheme', 'mainshowdown': 'main_scheme',
    'sidescheme': 'side_scheme', 'scheme': 'side_scheme',
    'playersidescheme': 'player_side_scheme',
    'attachment': 'attachment', 'attachement': 'attachment', 'attachemnt': 'attachment',
    'attchment': 'attachment',
    'treachery': 'treachery', 'treachry': 'treachery', 'treachy': 'treachery',
    'obligation': 'obligation', 'environment': 'environment', 'enviroment': 'environment',
    'means': 'evidence_means', 'motive': 'evidence_motive',
    'opportunity': 'evidence_opportunity', 'oppurtunity': 'evidence_opportunity',
}

ROMAN = {'i', 'ii', 'iii', 'iv', 'v'}

# The aspect a player card is written under, colour names included: the Core
# Set folders write `Red_` for Aggression and `Grey_` for Basic.
ASPECT_WORDS = {
    'aggression': 'aggression', 'agression': 'aggression', 'aggresison': 'aggression',
    'red': 'aggression',
    'justice': 'justice', 'yellow': 'justice',
    'leadership': 'leadership', 'leardship': 'leadership', 'blue': 'leadership',
    'protection': 'protection', 'green': 'protection',
    'basic': 'basic', 'grey': 'basic', 'gray': 'basic',
}

# `4.15`, `12.15`, `5.8`: the scan's place among the copies of its deck, or
# `4 copies`. Neither is part of the card's identity.
COPY_FIELD = re.compile(r'^(\d+\.\d+|\d+ copies)$', re.IGNORECASE)
# `-2`, `-denoise-sharpen` and the like, left by whatever produced the scan.
TRAILING_NOISE = re.compile(r'(-denoise-sharpen|-\d+)+$', re.IGNORECASE)

# A number with a face letter, as `4B` (an encounter set's fourth card, back
# side), `105b` or `1a` (the collector number and the face).
NUMBER_FACE = re.compile(r'^(\d+)([a-d])$', re.IGNORECASE)
# `140a-142a`: one scan standing for a run of collector numbers.
NUMBER_RANGE = re.compile(r'^(\d+)([a-d]?)\s*(?:-|to)\s*(\d+)([a-d]?)$', re.IGNORECASE)
# `Back for cards 126 to 129`, the same thing written out.
SHARED_RANGE = re.compile(r'_?back\s*for\s*cards\s*(\d+)\s*to\s*(\d+)', re.IGNORECASE)
# A stage written as a letter and a digit, as the Collector's `A1` and `A2`.
LETTER_STAGE = re.compile(r'^([a-d])(\d)$', re.IGNORECASE)

# Scoped to a single pack, so a loose threshold is safe.
FUZZY_CUTOFF = 0.82
FUZZY_MARGIN = 0.04
# Folders resolve against every hero or encounter set, so they want more.
FOLDER_CUTOFF = 0.85


def squash(text):
    """Letters and digits only.

    The archive writes an apostrophe as `_` and a slash as `!!`, so `Captain
    America_s Shield` has to collapse to the same thing as `Captain America's
    Shield`, and `SP!!dr` to the same thing as `SP//dr`.
    """
    text = unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode()
    return re.sub(r'[^a-z0-9]+', '', text.lower())


def fetch_json(url):
    request = urllib.request.Request(url, headers={'User-Agent': 'proxynexus-rs'})
    with urllib.request.urlopen(request) as response:
        return json.load(response)


def load_catalog(refresh=False):
    """Read the cached MarvelCDB catalog, downloading it first if absent."""
    if refresh or not os.path.exists(CATALOG_CACHE):
        print('Downloading MarvelCDB catalog...')
        data = {'cards': fetch_json(CARDS_URL), 'packs': fetch_json(PACKS_URL)}
        with open(CATALOG_CACHE, 'w', encoding='utf-8') as handle:
            json.dump(data, handle)
    with open(CATALOG_CACHE, encoding='utf-8') as handle:
        data = json.load(handle)
    return data['cards'], data['packs']


class Catalog:
    """Every card, indexed the ways a filename or folder addresses them."""

    def __init__(self, cards, packs=()):
        self.cards = cards
        self.by_code = {c['code']: c for c in cards}
        self.by_pack = defaultdict(list)
        for card in cards:
            self.by_pack[card['pack_code']].append(card)
        # A double-sided card is two codes, and the hidden one is the back of
        # the code that links to it.
        self.front_of = {c['linked_to_code']: c['code']
                         for c in cards if c.get('linked_to_code')}
        self.sets = defaultdict(list)
        for card in cards:
            if card.get('card_set_name'):
                self.sets[squash(card['card_set_name'])].append(card)
        self.heroes = [c for c in cards if c['type_code'] == 'hero']
        self.packs = {squash(p['name']): p['code'] for p in packs}

    def partners(self, card):
        """The other face of a card MarvelCDB lists as two linked codes."""
        other = card.get('linked_to_code') or self.front_of.get(card['code'])
        return [self.by_code[other]] if other in self.by_code else []

    def titles(self, cards):
        """Squashed title -> cards, over the given cards' fronts and backs."""
        index = defaultdict(list)
        for card in cards:
            index[squash(card['name'])].append(card)
            if card.get('back_name') and not card.get('linked_to_code'):
                index[squash(card['back_name'])].append(card)
        return index

    def hero_pack(self, folder):
        """The pack of the hero a `Heros/` folder is named after.

        Folders are `Alter Ego_Hero`, sometimes with a `MC 08_` product number
        in front and a `(u)` or `(ns)` after. Two heroes share a name as often as
        not -- there are two Spider-Men and two Black Panthers -- so the alter
        ego picks between them where the name alone does not.
        """
        name = re.sub(r'\s*\((u|ns)\)\s*$', '', folder, flags=re.IGNORECASE)
        name = re.sub(r'^MC \d+_', '', name)
        parts = [squash(p) for p in name.split('_') if p.strip()]
        hero_name, alter_ego = parts[-1], (parts[0] if len(parts) > 1 else '')
        # A hero pack is named after its hero, and MarvelCDB's hero card is not
        # always: `SP!!dr` is the pack SP//dr and the hero SP//dr Suit.
        if hero_name in self.packs:
            return self.packs[hero_name]

        def score(card):
            hero = difflib.SequenceMatcher(None, hero_name, squash(card['name'])).ratio()
            back = self.by_code.get(card.get('linked_to_code') or '')
            alter = (difflib.SequenceMatcher(None, alter_ego, squash(back['name'])).ratio()
                     if back and alter_ego else 0)
            return hero, alter

        # `Peni Parker_SP!!dr` is the hero MarvelCDB calls SP//dr Suit, so a
        # close alter ego is enough on its own.
        scored = sorted(((score(c), c) for c in self.heroes),
                        key=lambda sc: (max(sc[0]), sc[0]), reverse=True)
        (hero, alter), card = scored[0]
        return card['pack_code'] if max(hero, alter) >= FOLDER_CUTOFF else None

    def set_pack(self, folder):
        """The pack an encounter set of the given folder name is printed in."""
        key = squash(folder)
        if key not in self.sets:
            close = difflib.get_close_matches(key, list(self.sets), n=1, cutoff=FOLDER_CUTOFF)
            if not close:
                return None
            key = close[0]
        return Counter(c['pack_code'] for c in self.sets[key]).most_common(1)[0][0]


def parse_name(stem):
    """Split a filename stem into the parts that identify a card.

    Returns the fields a title is searched for in, with the type, stages,
    numbers and side the filename writes alongside it. A field that reads as a
    stage or side is also kept as a title field, since `I` and `A` begin titles
    as often as they mark a stage.
    """
    stem = TRAILING_NOISE.sub('', stem.strip())
    # `(Kate Bishop)`: which character an ally shows, which MarvelCDB leaves out
    # of the title.
    stem = re.sub(r'\s*\([^)}]*[)}]', '', stem)
    info = {'type': None, 'stages': set(), 'numbers': set(), 'faces': set(),
            'side': None, 'fields': [], 'range': None, 'aspect': None}

    match = SHARED_RANGE.search(stem)
    if match:
        info['range'] = (int(match.group(1)), int(match.group(2)))
        stem = stem[:match.start()]

    raw = [f.strip() for f in stem.split('_') if f.strip()]
    # `7 Security Clearence` has lost the `_` after its number, and
    # `All Tied Up-Attachment` the one before its type.
    if raw:
        match = re.match(r'^(\d+[a-d]?)\s+(.*)$', raw[0], re.IGNORECASE)
        if match:
            raw[:1] = [match.group(1), match.group(2)]
    split = []
    for field in raw:
        match = re.match(r'^(.+)-([a-z ]+)$', field, re.IGNORECASE)
        if match and squash(match.group(2)) in TYPE_WORDS:
            split.extend([match.group(1), match.group(2)])
        else:
            split.append(field)

    for field in split:
        low = field.lower()
        key = squash(field)
        if COPY_FIELD.match(field):
            continue
        match = NUMBER_RANGE.match(low)
        if match:
            info['range'] = (int(match.group(1)), int(match.group(3)))
            info['faces'].update(f for f in (match.group(2), match.group(4)) if f)
            continue
        if re.fullmatch(r'side\s*\d*[ab]', low) or low in ('a', 'b'):
            info['side'] = 'front' if low[-1] == 'a' else 'back'
            if low in ('a', 'b'):
                info['fields'].append(field)
            continue
        if low == 'back':
            info['side'] = 'back'
            continue
        if key in TYPE_WORDS:
            info['type'] = TYPE_WORDS[key]
            continue
        if key in ASPECT_WORDS and info['aspect'] is None:
            info['aspect'] = ASPECT_WORDS[key]
            continue
        # `Main Scheme Side A`, typed with a space where the `_` belongs.
        match = re.fullmatch(r'(.+?)\s+side\s*([ab])', low)
        if match and squash(match.group(1)) in TYPE_WORDS:
            info['type'] = TYPE_WORDS[squash(match.group(1))]
            info['side'] = 'front' if match.group(2) == 'a' else 'back'
            continue
        if key.isdigit():
            info['numbers'].add(int(key))
            continue
        match = NUMBER_FACE.match(key)
        if match:
            info['numbers'].add(int(match.group(1)))
            info['faces'].add(match.group(2).lower())
            info['stages'].add(key.upper())
            continue
        if low in ROMAN:
            info['stages'].add(low.upper())
            info['fields'].append(field)
            continue
        match = re.fullmatch(r'stage\s*([iv]+)', low)
        if match:
            info['stages'].add(match.group(1).upper())
            continue
        match = LETTER_STAGE.match(key)
        if match:
            info['stages'].add(key.upper())
            continue
        info['fields'].append(field)
    return info


def runs(fields):
    """Contiguous runs of fields, with where each ends. Longest first.

    The title sits in a different field from one filename to the next, and is
    split across two wherever the archive wrote an apostrophe as `_`.
    """
    out = []
    for size in range(len(fields), 0, -1):
        for start in range(len(fields) - size + 1):
            out.append((' '.join(fields[start:start + size]), start + size, size))
    return out


def fuzzy(info, index, sets=()):
    """The one title a misspelt filename is closest to. Returns (cards, how).

    Every run of fields is scored and the closest title over all of them wins,
    so `Protection_Defensive Stage_Upgrade` is Defensive Stance rather than the
    first run that comes near anything, which is `Protection` to Protector. A
    title of the type the filename writes is preferred.

    Two titles equally close are told apart by the encounter set the folders
    name, where only one of them is in it: `Sentinel Mark VII` sits between
    Mark VI and Mark VIII, and only VIII is Master Mold's.
    """
    if info['type']:
        typed = {t: cs for t, cs in index.items()
                 if any(c['type_code'] == info['type'] for c in cs)}
        if typed:
            cards, how = fuzzy({**info, 'type': None}, typed, sets)
            if cards:
                return cards, how

    best = None
    for text, _end, _size in runs(info['fields']):
        key = squash(text)
        if len(key) < 4:
            continue
        scored = sorted(((difflib.SequenceMatcher(None, key, title).ratio(), title)
                         for title in index), reverse=True)
        if scored and scored[0][0] >= FUZZY_CUTOFF and (best is None or scored[0][0] > best[0][0]):
            best = scored
    if best is None:
        return [], 'no title matched'

    tied = [title for score, title in best if best[0][0] - score < FUZZY_MARGIN]
    # Titles that are equally close and name the same cards are one answer.
    if all(index[title] == index[tied[0]] for title in tied):
        tied = tied[:1]
    if len(tied) > 1:
        in_set = [t for t in tied
                  if any(squash(c.get('card_set_name') or '') in sets for c in index[t])]
        if len(in_set) != 1:
            return [], f'ambiguous between {tied[0]!r} and {tied[1]!r}'
        tied = in_set
    return list(index[tied[0]]), 'spelling'


def title_matches(info, index, sets=()):
    """Cards the filename's fields name, and how they were found."""
    hits = []
    for text, end, size in runs(info['fields']):
        cards = index.get(squash(text))
        if cards:
            hits.append((end, size, cards))

    wanted = info['type']
    typed = [h for h in hits if wanted and any(c['type_code'] == wanted for c in h[2])]
    if hits and wanted and not typed:
        # `Shadowcat_Phased Strike_Event` finds the hero Shadowcat exactly and
        # the misspelt event not at all, so a title of the wrong type gives way
        # to a misspelling of the right one.
        typed_index = {t: [c for c in cs if c['type_code'] == wanted]
                       for t, cs in index.items()}
        cards, how = fuzzy(info, {t: cs for t, cs in typed_index.items() if cs}, sets)
        if cards:
            return cards, how
    if hits:
        # A folder or encounter set is often a card of its own and is written
        # ahead of the card's title, so the title ending last wins; of two
        # ending together, the longer is meant.
        pool = typed or hits
        pool.sort(key=lambda h: (h[0], h[1]), reverse=True)
        end = pool[0][0]
        if end < len(info['fields']):
            # `Hulk_Unstappable Force`: the hero is found exactly and the card
            # after it only by its spelling, and the card is the one meant.
            # A field that is itself an exact title, as the pack name `Thor` is
            # in `Leadership_Teamwork_Event_Thor`, was already weighed above.
            cards, how = fuzzy({**info, 'fields': info['fields'][end:]}, index, sets)
            exact = {squash(c['name']) for h in hits for c in h[2]}
            if cards and squash(cards[0]['name']) not in exact \
                    and (not wanted or any(c['type_code'] == wanted for c in cards)):
                return cards, how
        return list(pool[0][2]), 'title'
    return fuzzy(info, index, sets)


def by_number(cards, numbers):
    """The cards a filename's numbers point at: the collector number first,
    then the place in the encounter set, which a card with several copies
    holds a run of."""
    at = [c for c in cards if c['position'] in numbers]
    if at:
        return at
    return [c for c in cards
            if c.get('set_position') is not None
            and any(c['set_position'] <= n < c['set_position'] + (c.get('quantity') or 1)
                    for n in numbers)]


def narrow(cards, info, catalog, set_names):
    """Cut a same-title group down using what else the path and filename say.

    Each step only applies where it leaves something, so a filename that
    writes a wrong type or a stage the card does not carry still resolves.
    """
    def keep(kept):
        nonlocal cards
        if kept:
            cards = kept

    cards = list({c['code']: c for c in cards}.values())
    # `Brawler_Basic_Coup de Grace_Upgrade` names its encounter set in a field
    # of its own. A field that is the card's own title, as `Hawkeye` is in
    # `Leadership_Hawkeye_Ally`, names the card rather than a set.
    titles = {squash(c['name']) for c in cards}
    field_sets = {squash(f) for f in info['fields']} - titles
    for name in list(set_names) + sorted(field_sets):
        if len(cards) < 2:
            break
        in_set = [c for c in cards if squash(c.get('card_set_name') or '') == name]
        if in_set:
            cards = in_set
            break
    if len(cards) > 1:
        # `Leadership_Teamwork_Event_Thor`: the pack a reprint was scanned from.
        packs = {catalog.packs[f] for f in field_sets if f in catalog.packs}
        keep([c for c in cards if c['pack_code'] in packs])
    # `Hero` and `Alter-Ego` are the one type the archive never copies from the
    # other face, and it letters an identity card's faces its own way:
    # `Rocket Raccoon_Alter-Ego_29a` is MarvelCDB's 16029b.
    if len(cards) > 1 and info['type'] in ('hero', 'alter_ego'):
        keep([c for c in cards if c['type_code'] == info['type']])
    # Otherwise a collector number's face letter is the code's own suffix, and
    # outranks the type: `Team Assembled_Environment_190a` is the front, 40190a,
    # a player side scheme, whichever face's title and type it was written with.
    if len(cards) > 1 and info['faces']:
        keep([c for c in cards if c['code'][-1:].lower() in info['faces']])
    if len(cards) > 1 and info['type']:
        keep([c for c in cards if c['type_code'] == info['type']])
    if len(cards) > 1 and info['aspect']:
        keep([c for c in cards if c.get('faction_code') == info['aspect']])
    if len(cards) > 1 and info['stages']:
        keep([c for c in cards if (c.get('stage') or '').upper() in info['stages']])
    if len(cards) > 1 and info['numbers']:
        keep(by_number(cards, info['numbers']))
    if len(cards) > 1:
        back = bool(info['side'] == 'back' or (info['faces'] and info['faces'] <= {'b'}))
        keep([c for c in cards if (c['code'] in catalog.front_of) == back])
    if len(cards) > 1:
        # MarvelCDB lists most two-sided main schemes twice: once as a pair of
        # linked codes, `01097a` and `01097b`, and once more as a plain `01097`.
        # The pair is the one that describes both faces.
        paired = {c['code'][:-1] for c in cards if c['code'][-1:].isalpha()}
        keep([c for c in cards if c['code'] not in paired])
    return cards


def same_card(cards):
    """Whether every candidate is one card MarvelCDB lists under two codes."""
    return len({(c['pack_code'], c['position'], squash(c['name'])) for c in cards}) == 1 \
        and not any(c.get('hidden') for c in cards)


def output_name(card, info, catalog):
    """The Proxy Nexus filename a resolved scan is written under."""
    front = catalog.front_of.get(card['code'])
    if front:
        return f"{front}@{card['pack_code']}~back.jpg"
    if info['side'] == 'back' and card.get('back_name') is not None:
        return f"{card['code']}@{card['pack_code']}~back.jpg"
    return f"{card['code']}@{card['pack_code']}.jpg"


def other_side(name, card, catalog):
    """The filename of the other face of a two-sided card, or None."""
    if not (catalog.partners(card) or card.get('back_name') is not None):
        return None
    if name.endswith('~back.jpg'):
        return name[:-len('~back.jpg')] + '.jpg'
    return name[:-len('.jpg')] + '~back.jpg'


def folder_scope(parts, catalog):
    """The pack a file's folders place it in, or why they place it nowhere.

    Returns (pack, set names from the folders, skip reason).
    """
    keys = [squash(p) for p in parts[:-1]]
    sets = [k for k in reversed(keys) if k in catalog.sets]
    if not keys:
        return None, sets, 'Loose files at the top of the archive'
    top = keys[0]
    if top in SKIPPED_DIRS:
        return None, sets, SKIPPED_DIRS[top]
    if top == 'coreset':
        return 'core', sets, None
    if len(keys) > 1 and (top, keys[1]) in PRODUCT_DIRS:
        return PRODUCT_DIRS[(top, keys[1])], sets, None
    if top == 'heros' and len(parts) > 2:
        pack = catalog.hero_pack(parts[1])
        return pack, sets, None if pack else 'Hero folders naming no hero'
    if top == 'modularsets' and len(parts) > 2:
        pack = catalog.set_pack(parts[1])
        close = difflib.get_close_matches(keys[1], list(catalog.sets), n=1, cutoff=FOLDER_CUTOFF)
        return pack, close or sets, None if pack else 'Modular set folders naming no set'
    if top == 'aspects':
        return '*', sets, None
    return None, sets, 'Folders naming no pack'


def resolve(entry, catalog, indexes):
    """The cards one file is a scan of. Returns (cards, how)."""
    info = entry['info']
    # An `Aspects/` folder names no pack, so its cards are looked for in all of them.
    pool = catalog.cards if entry['pack'] == '*' else catalog.by_pack[entry['pack']]

    if info['range']:
        # One scan standing for a run of cards that print the same face:
        # `Sinister Experiment_Main Scheme_140a-142a` is the front of three
        # schemes whose backs differ.
        low, high = info['range']
        cards = [c for c in pool if low <= c['position'] <= high]
        cards = narrow(cards, {**info, 'numbers': set()}, catalog, entry['sets'])
        return cards, 'one scan shared by a run of cards'

    pack = entry['pack']
    if pack not in indexes:
        indexes[pack] = catalog.titles(pool)
    cards, how = title_matches(info, indexes[pack], entry['sets'])
    if cards:
        if info['side'] or info['faces']:
            # The side a filename writes outranks the title it gives: the Hydra
            # Campaign's `1B_Basic_Basic Attack Upgrade` is the back, whose own
            # title is Improved Attack Upgrade.
            cards = cards + [partner for c in cards for partner in catalog.partners(c)]
        return narrow(cards, info, catalog, entry['sets']), how

    # A title too misspelt to find (`Rocky Outpost` for Rocky Outcrop) can
    # still be the only card of its type at the number the filename gives.
    if info['type'] and info['numbers']:
        typed = [c for c in pool if c['type_code'] == info['type']]
        if entry['sets']:
            typed = [c for c in typed
                     if squash(c.get('card_set_name') or '') in entry['sets']] or typed
        cards = narrow(by_number(typed, info['numbers']), info, catalog, entry['sets'])
        if len(cards) == 1 or (cards and same_card(cards)):
            return cards, 'type and number'
    return [], how


def convert(src, dst, quality):
    """Write the scan as JPEG. The TIFFs are not all RGB."""
    from PIL import Image
    with Image.open(src) as image:
        if image.mode != 'RGB':
            image = image.convert('RGB')
        image.save(dst, 'JPEG', quality=quality, optimize=True, subsampling=0)


def collect(source, catalog, reports):
    """Walk the archive, placing every card scan in a pack."""
    entries = []
    for root, dirs, files in os.walk(source):
        dirs.sort()
        for name in sorted(files):
            if not name.lower().endswith(SOURCE_EXTS):
                continue
            path = os.path.join(root, name)
            rel = os.path.relpath(path, source)
            parts = rel.split(os.sep)
            stem = os.path.splitext(name)[0]
            pack, sets, skipped = folder_scope(parts, catalog)
            if skipped:
                reports[skipped].append(rel)
                continue
            if NOT_A_CARD.search(stem):
                reports['Card backs, reference cards and decklists'].append(rel)
                continue
            entries.append({'rel': rel, 'path': path, 'pack': pack, 'sets': sets,
                            'folder': os.path.dirname(rel), 'info': parse_name(stem)})
    return entries


def main():
    parser = argparse.ArgumentParser(
        description='Rename Marvel Champions scans to the Proxy Nexus convention.')
    parser.add_argument('input', help='The archive folder.')
    parser.add_argument('-o', '--output', default='marvel_champions_out',
                        help='Output directory.')
    parser.add_argument('--quality', type=int, default=92, help='JPEG quality (default 92).')
    parser.add_argument('--dry-run', action='store_true', help='Report without writing.')
    parser.add_argument('--refresh-catalog', action='store_true',
                        help='Re-download the MarvelCDB catalog.')
    parser.add_argument('--packs', default=','.join(sorted(OUT_OF_PRINT_PACKS)),
                        help='Pack codes to keep, comma separated. Defaults to the packs '
                             'out of print; pass "all" for every pack the archive holds.')
    parser.add_argument('--exclude', action='append', default=[], metavar='DIR',
                        help='A folder of already-named images whose cards to leave out. '
                             'Repeatable.')
    args = parser.parse_args()

    source = os.path.abspath(os.path.expanduser(args.input))
    cards, packs = load_catalog(args.refresh_catalog)
    catalog = Catalog(cards, packs)
    wanted = (set(catalog.by_pack) if args.packs == 'all'
              else {p.strip() for p in args.packs.split(',') if p.strip()})
    unknown = wanted - set(catalog.by_pack)
    if unknown:
        parser.error(f"--packs names no MarvelCDB pack: {', '.join(sorted(unknown))}")

    excluded = set()
    for folder in args.exclude:
        for name in os.listdir(os.path.expanduser(folder)):
            excluded.add(name.split('.')[0])

    reports = defaultdict(list)
    entries = collect(source, catalog, reports)
    # Scans that write their side go first, so an unmarked scan meets its
    # marked twin already placed.
    entries.sort(key=lambda e: not (e['info']['side'] or e['info']['faces']))
    indexes = {}
    written, types_of, how_counts = {}, {}, Counter()

    for entry in entries:
        cards, how = resolve(entry, catalog, indexes)
        if not cards:
            reports[f'Unmatched: {how}'].append(entry['rel'])
            continue
        packs = {c['pack_code'] for c in cards}
        if not packs & wanted:
            reports['Packs not asked for'].append(entry['rel'])
            continue
        cards = [c for c in cards if c['pack_code'] in wanted]
        if len(cards) > 1 and not entry['info']['range'] and not same_card(cards):
            choices = ', '.join(f"{c['code']}@{c['pack_code']}" for c in cards)
            reports['Several cards fit the filename'].append(f"{entry['rel']}  [{choices}]")
            continue
        for card in cards:
            name = output_name(card, entry['info'], catalog)
            other = other_side(name, card, catalog)
            unmarked = not (entry['info']['side'] or entry['info']['faces'])
            retyped = name in written and entry['info']['type'] != types_of[name]
            if name in written and (unmarked or retyped) and other and other not in written:
                # `4_The Art of Evasion_Main Scheme` beside
                # `4_The Art of Evasion_Main Scheme_1A`: the archive writes the
                # side on one face and leaves the other bare. And where it
                # writes the same side on both, as `7_Upper Manhattan` does, the
                # type it gives each tells them apart.
                reports['Scans placed on the side their twin left free'].append(
                    f"{entry['rel']}  ->  {other}")
                name = other
            if name in written:
                reports['Extra scanned copies of one card'].append(
                    f"{entry['rel']}  ->  {name}")
                continue
            written[name] = entry['path']
            types_of[name] = entry['info']['type']
            how_counts[how] += 1

    # A card is left out only when the excluded collection has every face of it
    # that this archive holds. Leaving out one face of a card and keeping the
    # other would split it across two collections, and a card needs its front
    # and its back in the same one.
    faces = defaultdict(set)
    for name in written:
        faces[name.split('.')[0].replace('~back', '')].add(name)
    for card, names in faces.items():
        if all(n.split('.')[0] in excluded for n in names):
            for name in names:
                reports['Cards an excluded collection already has'].append(
                    f'{os.path.relpath(written.pop(name), source)}  ->  {name}')
        elif any(n.split('.')[0] in excluded for n in names):
            reports['Cards the excluded collection has only one face of'].append(card)

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
