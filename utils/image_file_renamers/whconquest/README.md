# Warhammer 40k Conquest Image File Renamer

Renames Warhammer 40k Conquest card scans to the current
[image file naming convention](../../../README.md#image-file-naming-convention), resolving them
against the catalog the `whconquest` adapter embeds.

Conquest has no card database API. The catalog is built instead from
[warhammer_40K_conquest_card_data](https://github.com/rgmc4/warhammer_40K_conquest_card_data), a
JSON conversion of the OCTGN card definitions, by `build_catalog.py`. Both scripts live here so
the two stay in step: the renamer matches scans against the same file the adapter reads.

## Building the catalog

Only needed when the source data changes.

```bash
uv run build_catalog.py ~/warhammer_40K_conquest_card_data
```

Writes `whc_cards.json` and `whc_packs.json` into `proxynexus-core/src/games/whconquest/`, where
the adapter embeds them with `include_str!`.

The card id is a slug of the card's name (`captain-cato-sicarius`), which works because card names
are unique across the whole catalog, official and fan-made alike. The pack id is a slug of the set
name (`zogworts-curse`).

Three things are dropped or moved on the way through:

| Source | In the catalog | Why |
|---|---|---|
| `Token`, `Skull`, `Initiative`, `Dial` | dropped | Punchboard pieces, not poker-sized cards |
| Set "Markers and Planets" | folded into `core-set` | The planets are Core Set cards; their numbers 175-188 continue the Core Set's 1-174 |
| Set "The Final Gamit" | `the-final-gambit` | Typo in the source data |

Release dates come from `RELEASE_DATES` in the script. The release *order* is documented, the exact
days are not, so those dates are the release months as first-of-month — enough to sort the packs.
The fan-made Black Crusade and Apoka sets have no date at all and sort last.

## Getting the scans

Two archives circulate, laid out differently. Either works on its own; passing both gives the best
result, because each covers cards the other is missing and the sharper scan of a card wins.

### The faction archive (the better of the two)

Images pulled from the Tabletop Simulator mod, sorted by faction and card type. Roughly 60% of it
is ~575 dpi — about twice the linear resolution of the pack archive — and it covers every FFG card
except three `champions` promos, warlord bloodied sides included. Shared as a Google Drive folder
on the Conquest Discord.

```
SortedConquestImages/
├── Blanked/            blank templates and loyalty icons for making custom cards
├── Dark Eldar/
│   ├── Event/Raid.jpg
│   └── Warlords/Urien Rakarth/
│       ├── Urien_Rakarth.jpg
│       └── Urien_Rakarth_bloodied.jpg
├── Planets/
└── Tokens/
```

It also holds a good deal of fan-made content that is in no pack, and fan reworkings of real cards
marked `_apoka`. Neither is used — see **How a file is resolved**.

### The pack archive

A bleed-cut MPC set circulating as `Warhammer 40k Conquest (COMPLETE)`, sorted into a folder per
pack, sometimes nested under a cycle folder or split further by warlord. Uniformly 300 dpi.

```
Warhammer 40k Conquest (COMPLETE)/
├── bleed_40k Back.png
├── Core Set/
│   ├── bleed_Land Raider.png
│   ├── Planets/bleed_Atrox Prime.png
│   └── Warlords/Zarathur/
│       ├── bleed_Zarathur, High Sorcerer (1).png
│       └── bleed_Zarathur, High Sorcerer (2).png
└── Warlord Cycle/Zogworts Curse/bleed_Doombolt.png
```

Despite the name it is not complete: seven cards are missing from it, all of which the faction
archive has. Its bleed border is not a real one — it is a pixel-exact mirror of the card's own
edge, which is what Proxy Nexus generates anyway, so nothing is lost by preferring a sharper
bleedless scan.

## Running it

Requires [`uv`](https://docs.astral.sh/uv/).

```bash
uv run rename.py \
  --faction-archive ~/Downloads/SortedConquestImages \
  --pack-archive "~/Downloads/Warhammer 40k Conquest (COMPLETE)" \
  --dry-run                                    # preview

uv run rename.py \
  --faction-archive ~/Downloads/SortedConquestImages \
  --pack-archive "~/Downloads/Warhammer 40k Conquest (COMPLETE)" \
  -o whconquest_out                            # apply
```

Scans are copied into the output folder. The sources are never modified.

Dry-run first and read the report sections. `Filenames matched to a card by spelling` and
`Folders matched to a pack by spelling` list every fuzzy match, and both are short enough to check
by eye.

The faction archive's scans are cut to the card, so the white scanner background shows in their
rounded corners. Run [`corner_infill.py`](../../corner_infill/README.md) over the output folder
afterwards, or Proxy Nexus will smear that white into the generated bleed.

## How a file is resolved

**Card, from the filename.** Punctuation is dropped from both sides for the exact match, because
the archives punctuate inconsistently — the faction archive spells both a space and an apostrophe
as `_`, so `Straken_s_Cunning` and `23rd_Mechanised_Battalion` need opposite readings of the same
character. That is safe: no two cards in the catalog have names differing only in punctuation.
Whatever punctuation cannot fix, spelling does. The scan filenames were hand-typed and hold a good
number of misspellings — `Guass Flayer`, `Lemon Russ Conqueror`, `Hostile Enviroment Gear` — so an
unmatched name falls back to a closest-spelling match, and a name that two candidates are equally
close to is reported rather than guessed.

**Pack, from the folder or from the card.** In the pack archive it is the deepest folder between
the file and the source root that names a pack, so `Warlords`, `Planets` and a warlord's own folder
are walked past to the pack folder above them; a folder that names no pack exactly is matched by
spelling, which is what carries `Descendants of Isha` to `the-descendants-of-isha`. The faction
archive has no pack folder at all, so the card's own catalog entry supplies it.

**Which scan gets used.** Resolution across the card decides it, so a bleed scan and a bleedless
one can be compared on the same footing. A bleed border already in the file and a lossless format
break ties, in that order, since each only saves a step rather than adding detail. Everything else
an archive holds for that face is passed over and counted.

**Sides.** Warlords are the only double-sided cards. The pack archive numbers the two sides as
copies 1 and 2; the faction archive names the reverse outright with `_bloodied`. A warlord that
arrives with only one side is reported, since it would otherwise print with the generic card back.

**What is left out.** In the faction archive, `Blanked/`, `Tokens/` and `Misc/` hold no printable
card; `_apoka` files are fan reworkings of real cards rather than the printed card; and anything
matching no catalog card at all is fan-made content belonging to no pack. All are counted in the
summary rather than listed.

## Card backs

`bleed_40k Back.png` at the root of the pack archive is the standard card back. It is not renamed;
it lives in the adapter instead, as `proxynexus-core/src/games/whconquest/backs/card_original.png`,
cropped back to the card and run through [`corner_infill.py`](../../corner_infill/README.md) — the
archive's copy has the white scanner background in its rounded corners, mirrored out into the bleed
as four white diamonds.

Every card shares that one back. Planets look like an exception but are not: they are ordinary
portrait cards whose art is printed sideways, not landscape cards with a reverse of their own. The
faction archive's `cardback.jpg` is the same scan at the same resolution, and its
`Misc/CardbackRotated.jpg` is a sideways crop for the Tabletop Simulator table rather than a scan
of anything printed, so neither adds a back worth offering.

## Known gaps

Running both archives together resolves 973 scans covering 929 cards, 717 of them at ~575 dpi.
Every FFG pack is complete except `champions`, the four-card tournament promo set, where only
Herald of the WAAAGH! has a scan.

The fan-made Apoka sets come out nearly complete; Black Crusade's `the-eye-of-terror` has one card
of 29. Delete those files from the output folder if you want an FFG-only collection.
