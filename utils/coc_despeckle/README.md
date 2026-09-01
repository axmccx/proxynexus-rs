# Call of Cthulhu Despeckle

Takes the print screen out of Call of Cthulhu scans and resamples them for print.

The cards are offset-printed, so the scans carry the rosette of a ~169 line/inch screen, 3.5px
across at the ~590dpi they were scanned at.

```bash
uv run coc_despeckle.py ~/Pictures/proxynexus_collections/coclcg -o despeckled
uv run coc_despeckle.py scans/ --strength 5      # gentler
uv run coc_despeckle.py scans/ --jobs 12         # more at once
```

Non-local means is the slow part, a couple of seconds a card, so cards are worked on one per
process — 1583 of them take about an hour one at a time and ten minutes spread across a machine's
cores. Nothing about a card depends on the others, so the output does not depend on how many run
at once.

Two passes, in this order:

1. **Non-local means.** It reads a pixel from other patches that look like its neighbourhood, so
   flat paper is averaged hard and the edge of a letter is not. Measured on a card's text box, it
   takes the screen down by a third while holding the type.
2. **An area resample to print resolution.** At 300dpi the screen lands at 1.8px, right at the
   limit of what the print can carry, and a poor resampler turns it into moiré rather than
   averaging it out. Doing it here settles which resampler gets used, and takes the collection to
   about a third of its size.

Denoising comes first on purpose. Shrinking first runs three times quicker but is measurably
softer on the type, because by then the screen and the type are the same size.

## Why it downscales here, and not later

The long side defaults to **1038px**, the height Proxy Nexus lays a card out at inside MPC's
816x1110 bleed sheet. At that size nothing further has to scale: `add_mpc_bleed_border` finds the
image already fills the cut line, passes it through untouched, and the sheet comes out at exactly
MPC's 300dpi spec.

That matters because **Proxy Nexus never downscales**. It resamples only when an image is *smaller*
than the cut line, scaling it up with Lanczos3; anything larger passes through and the bleed sheet
is built proportionally bigger around it. A full-resolution card reaches MPC as a 1599x2174 sheet
against a spec of 816x1110, and MPC's own resampler decides how it lands.

Both were measured on this collection, comparing at print size:

| at print size | screen left on paper | edge kept on type |
|---|---|---|
| downscaled here, by area | 6.76 | 35.5 |
| kept large, resampled by area | 6.72 | 35.4 |
| kept large, resampled by lanczos | 7.57 | 49.9 |
| kept large, resampled by bilinear | 7.09 | 42.8 |

Resampled by area the two are indistinguishable, so downscaling here costs nothing. Under a sharper
filter the type comes out crisper and some of the print screen returns — visibly, in flat colour.
Downscaling at this step is what keeps that choice here rather than leaving it to whoever prints.

`--long-side 0` turns the resample off, for an archival copy at scan resolution. That copy is not
the one to build a collection from.

Runs last, on images whose corners and edges have already been cleaned. Part of the Call of
Cthulhu pipeline — see [the renamer's README](../image_file_renamers/coclcg/README.md) for where
it sits.
