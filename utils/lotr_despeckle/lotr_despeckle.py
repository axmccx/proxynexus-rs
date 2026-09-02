# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy>=1.24.0", "opencv-python>=4.8.0", "Pillow"]
# ///
"""Take the print screen out of the four LotR cards downloaded from cardgame.tools.

Almost every image in the LotR collection arrives already cleaned: the Enhanced
Proxies were denoised and sharpened before they were published, and their screen
is gone. The four cards in `lotrlcg-gap-fills/` taken from lotr.cardgame.tools
are the exception -- they are renders of the offset-printed card, and they carry
its rosette at 3.5px across the 1468x2080 they arrive at.

The screen is worst in the shadows, which is not where a flat-paper measurement
looks. Where the ink is near solid the dots are the paper showing through, so
they are bright against dark and at their highest contrast; on the light card
stock behind the type they are faint. Judge this on a dark area of the art, at
1:1, and beware of judging it on a screenshot that has been scaled to fit --
the scaling beats against the screen and invents a pattern of its own.

Two passes:

  1. A notch on the screen itself. A halftone is periodic, so it stands in the
     spectrum as a few sharp spikes -- here the rosette at 3.53px, at the usual
     15/45/75 degree separation angles, 60 to 176 times its own neighbourhood.
     Picture detail is not periodic and spreads smoothly, so a spike that far
     above its surroundings is screen and almost nothing else, and removing it
     costs the picture almost nothing. This is what does most of the work.
  2. Non-local means, on what the notch leaves. That residue is no longer a
     screen but ordinary grain, which is the case non-local means is actually
     good at, so it needs far less strength here than it would on its own.

Non-local means alone was tried first and is much the worse tool for this. It
averages a pixel from other patches that look like its neighbourhood, and a
regular rosette is the most self-similar thing on the card, so it preserves the
screen rather than removing it: even at 20 it left 22% of it, and by then the
art was going flat. The two together leave 2% and hold more detail than that
did.

The resolution is left alone. The Call of Cthulhu pass resamples to MPC's
300dpi cut line, because there the whole collection is built that way and a
screen left in would alias under whatever filter MPC used. Neither holds here:
the rest of lotrlcg sits at about 578dpi, these four arrive at 592dpi, and
downscaling only these to 300 would make them the one set of cards at half
everyone else's resolution. With the screen actually gone, letting MPC resample
costs little -- measured at print size, keeping full resolution and resampling
by lanczos leaves 19% more residual but 9% more detail than downscaling here.
`--long-side` resamples anyway, for a collection built the other way.

    uv run lotr_despeckle.py ~/Downloads/cardgame-tools-lotr -o despeckled
    uv run lotr_despeckle.py ~/Downloads/cardgame-tools-lotr --strength 14  # gentler
"""

import argparse
import os

import cv2
import numpy as np
from PIL import Image

IMAGE_EXTS = ('.png', '.jpg', '.jpeg')

# The band the screen is looked for in, as a pixel period. The rosette is at
# 3.53px and its harmonics fold back above 2.2px, while 8px is well clear of
# anything periodic the picture itself holds.
SCREEN_MIN_PERIOD = 2.2
SCREEN_MAX_PERIOD = 8.0

# How far above its own neighbourhood a bin has to stand to count as screen,
# in log units, and the blur that estimates that neighbourhood. 1.5 picks out
# about 0.1% of the band, which is the right order for a rosette's spikes;
# lower starts taking picture detail with it.
PEAK_EXCESS = 1.5
BACKGROUND_KERNEL = 63

# How far each spike is grown before it is notched out, in bins.
NOTCH_RADIUS = 3

# The most any one pixel may be moved by the notch. The screen has a bounded
# amplitude so its correction does too; a larger swing is the notch ringing
# against a hard edge, which without this clipped to black speckle along the
# card's metal ornament. 1.8% of pixels clipped before this was added.
CORRECTION_LIMIT = 28.0

# How hard non-local means averages what the notch leaves. That residue is
# grain rather than screen, so this is far lower than non-local means needs
# when it has to fight the screen alone: 14 here against 20 on its own, for a
# tenth of the screen left and more detail kept.
STRENGTH = 14

# The window it matches on, and how far it looks for matches. OpenCV's defaults,
# which suit a screen this size.
TEMPLATE_WINDOW = 7
SEARCH_WINDOW = 21

# The long side to resample to; 0 leaves the resolution alone, which is the
# default. See the module docstring for why these are not downscaled. Passing
# 1038 puts them on MPC's cut line, and 1110 is what a `.bleed` image would
# want instead -- the two differ by the width of the bleed.
TARGET_LONG_SIDE = 0

# The screen is gone by this point and the image is a third of the size, so the
# quality can be high; 4:4:4 keeps chroma off the small type.
JPEG_QUALITY = 95


def is_not_for_this_pass(name):
    """`lotrlcg-gap-fills/` also holds three cards copied out of lotrlcg-enhanced,
    named `.bleed`. Those come from the Enhanced Proxies, which were descreened
    before they were published, so this has nothing to take off them and would
    only soften them. They are told apart by name and left alone."""
    stem = name.rsplit('.', 1)[0]
    return stem.lower().endswith('.bleed')


def _notch_mask(mag):
    """1 everywhere except a soft hole over each periodic spike."""
    height, width = mag.shape
    logmag = np.log1p(mag).astype(np.float32)
    background = cv2.blur(logmag, (BACKGROUND_KERNEL, BACKGROUND_KERNEL))

    rows, cols = np.mgrid[0:height, 0:width].astype(np.float32)
    cycles = np.hypot((rows - height / 2) / height, (cols - width / 2) / width)
    band = ((cycles > 1.0 / SCREEN_MAX_PERIOD) &
            (cycles < 1.0 / SCREEN_MIN_PERIOD))

    spikes = ((logmag - background) > PEAK_EXCESS) & band
    if not spikes.any():
        return None

    size = NOTCH_RADIUS * 2 + 1
    grown = cv2.dilate(spikes.astype(np.uint8),
                       cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (size, size)))
    holes = cv2.GaussianBlur(grown.astype(np.float32), (0, 0), NOTCH_RADIUS * 0.6)
    return np.clip(1.0 - holes, 0.0, 1.0)


def descreen(img, limit=CORRECTION_LIMIT):
    """Take the periodic screen out, a channel at a time."""
    channels = []
    for channel in cv2.split(img):
        source = channel.astype(np.float32)
        spectrum = np.fft.fftshift(np.fft.fft2(source))
        mask = _notch_mask(np.abs(spectrum))
        if mask is None:
            channels.append(channel)
            continue
        notched = np.real(np.fft.ifft2(np.fft.ifftshift(spectrum * mask)))
        correction = np.clip(source - notched, -limit, limit)
        channels.append(np.clip(source - correction, 0, 255).astype(np.uint8))
    return cv2.merge(channels)


def despeckle(img, strength=STRENGTH):
    return cv2.fastNlMeansDenoisingColored(
        img, None, strength, strength, TEMPLATE_WINDOW, SEARCH_WINDOW)


def resample(img, long_side=TARGET_LONG_SIDE):
    """Down to the given long side, by area, which averages rather than samples.
    0 leaves the resolution alone, which is what the default asks for."""
    if not long_side:
        return img
    height, width = img.shape[:2]
    scale = long_side / max(height, width)
    if scale >= 1.0:
        return img
    size = (max(1, round(width * scale)), max(1, round(height * scale)))
    return cv2.resize(img, size, interpolation=cv2.INTER_AREA)


def process(img, strength=STRENGTH, long_side=TARGET_LONG_SIDE):
    return resample(despeckle(descreen(img), strength), long_side)


def save(img, path, quality=JPEG_QUALITY):
    rgb = Image.fromarray(cv2.cvtColor(img, cv2.COLOR_BGR2RGB))
    if path.lower().endswith('.png'):
        rgb.save(path, format='PNG')
    else:
        rgb.save(path, format='JPEG', quality=quality, subsampling=0)


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("input", help="An image, or a directory of images.")
    parser.add_argument("-o", "--output",
                        help="Where to write. Defaults to '<input>-despeckled'.")
    parser.add_argument("--strength", type=int, default=STRENGTH,
                        help=f"How hard to average (default {STRENGTH}).")
    parser.add_argument("--long-side", type=int, default=TARGET_LONG_SIDE,
                        help="Resample the long side to this. 0, the default, leaves the "
                             "resolution alone; 1038 is MPC's cut line and 1110 the "
                             "bleed sheet.")
    parser.add_argument("--quality", type=int, default=JPEG_QUALITY,
                        help=f"JPEG quality (default {JPEG_QUALITY}).")
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

    done, before, after = 0, 0, 0
    for path in files:
        name = os.path.basename(path)
        if is_not_for_this_pass(name):
            print(f"[SKIP] {name} (a .bleed copy from lotrlcg-enhanced, "
                  f"already descreened upstream)")
            continue

        img = cv2.imread(path, cv2.IMREAD_COLOR)
        if img is None:
            print(f"[SKIP] {name} (could not read)")
            continue

        out_path = os.path.join(output_dir, name)
        # Read before writing: -o may be the folder being read from, and after
        # the write the source size is the written one.
        source_size = os.path.getsize(path)

        result = process(img, args.strength, args.long_side)
        save(result, out_path, args.quality)

        before += source_size
        after += os.path.getsize(out_path)
        done += 1
        print(f"[OK]   {name}: {img.shape[1]}x{img.shape[0]} -> "
              f"{result.shape[1]}x{result.shape[0]}")

    print(f"\nSummary: {done} processed.")
    if done:
        print(f"Size: {before/1e6:.0f}MB -> {after/1e6:.0f}MB")


if __name__ == '__main__':
    main()
