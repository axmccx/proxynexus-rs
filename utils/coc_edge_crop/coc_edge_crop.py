# /// script
# requires-python = ">=3.9"
# dependencies = ["numpy>=1.24.0", "Pillow"]
# ///
"""Trim the scanner ground from the edges of a Call of Cthulhu scan.

The scans are cut flush to the card, so each carries a dark rim a few pixels
deep all the way round where the scanner ground meets the card edge. It is easy
to miss -- the corner infill scripts clear the four corners and leave it -- but
Proxy Nexus builds a card's bleed by repeating the outermost pixel outward, so
that rim becomes the entire bleed, and any outward drift in the cut shows it
against the card's white border.

The depth is measured per image, over the straight part of each edge and on each
axis separately, so a card needing 6px loses 6px and a landscape scan is not
left with rim on its long edges. A card whose border is as dark as the ground --
art printed to the edge -- is left alone: there is nothing to tell the two apart,
and its bleed is meant to be dark.

It trims the ground, not the card. The white border these cards are printed with
is part of the card and stays.

The constants were measured against that collection; another game's scans would
want them re-measured.

    uv run coc_edge_crop.py ~/Downloads/coclcg_infilled/
    uv run coc_edge_crop.py ~/Downloads/coclcg_infilled/ --dry-run
"""

import argparse
import os

import numpy as np
from PIL import Image, JpegImagePlugin

IMAGE_EXTS = ('.png', '.jpg', '.jpeg')

# A pixel at or above this is the card's own border rather than the ground.
BORDER_CUTOFF = 200

# How far in to look for that border, as a fraction of the short side. An edge
# still dark at this depth is art printed to the edge, not ground.
SEARCH_FRACTION = 0.04

# Both ends of each edge are skipped, so the rounded corners -- where the ground
# runs deep into the image by design -- are not measured as rim.
CORNER_FRACTION = 0.10

# Taken past the last dark pixel, for the halftone speckle along the edge.
MARGIN = 2


def rim_depth(gray, search):
    """How deep the dark rim runs, across the image and down it, over the
    straight part of the four edges.

    The two axes are measured apart because the rim is a physical depth in
    pixels and the image is not square: one depth scaled to both would leave
    rim on the long edges of a landscape scan.

    Returns None where an edge never reaches the border cutoff, which means the
    card's own edge is dark and no crop can tell it from the ground.
    """
    height, width = gray.shape
    inset_y, inset_x = int(height * CORNER_FRACTION), int(width * CORNER_FRACTION)

    axes = (
        (gray[inset_y:height - inset_y, :search],
         gray[inset_y:height - inset_y, width - search:][:, ::-1]),
        (gray[:search, inset_x:width - inset_x].T,
         gray[height - search:, inset_x:width - inset_x].T[:, ::-1]),
    )

    depths = []
    for edges in axes:
        depth = 0
        for edge in edges:
            border = edge >= BORDER_CUTOFF
            if not border.any(axis=1).all():
                return None
            depth = max(depth, int(border.argmax(axis=1).max()))
        depths.append(depth)

    return tuple(depths)


def jpeg_encoder_settings(img):
    """Encoder settings that reproduce `img`'s own JPEG encoding, so that
    writing the crop back costs no further quality."""
    settings = {'format': 'JPEG', 'qtables': img.quantization}
    sampling = JpegImagePlugin.get_sampling(img)

    if sampling in (0, 1, 2):
        settings['subsampling'] = sampling
    for key in ('exif', 'icc_profile'):
        if key in img.info:
            settings[key] = img.info[key]
    return settings


def save_settings(img):
    if img.format != 'JPEG':
        return {'format': img.format}
    return jpeg_encoder_settings(img)


def crop_box(img, search_fraction=SEARCH_FRACTION):
    """The box to keep, or None to leave the image as it is.

    Each axis is cropped by at least what its own rim needs, then whichever
    axis asks for more sets the other through the image's aspect, so that
    cropping leaves the aspect where it found it.
    """
    width, height = img.size
    search = max(4, int(min(height, width) * search_fraction))

    depths = rim_depth(np.asarray(img.convert('L')).astype(int), search)
    if depths is None:
        return None

    need_x = min(depths[0] + MARGIN, search)
    need_y = min(depths[1] + MARGIN, search)
    crop_x = max(need_x, round(need_y * width / height))
    crop_y = max(need_y, round(crop_x * height / width))
    if crop_x == 0 and crop_y == 0:
        return None

    return (crop_x, crop_y, width - crop_x, height - crop_y)


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("input", help="An image, or a directory of images.")
    parser.add_argument("-o", "--output",
                        help="Where to write. Defaults to '<input>-cropped'.")
    parser.add_argument("--dry-run", action="store_true",
                        help="Report what would be cropped without writing.")
    args = parser.parse_args()

    input_path = os.path.abspath(args.input)
    output_dir = args.output or f"{input_path.rstrip(os.sep)}-cropped"

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

    if not args.dry_run:
        os.makedirs(output_dir, exist_ok=True)

    print(f"Processing {len(files)} image(s) into {output_dir}")
    cropped, left_alone, depths = 0, [], []

    for path in files:
        name = os.path.basename(path)
        with Image.open(path) as img:
            img.load()
            box = crop_box(img)

            if box is None:
                left_alone.append(name)
                print(f"[KEEP] {name}: no border brighter than the ground, left as-is")
                if not args.dry_run:
                    img.save(os.path.join(output_dir, name), **save_settings(img))
                continue

            depths.append(box[0])
            print(f"[OK]   {name}: trimmed {box[0]}px by {box[1]}px")
            cropped += 1
            if not args.dry_run:
                img.crop(box).save(os.path.join(output_dir, name), **save_settings(img))

    print(f"\nSummary: {cropped} cropped, {len(left_alone)} left as-is.")
    if depths:
        print(f"Trimmed between {min(depths)}px and {max(depths)}px, "
              f"median {int(np.median(depths))}px.")


if __name__ == '__main__':
    main()
