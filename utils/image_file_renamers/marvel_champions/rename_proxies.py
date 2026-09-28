# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow"]
# ///
"""
Renames a second Marvel Champions source, one image per card face, already
named by MarvelCDB code and already carrying a bleed border:

    Wasp/13001a_front.png
    Wasp/13001a_back.png

So there is nothing to match: the code gives the card, the pack comes from
MarvelCDB, and `_back` is the card's second face, which for a double-sided card
is the face MarvelCDB hides behind the code that links to it.

`_Alternate Art/` names the product an alt art came in rather than a code's own
pack, so those are written as a printing of their own:

    _Alternate Art/08013_front Stealth Strike (Gambit).png  ->  08013@alt_gambit

The images are PNGs that already carry a bleed, so they are written as `.bleed`
and Proxy Nexus builds none of its own. They are saved as JPEG at quality 92,
which takes the collection from 12.3GB to 2.5GB.

See README.md for where the images come from and what they cover.
"""

import argparse
import os
import re
from collections import Counter, defaultdict

from catalog import OUT_OF_PRINT_PACKS, load_catalog
from PIL import Image

# `13001a_front.png`, `13001a_back.png`.
FACE = re.compile(r'^(\w+)_(front|back)\.png$', re.IGNORECASE)
# `08013_front Stealth Strike (Gambit).png`: an alt art, and the product it
# came in. Alt arts of one card are told apart by that, as `Promo1`/`Promo2`.
ALT_FACE = re.compile(r'^(\w+)_(front|back) .*\(([^)]+)\)$', re.IGNORECASE)
# `42005_front(1).png`, a second copy of a file already there.
COPY = re.compile(r'\(\d+\)\.png$', re.IGNORECASE)

# The generic backs, which live in the adapter rather than in a collection.
SHARED_BACKS = '_shared backs'
# Alt arts, which are a printing of their own rather than a pack's.
ALTERNATE_ART = '_alternate art'


def output_name(code, face, printing):
    side = '~back' if face == 'back' else ''
    return f'{code}@{printing}{side}.bleed.jpg'


def alt_printing(product):
    """The printing label an alt art is written under.

    `alt_` keeps it clear of the pack codes, several of which a product shares
    a name with: an alt art from the Gambit pack is `alt_gambit`, not `gambit`,
    which would read as the card having been printed in that pack.
    """
    return 'alt_' + re.sub(r'[^a-z0-9]+', '_', product.lower()).strip('_')


def convert(src, dst, quality):
    """Write the source PNG as JPEG."""
    with Image.open(src) as image:
        if image.mode != 'RGB':
            image = image.convert('RGB')
        image.save(dst, 'JPEG', quality=quality, optimize=True, subsampling=0)


def two_faced(card):
    """Whether MarvelCDB gives this card a second printed face."""
    return bool(card.get('linked_to_code')) or card.get('back_name') is not None


def report_one_sided(written, by_code, reports):
    """Note the two-faced cards the source has one side of.

    They are kept: a front on its own prints with the game's generic back,
    which is better than not having the card. A back on its own is not kept,
    because a collection will not import one.
    """
    faces = defaultdict(set)
    for name in written:
        faces[name.split('@')[0] + '@' + name.split('@')[1].split('~')[0].split('.')[0]].add(
            'back' if '~back' in name else 'front')

    for card, sides in sorted(faces.items()):
        code = card.split('@')[0]
        if two_faced(by_code[code]) and sides == {'front'}:
            reports['Two-faced cards the source has the front of only'].append(card)


def original(card, by_code):
    """The card a reprint reprints, following reprints of reprints."""
    seen = set()
    while card.get('duplicate_of_code') in by_code and card['code'] not in seen:
        seen.add(card['code'])
        card = by_code[card['duplicate_of_code']]
    return card


def add_reprints(written, by_code, wanted, reports):
    """Give each reprint the source lacks the image of the card it reprints.

    MarvelCDB gives a reprint a code of its own, in the pack that reprints it,
    and the source draws the card only under the code it reprints. The image is
    written under the reprint's code and pack, so the pack is complete whether
    or not the pack it reprints from is kept.
    """
    faces = {}
    for name, path in written.items():
        code, rest = name.split('@')
        if not rest.startswith('alt_'):
            faces[(code, 'back' if '~back' in rest else 'front')] = path

    for card in by_code.values():
        first = original(card, by_code)
        if first is card or card['pack_code'] not in wanted:
            continue
        for face in ('front', 'back'):
            if (card['code'], face) in faces or (first['code'], face) not in faces:
                continue
            written[output_name(card['code'], face, card['pack_code'])] = faces[(first['code'], face)]
            reports['Reprints drawn from the card they reprint'].append(
                f"{card['code']} {face}  from {first['code']}")


def collect(source, by_code, reports):
    """Every card face in the source, as output name -> path."""
    written = {}
    for root, dirs, files in os.walk(source):
        dirs.sort()
        for name in sorted(files):
            path = os.path.join(root, name)
            rel = os.path.relpath(path, source)
            if os.path.basename(root).lower() == SHARED_BACKS:
                reports['Card backs, which live in the adapter'].append(rel)
                continue
            if not name.lower().endswith('.png'):
                continue
            if COPY.search(name):
                reports['Extra copies of one file'].append(rel)
                continue
            alt = os.path.basename(root).lower() == ALTERNATE_ART
            match = (ALT_FACE if alt else FACE).match(os.path.splitext(name)[0]
                                                      if alt else name)
            if not match:
                reports['Files not named after a card face'].append(rel)
                continue
            code, face = match.group(1), match.group(2).lower()
            if code not in by_code:
                reports['Codes MarvelCDB does not list'].append(rel)
                continue
            printing = (alt_printing(match.group(3)) if alt
                        else by_code[code]['pack_code'])
            out = output_name(code, face, printing)
            if out in written:
                reports['Extra copies of one file'].append(rel)
                continue
            written[out] = path
    return written


def main():
    parser = argparse.ArgumentParser(
        description='Rename the pre-bled Marvel Champions proxies to the Proxy Nexus convention.')
    parser.add_argument('input', help='The source folder.')
    parser.add_argument('-o', '--output', default='marvel_champions_proxies_out',
                        help='Output directory.')
    parser.add_argument('--packs', default=','.join(sorted(OUT_OF_PRINT_PACKS)),
                        help='Pack codes to keep, comma separated. Defaults to the packs out of '
                             'print; pass "all" for every pack the source holds.')
    parser.add_argument('--quality', type=int, default=92, help='JPEG quality (default 92).')
    parser.add_argument('--dry-run', action='store_true', help='Report without writing.')
    parser.add_argument('--refresh-catalog', action='store_true',
                        help='Re-download the MarvelCDB catalog.')
    args = parser.parse_args()

    source = os.path.abspath(os.path.expanduser(args.input))
    cards, _packs = load_catalog(args.refresh_catalog)
    by_code = {c['code']: c for c in cards}
    packs = {c['pack_code'] for c in by_code.values()}
    wanted = (packs if args.packs == 'all'
              else {p.strip() for p in args.packs.split(',') if p.strip()})
    unknown = wanted - packs
    if unknown:
        parser.error(f"--packs names no MarvelCDB pack: {', '.join(sorted(unknown))}")

    reports = defaultdict(list)
    written = collect(source, by_code, reports)
    add_reprints(written, by_code, wanted, reports)
    report_one_sided(written, by_code, reports)
    kept = {}
    for name, path in written.items():
        # An alt art is filed under the pack of the card it is an art of.
        if by_code[name.split('@')[0]]['pack_code'] in wanted:
            kept[name] = path
        else:
            reports['Packs not asked for'].append(os.path.relpath(path, source))
    written = kept

    per_pack = Counter(by_code[name.split('@')[0]]['pack_code'] for name in written)
    print(f'{len(written)} files resolved, across {len(per_pack)} packs')
    for pack, count in sorted(per_pack.items()):
        print(f'  {count:5}  {pack}')
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
