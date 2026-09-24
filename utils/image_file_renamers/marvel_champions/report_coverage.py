# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
Writes what a Marvel Champions collection holds and what it misses.

Files, meant to be shared with whoever made the source:

    marvel-champions-collections.md              the state of the collection
    marvel-champions-collection-differences.csv  one row per set
    <collection>-missing.csv                     one row per card face it lacks

Counts come from the collection folder and from MarvelCDB, so it reads the
finished collection rather than rebuilding anything. More than one folder can
be passed, and each file then says which of them holds a face.
"""

import argparse
import csv
import os

from catalog import OUT_OF_PRINT_PACKS, load_catalog


def printed_cards(cards):
    """The cards printed as cards of their own, as the adapter counts them."""
    backs = {c['linked_to_code'] for c in cards if c.get('linked_to_code')}
    paired = {c['code'][:-1] for c in cards
              if c.get('linked_to_code') and c['code'].endswith('a')}
    def printed(card):
        if card['hidden']:
            # A hidden face is the back of the card linking to it, unless
            # nothing links to it, which leaves it a card of its own.
            return card.get('linked_to_code') and card['code'] not in backs
        # MarvelCDB lists most two-sided main schemes a second time under a
        # plain code beside the linked pair; the pair describes both faces.
        return card['code'] not in paired

    return [c for c in cards if printed(c)]


def two_faced(card):
    return bool(card.get('linked_to_code')) or card.get('back_name') is not None


def wanted_faces(cards):
    """Every image an out-of-print pack needs: a front each, and a back per face."""
    faces = {}
    for card in cards:
        if card['pack_code'] not in OUT_OF_PRINT_PACKS:
            continue
        slot = f"{card['code']}@{card['pack_code']}"
        faces[slot] = (card, 'front')
        if two_faced(card):
            faces[f'{slot}~back'] = (card, 'back')
    return faces


def slots(folder):
    """The card faces a collection folder holds, by `code@pack[~back]`."""
    return {name.split('.')[0] for name in os.listdir(folder)}


def write_csv(path, rows):
    with open(path, 'w', newline='', encoding='utf-8') as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    print(f'  {len(rows):5} rows  {path}')


def per_set_rows(faces, held, pack_names):
    rows = []
    for pack in sorted(OUT_OF_PRINT_PACKS):
        want = {slot for slot in faces if slot.split('@')[1].split('~')[0] == pack}
        counts = {name: len(want & have) for name, have in held.items()}
        rows.append({'set_code': pack, 'set_name': pack_names.get(pack, ''),
                     'images_printed': len(want),
                     **{f'in_{name}': counts[name] for name in held},
                     'in_neither': len(want) - sum(counts.values())})
    rows.sort(key=lambda row: (-row['images_printed'], row['set_code']))
    return rows


def missing_rows(faces, by_code, pack_names, held, source):
    other = {name: have for name, have in held.items() if name != source}
    rows = []
    for slot, (card, face) in faces.items():
        if slot in held[source]:
            continue
        shown = by_code.get(card.get('linked_to_code') or '') if face == 'back' else card
        rows.append({'set_code': card['pack_code'],
                     'set_name': pack_names.get(card['pack_code'], ''),
                     'card_code': card['code'], 'face': face,
                     'card_name': (shown or card)['name'],
                     'type': (shown or card)['type_code'],
                     'collector_number': card['position'],
                     'reprint_of': card.get('duplicate_of_code') or '',
                     **{f'in_{name}': 'yes' if slot in have else 'no'
                        for name, have in other.items()}})
    rows.sort(key=lambda row: (row['set_name'], row['card_code'], row['face']))
    return rows


def summary(faces, by_code, pack_names, held, set_rows):
    """The markdown report, as a list of lines."""
    names = list(held)
    covered = set().union(*held.values()) & set(faces)
    missing = set(faces) - covered
    reprints = sum(1 for slot in missing if faces[slot][0].get('duplicate_of_code'))
    empty = sum(1 for row in set_rows if row['images_printed'] and not
                sum(row[f'in_{name}'] for name in names))
    in_print = sorted({c['pack_code'] for c in by_code.values()} - OUT_OF_PRINT_PACKS)
    lines = [
        '# Marvel Champions proxy collections',
        '',
        f'{len(held)} collection{"s" if len(held) > 1 else ""}. Only the sets Fantasy Flight',
        'lists as out of print are included; a set still in print is left out, whatever a source',
        'holds for it.',
        '',
        '## What each collection holds',
        '',
        f'| Collection | Images | Cards | Of the {len(faces)} out-of-print images |',
        '|---|---|---|---|',
    ]
    for name in names:
        have = held[name]
        cards = {slot.replace('~back', '') for slot in have}
        lines.append(f'| `{name}` | {len(have)} | {len(cards)} | {len(have & set(faces))} |')
    lines += [
        '',
        f'Together they hold {len(covered)} of the {len(faces)}.'
        if len(held) > 1 else
        f'That is {len(covered)} of the {len(faces)}.',
        '',
        '## The out-of-print sets',
        '',
        f'{len(OUT_OF_PRINT_PACKS)} sets. {empty} of them have no image in either collection.'
        if empty else
        f'{len(OUT_OF_PRINT_PACKS)} sets, every one of them in the collections:',
        '',
        '| Set | Code | Images | ' + ' | '.join(names) + ' | In neither |',
        '|---|---|---|' + '---|' * (len(names) + 1),
    ]
    for row in set_rows:
        counts = ' | '.join(str(row[f'in_{name}']) for name in names)
        lines.append(f"| {row['set_name']} | `{row['set_code']}` | "
                     f"{row['images_printed']} | {counts} | {row['in_neither']} |")
    lines += [
        '',
        '## Sets left out, because they are still in print',
        '',
        f'{len(in_print)} sets, none of them in a collection here:',
        '',
        ', '.join(f'{pack_names.get(code, code)} (`{code}`)' for code in in_print) + '.',
        '',
        'The Once and Future Kang is among them: it left Asmodee\'s distribution catalogue in',
        'September 2026, but Fantasy Flight has not marked it out of print.',
        '',
        '## What is missing',
        '',
        f'{len(missing)} of the {len(faces)} are in no collection here, {reprints} of them',
        'reprints: MarvelCDB gives a reprint its own code, and the image of that card sits',
        "under the code it reprints instead. The per-card CSVs",
        'list every face a source lacks, with a `reprint_of` column naming the card it reprints.',
    ]
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[1])
    parser.add_argument('collections', nargs='+', metavar='DIR',
                        help='The collection folders, best source first.')
    parser.add_argument('-o', '--output', default='.', help='Where to write the files.')
    args = parser.parse_args()

    cards, packs = load_catalog()
    by_code = {c['code']: c for c in cards}
    pack_names = {p['code']: p['name'] for p in packs}
    faces = wanted_faces(printed_cards(cards))
    held = {os.path.basename(os.path.abspath(folder.rstrip(os.sep))):
            slots(os.path.expanduser(folder)) for folder in args.collections}

    out = os.path.expanduser(args.output)
    os.makedirs(out, exist_ok=True)
    set_rows = per_set_rows(faces, held, pack_names)
    print('wrote:')
    write_csv(os.path.join(out, 'marvel-champions-collection-differences.csv'), set_rows)
    for name in held:
        write_csv(os.path.join(out, f'{name}-missing.csv'),
                  missing_rows(faces, by_code, pack_names, held, name))
    path = os.path.join(out, 'marvel-champions-collections.md')
    with open(path, 'w', encoding='utf-8') as handle:
        handle.write('\n'.join(summary(faces, by_code, pack_names, held, set_rows)) + '\n')
    print(f'            {path}')


if __name__ == '__main__':
    main()
