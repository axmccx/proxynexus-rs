# /// script
# requires-python = ">=3.10"
# dependencies = ["Pillow", "pymupdf", "unidecode"]
# ///
"""Rename the card images extracted from FFG's print-and-play card PDFs.

Source: the output of `utils/lotr_ffg_pdf_slicer/`, one folder per PDF holding
`cards/` and a `manifest.csv`. The slicer names a card by the page it came from
and the collector number printed on it, and cards resolve on that number, so
nothing here infers a card from a filename.

The source PDF is read alongside the images. Where a collector number covers two
faces that Hall of Beorn lists as two cards, the page text is what says which
face is which.

Writes `lotrlcg-ffg/`. Sources are never modified.
"""

import argparse
import collections
import csv
import importlib.util
import os
import pathlib
import re
import statistics
import sys

import pymupdf
from PIL import Image
from unidecode import unidecode

# Matching, id normalization and logging are shared with rename.py. Loaded by
# explicit path rather than `import rename`: all four games ship a rename.py, so
# a bare import resolves through sys.path and can pick up another game's module
# when the whole test suite runs in one process.
_spec = importlib.util.spec_from_file_location(
    "lotrlcg_rename_helpers", pathlib.Path(__file__).resolve().parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)

normalize_title = rename.normalize_title
load_catalog = rename.load_catalog
log = rename.log

OUTPUT_FOLDER = "lotrlcg-ffg-pdf"

# Which Hall of Beorn card set each PDF prints. A PDF that spans two sets maps
# its collector numbers instead: `mec104-mec105_replacement_cards.pdf` reprints
# five cards across the two Starter Decks named in its own title page, and its
# pages are not grouped by set.
PACKS = {
    "core_set_campaign_cards": "Revised Core Set",
    "dark_of_mirkwood_campaign_cards": "The Dark of Mirkwood",
    "mec108_angmar_campaign_cards_wprint_permission": "Angmar Awakened Campaign Expansion",
    "mec111_dreamchaser_campaign_cards_web2": "Dream-chaser Campaign Expansion",
    "mec115_ered_mithrin_camp_box_-_campaign_cards": "Ered Mithrin Campaign Expansion",
    "mec104-mec105_replacement_cards": {
        10: "Defenders of Gondor",
        13: "Defenders of Gondor",
        33: "Defenders of Gondor",
        20: "Elves of Lórien",
        21: "Elves of Lórien",
    },
}

# Hall of Beorn numbers the Ered Mithrin Campaign Expansion one higher than the
# cards do, across the whole set: the card printed 164 is Journey Up the Anduin,
# which the catalog has at 165, and the offset holds through to the last card.
# Applied to the printed number to reach the catalog's.
NUMBER_OFFSETS = {"Ered Mithrin Campaign Expansion": 1}

# How much of a catalog card's wording has to appear on the page. A correct
# match runs from 0.84 to 1.00 across these PDFs, so a card below the floor is
# worth a look, but no per-card floor separates right from wrong on its own:
# two similar cards in one set share enough wording that a card matched to its
# neighbour still scores up to 0.94.
#
# What does separate them is the median over a whole PDF, which is 0.97 or
# better when the set lines up and 0.43 or worse when it is off by one. That
# median is the real guard, and the one that would catch a NUMBER_OFFSETS entry
# going stale.
MATCH_FLOOR = 0.5
SET_MATCH_FLOOR = 0.8

WORDS = re.compile(r"[a-z0-9]+")


def words(text):
    return set(WORDS.findall(unidecode(text or "").lower()))


def catalog_words(card):
    """Every word Hall of Beorn holds for a card's face."""
    face = card.get("Front") or {}
    parts = [card.get("Title") or "", face.get("Subtitle") or ""]
    for key in ("Text", "Traits", "Keywords"):
        parts.extend(face.get(key) or [])
    return words(" ".join(parts))


def overlap(card, page_text):
    """The share of a catalog card's wording that appears on a page."""
    wanted = catalog_words(card)
    if not wanted:
        return 0.0
    return len(wanted & words(page_text)) / len(wanted)


def build_lookup(catalog):
    """`{card set: {number: [card, ...]}}`, one entry per distinct slug.

    Hall of Beorn lists some cards twice under the same slug -- every Ered
    Mithrin treasure, for one -- and a number carrying two distinct slugs is a
    card with a different card on each face.
    """
    lookup = collections.defaultdict(lambda: collections.defaultdict(list))
    for card in catalog:
        card_set, number, slug = card.get("CardSet"), card.get("Number"), card.get("Slug")
        if not card_set or number is None or not slug:
            continue
        entries = lookup[card_set][number]
        if slug not in {entry["Slug"] for entry in entries}:
            entries.append(card)
    return lookup


def pack_for(pdf_stem, number):
    pack = PACKS.get(pdf_stem)
    return pack.get(number) if isinstance(pack, dict) else pack


def assign_faces(entries, front_text, back_text):
    """Say which of two catalog cards is the front page and which is the back.

    Both faces of one physical card repeat much of each other's wording -- an
    upgraded ship keeps the base ship's traits and most of its text -- so each
    card is scored against both pages and takes the page it fits better.
    """
    first, second = entries
    first_front = overlap(first, front_text) - overlap(first, back_text)
    second_front = overlap(second, front_text) - overlap(second, back_text)
    if first_front == second_front:
        return None
    return (first, second) if first_front > second_front else (second, first)


def write_image(src, dest, dry_run):
    if dry_run:
        return True
    try:
        with Image.open(src) as img:
            if img.mode in ("RGBA", "P"):
                img = img.convert("RGB")
            img.save(dest, format="JPEG", quality=90)
        return True
    except Exception as e:  # noqa: BLE001 - report and keep going
        log(f"[ERR]  {os.path.basename(src)}: {e}")
        return False


def process_pdf(pdf_path, lookup, out_folder, args):
    """Write one image per catalog card the PDF prints."""
    stem = pdf_path.stem
    slices = pdf_path.parent / stem
    manifest = slices / "manifest.csv"
    if not manifest.exists():
        log(f"[SKIP] {stem}: no manifest.csv beside the PDF; run the slicer first")
        return 0, 0, []

    if stem not in PACKS:
        log(f"[SKIP] {stem}: no card set declared for this PDF")
        return 0, 0, []

    doc = pymupdf.open(pdf_path)
    page_text = [page.get_text() for page in doc]
    doc.close()

    written = skipped = 0
    audit = []
    scores = []

    with open(manifest, newline="", encoding="utf-8") as handle:
        rows = [row for row in csv.DictReader(handle) if row["role"] == "front" and row["file"]]

    for row in rows:
        page = int(row["page"])
        printed = row["number"]
        files = [slices / "cards" / name for name in row["file"].split(" + ")]
        front_file = files[0]
        back_file = files[1] if len(files) > 1 else None

        if not printed:
            log(f"[SKIP] {stem} p{page}: no collector number printed on the card")
            skipped += 1
            continue

        # '158a' and '158b' are the two faces of card 158.
        number = int(printed.rstrip("abc"))
        pack = pack_for(stem, number)
        if not pack:
            log(f"[SKIP] {stem} p{page}: no card set for printed number {printed}")
            skipped += 1
            continue

        entries = lookup.get(pack, {}).get(number + NUMBER_OFFSETS.get(pack, 0), [])
        printing = normalize_title(pack)
        front_text = page_text[page - 1]
        back_text = page_text[int(re.search(r"page (\d+)", row["note"]).group(1)) - 1] if back_file else ""

        if not entries:
            log(f"[SKIP] {stem} p{page}: {pack} has no card numbered {printed}")
            skipped += 1
            continue

        if len(entries) > 2:
            log(f"[SKIP] {stem} p{page}: {pack} #{printed} resolves to {len(entries)} cards")
            skipped += 1
            continue

        if len(entries) == 2:
            if back_file is None:
                log(f"[SKIP] {stem} p{page}: {pack} #{printed} is two cards but the page has one face")
                skipped += 1
                continue
            ordered = assign_faces(entries, front_text, back_text)
            if ordered is None:
                log(f"[SKIP] {stem} p{page}: cannot tell which face of {pack} #{printed} is which")
                skipped += 1
                continue
            if normalize_title(ordered[0]["Title"]) == normalize_title(ordered[1]["Title"]):
                # An upgradable card. FFG prints the Upgraded side on the back
                # of the base side, so this is one card with two faces, and the
                # back file is already that second face. Hall of Beorn lists the
                # Upgraded face as a card of its own; writing it out would file
                # the same two images under a second id and print the card
                # twice. rename.py takes the same line for the flip cards in
                # The Hunt for the Dreadnaught, see its has_split_sibling.
                pairs = [(ordered[0], front_file, back_file)]
            else:
                # Two different cards sharing one piece of cardboard, so each is
                # written in its own right and backed by the other: a decklist
                # naming either has to resolve. This is what rename.py does by
                # hand for Na'asiyah and Captain Sahir.
                pairs = [
                    (ordered[0], front_file, back_file),
                    (ordered[1], back_file, front_file),
                ]
        else:
            pairs = [(entries[0], front_file, back_file)]

        for card, face_file, other_file in pairs:
            face_text = front_text if face_file == front_file else back_text
            score = overlap(card, face_text)
            scores.append(score)
            if score < MATCH_FLOOR:
                log(
                    f"[WARN] {stem} p{page}: {card['Title']!r} ({pack} #{printed}) "
                    f"matches only {score:.0%} of the page text"
                )
            target_id = normalize_title(card["Slug"])
            name = f"{target_id}@{printing}.jpg"
            if write_image(face_file, os.path.join(out_folder, name), args.dry_run):
                written += 1
                audit.append([stem, page, printed, card["Title"], pack, name, f"{score:.2f}"])
            if other_file is not None:
                back_name = f"{target_id}@{printing}~back.jpg"
                if write_image(other_file, os.path.join(out_folder, back_name), args.dry_run):
                    written += 1
                    audit.append([stem, page, printed, card["Title"], pack, back_name, ""])

    median = statistics.median(scores) if scores else 0.0
    log(
        f"  {stem}: {written} images from {len(rows)} cards, {skipped} skipped, "
        f"median match {median:.0%}"
    )
    if scores and median < SET_MATCH_FLOOR:
        log(
            f"[ERROR] {stem}: the cards do not line up with {PACKS[stem]}. Check "
            f"NUMBER_OFFSETS and the card set declared in PACKS."
        )
    return written, skipped, audit


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", help="The folder holding the PDFs and the slicer's output")
    parser.add_argument("-o", "--output", default=".", help=f"Where to create {OUTPUT_FOLDER}/")
    parser.add_argument("--dry-run", action="store_true", help="Preview without writing images")
    args = parser.parse_args()

    root = pathlib.Path(args.input).expanduser()
    if not root.is_dir():
        sys.exit(f"No such folder: {root}")

    pdfs = sorted(root.glob("*.pdf"))
    if not pdfs:
        sys.exit(f"No PDFs in {root}")

    out_folder = os.path.abspath(os.path.join(args.output, OUTPUT_FOLDER))
    os.makedirs(out_folder, exist_ok=True)

    lookup = build_lookup(load_catalog())

    log_handle = open(os.path.join(out_folder, "migrate.log"), "w", encoding="utf-8")
    rename.set_log_file(log_handle)
    try:
        log(f"--- Scanning {'(DRY RUN) ' if args.dry_run else ''}{root} ---")
        written = skipped = 0
        audit_rows = [["pdf", "page", "printed", "title", "card set", "file", "match"]]
        for pdf in pdfs:
            count, missed, audit = process_pdf(pdf, lookup, out_folder, args)
            written += count
            skipped += missed
            audit_rows.extend(audit)

        if not args.dry_run and written:
            audit_path = os.path.join(out_folder, "migration_audit_log.csv")
            with open(audit_path, "w", newline="", encoding="utf-8") as handle:
                csv.writer(handle).writerows(audit_rows)
            log(f"\nAudit log written to: {audit_path}")

            orphans = rename.find_orphaned_backs({out_folder: os.listdir(out_folder)})
            total = sum(len(v) for v in orphans.values())
            if total:
                log(f"\n[ERROR] Validation failed! {total} back images have no front:")
                for backs in orphans.values():
                    for back in backs:
                        log(f"  Missing front for: {back}")
            else:
                log("\n[OK] Validation passed: every back image has a front.")

        log(f"\nSummary: {written} images written, {skipped} skipped.")
    finally:
        log_handle.close()
        rename.set_log_file(None)


if __name__ == "__main__":
    main()
