# Edge Trim Utility

The sheets SCED cuts its pictures from lay each card on a dark ground, and a card does not always
fill the grid cell it sits in. A slot can come away with a few columns of that ground still
attached along a whole edge.

[corner_infill](../corner_infill/README.md) does not remove it. That script looks for a wedge
outside a rounded corner; this is a straight strip running the length of an edge, and the corner
arcs it does fill are only its ends.

It matters because Proxy Nexus builds bleed by repeating the outermost pixel, so two columns of
ground on an edge become the entire 36px bleed on that side. That is the black line MPC rejects an
order for, and the reason a card can be reported as having insufficient bleed while looking correct
on screen.

```bash
uv run ahlcg_edge_trim.py ~/Pictures/proxynexus_collections/ahlcg/ahlcg-tts --dry-run
uv run ahlcg_edge_trim.py ~/Pictures/proxynexus_collections/ahlcg/ahlcg-tts -o ahlcg-tts-trimmed
```

Originals are never modified; output goes to a separate directory. Files with nothing to take are
copied through, so the output is a complete collection. `--no-copy-through` writes only the files
that changed, which is what to use when comparing against the collection the images came from.

Dry-run first and read the report. `Edges carrying a strip` gives the depth range per edge and
`Deepest cuts` names the files that lost the most, which are the ones to look at.

## How it decides

Each of the four edges is measured on its own, ring by ring inward, sampling only the straight part
between the corner arcs. A strip shows itself two ways and both are read, because neither covers
the material alone:

| | |
|---|---|
| **flat** | The ground is solid and far from the card behind it. This is most of them. |
| **step** | A strip one or two pixels wide is blended with the card by JPEG chroma subsampling and is no longer flat, but the boundary where it ends is still abrupt *and crosses the same way at every point along the edge*. |

The second test is what separates a strip from art. A gradient running to the edge moves a few
levels at a time and not all one way, and a card whose border is one solid colour never steps at
all, so neither is cut. The reference each edge is judged against sits just past the cap rather
than at some fixed depth, because these cards carry a bright frame a dozen pixels wide and then go
dark; a window far enough in to clear that frame is reading the art, not the border.

Thresholds are in [edge_trace.py](edge_trace.py) and each is a command-line flag. The two that
decide how much is taken are `--max-frac`, the share of the width one edge may lose, and
`--step-frac`, the tighter cap on what a boundary alone may take. Both are fractions so they hold
at any resolution.

The cost either way is not symmetric: a strip left in place is a rejected order, and two pixels
taken off a 750px card that did not need it is 0.3% of the card. The thresholds are set with that
in mind, and the caps are what bound the damage when a measurement is wrong.

## Reviewing a run

```bash
uv run bleed_preview.py ORIGINALS TRIMMED -o sheets/ --zoom
```

The trim is a few pixels and does not read on a card; what reads is the bleed built from the pixels
it removes. So each card is shown the way `print_prep` builds it, before and after, with the cut
line marked — `--zoom` magnifies one corner of the bleed, which is where the difference is.

## Tests

```bash
uv run --with pytest --with numpy --with opencv-python --no-project pytest utils/ahlcg_edge_trim/tests/ -v
```

Detection against images built with a known ground width: every edge, both grounds, and the cases
that must not be cut — a dark card, a flat border, a gradient, and corner wedges. No network calls.

## What it found

| Collection | Images | With a strip |
|---|---|---|
| `ahlcg-tts` | 6063 | 1071 |
| `ahlcg-hq` | 488 | 2 |

Depths run 1-6px. `ahlcg-hq` comes from scans cut to the card and has no sheet ground to carry; its
two are worth looking at rather than trusting.
