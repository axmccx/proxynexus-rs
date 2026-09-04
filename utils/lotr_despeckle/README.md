# LotR Despeckle

Takes the print screen out of the LotR cards that still carry one.

Most of the collection does not. The Enhanced Proxies, which supply 3,574 of `lotrlcg-enhanced`'s
3,922 images, were denoised and sharpened before they were published and their screen is already
gone; so are the ALeP and FFG images, which are digital sources rather than scans, and the
Nightmare remasters. Two groups are left, and they are not the same problem:

| | what they are | screen at | strength |
|---|---|---|---|
| 348 images `rename.py` takes from `Lord of the Rings LCG` and `Lord of the Rings LCG RAW` | flatbed scans of the printed cards | 2.4-3.4px | 8 |
| the four cards in `lotrlcg-gap-fills/` from lotr.cardgame.tools | renders of the offset-printed card | 3.5px | 14, the default |

A render's screen is a sharp spike and the notch takes out nearly all of it. A scan's is broadened
by the scanner, so the notch leaves 25-33% against 19% on a render and non-local means has to do
more of the work — hence the lower strength on the group with the *worse* screen. See
*Which strength* below.

```bash
uv run lotr_despeckle.py ~/Downloads/cardgame-tools-lotr-infilled -o despeckled
uv run lotr_despeckle.py scans/ --strength 8      # gentler
```

The scan archives do not go through this command line. `rename.py` loads this module and calls
`descreen()` and `despeckle()` itself, so the pass runs on the scan's own pixels rather than on
quality-90 artefacts of the screen — see [the renamer's
README](../image_file_renamers/lotrlcg/README.md#how-it-maps). It also means the `.bleed` skip
below never sees them.

About 1.3s a card, so the four here are instant and the renamer's 348 add about seven minutes to a
run. Neither needs the process pool [the Call of Cthulhu pass](../coc_despeckle/README.md) uses for
its 1583 cards.

Two passes:

1. **A notch on the screen itself.** A halftone is periodic, so it stands in the spectrum as a few
   sharp spikes — here the rosette at 3.53px, at the usual 15/45/75° separation angles, 60 to 176
   times its own neighbourhood. Picture detail is not periodic and spreads smoothly, so a spike
   that far above its surroundings is screen and almost nothing else. This does most of the work.
2. **Non-local means**, on what the notch leaves. That residue is ordinary grain rather than a
   screen, which is the case non-local means is actually good at, so it runs much gentler here
   than it would have to alone.

The resolution is left alone — see *Why it does not downscale* below. `--long-side` resamples, by
area, for a collection built on MPC's cut line the way Call of Cthulhu's is.

## Why a notch, and not just non-local means

**Where to look matters more than the settings.** The screen is worst in the shadows. Where the ink
is near solid the dots are the paper showing through, so they are bright against dark and at their
highest contrast; on the light card stock behind the type they are faint. A measurement taken on
flat paper — the obvious place — reports a screen that is barely there and picks a setting far too
weak. Judge this on a dark area of the art, at 1:1. A screenshot scaled to fit will beat against
the screen and invent a pattern of its own, which is its own way of reading this wrong.

**Non-local means is the wrong tool for a screen, on its own.** It averages a pixel from other
patches that look like its neighbourhood, so a regular rosette — the most self-similar thing on
the card — is the case it *preserves*. The Call of Cthulhu pass gets away with 8 because those are
flatbed scans and the scanner has already softened their screen into something the filter reads as
noise. These are renders, and theirs is sharp: at 8 almost nothing moved, and 20 still left a fifth
of it while the art was going flat.

**The notch removes it directly and costs almost nothing.** On the four renders — screen band in a
dark patch, against the untouched image, with detail measured as the strong gradients across the
card:

| method | screen left | detail kept |
|---|---|---|
| source | 100% | 100% |
| non-local means 20, alone | 22% | 91% |
| notch | 19% | 93% |
| notch + non-local means 6 | 16% | 93% |
| notch + non-local means 10 | 6% | 92% |
| **notch + non-local means 14** | **2%** | **91%** |

The pair leaves a tenth of the screen that non-local means alone did, at the same detail.

**The correction has to be bounded.** A notch rings against a hard edge, and the overshoot clips —
1.8% of pixels, which showed as black speckle along the card's metal ornament. The screen has a
bounded amplitude so its correction does too, and capping the per-pixel change at
`CORRECTION_LIMIT` keeps the descreen and drops the ringing.

**A blur is not the alternative.** A Gaussian heavy enough to touch the screen takes the type with
it in proportion, and every resampler tested traded the two 1:1. Neither separates the screen from
the picture the way a notch does, because only the notch uses the one property that tells them
apart: the screen is periodic and the picture is not.

The screen is not even across the four. In a flat patch it measures 23.4 on Abandoned Camp, 15.7 on
Crumbling Stairs, 6.3 on Obsidian Arrows and 4.8 on Wild Wargs, so Abandoned Camp is the card to
judge on and the one this was checked against.

**The type was never the constraint.** It is unchanged across every setting tried — the strokes are
far coarser than the screen, so nothing ever confused the two.

## Which strength

14 for the renders, 8 for the scans, and the reason is the notch rather than the screen.

A render's rosette is a clean spike, so the notch takes nearly all of it and non-local means is only
tidying up grain. A scan's has been through the scanner's optics and is broadened into a peak the
notch cannot fully cover: on the scan archives it leaves 25-33%, against 19% on a render. The rest
has to come from non-local means, so a *worse* screen wants a *lower* strength — because most of
what is left for the filter is no longer periodic.

Twelve scan-sourced cards, notch first in every row:

| method | screen left | detail kept |
|---|---|---|
| non-local means 8, alone | 52-72% | — |
| notch, alone | 25-33% | — |
| notch + non-local means 6 | 10.1% | 91.4% |
| **notch + non-local means 8** | **5.7%** | **90.5%** |
| notch + non-local means 14 | 2.5% | 86.0% |

Judged at 1:1 on a dark patch, 6 and 8 are indistinguishable on the lighter-screen packs, and 6
leaves visible dotting on the Two-Player Starter cards, which carry the heaviest screen in the
archive. 14 clears those but flattens the art.

Read that detail column as a floor. It counts strong gradients, and on these cards a large share of
the strong gradients *are* the screen, so taking it out reads as detail lost — the 86% at 14 looks
worse than it is, and the gap between 8 and 14 is smaller in the pixels than in the number. The
column separates settings; it does not measure damage. The type is unchanged at every strength.

## Why it does not downscale

The Call of Cthulhu pass resamples to 1038px, MPC's 300dpi cut line, and argues hard for it: Proxy
Nexus never downscales, so anything larger reaches MPC oversized and is resampled by a filter
nobody chose, which brought that collection's screen back.

Neither half of that applies here.

**The collection is not built that way.** `lotrlcg-enhanced` sits at 1568x2140 with bleed, about
578dpi on the card, and these four arrive at 1468x2080, about 592dpi. They already match. Putting
only these on the 300dpi cut line would make them the one set of cards in the collection at half
everyone else's resolution — which is what the first version of this did, and it showed.

**There is no screen left to come back.** That was the real cost of letting MPC resample, and at
the screen is gone. Measured at print size, against downscaling here:

| at print size | screen left | detail |
|---|---|---|
| downscaled here to 1038 | 100% | 100% |
| kept at 1468, MPC resamples by area | 100% | 100% |
| kept at 1468, MPC resamples by bilinear | 113% | 105% |
| kept at 1468, MPC resamples by lanczos | 119% | 109% |

Under the sharpest filter MPC might use, keeping full resolution leaves 19% more residual and 9%
more detail. That is a trade worth taking, and it was not: with the screen still in, the same
comparison ran far worse.

`--long-side` resamples anyway — 1038 for the cut line, 1110 for a `.bleed` image, which differ by
the width of the bleed.

That argument is about the four renders. The scan archives never reach this question: `rename.py`
calls `descreen()` and `despeckle()` and leaves `resample()` alone, and the images it writes sit at
1632x2220 or about 1465x2090, alongside the Enhanced Proxies' 1568x2140.

For the four, this runs on images whose corners have already been filled, so after
[`corner_infill_dark.py`](../corner_infill/README.md) and before they are renamed by hand. See
[the renamer's README](../image_file_renamers/lotrlcg/README.md#hand-filled-gaps) for where it sits.

## Tests

```bash
uv run --with pytest --with opencv-python --with numpy --with pillow --no-project \
  pytest utils/lotr_despeckle/tests/ -v
```

Covers the notch taking a periodic screen out, beating non-local means alone on one, leaving a hard
edge alone, bounding its correction, and doing nothing to an image with no screen; the type
surviving every strength offered; the default leaving the resolution alone and a zero long side not
collapsing the image; the resample's target, aspect
and orientation when one is asked for, area resampling not aliasing the screen into moiré, the
`.bleed` copies being skipped, the size summary when writing over the input, and 4:4:4 chroma. No file I/O beyond `tmp_path` and no image
fixtures, so the suite passes on a fresh clone.
