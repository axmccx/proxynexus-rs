# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
Builds the two JSON files the whconquest adapter embeds, from the
warhammer_40K_conquest_card_data repo:

  whc_cards.json   one row per card, slimmed down from the OCTGN dump
  whc_packs.json   one row per pack, with the release dates in RELEASE_DATES

Conquest has no card database API, so these files are the catalog. Re-run this
after the source repo changes:

  uv run build_catalog.py ~/warhammer_40K_conquest_card_data

See README.md for the mapping rules.
"""

import os
import re
import json
import argparse
import unicodedata
from collections import Counter

GAMES_DIR = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)),
                 '..', '..', '..', 'proxynexus-core', 'src', 'games', 'whconquest')
)

# Punchboard pieces, not poker-sized cards. No scan of them can go in a
# collection, so they are left out of the catalog entirely.
NON_CARD_TYPES = {'Token', 'Skull', 'Initiative', 'Dial'}

# The planets and their tokens are filed under their own "set" in the source
# data, but they are Core Set cards -- their numbers (175-188) continue the
# Core Set's 1-174.
SET_MERGES = {'Markers and Planets': 'Core Set'}

# Typo in the source data.
SET_RENAMES = {'The Final Gamit': 'The Final Gambit'}

# Conquest's deckbuilding rule is 3 copies of a card, and the source data only
# fills in `Copies` where a card departs from that -- signature squad cards,
# and the warlords and planets that come one to a box.
DEFAULT_COPIES = 3
SINGLETON_TYPES = {'Warlord', 'Planet'}

# Release order is documented; the exact days are not, so these are the
# release months as first-of-month. They exist to sort the packs.
RELEASE_DATES = {
    'core-set': '2014-08-01',
    'the-howl-of-blackmane': '2014-10-01',
    'the-scourge': '2014-11-01',
    'gift-of-the-ethereals': '2014-12-01',
    'zogworts-curse': '2015-02-01',
    'the-threat-beyond': '2015-03-01',
    'the-descendants-of-isha': '2015-04-01',
    'champions': '2015-04-01',
    'the-great-devourer': '2015-05-01',
    'decree-of-ruin': '2015-07-01',
    'boundless-hate': '2015-08-01',
    'deadly-salvage': '2015-09-01',
    'what-lurks-below': '2015-10-01',
    'wrath-of-the-crusaders': '2015-12-01',
    'the-final-gambit': '2016-01-01',
    'legions-of-death': '2016-03-01',
    'jungles-of-nectavus': '2016-05-01',
    'unforgiven': '2016-06-01',
    'slash-and-burn': '2016-08-01',
    'searching-for-truth': '2016-09-01',
    'against-the-great-enemy': '2016-10-01',
    'the-warp-unleashed': '2016-12-01',
}


def slug(text):
    """Lowercase hyphenated id. Apostrophes are dropped rather than
    hyphenated, so "Zogwort's Curse" is zogworts-curse."""
    text = unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode()
    text = text.replace("'", '')
    return re.sub(r'[^a-z0-9]+', '-', text.lower()).strip('-')


def read_source(source_dir):
    cards = []
    for group in sorted(os.listdir(source_dir)):
        group_dir = os.path.join(source_dir, group)
        if not os.path.isdir(group_dir) or group.startswith('.'):
            continue
        for name in sorted(os.listdir(group_dir)):
            if not name.endswith('.json'):
                continue
            with open(os.path.join(group_dir, name), encoding='utf-8') as handle:
                for card in json.load(handle):
                    card['_designer'] = group
                    cards.append(card)
    return cards


def copies(card):
    if card['Type'] in SINGLETON_TYPES:
        return 1
    raw = card.get('Copies') or ''
    return int(raw) if raw.isdigit() else DEFAULT_COPIES


def build(cards):
    rows, packs = [], {}

    for card in cards:
        if card['Type'] in NON_CARD_TYPES:
            continue

        set_name = card['Set']
        set_name = SET_MERGES.get(set_name, set_name)
        set_name = SET_RENAMES.get(set_name, set_name)
        pack_code = slug(set_name)

        packs.setdefault(pack_code, set_name)
        number = card['CardNumber']

        rows.append({
            'unique_id': slug(card['Name']),
            'name': card['Name'],
            'pack_code': pack_code,
            'type': card['Type'],
            'faction': card['Faction'],
            'card_number': int(number) if number.isdigit() else None,
            'card_quantity': copies(card),
        })

    rows.sort(key=lambda row: (row['pack_code'], row['card_number'] or 0, row['unique_id']))

    pack_rows = [
        {'code': code, 'name': name, 'date_release': RELEASE_DATES.get(code)}
        for code, name in packs.items()
    ]
    pack_rows.sort(key=lambda row: (row['date_release'] is None, row['date_release'] or '', row['name']))

    return rows, pack_rows


def main():
    parser = argparse.ArgumentParser(
        description='Builds whc_cards.json and whc_packs.json for the whconquest adapter.')
    parser.add_argument('source', help='Path to a warhammer_40K_conquest_card_data checkout')
    parser.add_argument('-o', '--output', default=GAMES_DIR, help='Destination folder')
    args = parser.parse_args()

    cards = read_source(os.path.abspath(args.source))
    rows, pack_rows = build(cards)

    ids = Counter(row['unique_id'] for row in rows)
    duplicates = {key: count for key, count in ids.items() if count > 1}
    if duplicates:
        raise SystemExit(f'card ids are not unique: {duplicates}')

    os.makedirs(args.output, exist_ok=True)
    for name, data in (('whc_cards.json', rows), ('whc_packs.json', pack_rows)):
        with open(os.path.join(args.output, name), 'w', encoding='utf-8') as handle:
            json.dump(data, handle, indent=2, ensure_ascii=False)
            handle.write('\n')

    undated = [row['code'] for row in pack_rows if row['date_release'] is None]
    by_pack = Counter(row['pack_code'] for row in rows)
    print(f'{len(rows)} cards across {len(pack_rows)} packs -> {args.output}')
    print(f'dropped {len(cards) - len(rows)} non-card pieces ({", ".join(sorted(NON_CARD_TYPES))})')
    if undated:
        print(f'no release date: {", ".join(undated)}')
    for row in pack_rows:
        print(f"  {row['code']:26} {row['date_release'] or '-':12} {by_pack[row['code']]:>4} cards")


if __name__ == '__main__':
    main()
