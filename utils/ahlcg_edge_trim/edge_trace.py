"""Measure how far the sheet's ground reaches in from each straight edge of a card image.

The sheets SCED cuts from lay their cards on a dark ground, and a card does not always fill the
grid cell it sits in, so a slot can come away with a few columns of that ground attached along a
whole edge. `corner_infill` does not see it: the strip is straight, not a corner wedge, and the
corner arcs it fills are only its ends.

It matters because bleed is built by repeating the outermost pixel, so two columns of ground on
an edge become the entire bleed on that side -- the black line MPC rejects the file for.

A strip shows itself two ways, and an edge is read for both because neither alone covers the
material:

  flat   The ground is solid and far from the card behind it. Reliable when the strip is wide
         enough to survive JPEG chroma subsampling, which is where most of them are.
  step   A strip only a pixel or two wide is blended with the card by that same subsampling and
         is no longer flat, but the boundary where it ends is still abrupt *and in the same
         direction at every point along the edge*. Art does not do that; a gradient running to
         the edge moves a few pixels at a time and not all one way, which is what separates the
         two, and a card whose border is one solid colour never steps at all.

Both are measured against a reference taken from well inside the edge, so a card whose own
border really is a flat dark line is left alone: there is no step to find.
"""
import numpy as np

FLAT_STD = 8.0       # along-edge luminance spread a solid ground stays under
COLOUR_TOL = 14      # per-channel distance from the outermost ring that is still the same strip
STEP = 40.0          # how far a flat strip must sit from the card behind it to be foreign
CONTRAST = 70.0      # the same for a blended strip, which is only read from its boundary and
                     # so needs the wider margin: a card's own edge softens by 20-40 under
                     # resampling, and nothing short of this tells the two apart
JUMP = 18.0          # median luminance change that marks the strip's inner boundary
AGREE = 0.70         # share of the edge that must cross that boundary the same way
MAX_FRAC = 0.008     # never take more than this share of the width from one edge
STEP_FRAC = 0.004    # and no more than this from a boundary alone: a strip wide enough to
                     # need more is wide enough to be flat, and reading only a boundary that
                     # deep starts finding the edges of the art instead
CORNER_FRAC = 0.14   # share of the width at each end of an edge that is corner, not edge
REF_DEPTH = 5        # rings past the cap averaged for the card reference
EDGES = ("top", "bottom", "left", "right")


def _ring(img, edge, k, corner):
    h, w = img.shape[:2]
    if edge == "top":
        return img[k, corner:w - corner]
    if edge == "bottom":
        return img[h - 1 - k, corner:w - corner]
    if edge == "left":
        return img[corner:h - corner, k]
    return img[corner:h - corner, w - 1 - k]


def _lum(a):
    return 0.114 * a[..., 0] + 0.587 * a[..., 1] + 0.299 * a[..., 2]


def _flat_depth(meds, stds, cap, flat_std, colour_tol):
    """Rings from the edge that are solid and hold the outermost ring's colour."""
    depth = 0
    for k in range(cap):
        if stds[k] > flat_std or np.abs(meds[k] - meds[0]).max() > colour_tol:
            break
        depth = k + 1
    return depth


def _step_depth(lums, lines, ref, cap, contrast, jump, agree):
    """The deepest abrupt, edge-wide boundary that has only foreign matter outside it."""
    direction = 1.0 if ref > lums[0] else -1.0
    best = 0
    for d in range(1, cap + 1):
        if (ref - lums[d - 1]) * direction < contrast:
            break
        if (lums[d] - lums[d - 1]) * direction < jump:
            continue
        crossing = (_lum(lines[d]) - _lum(lines[d - 1])) * direction
        if float((crossing > jump / 2).mean()) >= agree:
            best = d
    return best


def strip_depth(img, edge, flat_std=FLAT_STD, colour_tol=COLOUR_TOL, step=STEP,
                contrast=CONTRAST, jump=JUMP, agree=AGREE, max_frac=MAX_FRAC,
                step_frac=STEP_FRAC, corner_frac=CORNER_FRAC, ref_depth=REF_DEPTH):
    """How many rings in from `edge` are the sheet's ground. 0 when the card reaches the edge.

    The reference sits just past the cap rather than at some fixed depth, because these cards
    carry a bright frame a dozen pixels wide and then go dark: a window far enough in to clear
    that frame is reading the art, not the border the strip has to be told apart from.
    """
    h, w = img.shape[:2]
    corner = int(w * corner_frac)
    cap = max(1, int(w * max_frac))
    depth = cap + ref_depth + 1
    if corner * 2 >= min(h, w) or depth >= min(h, w) // 2:
        return 0

    lines = [_ring(img, edge, k, corner).astype(np.float32) for k in range(depth)]
    meds = [np.median(l, axis=0) for l in lines]
    lums = [float(np.median(_lum(l))) for l in lines]
    stds = [float(_lum(l).std()) for l in lines]
    ref = float(np.median(lums[cap:cap + ref_depth]))

    flat = _flat_depth(meds, stds, cap, flat_std, colour_tol)
    # A flat run is only ground when the card behind it looks different; a card whose border is
    # one solid colour is flat to the reference and has nothing to cut.
    if flat and abs(ref - lums[0]) < step:
        flat = 0

    step_cap = max(1, int(w * step_frac))
    return max(flat, _step_depth(lums, lines, ref, step_cap, contrast, jump, agree))


def trim_box(img, **kw):
    """Rings to remove from each edge."""
    return {e: strip_depth(img, e, **kw) for e in EDGES}
