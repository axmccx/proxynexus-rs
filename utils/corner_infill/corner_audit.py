# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy>=1.24.0", "opencv-python>=4.8.0"]
# ///
"""Census of what sits in the rounded corners of a folder of card images.

Card images are usually rendered with the card's rounded corner already cut, leaving a solid
ground in each corner of the rectangular file. That ground only matters for printing when it
differs from the card's own edge: a white wedge on a dark card is a visible notch when the cut
drifts, and it is the wrong colour to extend into bleed. A black wedge against a black border
is neither.

So each corner is scored twice: the ground wedge outside the card's arc, and a thin band of the
card just inside it. A corner needs work when the wedge is flat (a solid ground rather than art)
and its luminance is far from the band's.

The arc radius is 1/8in on a 2.5in-wide card, so ~4.1% of image width; measured against this
collection the median lands at the same place.
"""
import argparse, json, os
import numpy as np, cv2
from concurrent.futures import ProcessPoolExecutor

R_FRAC = 0.041      # corner arc radius as a fraction of image width
BAND = 0.020        # thickness of the card band sampled just inside the arc
GUARD = 2           # px dropped either side of the arc, to dodge its antialiasing
FLAT_STD = 14       # a solid ground varies less than this
STEP = 35           # luminance gap at which the wedge becomes visible against the card
CORNERS = ("tl", "tr", "bl", "br")


def _masks(w):
    r = max(5, int(w * R_FRAC))
    b = max(3, int(w * BAND))
    n = r + b
    yy, xx = np.mgrid[0:n, 0:n]
    dist = np.hypot(r - 1 - xx, r - 1 - yy)
    wedge = (dist > r + GUARD) & (xx < r) & (yy < r)
    band = (dist <= r - GUARD) & (dist > r - b)
    return n, wedge, band


def _lum(bgr):
    return 0.114 * bgr[0] + 0.587 * bgr[1] + 0.299 * bgr[2]


def audit_corners(img):
    h, w = img.shape[:2]
    n, wedge, band = _masks(w)
    if n * 2 >= min(h, w) or not wedge.any():
        return None
    out = []
    for ci, (sy, sx) in enumerate([(1, 1), (1, -1), (-1, 1), (-1, -1)]):
        sub = img[::sy, ::sx][0:n, 0:n]
        wv, bv = sub[wedge], sub[band]
        g_bgr = np.median(wv, axis=0)
        c_bgr = np.median(bv, axis=0)
        std = float(cv2.cvtColor(wv.reshape(-1, 1, 3), cv2.COLOR_BGR2GRAY).std())
        gl, cl = _lum(g_bgr), _lum(c_bgr)
        step = abs(gl - cl)
        flat = std < FLAT_STD
        if not flat:
            verdict = "art"                       # corner carries picture, not a ground
        elif step < STEP:
            verdict = "matched"                   # ground present but invisible against the card
        else:
            verdict = "white" if gl > 235 else "light" if gl > 200 else \
                      "black" if gl < 20 else "dark" if gl < 60 else "mid"
        out.append(dict(corner=CORNERS[ci], std=round(std, 1), step=round(step, 1),
                        ground=[round(float(v)) for v in g_bgr[::-1]],
                        card=[round(float(v)) for v in c_bgr[::-1]], verdict=verdict))
    return out


def audit_file(path):
    img = cv2.imread(path, cv2.IMREAD_COLOR)
    if img is None:
        return None
    c = audit_corners(img)
    if c is None:
        return None
    return dict(f=os.path.basename(path), w=img.shape[1], h=img.shape[0], c=c)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("input", help="Directory of card images.")
    ap.add_argument("-o", "--output", help="Write the per-image census here as JSON.")
    args = ap.parse_args()
    files = sorted(os.path.join(args.input, f) for f in os.listdir(args.input)
                   if f.lower().endswith((".png", ".jpg", ".jpeg")))
    with ProcessPoolExecutor() as ex:
        rows = [r for r in ex.map(audit_file, files, chunksize=32) if r]
    tally, worst = {}, {}
    for r in rows:
        for c in r["c"]:
            tally[c["verdict"]] = tally.get(c["verdict"], 0) + 1
        bad = [c for c in r["c"] if c["verdict"] in ("white", "light", "black", "dark", "mid")]
        if bad:
            worst[r["f"]] = len(bad)
    print(f"{len(rows)} images, {len(rows) * 4} corners")
    for k in sorted(tally, key=lambda k: -tally[k]):
        print(f"  {tally[k]:7}  {k}")
    print(f"images with >=1 corner needing work: {len(worst)}")
    if args.output:
        json.dump(rows, open(args.output, "w"))
        print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
