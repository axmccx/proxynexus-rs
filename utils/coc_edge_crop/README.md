# Call of Cthulhu Edge Crop

Trims the scanner ground from the edges of Call of Cthulhu scans.

The scans are cut flush to the card, so each carries a dark rim a few pixels deep all the way
round — median 8px, worst 28px — where the scanner ground meets the card edge. The corner infill
scripts clear the four corners and leave that rim behind.

It matters because Proxy Nexus builds a card's bleed by repeating the outermost pixel outward. Left
alone, the rim becomes the entire bleed, and any outward drift in the cut shows it against the
card's white border.

```bash
uv run coc_edge_crop.py ~/Downloads/coclcg_infilled -o coclcg_trimmed
uv run coc_edge_crop.py ~/Downloads/coclcg_infilled --dry-run
```

**It trims the ground, not the card.** These cards are printed with a band of white between the
frame and the card's edge, about 0.12in of a 2.5in card, and that band is part of the card. It
stays.

Two things it does that a fixed trim cannot:

- **Each axis is measured on its own.** The rim is a depth in pixels and the image is not square,
  so one depth scaled to both leaves rim on the long edges of a landscape scan.
- **A card whose border is as dark as the ground is left alone.** The 15 full-art promos have
  nothing to tell the two apart, and their bleed is meant to be dark.

Run it after the corner infill and before any rotate. That order is measured, not assumed: trimming
first crops the image corner inward along the card's rounded arc, and on some cards the new corner
pixel lands on the white border rather than on ground, leaving the infill nothing to seed from.
Over the whole collection, infill-then-trim leaves black in the corner of 1 card; trim-then-infill
leaves it in 8, and four cards report no ground at any corner at all.

JPEGs are written back with the quantization tables read off the source, so the crop costs no
quality.

Part of the Call of Cthulhu pipeline — see
[the renamer's README](../image_file_renamers/coclcg/README.md) for where it sits.
