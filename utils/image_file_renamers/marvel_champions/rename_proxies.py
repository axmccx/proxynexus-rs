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

A card the source has only one side of is left out, because a card's two sides
have to sit in the same collection, and the other source holds both.

The images are PNGs that already carry a bleed, so they are written as `.bleed`
and Proxy Nexus builds none of its own. They are saved as JPEG at quality 92,
which takes the collection from 9GB to 1GB.

See README.md for where the images come from and what they cover.
"""

import argparse
import json
import os
import re
from collections import Counter, defaultdict

from PIL import Image

# The one list of what is out of print lives in rename.py, next door.
from rename import OUT_OF_PRINT_PACKS

CATALOG_CACHE = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                             'marvel_champions_catalog_cache.json')

# `13001a_front.png`, `13001a_back.png`.
FACE = re.compile(r'^(\w+)_(front|back)\.png$', re.IGNORECASE)
# `42005_front(1).png`, a second copy of a file already there.
COPY = re.compile(r'\(\d+\)\.png$', re.IGNORECASE)

# The generic backs, which live in the adapter rather than in a collection.
SHARED_BACKS = '_shared backs'


def load_catalog():
    with open(CATALOG_CACHE, encoding='utf-8') as handle:
        return json.load(handle)['cards']


def output_name(code, face, pack):
    side = '~back' if face == 'back' else ''
    return f'{code}@{pack}{side}.bleed.jpg'


def convert(src, dst, quality):
    """Write the source PNG as JPEG."""
    with Image.open(src) as image:
        if image.mode != 'RGB':
            image = image.convert('RGB')
        image.save(dst, 'JPEG', quality=quality, optimize=True, subsampling=0)


def two_faced(card):
    """Whether MarvelCDB gives this card a second printed face."""
    return bool(card.get('linked_to_code')) or card.get('back_name') is not None


def whole_cards(written, by_code, reports):
    """The faces of the cards this source holds every side of."""
    faces = defaultdict(dict)
    for name, path in written.items():
        code = name.split('@')[0]
        faces[code]['back' if '~back' in name else 'front'] = (name, path)

    kept = {}
    for code, sides in faces.items():
        if two_faced(by_code[code]) and len(sides) < 2:
            side, (name, _path) = next(iter(sides.items()))
            reports['Cards the source has one side of, left to the other source'].append(
                f'{name}  ({side} only)')
            continue
        kept.update(dict(sides.values()))
    return kept


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
            match = FACE.match(name)
            if not match:
                reports['Files not named after a card face'].append(rel)
                continue
            code, face = match.group(1), match.group(2).lower()
            if code not in by_code:
                reports['Codes MarvelCDB does not list'].append(rel)
                continue
            out = output_name(code, face, by_code[code]['pack_code'])
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
    args = parser.parse_args()

    source = os.path.abspath(os.path.expanduser(args.input))
    by_code = {c['code']: c for c in load_catalog()}
    packs = {c['pack_code'] for c in by_code.values()}
    wanted = (packs if args.packs == 'all'
              else {p.strip() for p in args.packs.split(',') if p.strip()})
    unknown = wanted - packs
    if unknown:
        parser.error(f"--packs names no MarvelCDB pack: {', '.join(sorted(unknown))}")

    reports = defaultdict(list)
    written = whole_cards(collect(source, by_code, reports), by_code, reports)
    kept = {}
    for name, path in written.items():
        if name.split('@')[1].split('~')[0].split('.')[0] in wanted:
            kept[name] = path
        else:
            reports['Packs not asked for'].append(os.path.relpath(path, source))
    written = kept

    per_pack = Counter(name.split('@')[1].split('~')[0].split('.')[0] for name in written)
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
