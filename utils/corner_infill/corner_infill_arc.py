# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy>=1.24.0", "opencv-python>=4.8.0", "Pillow"]
# ///
"""Remove the solid ground from a card image's rounded corners, whatever its colour.

`corner_infill.py` keys on white and `corner_infill_dark.py` floods from a dark ground; both
need to know what the ground looks like, and both fail when the card's own border matches it.
This one measures each corner instead, via `corner_trace.ground_mask`, so it handles a light
ground, a dark one, and the differing corner radii that come of sources cropping the card more
or less tightly. The traced region is masked and inpainted, which pulls the card's edge outward
into the corner.

Only corners that actually hold a ground are touched. `corner_audit.audit_corners` reports a
corner as 'art' when picture content runs to the edge, and full-bleed art is worth keeping; it
reports 'matched' when a ground is present but the same colour as the card border, which prints
identically either way. Both are left alone unless --all is given.

Originals are never modified: output goes to a separate directory.
"""
import argparse, os, shutil, sys
import numpy as np, cv2
from PIL import Image
from PIL.JpegImagePlugin import get_sampling
from concurrent.futures import ProcessPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from corner_audit import audit_corners
from corner_trace import ground_mask, TOL

GROUND = {"white", "light", "black", "dark", "mid"}
INPAINT_RADIUS = 4
DILATE = 3          # px the mask grows, to swallow the arc's antialiased edge


def save_like(src_path, out_path, bgr):
    """Write `bgr` to out_path using the source file's own encoding settings.

    Re-encoding at some fixed quality inflates the file for nothing: a source saved at quality 80
    rewritten at 95 nearly doubles in size while looking no better. Reusing the source's own
    quantization tables keeps the size, and the pixels the mask never touched, close to where
    they started. PNG is lossless either way, so it is only a matter of compressing as hard as
    the source did rather than as fast as OpenCV defaults to.
    """
    out = Image.fromarray(cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB))
    if os.path.splitext(out_path)[1].lower() in (".jpg", ".jpeg"):
        with Image.open(src_path) as src:
            qtables = getattr(src, "quantization", None)
            subsampling = get_sampling(src)
            progressive = "progression" in src.info or "progressive" in src.info
        kwargs = {"optimize": True, "progressive": progressive, "subsampling": subsampling}
        if qtables:
            kwargs["qtables"] = qtables
        out.save(out_path, "JPEG", **kwargs)
    else:
        out.save(out_path, "PNG", optimize=True, compress_level=9)


def process(args):
    path, out_path, tol, dilate, do_all, copy_through = args
    def passthrough(status):
        if out_path and copy_through:
            shutil.copy2(path, out_path)
        return path, None, status

    img = cv2.imread(path, cv2.IMREAD_COLOR)
    if img is None:
        return passthrough("unreadable")
    verdicts = audit_corners(img)
    if verdicts is None:
        return passthrough("too small")
    if do_all:
        which = [c["corner"] for c in verdicts]
    else:
        which = [c["corner"] for c in verdicts if c["verdict"] in GROUND]
    if not which:
        return passthrough("no ground")
    if out_path is None:
        return path, len(which), "would fill"
    h, w = img.shape[:2]
    mask, _ = ground_mask(img, tol=tol, corners=set(which))
    if not mask.any():
        return passthrough("no ground")
    if dilate:
        mask = cv2.dilate(mask, np.ones((dilate, dilate), np.uint8), iterations=1)
    filled = cv2.inpaint(img, mask, INPAINT_RADIUS, cv2.INPAINT_NS)
    save_like(path, out_path, filled)
    return path, len(which), "filled"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("input", help="Directory of card images.")
    ap.add_argument("-o", "--output", help="Output directory. Defaults to '<input>-infilled'.")
    ap.add_argument("--tol", type=int, default=TOL,
                    help=f"Per-channel distance from the corner colour still read as ground (default {TOL}).")
    ap.add_argument("--dilate", type=int, default=DILATE, help="Mask dilation in px.")
    ap.add_argument("--all", action="store_true",
                    help="Fill every corner, including 'art' and 'matched' ones.")
    ap.add_argument("--only-changed", action="store_true",
                    help="Write only the images that were filled, instead of the whole folder.")
    ap.add_argument("--dry-run", action="store_true", help="Report what would be filled.")
    args = ap.parse_args()

    src = os.path.abspath(args.input.rstrip(os.sep))
    dst = args.output or f"{src}-infilled"
    files = sorted(f for f in os.listdir(src) if f.lower().endswith((".png", ".jpg", ".jpeg")))
    if not args.dry_run:
        os.makedirs(dst, exist_ok=True)

    jobs = [(os.path.join(src, f),
             None if args.dry_run else os.path.join(dst, f),
             args.tol, args.dilate, args.all, not args.only_changed) for f in files]
    tally, corners = {}, 0
    with ProcessPoolExecutor() as ex:
        for _, n, status in ex.map(process, jobs, chunksize=16):
            tally[status] = tally.get(status, 0) + 1
            corners += n or 0
    for k in sorted(tally, key=lambda k: -tally[k]):
        print(f"  {tally[k]:6}  {k}")
    n_img = tally.get("filled", 0) or tally.get("would fill", 0)
    where = "(dry run)" if args.dry_run else f"-> {dst}"
    print(f"{corners} corners across {n_img} images {where}")


if __name__ == "__main__":
    main()
