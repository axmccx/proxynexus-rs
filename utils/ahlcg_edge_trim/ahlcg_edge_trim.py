# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy>=1.24.0", "opencv-python>=4.8.0", "Pillow"]
# ///
"""Cut the sheet ground off the straight edges of card images.

See edge_trace.py for what the strip is and why it has to go. This walks a folder, measures
each file's four edges, and writes a copy cropped to the card. Files with nothing to take are
copied through unchanged, so the output is a complete collection either way.

    uv run ahlcg_edge_trim.py ~/Pictures/proxynexus_collections/ahlcg/ahlcg-tts --dry-run
    uv run ahlcg_edge_trim.py ~/Pictures/proxynexus_collections/ahlcg/ahlcg-tts -o ahlcg-tts-trimmed

Originals are never modified.
"""
import argparse, os, shutil, sys, json
from concurrent.futures import ProcessPoolExecutor

import numpy as np, cv2
from PIL import Image
from PIL.JpegImagePlugin import get_sampling

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from edge_trace import (trim_box, EDGES, FLAT_STD, COLOUR_TOL, STEP, CONTRAST, JUMP, AGREE,
                        MAX_FRAC, STEP_FRAC)

EXTS = (".jpg", ".jpeg", ".png")


def save_like(src_path, out_path, bgr):
    """Write `bgr` using the source file's own encoding settings, so a crop of a few pixels
    does not re-encode the whole card at some unrelated quality."""
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


def process(job):
    path, out_path, params, copy_through = job
    name = os.path.basename(path)
    img = cv2.imread(path, cv2.IMREAD_COLOR)
    if img is None:
        if out_path and copy_through:
            shutil.copy2(path, out_path)
        return name, None, "unreadable"

    box = trim_box(img, **params)
    if not any(box.values()):
        if out_path and copy_through:
            shutil.copy2(path, out_path)
        return name, box, "clean"

    h, w = img.shape[:2]
    cropped = img[box["top"]:h - box["bottom"], box["left"]:w - box["right"]]
    if cropped.size == 0:
        if out_path and copy_through:
            shutil.copy2(path, out_path)
        return name, box, "empty crop"
    if out_path:
        save_like(path, out_path, cropped)
    return name, box, "trimmed"


def main():
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("source", help="folder of card images")
    p.add_argument("-o", "--output", help="folder to write to; required unless --dry-run")
    p.add_argument("--dry-run", action="store_true", help="report only, write nothing")
    p.add_argument("--only", help="file holding one file name per line, to limit the run")
    p.add_argument("--no-copy-through", action="store_true",
                   help="write only the files that were trimmed")
    p.add_argument("--json", help="write the per-file measurements here")
    p.add_argument("--workers", type=int, default=os.cpu_count())
    p.add_argument("--flat-std", type=float, default=FLAT_STD)
    p.add_argument("--colour-tol", type=int, default=COLOUR_TOL)
    p.add_argument("--step", type=float, default=STEP)
    p.add_argument("--contrast", type=float, default=CONTRAST)
    p.add_argument("--jump", type=float, default=JUMP)
    p.add_argument("--agree", type=float, default=AGREE)
    p.add_argument("--max-frac", type=float, default=MAX_FRAC)
    p.add_argument("--step-frac", type=float, default=STEP_FRAC)
    a = p.parse_args()

    source = os.path.expanduser(a.source)
    out_dir = os.path.expanduser(a.output) if a.output else None
    if not a.dry_run and not out_dir:
        p.error("-o/--output is required unless --dry-run")
    if out_dir:
        os.makedirs(out_dir, exist_ok=True)
        if os.path.abspath(out_dir) == os.path.abspath(source):
            p.error("output must differ from source; originals are not modified")

    names = sorted(f for f in os.listdir(source) if f.lower().endswith(EXTS))
    if a.only:
        keep = {l.strip() for l in open(os.path.expanduser(a.only)) if l.strip()}
        names = [f for f in names if f in keep]
    if not names:
        p.error(f"no images found in {source}")

    params = dict(flat_std=a.flat_std, colour_tol=a.colour_tol, step=a.step,
                  contrast=a.contrast, jump=a.jump, agree=a.agree,
                  max_frac=a.max_frac, step_frac=a.step_frac)
    copy_through = not a.no_copy_through
    jobs = [(os.path.join(source, f),
             None if a.dry_run else os.path.join(out_dir, f),
             params, copy_through) for f in names]

    results = []
    with ProcessPoolExecutor(a.workers) as pool:
        for i, r in enumerate(pool.map(process, jobs, chunksize=16), 1):
            results.append(r)
            if i % 500 == 0:
                print(f"  ... {i}/{len(names)}", file=sys.stderr)

    trimmed = [r for r in results if r[2] == "trimmed"]
    bad = [r for r in results if r[2] in ("unreadable", "empty crop")]

    print(f"\n{len(names)} images, {len(trimmed)} with a strip to cut, "
          f"{len(results) - len(trimmed) - len(bad)} already clean")

    per_edge = {e: sum(1 for _, b, _ in trimmed if b[e]) for e in EDGES}
    print("\nEdges carrying a strip")
    for e in EDGES:
        depths = [b[e] for _, b, _ in trimmed if b[e]]
        if depths:
            print(f"  {e:7s} {per_edge[e]:5d} files, {min(depths)}-{max(depths)}px "
                  f"(median {int(np.median(depths))})")
        else:
            print(f"  {e:7s}     0 files")

    worst = sorted(trimmed, key=lambda r: -sum(r[1].values()))[:15]
    print("\nDeepest cuts")
    for name, b, _ in worst:
        print(f"  {name:34s} l{b['left']:<3d} t{b['top']:<3d} r{b['right']:<3d} b{b['bottom']:<3d}")

    if bad:
        print(f"\nNeeds looking at ({len(bad)})")
        for name, _, status in bad:
            print(f"  {name:34s} {status}")

    if a.json:
        with open(os.path.expanduser(a.json), "w") as fh:
            json.dump({n: b for n, b, s in results if b}, fh, indent=1, sort_keys=True)
        print(f"\nMeasurements written to {a.json}")


if __name__ == "__main__":
    main()
