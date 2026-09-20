# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy>=1.24.0", "opencv-python>=4.8.0", "Pillow"]
# ///
"""
Fills the scanner ground outside the rounded corners of Marvel Champions scans.

The archive's scans are cut to the card, so each corner holds a wedge of the
scanner bed outside the card's rounded corner. Proxy Nexus builds a bleed by
repeating the outermost pixel, so a wedge left in place becomes a pale notch in
every corner of the bleed.

../../corner_infill/corner_infill_arc.py finds a corner's ground by its colour.
Here the ground runs from light grey to brown between scans, darkens into a
shadow along the card's edge, and is close in colour to the grey dotted border
many cards have, so that script leaves most of these corners alone, and finding
the card's edge by colour here puts it inside the shadow.

So the corner is found by its shape instead: everything outside a circle of
CORNER_RADIUS touching both edges is inpainted from the card beside it. The
radius is a little more than the card's own, measured on the scans that fill
worst, so the shadow goes too. Where a card's picture runs into the corner, the
wedge filled is the part a rounded corner cuts off when printed.

Output goes to a separate directory; the input is never modified.
"""

import argparse
import os
import shutil
from concurrent.futures import ProcessPoolExecutor

import cv2
import numpy as np
from PIL import Image
from PIL.JpegImagePlugin import get_sampling

# In pixels, for the archive's ~580 dpi scans. The card's corner is 3 mm, about
# 70 px; the ground and its shadow reach up to 50 px along an edge.
CORNER_RADIUS = 85
INPAINT_RADIUS = 4


def corner_mask(height, width, radius=CORNER_RADIUS):
    """Everything outside a circle of `radius` touching both edges, at each corner."""
    ys, xs = np.mgrid[0:radius, 0:radius]
    wedge = ((xs - radius) ** 2 + (ys - radius) ** 2 > radius ** 2).astype(np.uint8) * 255
    mask = np.zeros((height, width), np.uint8)
    mask[:radius, :radius] |= wedge
    mask[:radius, width - radius:] |= wedge[:, ::-1]
    mask[height - radius:, :radius] |= wedge[::-1]
    mask[height - radius:, width - radius:] |= wedge[::-1, ::-1]
    return mask


def fill(image):
    """The image with its four corner wedges inpainted."""
    height, width = image.shape[:2]
    return cv2.inpaint(image, corner_mask(height, width), INPAINT_RADIUS, cv2.INPAINT_NS)


def save_like(src_path, out_path, bgr):
    """Write `bgr` with the source JPEG's own quantization tables and subsampling."""
    out = Image.fromarray(cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB))
    with Image.open(src_path) as src:
        kwargs = {'optimize': True, 'subsampling': get_sampling(src)}
        if getattr(src, 'quantization', None):
            kwargs['qtables'] = src.quantization
    out.save(out_path, 'JPEG', **kwargs)


def process(job):
    src, out = job
    image = cv2.imread(src, cv2.IMREAD_COLOR)
    if image is None:
        shutil.copy2(src, out)
        return False
    save_like(src, out, fill(image))
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[1])
    parser.add_argument('input', help='Directory of scans.')
    parser.add_argument('-o', '--output', help="Output directory. Defaults to '<input>-infilled'.")
    args = parser.parse_args()

    source = os.path.abspath(os.path.expanduser(args.input.rstrip(os.sep)))
    dest = os.path.expanduser(args.output) if args.output else f'{source}-infilled'
    os.makedirs(dest, exist_ok=True)
    names = sorted(n for n in os.listdir(source) if n.lower().endswith(('.jpg', '.jpeg')))
    jobs = [(os.path.join(source, n), os.path.join(dest, n)) for n in names]
    with ProcessPoolExecutor() as pool:
        filled = sum(pool.map(process, jobs, chunksize=16))
    print(f'filled {filled} of {len(names)} images -> {dest}')
    if filled < len(names):
        print(f'{len(names) - filled} could not be read and were copied as they were')


if __name__ == '__main__':
    main()
