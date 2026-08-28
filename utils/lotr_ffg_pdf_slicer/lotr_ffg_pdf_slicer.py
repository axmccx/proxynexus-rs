# /// script
# requires-python = ">=3.10"
# dependencies = ["pymupdf", "Pillow"]
# ///
"""Extract card images from the Lord of the Rings LCG print-and-play card PDFs
that Fantasy Flight Games publishes on its product support pages.

Every page holds one card face, and the page's trim box is the card at cut size.
Pages run in front/back pairs, and which face of the pair comes first varies per
PDF. A face backed by a stock player or encounter back is a page carrying an
image and no text at all, which is what identifies it, and what fixes the
pairing order for the whole file.
"""

import argparse
import csv
import pathlib
import re
import sys

import pymupdf
from PIL import Image, ImageDraw, ImageFont

# The collector number, printed small in the footer. Quest backs carry none; the
# two faces of a doubled card carry '158a' and '158b'.
COLLECTOR_NUMBER = re.compile(r"^\d{1,3}[a-z]?$")

# FFG's print-permission page, which most of these PDFs carry either first or
# last. It is a notice, not a card.
NOTICE_TEXT = "Permission to print support items"

NOTICE = "notice"
STOCK_BACK = "stock back"
CARD = "card"

FRONT = "front"
BACK = "back"

# A card that needs several copies is printed several times, and the copies are
# not always the same bytes: the same art gets embedded again under a different
# JPEG encoding. Copies differ by at most 2 levels at this thumbnail size, and
# two different cards by 88 or more.
SIGNATURE_SIZE = 16
SIGNATURE_TOLERANCE = 8

CONTACT_COLUMNS = 10
CONTACT_THUMB_DPI = 60
CONTACT_LABEL_HEIGHT = 16

MANIFEST_FIELDS = ["page", "role", "number", "file", "note"]


def collector_number(page):
    """Return the collector number printed on this face, or None.

    A face sets several numbers -- resource cost, willpower, victory points --
    so the smallest type on the page is what picks out the collector number, and
    the lowest span of that size the one in the footer.
    """
    candidates = []
    for block in page.get_text("dict")["blocks"]:
        for line in block.get("lines", []):
            for span in line["spans"]:
                text = span["text"].strip()
                if COLLECTOR_NUMBER.match(text):
                    candidates.append((round(span["size"], 2), span["bbox"][3], text))
    if not candidates:
        return None
    smallest = min(size for size, _, _ in candidates)
    footer = [c for c in candidates if c[0] == smallest]
    return max(footer, key=lambda c: c[1])[2]


def classify(page):
    text = page.get_text()
    if NOTICE_TEXT in text:
        return NOTICE
    if not text.strip():
        return STOCK_BACK
    return CARD


def pair_faces(roles):
    """Work out which pages are fronts, which are backs, and what pairs with what.

    Takes `{page number: role}` and returns `({page: (side, partner page)}, note)`
    covering every page that is not the notice. `partner` is the page holding the
    other face of the same card, or None where there is no such page.

    Stock backs are only ever backs, so the parity they sit on is the parity of
    every back in the file, and a file whose backs sit on odd positions opens
    with a back and so runs back-then-front throughout. Positions are counted
    over the card pages with the notice dropped, because that page leads in some
    PDFs and trails in others and would otherwise flip the parity of everything
    after it.
    """
    pages = [page for page in sorted(roles) if roles[page] != NOTICE]
    parities = {
        position % 2
        for position, page in enumerate(pages, start=1)
        if roles[page] == STOCK_BACK
    }

    if len(parities) != 1:
        # Nothing says where the backs are. A sheet of single-sided player cards
        # is the honest case; mixed parities mean the file is not what this
        # script understands, and pairing anything up would be a guess.
        note = (
            "no stock backs: every card page is read as a single-sided front"
            if not parities
            else "[WARN] stock backs sit on both odd and even positions: "
            "pairing is off and every card page is written as a front"
        )
        return {page: (FRONT, None) for page in pages}, note

    backs_on = parities.pop()
    step = -1 if backs_on else 1
    faces = {}
    for position, page in enumerate(pages, start=1):
        is_back = position % 2 == backs_on
        partner_position = position - step if is_back else position + step
        partner = (
            pages[partner_position - 1] if 1 <= partner_position <= len(pages) else None
        )
        faces[page] = (BACK if is_back else FRONT, partner)
    return faces, f"backs come {'before' if backs_on else 'after'} their fronts"


def render(page, dpi):
    """Render the page's trim box, which is the card without its crop marks."""
    page.set_cropbox(page.trimbox)
    return page.get_pixmap(dpi=dpi)


def to_image(pixmap):
    return Image.frombytes("RGB", (pixmap.width, pixmap.height), pixmap.samples)


def signature(pixmap):
    """A coarse grayscale thumbnail of a face, for recognising a repeated copy."""
    thumb = to_image(pixmap).convert("L")
    return thumb.resize((SIGNATURE_SIZE, SIGNATURE_SIZE), Image.BOX).tobytes()


def signatures_match(one, other):
    """Whether two card signatures are the same card. None matches only None."""
    if one is None or other is None:
        return one is other
    return max(abs(a - b) for a, b in zip(one, other)) <= SIGNATURE_TOLERANCE


def build_contact_sheet(thumbnails, labels, path):
    width = max(thumb.width for thumb in thumbnails)
    height = max(thumb.height for thumb in thumbnails)
    cell_height = height + CONTACT_LABEL_HEIGHT
    rows = -(-len(thumbnails) // CONTACT_COLUMNS)
    sheet = Image.new(
        "RGB", (CONTACT_COLUMNS * width, rows * cell_height), (32, 32, 32)
    )
    draw = ImageDraw.Draw(sheet)
    try:
        font = ImageFont.load_default(size=11)
    except TypeError:  # Pillow below 10.1 has no sizeable default font
        font = ImageFont.load_default()
    for index, (thumb, label) in enumerate(zip(thumbnails, labels)):
        x = (index % CONTACT_COLUMNS) * width
        y = (index // CONTACT_COLUMNS) * cell_height
        sheet.paste(thumb, (x, y))
        draw.text((x + 2, y + height + 2), label, fill=(225, 225, 225), font=font)
    sheet.save(path)


def slice_pdf(pdf_path, out_dir, dpi, dry_run):
    doc = pymupdf.open(pdf_path)

    roles = {}
    numbers = {}
    for page_number, page in enumerate(doc, start=1):
        roles[page_number] = classify(page)
        if roles[page_number] == CARD:
            numbers[page_number] = collector_number(page)

    faces, note = pair_faces(roles)
    print(f"  {note}")

    cards_dir = out_dir / "cards"
    if not dry_run:
        cards_dir.mkdir(parents=True, exist_ok=True)

    written = []
    manifest = []
    thumbnails = []
    labels = []
    images = 0

    for page_number in sorted(roles):
        role = roles[page_number]
        number = numbers.get(page_number) or ""
        side, partner = faces.get(page_number, (None, None))
        files = []

        if role == NOTICE:
            row_role, row_note = NOTICE, "print permission notice"
        elif role == STOCK_BACK:
            row_role, row_note = STOCK_BACK, "stock back, not written"
        elif side == BACK:
            # Backs are written by the front that claims them, so their own turn
            # here only records what they were.
            row_role = BACK
            if partner is not None and roles[partner] == CARD:
                row_note = f"back of page {partner}"
            else:
                row_note = "unclaimed back"
                print(
                    f"  [WARN] page {page_number}: card back with no front to pair to"
                )
        else:
            row_role = FRONT
            pixmap = render(doc[page_number - 1], dpi)
            back_pixmap = None
            if partner is not None and roles[partner] == CARD:
                back_pixmap = render(doc[partner - 1], dpi)

            signatures = (
                signature(pixmap),
                signature(back_pixmap) if back_pixmap is not None else None,
            )
            copied = next(
                (
                    page
                    for page, seen in written
                    if all(signatures_match(a, b) for a, b in zip(seen, signatures))
                ),
                None,
            )

            if copied is not None:
                row_note = f"duplicate of page {copied}"
            else:
                written.append((page_number, signatures))
                name = f"{page_number:03d} - {number or 'unknown'}"
                files.append(f"{name}.png")
                if not dry_run:
                    pixmap.save(cards_dir / files[0])
                if back_pixmap is not None:
                    files.append(f"{name}~back.png")
                    if not dry_run:
                        back_pixmap.save(cards_dir / files[1])
                    row_note = f"back on page {partner}"
                    if numbers.get(partner):
                        row_note += f", printed {numbers[partner]}"
                elif partner is None:
                    row_note = "no back page"
                else:
                    row_note = "stock back"
                images += len(files)

        manifest.append(
            {
                "page": page_number,
                "role": row_role,
                "number": number,
                "file": " + ".join(files),
                "note": row_note,
            }
        )
        thumbnails.append(to_image(render(doc[page_number - 1], CONTACT_THUMB_DPI)))
        label = f"{page_number} {row_role} {number}".rstrip()
        if row_note.startswith("duplicate"):
            label += " dup"
        labels.append(label)

    if not dry_run:
        build_contact_sheet(thumbnails, labels, out_dir / "debug_contact_sheet.png")
        with open(out_dir / "manifest.csv", "w", newline="") as handle:
            writer = csv.DictWriter(handle, fieldnames=MANIFEST_FIELDS)
            writer.writeheader()
            writer.writerows(manifest)

    cards = sum(1 for row in manifest if row["file"])
    duplicates = sum(1 for row in manifest if row["note"].startswith("duplicate"))
    print(
        f"  {doc.page_count} pages -> {cards} cards, {images} images"
        f" ({duplicates} duplicate copies skipped)"
    )
    doc.close()


def main():
    parser = argparse.ArgumentParser(
        description="Extract card images from the LotR LCG print-and-play card PDFs."
    )
    parser.add_argument("input", help="A PDF, or a directory of PDFs.")
    parser.add_argument(
        "-o",
        "--output",
        help="Where the per-PDF output folders are created. Defaults to the input.",
    )
    parser.add_argument(
        "--dpi",
        type=int,
        default=600,
        help="Render resolution. The embedded art is 300 dpi; the frames and the "
        "text are vector and keep sharpening above that (default: 600).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Report what would be written without writing anything.",
    )
    args = parser.parse_args()

    source = pathlib.Path(args.input).expanduser()
    if source.is_dir():
        pdfs = sorted(source.glob("*.pdf"))
        default_output = source
    elif source.is_file():
        pdfs = [source]
        default_output = source.parent
    else:
        print(f"Error: '{source}' does not exist.")
        return 1

    if not pdfs:
        print(f"No PDFs found in '{source}'.")
        return 1

    out_root = pathlib.Path(args.output).expanduser() if args.output else default_output
    for pdf in pdfs:
        print(f"--- {pdf.name}")
        slice_pdf(pdf, out_root / pdf.stem, args.dpi, args.dry_run)
    return 0


if __name__ == "__main__":
    sys.exit(main())
