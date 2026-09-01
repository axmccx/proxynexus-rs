# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy>=1.24.0", "opencv-python>=4.8.0"]
# ///
"""Fill in the rounded corners of card images composited onto a black ground.

`corner_infill.py` looks for the white a flatbed scanner leaves outside a card's
rounded corners. Some sources instead composite the card onto black -- the scans
at lotr.cardgame.tools do -- and the white detector never fires on those.

Thresholding for "dark" instead does not work: a card with a black border is dark
in the same corner the background is. What separates them is that the background
runs continuously from the image's own corner, so this floods outward from each
corner pixel and takes only what it reaches. On the LotR scans this was checked
against the card edges, whose darkest midpoint is (32, 17, 20) versus a
background of exactly (0, 0, 0), so the flood stops at the card.

The ground the flood reaches is taken whole. A card whose border is as dark as
the ground has nothing to stop the flood, so a corner that swallows more of the
image than any real ground could is treated as a leak and clipped back to a
square at that corner. Clipping every corner unconditionally is what the earlier
version did, and it costs more than it saves: where the ground runs past the
square, the inpaint reads its colour from the ground left outside the mask and
fills the corner with more ground.

    uv run corner_infill_dark.py ~/Downloads/scans/
    uv run corner_infill_dark.py ~/Downloads/scans/ --debug
"""

import argparse
import os

import cv2
import numpy as np

# A corner pixel at or below this is the ground the card sits on. The lotr.
# cardgame.tools scans composite onto pure black, so the default allows little
# more than JPEG ringing. A flatbed scan of a card laid on a dark surface needs
# a good deal more; --background-cutoff raises it.
BACKGROUND_CUTOFF = 16

# How far a pixel may differ from the corner it was reached from and still count
# as the same ground. Raised with --flood-tolerance for an uneven ground.
FLOOD_TOLERANCE = 16

# The card's corner radius is about 0.125in on a 2.48in card, so the ground sits
# within the outer ~5%. Taken a little wider, and used only to clip a leak.
CORNER_FRACTION = 0.07

# The ground is not one value. A corner pixel can sit a hundred levels away from
# the ground a little along the same edge, and the flood measures every pixel
# against the one it started from, so a single seed reaches only part of it.
# Seeding along both of a corner's edges as well as at the corner itself covers
# the rest. Letting the flood measure against the neighbour it came from instead
# would too, and was tried: it walks the halftone's own gradient straight into
# the artwork, taking most of the card.
SEED_SPAN = 90
SEED_STEP = 12

# A corner's share of the image that no real ground reaches. Measured over 120
# Call of Cthulhu scans: ground takes at most 0.33% of the image per corner,
# while a leak across a black-bordered card takes around 12%.
MAX_GROUND_FRACTION = 0.02

KERNEL_SIZE = (5, 5)
DILATE_ITERATIONS = 3
INPAINT_RADIUS = 3


def corner_seeds(img, y, x, cutoff):
    """Where to start the flood at one corner: the corner itself and points
    along both of its edges, keeping the ones that are ground."""
    points = [(y, x)]
    for step in range(SEED_STEP, SEED_SPAN, SEED_STEP):
        points.append((y, x + step if x == 0 else x - step))
        points.append((y + step if y == 0 else y - step, x))

    return [(py, px) for py, px in points if np.all(img[py, px] <= cutoff)]


def background_mask(img, cutoff=BACKGROUND_CUTOFF, tolerance=FLOOD_TOLERANCE):
    """Mask the ground reachable from each corner, a leak clipped to its corner."""
    height, width = img.shape[:2]
    depth = int(min(height, width) * CORNER_FRACTION)
    mask = np.zeros((height, width), dtype=np.uint8)
    leaked = 0

    for y, x in ((0, 0), (0, width - 1), (height - 1, 0), (height - 1, width - 1)):
        reached = np.zeros((height, width), dtype=np.uint8)

        for seed_y, seed_x in corner_seeds(img, y, x, cutoff):
            # floodFill wants a mask two pixels larger than the image, and writes
            # into its interior.
            flood = np.zeros((height + 2, width + 2), dtype=np.uint8)
            cv2.floodFill(
                img.copy(),
                flood,
                (seed_x, seed_y),
                0,
                (tolerance,) * 3,
                (tolerance,) * 3,
                cv2.FLOODFILL_MASK_ONLY | cv2.FLOODFILL_FIXED_RANGE | (255 << 8),
            )
            reached |= flood[1 : height + 1, 1 : width + 1]

        if not reached.any():
            continue  # this corner is card art, not ground

        if reached.sum() / 255 > height * width * MAX_GROUND_FRACTION:
            leaked += 1
            clip = np.zeros((height, width), dtype=np.uint8)
            top, left = (0 if y == 0 else height - depth), (0 if x == 0 else width - depth)
            clip[top : top + depth, left : left + depth] = 1
            reached = reached * clip

        mask |= reached

    return mask, leaked


def process_corners(img, cutoff=BACKGROUND_CUTOFF, tolerance=FLOOD_TOLERANCE):
    mask, leaked = background_mask(img, cutoff, tolerance)
    if not mask.any():
        return img, mask, leaked
    grown = cv2.dilate(mask, np.ones(KERNEL_SIZE, np.uint8), iterations=DILATE_ITERATIONS)
    return cv2.inpaint(img, grown, INPAINT_RADIUS, cv2.INPAINT_NS), grown, leaked


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("input", help="An image, or a directory of images.")
    parser.add_argument("-o", "--output",
                        help="Where to write. Defaults to '<input>-infilled'.")
    parser.add_argument("--background-cutoff", type=int, default=BACKGROUND_CUTOFF,
                        help=f"How bright a corner pixel may be and still count as ground "
                             f"(default {BACKGROUND_CUTOFF}).")
    parser.add_argument("--flood-tolerance", type=int, default=FLOOD_TOLERANCE,
                        help=f"How far the ground may vary from the corner it is reached from "
                             f"(default {FLOOD_TOLERANCE}).")
    parser.add_argument("--debug", action="store_true",
                        help="Also write the mask that was inpainted, in magenta.")
    args = parser.parse_args()

    input_path = os.path.abspath(args.input)
    output_dir = args.output or f"{input_path.rstrip(os.sep)}-infilled"
    os.makedirs(output_dir, exist_ok=True)

    if os.path.isfile(input_path):
        files = [input_path]
    elif os.path.isdir(input_path):
        files = sorted(
            os.path.join(input_path, f)
            for f in os.listdir(input_path)
            if f.lower().endswith((".png", ".jpg", ".jpeg"))
        )
    else:
        raise SystemExit(f"No such file or directory: {input_path}")

    print(f"Processing {len(files)} image(s) into {output_dir}")

    for path in files:
        img = cv2.imread(path, cv2.IMREAD_COLOR)
        if img is None:
            print(f"[SKIP] {os.path.basename(path)} (could not read)")
            continue

        filled, mask, leaked = process_corners(
            img, args.background_cutoff, args.flood_tolerance)
        filled_pixels = int((mask > 0).sum())
        name = os.path.basename(path)

        if filled_pixels == 0:
            print(f"[WARN] {name}: no dark ground found at any corner, copied as-is")
        else:
            share = filled_pixels / (img.shape[0] * img.shape[1])
            note = f", {leaked} corner(s) clipped as a leak" if leaked else ""
            print(f"[OK]   {name}: filled {filled_pixels} px ({share:.2%}){note}")

        cv2.imwrite(os.path.join(output_dir, name), filled)

        if args.debug:
            marked = img.copy()
            marked[mask > 0] = (255, 0, 255)
            stem, ext = os.path.splitext(name)
            cv2.imwrite(os.path.join(output_dir, f"{stem}.mask{ext}"), marked)


if __name__ == "__main__":
    main()
