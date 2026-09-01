# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
Builds the Call of Cthulhu catalog the `coclcg` adapter embeds, from the
community collection spreadsheet on BoardGameGeek.

    uv run build_catalog.py ~/Downloads/CoC_card__list_\\(collection\\).csv

Writes `coc_cards.json` and `coc_packs.json` into
`proxynexus-core/src/games/coclcg/`.

See README.md for where the spreadsheet comes from and which of its values are
corrected on the way through.
"""

import os
import re
import csv
import json
import argparse
import unicodedata
from collections import defaultdict

OUTPUT_DIR = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)),
                 '..', '..', '..', 'proxynexus-core', 'src', 'games', 'coclcg')
)

# The first two lines are a merged group header ("Release", "Catalogue", ...)
# over the real column names.
HEADER_ROW = 1

# Release dates the spreadsheet does not carry. It dates every set from
# Summons of the Deep onwards; for the Core Set and the six Forgotten Lore
# packs it lists first, the date columns hold the cycle and pack index instead.
#
# The Core Set is documented as October 2008. Forgotten Lore is not the 2008
# cycle its position suggests: it is the revised edition, reprinted in 2011 as
# one set with continuous numbering, which is the numbering the spreadsheet
# carries and the printing the scans are -- their copyright line reads 2011.
# The months below spread that cycle across 2011 to hold its order. Only the
# order is documented, so read these six as "in this order, around here"
# rather than as exact days.
RELEASE_DATES = {
    'core-set': '2008-10-01',
    'spawn-of-madness': '2011-06-01',
    'kingsport-dreams': '2011-07-01',
    'conspiracies-of-chaos': '2011-08-01',
    'dunwich-denizens': '2011-09-01',
    'the-mountains-of-madness': '2011-10-01',
    'ancient-horrors': '2011-11-01',
}

# The promos are a product line rather than a set, released one at a time over
# the game's life, so the pack carries no date and sorts last.
PROMOS_PACK = ('promos', 'Promos')

# Every promo scan in the archive is an alternate art of a card that is already
# in a pack, identified here by (title, subtitle) as the spreadsheet spells it
# after TITLE_FIXES. Each is checked to name exactly one card.
PROMOS = [
    ('Azathoth', 'The Blind Idiot God'),
    ('Clover Club Torch Singer', ''),
    ('Cthulhu', "Lord of R'lyeh"),
    ('Cthuloid Spawn', ''),
    ('Daybreak!', ''),
    ('Dreamlands Fanatic', ''),
    ('John Henry Price', 'Cold and Paternal'),
    ('Laboratory Assistant', ''),
    ('Power Drain', ''),
    ('Snow Graves', ''),
    ('Terrors in the Dark', ''),
    ('The Night', 'Darkness Incarnate'),
    ('Twilight Gate', ''),
    ('Uroborus', 'Fang of Yig'),
    ('Voice of the Jungle', ''),
]

# Titles the spreadsheet misspells, read off the scans of the cards themselves.
# `Student Archaelogist` is not a typo on this side: the printed card spells it
# that way, and the spreadsheet is the one that corrects it.
TITLE_FIXES = {
    'Aliki Zona Uperetria': 'Aliki Zoni Uperetria',
    'Archeological Dig Site': 'Archaeological Dig Site',
    'Atrtifact of the Loct Cities': 'Artifact of the Lost Cities',
    'Bord of Directors': 'Board of Directors',
    'Dark Sarcophagi': 'Dark Sarcophagus',
    'Demented Phrenelogist': 'Demented Phrenologist',
    'Doppleganger': 'Doppelgänger',
    'Expendible Muscle': 'Expendable Muscle',
    'Gateway Vehicle': 'Getaway Vehicle',
    'The Greatest Fear…': 'The Greatest Fear',
    'Giant Albino Penguins': 'Giant Albino Penguin',
    'Hasur': 'Hastur',
    'Inter-dimensional Transponder': 'Inter-dimensional Transporter',
    "Johhy V's Dame": "Johnny V's Dame",
    'Master Artificier': 'Master Artificer',
    'Out of Dhe Darkness': 'Out of the Darkness',
    'Police Hedquarters': 'Police Headquarters',
    'Rich Window': 'Rich Widow',
    'Rumormil': 'Rumormill',
    'Sirens of Hell': 'The Sirens of Hell',
    'Stalking Around': 'Stalking Hound',
    'Student Archaeologist': 'Student Archaelogist',
    'Suprising Find': 'Surprising Find',
    'Theif for Hire': 'Thief for Hire',
}

# Two cards where the spreadsheet ran the subtitle into the title.
SUBTITLE_FIXES = {
    'Beneath the Surface Eureka!': ('Beneath the Surface', 'Eureka!'),
    'The Rays of Dawn Cleansing Light': ('The Rays of Dawn', 'Cleansing Light'),
}

# Card numbers the spreadsheet has wrong, read off the number printed on each
# card ("F 147" in the band under the art). Both runs are in the Core Set: the
# supports at 141-147 are rotated by one, and the ten story cards are in a
# different order entirely.
POSITION_FIXES = {
    'core-set': {
        'Moving the Scenery': 147,
        'Mystic Bounty Hunter': 141,
        'Freelance Agent': 142,
        '.45 Pistols': 143,
        'Political Demonstration': 144,
        'Overzealous Initiate': 145,
        'Arkham Asylum': 146,
        'Rotting Away': 162,
        'Ancient Apocrypha': 159,
        'Nowhere to Hide': 156,
        'Frozen in Time': 163,
        'The Shadow out of Time': 157,
        'Dreamwalkers': 164,
        'Through the Gates': 161,
        'Opening Night': 158,
    },
}

# Story cards are the only ones not printed with the standard card back, and
# each of the three products holding them has a back of its own.
STORY_TYPE = 'Story'
STANDARD_BACK_GROUP = 'card'


def slug(text):
    """Lowercase hyphenated id."""
    text = unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode()
    text = text.replace("'", '')
    return re.sub(r'[^a-z0-9]+', '-', text.lower()).strip('-')


def read_rows(csv_path):
    with open(csv_path, encoding='utf-8-sig') as handle:
        rows = list(csv.reader(handle))
    header = rows[HEADER_ROW]
    return [dict(zip(header, row)) for row in rows[HEADER_ROW + 1:] if any(row)]


def clean_title(row):
    """The title and subtitle as the printed card spells them."""
    title = TITLE_FIXES.get(row['Title'].strip(), row['Title'].strip())
    subtitle = row['Subtitle'].strip()
    title, subtitle = SUBTITLE_FIXES.get(title, (title, subtitle))
    return title, subtitle


def release_date(rows, pack_code):
    """The set's release date, or None where neither source has one."""
    day, month, year = rows[0]['DD'].strip(), rows[0]['MM'].strip(), rows[0]['YY'].strip()
    if len(year) == 4:
        return f'{int(year):04d}-{int(month):02d}-{int(day or 1):02d}'
    return RELEASE_DATES.get(pack_code)


def disambiguate(cards):
    """Card ids and display titles, extended only where a title is shared.

    Most cards can go by their title alone. Where two cards share one -- the
    nine Necronomicons, the three Cthulhus -- the subtitle is added, and where
    that is still not enough the faction is. The card with no subtitle keeps
    the bare title, which is what `Hastur` and `Hastur (Lord of Carcosa)` are
    called anyway.

    A suffix is written into the display title too, as `The Necronomicon (Al
    Azif)`, the form the card store already reads as a title carrying a
    distinguishing suffix.
    """
    def grouped(group, key):
        by_key = defaultdict(list)
        for card in group:
            by_key[key(card)].append(card)
        return by_key.values()

    def name(card, suffix):
        card['id'] = f"{slug(card['title'])}-{slug(suffix)}" if suffix else slug(card['title'])
        card['display_title'] = f"{card['title']} ({suffix})" if suffix else card['title']

    for sharing_title in grouped(cards, lambda c: c['title'].lower()):
        if len(sharing_title) == 1:
            name(sharing_title[0], '')
            continue

        for sharing_subtitle in grouped(sharing_title, lambda c: c['subtitle'].lower()):
            if len(sharing_subtitle) == 1:
                name(sharing_subtitle[0], sharing_subtitle[0]['subtitle'])
                continue
            for card in sharing_subtitle:
                name(card, f"{card['subtitle']} {card['faction']}".strip())


def build(csv_path):
    rows = read_rows(csv_path)

    packs = []
    by_pack = defaultdict(list)
    for row in rows:
        by_pack[row['Set Name'].strip()].append(row)

    pack_of_row = {}
    for name, pack_rows in by_pack.items():
        code = slug(name)
        packs.append({'code': code, 'name': name, 'date_release': release_date(pack_rows, code)})
        for row in pack_rows:
            pack_of_row[id(row)] = code
    packs.append({'code': PROMOS_PACK[0], 'name': PROMOS_PACK[1], 'date_release': None})

    # One entry per printed card, merged into one catalog card further down.
    printings = []
    for row in rows:
        title, subtitle = clean_title(row)
        pack_code = pack_of_row[id(row)]
        number = POSITION_FIXES.get(pack_code, {}).get(title, int(row['ID']))
        printings.append({
            'title': title,
            'subtitle': subtitle,
            'faction': row['Faction'].strip(),
            'type': row['Type'].strip(),
            'pack_code': pack_code,
            'number': number,
            'quantity': int(row['Rarity']),
        })

    printed = {(p['pack_code'], p['title']) for p in printings}
    stale = [f'{pack}/{title}' for pack, fixes in POSITION_FIXES.items()
             for title in fixes if (pack, title) not in printed]
    if stale:
        raise SystemExit(f"POSITION_FIXES names cards that are not in their pack: {stale}")

    # A card is one title, subtitle, faction and type; a reprint is a second
    # printing of it. Only three cards in the game were ever reprinted.
    cards = {}
    for printing in printings:
        key = (printing['title'].lower(), printing['subtitle'].lower(),
               printing['faction'], printing['type'])
        card = cards.setdefault(key, {
            'title': printing['title'],
            'subtitle': printing['subtitle'],
            'faction': printing['faction'],
            'type': printing['type'],
            'versions': [],
        })
        card['versions'].append({
            'pack_code': printing['pack_code'],
            'number': printing['number'],
            'quantity': printing['quantity'],
        })

    cards = list(cards.values())
    disambiguate(cards)

    add_promos(cards)

    for card in cards:
        if card['type'] == STORY_TYPE:
            card['back_group'] = f"story-{card['versions'][0]['pack_code']}"
        else:
            card['back_group'] = STANDARD_BACK_GROUP

    out = [{
        'unique_id': card['id'],
        'name': card['display_title'],
        'subtitle': card['subtitle'] or None,
        'type': card['type'],
        'faction': card['faction'],
        'back_group': card['back_group'],
        'versions': sorted(card['versions'], key=lambda v: (v['pack_code'], v['number'] or 0)),
    } for card in sorted(cards, key=lambda c: c['id'])]

    return packs, out


def add_promos(cards):
    """Give each promo scan's card a printing in the promos pack."""
    by_key = defaultdict(list)
    for card in cards:
        by_key[(card['title'].lower(), card['subtitle'].lower())].append(card)

    for title, subtitle in PROMOS:
        matches = by_key.get((title.lower(), subtitle.lower()), [])
        if len(matches) != 1:
            raise SystemExit(
                f"promo '{title}' names {len(matches)} cards; PROMOS must name exactly one")
        matches[0]['versions'].append(
            {'pack_code': PROMOS_PACK[0], 'number': None, 'quantity': 1})


def check(packs, cards):
    """Catches a fix table that has gone stale against the spreadsheet."""
    pack_codes = {pack['code'] for pack in packs}

    for pack_code in POSITION_FIXES:
        if pack_code not in pack_codes:
            raise SystemExit(f"POSITION_FIXES names pack '{pack_code}', which is not in the CSV")

    ids = [card['unique_id'] for card in cards]
    duplicates = {i for i in ids if ids.count(i) > 1}
    if duplicates:
        raise SystemExit(f"duplicate card ids: {sorted(duplicates)}")

    for card in cards:
        for version in card['versions']:
            if version['pack_code'] not in pack_codes:
                raise SystemExit(
                    f"{card['unique_id']} is in pack '{version['pack_code']}', which is not a pack")


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('csv', help="The collection spreadsheet, converted to CSV.")
    parser.add_argument('-o', '--output', default=OUTPUT_DIR,
                        help="Where to write the two JSON files.")
    args = parser.parse_args()

    packs, cards = build(args.csv)
    check(packs, cards)

    os.makedirs(args.output, exist_ok=True)
    for name, data in (('coc_packs.json', packs), ('coc_cards.json', cards)):
        path = os.path.join(args.output, name)
        with open(path, 'w', encoding='utf-8') as handle:
            json.dump(data, handle, indent=2, ensure_ascii=False)
            handle.write('\n')
        print(f"Wrote {path}")

    versions = sum(len(card['versions']) for card in cards)
    print(f"{len(packs)} packs, {len(cards)} cards, {versions} printings")


if __name__ == '__main__':
    main()
