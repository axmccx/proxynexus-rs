"""Trace the exact ground region in a card image's corner, whatever its colour or radius.

A fixed arc radius does not fit this material: sources crop differently, so the same card can
carry a 30px corner in one file and an 80px one in another. Tracing measures it instead. From
the tip outward, each row's ground is the run of pixels that still match the tip's colour; a
convex corner makes those runs non-increasing down the rows, and enforcing that is what stops a
run escaping along a card border that happens to match the ground.
"""
import numpy as np

TOL = 18          # per-channel distance from the tip colour that still counts as ground
BOX_FRAC = 0.14   # corner box as a fraction of image width; the arc must fit inside it
MAX_FILL = 0.55   # give up past this share of the box: ground and card are indistinguishable


def trace_corner(box, tol=TOL):
    """Boolean mask of the ground in `box`, which must be oriented with the tip at (0, 0)."""
    n = box.shape[0]
    ground = box[0, 0].astype(np.int16)
    near = (np.abs(box.astype(np.int16) - ground).max(axis=2) <= tol)
    mask = np.zeros((n, n), bool)
    prev = n
    for y in range(n):
        row = near[y]
        run = int(np.argmin(row)) if not row.all() else n
        run = min(run, prev)
        if run <= 0:
            break
        mask[y, :run] = True
        prev = run
    return mask


def corner_boxes(h, w, box_frac=BOX_FRAC):
    """Yield (name, y-slice, x-slice, flip-y, flip-x) for each corner."""
    n = max(8, int(w * box_frac))
    n = min(n, min(h, w) // 2)
    for name, fy, fx in (("tl", 1, 1), ("tr", 1, -1), ("bl", -1, 1), ("br", -1, -1)):
        ys = slice(h - n, h) if fy == -1 else slice(0, n)
        xs = slice(w - n, w) if fx == -1 else slice(0, n)
        yield name, ys, xs, fy, fx, n


def ground_mask(img, tol=TOL, box_frac=BOX_FRAC, corners=None):
    """Full-image uint8 mask of corner ground, plus the per-corner fill fraction."""
    h, w = img.shape[:2]
    mask = np.zeros((h, w), np.uint8)
    fills = {}
    for name, ys, xs, fy, fx, n in corner_boxes(h, w, box_frac):
        if corners is not None and name not in corners:
            continue
        box = np.ascontiguousarray(img[ys, xs][::fy, ::fx])
        m = trace_corner(box, tol)
        frac = m.sum() / (n * n)
        fills[name] = float(frac)
        if frac > MAX_FILL:
            continue
        mask[ys, xs] |= (m[::fy, ::fx].astype(np.uint8) * 255)
    return mask, fills
