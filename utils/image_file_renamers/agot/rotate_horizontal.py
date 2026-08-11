# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow"]
# ///
"""
Rotates landscape AGOT scans to portrait, in place.

AGOT plot cards are printed landscape but have to be stored portrait to match
every other card in a collection. This is a pre-processing pass, not a renamer.
Run it on rename.py's output.

WARNING: modifies files IN PLACE. No output folder, no undo. Run it against a
disposable COPY of a scan folder, and --dry-run first.

JPEGs are re-encoded with the quantization tables and chroma subsampling read
off the source, so a rotate costs no visible quality.
"""

import os
import argparse
from PIL import Image, JpegImagePlugin

IMAGE_EXTS = ('.tif', '.tiff', '.jpg', '.jpeg', '.png')


def jpeg_encoder_settings(img):
    """Encoder settings that reproduce `img`'s own JPEG encoding.
    """
    settings = {'format': 'JPEG', 'qtables': img.quantization}
    sampling = JpegImagePlugin.get_sampling(img)

    if sampling in (0, 1, 2):
        settings['subsampling'] = sampling
    for key in ('exif', 'icc_profile'):
        if key in img.info:
            settings[key] = img.info[key]
    return settings


def save_settings(img, quality):
    if img.format == 'TIFF':
        settings = {'format': 'TIFF'}
        for key in ('dpi', 'compression', 'icc_profile'):
            if img.info.get(key) is not None:
                settings[key] = img.info[key]
        return settings
    if img.format != 'JPEG':
        return {'format': img.format}
    if quality is not None:
        return {'format': 'JPEG', 'quality': quality}
    return jpeg_encoder_settings(img)


def rotate_in_place(file_path, img, method, quality):
    """Rotate and overwrite, via a temp file in the same directory.
    """
    tmp_path = file_path + '.rotating'
    try:
        img.transpose(method).save(tmp_path, **save_settings(img, quality))
        os.replace(tmp_path, file_path)
    except BaseException:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)
        raise


def main():
    parser = argparse.ArgumentParser(
        description="Rotate landscape images in a folder to portrait. Modifies files IN PLACE.")
    parser.add_argument("folder", help="Folder containing images (rotated IN PLACE)")
    parser.add_argument("--ccw", action="store_true", help="Rotate counter-clockwise (default is clockwise)")
    parser.add_argument("--quality", type=int,
                        help="JPEG quality for the re-encode. Omit to reuse the source's own "
                             "quantization tables, which is what you almost always want.")
    parser.add_argument("--dry-run", action="store_true", help="Just count, don't rotate")
    args = parser.parse_args()

    path: str = os.path.abspath(args.folder)
    rotated_count = 0

    print(f"Scanning: {path}")

    for filename in sorted(os.listdir(path)):
        if not filename.lower().endswith(IMAGE_EXTS):
            continue

        file_path = os.path.join(path, filename)
        try:
            with Image.open(file_path) as img:
                w, h = img.size
                if w <= h:
                    continue
                if args.dry_run:
                    print(f"[DRY] Would rotate: {filename} ({w}x{h})")
                else:
                    # ROTATE_270 turns the image 90 degrees clockwise.
                    method = (Image.Transpose.ROTATE_90 if args.ccw
                              else Image.Transpose.ROTATE_270)
                    rotate_in_place(file_path, img, method, args.quality)
                    print(f"[OK]  Rotated: {filename} ({w}x{h} -> {h}x{w})")
                rotated_count += 1
        except (OSError, ValueError) as e:
            print(f"[ERR] Failed to process {filename}: {e}")

    print(f"\nSummary: {rotated_count} images {'would be ' if args.dry_run else ''}rotated.")


if __name__ == "__main__":
    main()
