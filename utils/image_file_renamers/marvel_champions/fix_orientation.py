# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow", "numpy"]
# ///
"""
Makes Marvel Champions' landscape images face the same way.

Main schemes, side schemes and player side schemes are printed landscape, and
are stored portrait so every card in a collection is the same shape. A quarter
turn either way does that, and the source turns them the way this does not, so
without it some schemes print upside down against the rest. Every one is turned to be read
by turning the card clockwise; `--ccw` turns them the other way.

MarvelCDB has no picture of many of these faces, so the images are compared with
each other instead. Each of these cards has a white title band along its top
edge, which a quarter turn puts along one long side of the portrait image: the
right side when it is stored clockwise, the left when anticlockwise. An image's
brightness across its width, averaged down its height, is its profile. A first
guess comes from which edge is brighter; a template is then built from every
profile turned to face clockwise, each image is scored against the template and
its mirror, and that repeats until no image changes side.

Output goes to a separate directory; the input is never modified.
"""

import argparse
import os
import re
import shutil

import numpy as np
from catalog import load_catalog
from PIL import Image, JpegImagePlugin

LANDSCAPE_TYPES = {'main_scheme', 'side_scheme', 'player_side_scheme'}

PROFILE_SIZE = (150, 216)
# Rows averaged into a profile, skipping the top and bottom border.
PROFILE_ROWS = slice(20, 196)
# Columns in from each edge the first guess compares, where the title band sits.
BAND = slice(5, 20)
# Images closer to the template's mirror than this are reported for a look.
UNSURE = 0.2
ROUNDS = 10


def printed_face(name, by_code):
    """The catalog card whose face a filename shows."""
    match = re.match(r'^(.+?)@[a-z_0-9]+(~back)?(?:\.bleed)?\.jpg$', name)
    if not match or match.group(1) not in by_code:
        return None
    card = by_code[match.group(1)]
    if match.group(2) and card.get('linked_to_code') in by_code:
        return by_code[card['linked_to_code']]
    return card


def profile(path):
    with Image.open(path) as image:
        small = np.asarray(image.convert('L').resize(PROFILE_SIZE, Image.BILINEAR),
                           dtype=np.float32) / 255
    return small[PROFILE_ROWS].mean(axis=0)


def correlation(a, b):
    a, b = a - a.mean(), b - b.mean()
    norm = np.linalg.norm(a) * np.linalg.norm(b)
    return float(a @ b / norm) if norm else 0.0


def classify(profiles):
    """Whether each image has its title band on the right, with a score.

    The score is how much closer the profile is to the template than to its
    mirror; its sign is the side.
    """
    right = {n: p[-BAND.stop:-BAND.start].mean() > p[BAND].mean() for n, p in profiles.items()}
    scores = {}
    for _ in range(ROUNDS):
        template = np.mean([p if right[n] else p[::-1] for n, p in profiles.items()], axis=0)
        scores = {n: correlation(p, template) - correlation(p[::-1], template)
                  for n, p in profiles.items()}
        settled = {n: s > 0 for n, s in scores.items()}
        if settled == right:
            break
        right = settled
    return right, scores


def save_settings(image, quality):
    """Reproduce the source's own JPEG encoding, so a turn costs no quality."""
    if image.format != 'JPEG':
        return {'format': image.format or 'JPEG'}
    if quality is not None:
        return {'format': 'JPEG', 'quality': quality}
    settings = {'format': 'JPEG', 'qtables': image.quantization}
    sampling = JpegImagePlugin.get_sampling(image)
    if sampling in (0, 1, 2):
        settings['subsampling'] = sampling
    for key in ('exif', 'icc_profile'):
        if key in image.info:
            settings[key] = image.info[key]
    return settings


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[1])
    parser.add_argument('input', help='Directory of renamed images.')
    parser.add_argument('-o', '--output', help="Output directory. Defaults to '<input>-faced'.")
    parser.add_argument('--ccw', action='store_true',
                        help='Store landscape art to be read by turning the card anticlockwise.')
    parser.add_argument('--quality', type=int,
                        help="JPEG quality for the re-encode. Omit to reuse the source's own "
                             'quantization tables.')
    parser.add_argument('--dry-run', action='store_true', help='Report without writing.')
    args = parser.parse_args()

    source = os.path.abspath(os.path.expanduser(args.input.rstrip(os.sep)))
    dest = os.path.expanduser(args.output) if args.output else f'{source}-faced'
    cards, _packs = load_catalog()
    by_code = {c['code']: c for c in cards}

    names = sorted(n for n in os.listdir(source) if n.endswith('.jpg'))
    landscape = [n for n in names
                 if (printed_face(n, by_code) or {}).get('type_code') in LANDSCAPE_TYPES]
    right, scores = classify({n: profile(os.path.join(source, n)) for n in landscape})
    # Clockwise storage puts the title band on the right.
    turn = {n for n in landscape if right[n] == args.ccw}
    unsure = sorted(n for n in landscape if abs(scores[n]) < UNSURE)

    print(f'{len(names)} images, {len(landscape)} landscape, {len(turn)} to turn 180')
    if unsure:
        print(f'\nClose to the template either way round, check these ({len(unsure)}):')
        for name in unsure:
            print(f'  {name}  {scores[name]:+.2f}')
    if args.dry_run:
        return

    os.makedirs(dest, exist_ok=True)
    for name in names:
        src, out = os.path.join(source, name), os.path.join(dest, name)
        if name not in turn:
            shutil.copy2(src, out)
            continue
        with Image.open(src) as image:
            image.load()
            image.transpose(Image.Transpose.ROTATE_180).save(out, **save_settings(image, args.quality))
    print(f'\nturned {len(turn)}, wrote {len(names)} files to {dest}')


if __name__ == '__main__':
    main()
