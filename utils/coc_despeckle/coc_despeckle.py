# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy>=1.24.0", "opencv-python>=4.8.0", "Pillow"]
# ///
"""Take the print screen out of Call of Cthulhu scans and resample them for print.

The cards are offset-printed, so the scans carry the rosette of a ~169 line/inch
screen, 3.5px across at the ~590dpi these were scanned at. Two passes deal with
it:

  1. Non-local means, which averages the screen away while leaving the edges of
     the type alone. It reads a pixel from other patches that look like its
     neighbourhood, so flat paper is smoothed hard and a letter's edge is not.
  2. An area resample down to print resolution, where the screen would land at
     1.8px -- right at the limit of what the print can carry, and where a poor
     resampler turns it into moire instead of averaging it out. Doing it here
     settles which resampler gets used.

Denoising comes first. Shrinking first is three times quicker but measurably
softer on the type, because by then the screen and the type are the same size.

    uv run coc_despeckle.py ~/Pictures/proxynexus_collections/coclcg -o out
    uv run coc_despeckle.py scans/ --strength 5      # gentler
"""

import argparse
import os
from concurrent.futures import ProcessPoolExecutor

import cv2
import numpy as np
from PIL import Image

IMAGE_EXTS = ('.png', '.jpg', '.jpeg')

# How hard non-local means averages. 8 takes the screen down by a third while
# holding the type; 12 goes further and starts to soften it.
STRENGTH = 8

# The window it matches on, and how far it looks for matches. OpenCV's defaults,
# which suit a screen this size.
TEMPLATE_WINDOW = 7
SEARCH_WINDOW = 21

# The long side to resample to. Proxy Nexus lays a card out as 744x1038 inside
# an 816x1110 bleed sheet, which is MPC's 300dpi sheet, so an image this tall
# comes out at exactly that resolution with nothing further to scale.
TARGET_LONG_SIDE = 1038

# The screen is gone by this point and the image is a third of the size, so the
# quality can be high; 4:4:4 keeps chroma off the small type.
JPEG_QUALITY = 95

# Non-local means is the slow part, at a couple of seconds a card. It is run one
# card per process, with OpenCV's own threading turned off inside each so the
# two do not oversubscribe.
DEFAULT_JOBS = min(8, os.cpu_count() or 1)


def despeckle(img, strength=STRENGTH):
    return cv2.fastNlMeansDenoisingColored(
        img, None, strength, strength, TEMPLATE_WINDOW, SEARCH_WINDOW)


def resample(img, long_side=TARGET_LONG_SIDE):
    """Down to print resolution, by area, which averages rather than samples."""
    height, width = img.shape[:2]
    scale = long_side / max(height, width)
    if scale >= 1.0:
        return img
    size = (max(1, round(width * scale)), max(1, round(height * scale)))
    return cv2.resize(img, size, interpolation=cv2.INTER_AREA)


def process(img, strength=STRENGTH, long_side=TARGET_LONG_SIDE):
    return resample(despeckle(img, strength), long_side)


def save(img, path, quality=JPEG_QUALITY):
    rgb = Image.fromarray(cv2.cvtColor(img, cv2.COLOR_BGR2RGB))
    if path.lower().endswith('.png'):
        rgb.save(path, format='PNG')
    else:
        rgb.save(path, format='JPEG', quality=quality, subsampling=0)


def _one(job):
    """One card, in its own process."""
    path, out_path, strength, long_side, quality = job
    cv2.setNumThreads(1)

    img = cv2.imread(path, cv2.IMREAD_COLOR)
    if img is None:
        return os.path.basename(path), None, None

    result = process(img, strength, long_side or max(img.shape[:2]))
    save(result, out_path, quality)
    return os.path.basename(path), (img.shape[1], img.shape[0]), (result.shape[1], result.shape[0])


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("input", help="An image, or a directory of images.")
    parser.add_argument("-o", "--output",
                        help="Where to write. Defaults to '<input>-despeckled'.")
    parser.add_argument("--strength", type=int, default=STRENGTH,
                        help=f"How hard to average (default {STRENGTH}).")
    parser.add_argument("--long-side", type=int, default=TARGET_LONG_SIDE,
                        help=f"Resample the long side to this (default {TARGET_LONG_SIDE}). "
                             f"0 leaves the resolution alone.")
    parser.add_argument("--quality", type=int, default=JPEG_QUALITY,
                        help=f"JPEG quality (default {JPEG_QUALITY}).")
    parser.add_argument("--jobs", type=int, default=DEFAULT_JOBS,
                        help=f"How many cards to work on at once (default {DEFAULT_JOBS}).")
    args = parser.parse_args()

    input_path = os.path.abspath(args.input)
    output_dir = args.output or f"{input_path.rstrip(os.sep)}-despeckled"

    if os.path.isfile(input_path):
        files = [input_path]
    elif os.path.isdir(input_path):
        files = sorted(
            os.path.join(input_path, name)
            for name in os.listdir(input_path)
            if name.lower().endswith(IMAGE_EXTS)
        )
    else:
        raise SystemExit(f"No such file or directory: {input_path}")

    os.makedirs(output_dir, exist_ok=True)
    print(f"Processing {len(files)} image(s) into {output_dir}")

    jobs = [(path, os.path.join(output_dir, os.path.basename(path)),
             args.strength, args.long_side, args.quality) for path in files]

    done, before, after = 0, 0, 0
    with ProcessPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        for path, (name, src, out) in zip(files, pool.map(_one, jobs)):
            if src is None:
                print(f"[SKIP] {name} (could not read)")
                continue

            before += os.path.getsize(path)
            after += os.path.getsize(os.path.join(output_dir, name))
            done += 1
            print(f"[OK]   {name}: {src[0]}x{src[1]} -> {out[0]}x{out[1]}")

    print(f"\nSummary: {done} processed.")
    if done:
        print(f"Size: {before/1e6:.0f}MB -> {after/1e6:.0f}MB")


if __name__ == '__main__':
    main()
