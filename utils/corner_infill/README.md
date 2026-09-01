# Corner Infill Utility

Physical cards have rounded corners. When scanned we'll have the white scanner background showing
in the corners of the rectangular images.

This script uses OpenCV to detect those white corners, and fills them in using the Navier-Stokes 
inpainting algorithm (`cv2.inpaint`).

The following command process a folder of raw scanned card images, 
and save their infilled version to `./raw_scans-infilled/`):

```bash
uv run corner_infill.py ./raw_scans/
```

## Corners on a dark ground

Some sources put the card on black instead of white, and the white detector never fires on those.
`corner_infill_dark.py` floods outward from each image corner and takes only what it reaches,
which separates a dark ground from a card's own dark border. See the script's docstring for why
thresholding for "dark" does not work.

```bash
uv run corner_infill_dark.py ./raw_scans/
uv run corner_infill_dark.py ./raw_scans/ --debug     # also writes the mask, in magenta
```

The defaults are for a card composited onto pure black. A flatbed scan of a card laid on a dark
surface has a ground that is neither black nor even, and needs both thresholds raised:

```bash
uv run corner_infill_dark.py ./raw_scans/ --background-cutoff 150 --flood-tolerance 60
```

`--background-cutoff` is how bright a corner pixel may be and still be read as ground; below it,
that corner is left alone. `--flood-tolerance` is how far the ground may vary from the corner it
is reached from. Both are safe to raise well past the ground's own brightness as long as they stay
clear of the card's edge, which is what stops the flood — the run these were measured on has a
ground reaching 130 and a white card border above 225.

Read the per-file lines afterwards. A file whose fill is several times the others' is one whose
card border is as dark as the ground, so the flood ran past the card edge; it is clipped to a
square at each corner, so the damage is bounded, but it is worth a look.
