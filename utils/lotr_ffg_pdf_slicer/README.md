# LotR FFG PDF Slicer

Extracts card images from the print-and-play card PDFs Fantasy Flight Games publishes for
The Lord of the Rings LCG on its product support pages. These carry the campaign and replacement
cards that were only printed in the 2022-onwards revised line, which no scan archive covers.

Each page holds one card face at cut size, with the card text set as vector rather than drawn into
the art, so the pages have to be rendered rather than have their images pulled out. There is
nothing to cut up: the page is the card. What the script works out instead is which pages are
fronts, which are backs, and which are neither.

## Getting the PDFs

Each product's page on `fantasyflightgames.com` links its campaign cards under Support. Download
them into one folder:

| File | Cards |
|---|---|
| `core_set_campaign_cards.pdf` | Mirkwood Paths, the Revised Core Set campaign |
| `dark_of_mirkwood_campaign_cards.pdf` | The Dark of Mirkwood |
| `mec104-mec105_replacement_cards.pdf` | Replacements for five cards in MEC104 and MEC105 |
| `mec108_angmar_campaign_cards_wprint_permission.pdf` | Angmar Awakened |
| `mec111_dreamchaser_campaign_cards_web2.pdf` | Dream-chaser |
| `mec115_ered_mithrin_camp_box_-_campaign_cards.pdf` | Ered Mithrin |

These are the cards that pair a campaign expansion with the earlier packs it replays, so an owner
of the original sets does not have to buy the revised ones to play the campaign.

## How to Run

Requires [`uv`](https://docs.astral.sh/uv/).

```bash
uv run lotr_ffg_pdf_slicer.py "~/Downloads/lotrlcg ffg pdf" --dry-run   # preview
uv run lotr_ffg_pdf_slicer.py "~/Downloads/lotrlcg ffg pdf"             # apply
uv run lotr_ffg_pdf_slicer.py "~/Downloads/lotrlcg ffg pdf" -o ~/Pictures/lotr
```

The argument is a PDF or a folder of PDFs. `-o` is where the per-PDF output folders are created,
defaulting to the input folder. `--dpi` sets the render resolution, 600 by default.

## Output

One folder per PDF:

```
core_set_campaign_cards/
├── cards/
│   ├── 001 - 129.png          # the front on page 1, printed number 129
│   ├── 001 - 129~back.png     # its back, page 2
│   └── 007 - 132.png          # a front whose back is a stock back
├── debug_contact_sheet.png
└── manifest.csv
```

The page number keeps the files in printed order and unique; the collector number is what
identifies the card. `~back` matches the
[image file naming convention](../../README.md#image-file-naming-convention), so renaming a pair
to a card id is a prefix substitution.

**`debug_contact_sheet.png`** is every page of the PDF as a thumbnail, labelled with its page
number, what the script decided it was, and its collector number. Read it before trusting a run:
a front and the back beneath it should be the two faces of one card, and a page labelled `dup`
should be a second copy of a card already written.

**`manifest.csv`** is one row per page: `page, role, number, file, note`. The `note` says which
page a back was taken from, which page a duplicate matched, or why a front got no back image.

At 600 dpi a card is 1477x2097, close to the 1568x2140 of the scans in the existing LotR LCG
collections. The embedded art is 300 dpi and is upsampled to get there; the frames, text and icons
are vector and are genuinely sharper for it.

## How it reads a PDF

**The trim box is the card.** Most pages are the card and nothing else, but the replacement-card
PDF sets a media box 30 pt larger all round and fills it with crop marks. Every page declares a
trim box at the cut line, so that is what gets rendered.

**A page with no text at all is a stock back.** The stock player and encounter backs are a single
full-page image. Every card face, front or back, carries at least a copyright line, so an empty
text layer identifies the stock backs exactly. They are not written out — Proxy Nexus supplies
the stock LotR LCG backs itself.

**The stock backs fix the pairing order.** Pages alternate front and back, but which comes first
varies: `mec115` opens with a back, the rest open with a front. Stock backs are only ever backs,
so the parity they sit on is the parity of every back in the file. A card page landing on that
parity is a quest back or the second face of a doubled card, and is written as the preceding
front's `~back`.

**The print-permission page is dropped before the parity is counted.** It leads in
`mec104-mec105` and trails in three others, and counting it would flip the parity of every page
after it.

**The collector number is the smallest type on the page.** A face sets several numbers — resource
cost, willpower, victory points — and the collector number is the small one in the footer, so the
smallest span size wins, and the lowest span of that size breaks the tie. Faces with no number
are written as `unknown`.

**Copies are written once.** These are print sheets, so a card needing four copies appears four
times. A card is recognised by a 16x16 grayscale thumbnail of each of its faces, and the manifest
records which page each duplicate matched. Exact pixels are not enough: some copies embed the
same art under a different JPEG encoding, and render a few levels apart. At this size two copies
of a card differ by at most 2 levels and two different cards by 88 or more, so the tolerance sits
at 8.

## Tests

```bash
uv run --with pytest --with pymupdf --with Pillow --no-project \
  pytest utils/lotr_ffg_pdf_slicer/tests/ -v
```

Covers the collector-number pattern, the duplicate tolerance, and every branch of the front/back
pairing: front-first, back-first, a card page that is another card's back, the notice page not
shifting the parity, a file with no stock backs, stock backs on both parities, and a front with
no page after it. No file I/O and no PDF fixtures.

## Known limitations

- **The output is not named for cards.** The script gets as far as the collector number printed
  on the face. Turning that into `{card_id}@{printing}` is a separate step:
  [`rename_ffg_pdfs.py`](../image_file_renamers/lotrlcg/README.md) takes this output and resolves it
  against [Hall of Beorn](https://hallofbeorn.com/).
- **Card titles are not extracted.** They are set in a display face that the text layer returns
  as loose glyph runs in no reliable position, so the number is the only identifier taken.
- **Pairing needs at least one stock back.** `mec104-mec105_replacement_cards.pdf` has none, and
  its five pages are read as five single-sided fronts. That is correct for that file, but a PDF
  of nothing but quest cards would be read the same way and would be wrong.
- **A doubled card's back number is only in the manifest.** Angmar's Protect the Innocent and
  Arnor Ravaged are one card printed `158a` and `158b`; the pair is written as `158a` and
  `158a~back`, and `158b` is recorded in the manifest's `note` column.
