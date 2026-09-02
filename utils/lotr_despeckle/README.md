# LotR Despeckle

Takes the print screen out of the four LotR cards downloaded from lotr.cardgame.tools and
resamples them for print.

Almost nothing in the LotR collection needs this. The Enhanced Proxies were denoised and sharpened
before they were published, and their screen is already gone. The four cards in
`lotrlcg-gap-fills/` are the exception: they are renders of the offset-printed card, and they
carry its rosette at 3.5px across the 1468x2080 they arrive at.

```bash
uv run lotr_despeckle.py ~/Downloads/cardgame-tools-lotr-infilled -o despeckled
uv run lotr_despeckle.py scans/ --strength 8      # gentler
```

Four files, a couple of seconds each, so there is nothing to parallelise — which is the one thing
this does not share with [the Call of Cthulhu pass](../coc_despeckle/README.md), whose 1583 cards
need a process pool.

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

**The notch removes it directly and costs almost nothing.** Screen band in a dark patch, against
the untouched image, with detail measured as the strong gradients across the card:

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

Runs on images whose corners have already been filled, so after
[`corner_infill_dark.py`](../corner_infill/README.md) and before the four are renamed by hand. See
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
