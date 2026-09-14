# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy>=1.24.0", "opencv-python>=4.8.0"]
# ///
"""Before-and-after sheets showing what the trim does to the bleed MPC receives.

The trim itself is a few pixels and does not read on a card; what reads is the bleed built from
the pixels it removes. So each card is shown the way print_prep builds it -- the outermost pixel
of every row and column repeated outward -- from the original and from the trimmed copy, with
the cut line marked. A strip that was left in place shows as a band of solid colour around the
original that the trimmed copy does not have.

    uv run bleed_preview.py ORIGINALS TRIMMED -o sheets/ [--limit 40]
"""
import argparse, os
import numpy as np, cv2

BLEED_W, BLEED_H, CUT_W, CUT_H = 816.0, 1110.0, 744.0, 1038.0
CARD_W = 300          # width each card is shown at, whole-card
ZOOM = 5              # magnification for the corner view
ZOOM_CARD = 26        # px of card shown beside the bleed in the corner view


def add_bleed(img):
    """print_prep's generate_bleed: edge pixels repeated out to the MPC bleed size."""
    h, w = img.shape[:2]
    scale = max(w / CUT_W, h / CUT_H)
    ow, oh = round(BLEED_W * scale), round(BLEED_H * scale)
    bx, by = (ow - w) // 2, (oh - h) // 2
    return cv2.copyMakeBorder(img, by, oh - h - by, bx, ow - w - bx, cv2.BORDER_REPLICATE), (bx, by)


def panel(path, label, zoom):
    """The card as MPC receives it: whole, or the top-left corner of the bleed magnified."""
    img = cv2.imread(path, cv2.IMREAD_COLOR)
    if img is None:
        return None
    bled, (bx, by) = add_bleed(img)

    if zoom:
        n = ZOOM_CARD
        crop = bled[:by + n, :bx + n]
        out = cv2.resize(crop, (crop.shape[1] * ZOOM, crop.shape[0] * ZOOM),
                         interpolation=cv2.INTER_NEAREST)
        x0, y0 = bx * ZOOM, by * ZOOM
        cv2.line(out, (x0, 0), (x0, out.shape[0]), (255, 0, 255), 1)
        cv2.line(out, (0, y0), (out.shape[1], y0), (255, 0, 255), 1)
        width = out.shape[1]
    else:
        h, w = bled.shape[:2]
        out = cv2.resize(bled, (CARD_W, round(CARD_W * h / w)), interpolation=cv2.INTER_AREA)
        s = CARD_W / w
        x0, y0 = round(bx * s), round(by * s)
        cv2.rectangle(out, (x0, y0), (out.shape[1] - x0 - 1, out.shape[0] - y0 - 1),
                      (255, 0, 255), 1)
        width = CARD_W

    tag = np.zeros((20, width, 3), np.uint8)
    cv2.putText(tag, label, (4, 14), cv2.FONT_HERSHEY_SIMPLEX, 0.42, (255, 255, 255), 1)
    return np.vstack([tag, out])


def main():
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("originals")
    p.add_argument("trimmed", help="folder of trimmed files; only these are shown")
    p.add_argument("-o", "--output", required=True)
    p.add_argument("--limit", type=int, default=40)
    p.add_argument("--per-row", type=int, default=6)
    p.add_argument("--zoom", action="store_true",
                   help="magnify the top-left corner of the bleed instead of showing the card")
    a = p.parse_args()

    src, trim = os.path.expanduser(a.originals), os.path.expanduser(a.trimmed)
    out_dir = os.path.expanduser(a.output)
    os.makedirs(out_dir, exist_ok=True)

    names = sorted(f for f in os.listdir(trim) if f.lower().endswith((".jpg", ".jpeg", ".png")))
    step = max(1, len(names) // a.limit)
    names = names[::step][:a.limit]

    pairs = []
    for n in names:
        before = panel(os.path.join(src, n), f"{n}  before", a.zoom)
        after = panel(os.path.join(trim, n), "after", a.zoom)
        if before is None or after is None:
            continue
        h = max(before.shape[0], after.shape[0])
        cell = [np.pad(x, ((0, h - x.shape[0]), (0, 8), (0, 0))) for x in (before, after)]
        pairs.append(np.hstack(cell))

    written = 0
    for i in range(0, len(pairs), a.per_row):
        row_cells = pairs[i:i + a.per_row]
        h = max(c.shape[0] for c in row_cells)
        row = np.hstack([np.pad(c, ((0, h - c.shape[0]), (0, 0), (0, 0))) for c in row_cells])
        path = os.path.join(out_dir, f"bleed_{i // a.per_row + 1:02d}.png")
        cv2.imwrite(path, row)
        written += 1
        print(path)
    print(f"{written} sheets, {len(pairs)} cards")


if __name__ == "__main__":
    main()
