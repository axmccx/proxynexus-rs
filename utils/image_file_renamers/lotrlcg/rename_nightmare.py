# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow", "unidecode"]
# ///
"""Rename the LotR LCG Nightmare deck scans.

Source: "LOTR LCG Nightmare Cards - Remastered", one folder per scenario, each
shipping a `Card list.txt` manifest that names every card's front and back file
and the number printed on it. Cards resolve on that number, so nothing here
infers a card from a filename.

Writes `lotrlcg-nightmare/`. Sources are never modified.
"""
import argparse
import csv
import importlib.util
import os
import pathlib
import re
import sys

from PIL import Image

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

clean_for_match = rename.clean_for_match
normalize_title = rename.normalize_title
load_catalog = rename.load_catalog
log = rename.log

# The JPEGs in this archive carry a 5mm bleed edge (see "Bleed details.txt"),
# so every output is named `.bleed`.
BLEED_SUFFIX = ".bleed"

# Card geometry, from the archive's "Card details.txt", and confirmed by the
# professional-printer PDFs: one card per page at 194.22 x 269.04 pt, which is
# 68.52 x 94.91 mm, less the 3mm bleed they state, giving 62.52 x 88.91 mm.
CARD_W_MM, CARD_H_MM = 62.5, 88.9
SOURCE_BLEED_MM = 5.0

# The MPC frame Proxy Nexus targets, from print_prep.rs.
MPC_CUT_W, MPC_CUT_H = 744.0, 1038.0
MPC_BLEED_W, MPC_BLEED_H = 816.0, 1110.0

# Quest and side quest cards are scanned sideways, 35 of them. Every collection
# is portrait -- the MPC frame is portrait and print_prep.rs scales against it --
# so they are turned upright. ROTATE_270 is a clockwise quarter turn, which is
# the orientation the rest of the LotR collections use and the default in
# ../agot/rotate_horizontal.py.
LANDSCAPE_ROTATION = Image.Transpose.ROTATE_270

# The archive misspells one scenario. Hall of Beorn, RingsDB and the card itself
# all read "Intruders"; only these folder and cover file names say "Invaders".
FOLDER_NAME_FIXES = {
    "Invaders in Chetwood": "Intruders in Chetwood",
}

# A manifest row whose Back column is one of these is single-sided: the back is
# the shared encounter-card design, which Proxy Nexus supplies itself.
SHARED_BACKS = {"encounter back"}

# Manifest row: "  3  Enemy   003 - Ungoliant's Brood   encounter back"
# Columns are separated by runs of two or more spaces.
MANIFEST_ROW = re.compile(r"^\s*(\d+)\s{2,}(\S.*?)\s{2,}(\S.*?)\s{2,}(\S.*?)\s*$")

# "004 - Jagged Cavern", "008 - 2A - Through the Marsh", "2A - Search for an
# Exit", "Lost Island". The stage group ("2A"/"2B") marks a quest card face.
CARD_REF = re.compile(
    r"^(?:(?P<number>\d+)\s*-\s*)?(?:\d+(?P<stage>[A-Za-z])\s*-\s*)?(?P<title>.*)$"
)

# Stage letters that denote the reverse of a quest card rather than a card of
# its own. Mirrors the back-face letters rename.py recognizes in filenames.
BACK_STAGE_LETTERS = {"B", "D", "F", "H"}

# A scan may carry a per-copy suffix: "003 - Ungoliant's Brood — 03 of 20.jpg".
# The separator is an em dash. Copy 1 is the one we keep (see --all-copies).
COPY_SUFFIX = re.compile(r"^(?P<stem>.*?)\s+—\s+(?P<copy>\d+) of (?P<total>\d+)$")


def parse_manifest(text):
    """Parse a `Card list.txt` into [(qty, card_type, front_ref, back_ref)].

    The header prose is skipped by requiring the four-column shape; the divider
    rules ("-----") and the summary lines do not match it.
    """
    rows = []
    for line in text.splitlines():
        m = MANIFEST_ROW.match(line)
        if not m:
            continue
        qty, card_type, front, back = m.groups()
        if front.startswith("-") or card_type == "Type":
            continue
        rows.append((int(qty), card_type, front, back))
    return rows


def parse_card_ref(ref):
    """Split a manifest Front/Back cell into (number, title, is_reverse).

    number is None for refs with no leading position: the shared double-sided
    backs that have no card of their own ("Lost Island"), and the shared quest
    front of Flight from Moria ("2A - Search for an Exit"), which is one face
    printed on three different cards.

    is_reverse marks a "2B"-style face, which belongs to the card its front row
    names rather than being a card in its own right.
    """
    m = CARD_REF.match(ref.strip())
    number, stage, title = m.group("number"), m.group("stage"), m.group("title")
    return (
        int(number) if number is not None else None,
        title.strip(),
        bool(stage) and stage.upper() in BACK_STAGE_LETTERS,
    )


def strip_copy_suffix(stem):
    """'003 - Forest Flies — 06 of 20' -> ('003 - Forest Flies', 6). Copy is 1 when absent."""
    m = COPY_SUFFIX.match(stem)
    if not m:
        return stem, 1
    return m.group("stem"), int(m.group("copy"))


def scan_key(ref):
    """Key a scan or a manifest reference so the two can be compared.

    Filenames replace apostrophes with underscores ("001 - Shelob_s Lair.jpg"
    for the manifest's "001 - Shelob's Lair"), and clean_for_match drops both
    along with the rest of the punctuation. The leading position number is kept,
    so cards sharing a title stay distinct.

    "(errata)" is stripped because The City of Corsairs' Patrol Ship scan
    carries it and the manifest does not.
    """
    return clean_for_match(re.sub(r"(?i)\s*\(errata\)", "", ref))


def index_scans(filenames):
    """Map a folder's scans to {scan_key: [(copy_number, filename)]}, copies sorted."""
    index = {}
    for filename in filenames:
        if not filename.lower().endswith((".jpg", ".jpeg", ".png")):
            continue
        stem, copy = strip_copy_suffix(os.path.splitext(filename)[0])
        index.setdefault(scan_key(stem), []).append((copy, filename))
    for copies in index.values():
        copies.sort()
    return index


def resolve_pack(folder, pack_lookup):
    """Resolve a scenario folder to its Hall of Beorn CardSet name.

    Cycles 1-6 nest as <cycle>/<scenario>; the Hobbit and LotR sagas add a
    product level, <cycle>/<product>/<scenario>. In both cases the scenario
    folder names the pack, give or take a leading index and the " Nightmare"
    suffix, so the parent is only consulted as a fallback.
    """
    scenario = re.sub(r"^[\d\s-]+", "", folder.name)
    scenario = FOLDER_NAME_FIXES.get(scenario, scenario)
    parent = re.sub(r"^[\d\s-]+", "", folder.parent.name)

    for candidate in (
        scenario,
        f"{scenario} Nightmare",
        parent,
        f"{parent} Nightmare",
        f"The Hobbit: {parent}",
        f"The Hobbit: {parent} Nightmare",
    ):
        hit = pack_lookup.get(clean_for_match(candidate))
        if hit:
            return hit
    return None


def pick_card(number, title, by_number, by_title):
    """Resolve a manifest reference to a catalog card.

    Position is the primary key: the manifest's numbers come off the printed
    cards and agree with Hall of Beorn's `Number` for every numbered card in the
    archive, including the ones whose titles disagree. Hall of Beorn stores
    "Writing Tentacle", "Gobline Trapper", "Swarming Mosquitos" and two more as
    printed typos, and every Nightmare Mode card carries a " Nightmare" suffix
    the manifest omits.

    Title wins only when it resolves unambiguously and position disagrees, for
    the opposite error: Hall of Beorn has Intruders in Chetwood #5 and #6
    transposed, and the scans read 5 = Outskirts of Archet, 6 = Greenway Path.

    Returns (card, how) where how is "position", "title" or "position-mismatch".
    """
    positional = by_number.get(number) if number is not None else None
    titular = by_title.get(clean_for_match(title)) if title else None

    if positional is None:
        return (titular, "title") if titular else (None, None)

    if clean_for_match(positional["Title"]) == clean_for_match(title):
        return positional, "position"

    if titular is not None and titular is not positional:
        return titular, "title"

    return positional, "position-mismatch"


def build_lookups(all_cards):
    """Return (pack_lookup, cards_by_pack) over the Hall of Beorn catalog.

    cards_by_pack[pack] is (by_number, by_title); a title colliding within a
    pack is dropped from by_title so it can never win over position.
    """
    pack_lookup = {}
    by_pack = {}
    for card in all_cards:
        pack = card.get("CardSet")
        if not pack or not card.get("Slug"):
            continue
        pack_lookup[clean_for_match(pack)] = pack
        by_number, by_title = by_pack.setdefault(pack, ({}, {}))
        if card.get("Number") is not None:
            by_number[card["Number"]] = card
        key = clean_for_match(card.get("Title", ""))
        by_title[key] = None if key in by_title else card
    for _, by_title in by_pack.values():
        for key in [k for k, v in by_title.items() if v is None]:
            del by_title[key]
    return pack_lookup, by_pack


def find_scenario_folders(root):
    """Every folder holding a manifest, skipping the print-ready PDF trees.

    The four "- Print at Home ..." folders and "- Professional Printer ..."
    hold the same cards as PDFs, and carry no manifest of their own.
    """
    folders = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if not d.startswith("- ")]
        if "Card list.txt" in filenames:
            folders.append(pathlib.Path(dirpath))
    return sorted(folders)


def trim_box(width, height):
    """Crop box reducing the archive's 5mm bleed to the bleed Proxy Nexus makes.

    Expects a portrait image; rotate landscape scans first.

    The target is the cut, not the frame. MPC cuts a `.bleed` upload at 744/816
    of its width and 1038/1110 of its height, and the PDF path's
    `crop_bleed_border` takes those same fractions off before pdf.rs stretches
    what is left onto a fixed 178.54 x 249.09 pt card. Either way the card has to
    sit at exactly those fractions, so the bleed left behind is 4.41% of the
    width and 3.24% of the height -- about 3mm on each edge. Anything more prints
    as a dark margin around the card.

    Returns None when the computed trim is not positive, so an image smaller than
    that is passed through rather than enlarged.
    """
    ppm_x = width / (CARD_W_MM + 2 * SOURCE_BLEED_MM)
    ppm_y = height / (CARD_H_MM + 2 * SOURCE_BLEED_MM)
    card_w = width - 2 * SOURCE_BLEED_MM * ppm_x
    card_h = height - 2 * SOURCE_BLEED_MM * ppm_y

    cut_x = (MPC_BLEED_W - MPC_CUT_W) / 2 / MPC_BLEED_W
    cut_y = (MPC_BLEED_H - MPC_CUT_H) / 2 / MPC_BLEED_H
    trim_x = round((width - card_w / (1 - 2 * cut_x)) / 2)
    trim_y = round((height - card_h / (1 - 2 * cut_y)) / 2)

    if trim_x <= 0 and trim_y <= 0:
        return None
    return (max(trim_x, 0), max(trim_y, 0), width - max(trim_x, 0), height - max(trim_y, 0))


def write_image(src, dest, dry_run):
    if dry_run:
        return True
    try:
        with Image.open(src) as img:
            if img.mode in ("RGBA", "P"):
                img = img.convert("RGB")
            if img.width > img.height:
                img = img.transpose(LANDSCAPE_ROTATION)
            box = trim_box(*img.size)
            if box:
                img = img.crop(box)
            img.save(dest, format="JPEG", quality=90)
        return True
    except Exception as e:  # noqa: BLE001 - report and keep going
        log(f"[ERR]  {os.path.basename(src)}: {e}")
        return False


def process_archive(root, pack_lookup, by_pack, out_folder, args):
    """Walk the archive and write one image per card face.

    Returns (written, skipped, audit_rows).
    """
    written = 0
    skipped = 0
    audit_rows = [["Source Path", "Output"]]
    seen = set()  # (card_id, pack_id, is_back)

    def emit(src_path, card_id, pack_id, is_back):
        nonlocal written, skipped
        key = (card_id, pack_id, is_back)
        if key in seen:
            return
        seen.add(key)
        part = "~back" if is_back else ""
        name = f"{card_id}@{pack_id}{part}{BLEED_SUFFIX}.jpg"
        log(f"{'[DRY] ' if args.dry_run else '[OK]  '} {os.path.basename(src_path)} -> {name}")
        audit_rows.append([src_path, name])
        if write_image(src_path, os.path.join(out_folder, name), args.dry_run):
            written += 1

    for folder in find_scenario_folders(root):
        pack = resolve_pack(folder, pack_lookup)
        if not pack:
            log(f"[SKIP] {folder} (no matching pack in the catalog)")
            skipped += 1
            continue

        pack_id = normalize_title(pack)
        by_number, by_title = by_pack.get(pack, ({}, {}))
        scans = index_scans(os.listdir(folder))
        log(f"\nScanning: {folder}  ->  {pack}")

        for _qty, card_type, front_ref, back_ref in parse_manifest(
            (folder / "Card list.txt").read_text(encoding="utf-8")
        ):
            front_files = scans.get(scan_key(front_ref))
            if not front_files:
                log(f"[SKIP] {front_ref} (no scan in {folder.name})")
                skipped += 1
                continue

            front_number, front_title, _ = parse_card_ref(front_ref)
            back_number, back_title, back_is_reverse = parse_card_ref(back_ref)

            # Flight from Moria prints one 2A face on three cards, each with a
            # different 2B reverse. The front carries no number there, so the
            # card is identified by the reverse it is paired with.
            if front_number is None and back_is_reverse:
                front_number = back_number

            # Position 0 is the pack cover, which Hall of Beorn does not list.
            # Only cards the catalog tracks are written, so it and its
            # introduction back are both dropped. See EXTRA_CARDS.md.
            if front_number == 0:
                continue

            card, how = pick_card(front_number, front_title, by_number, by_title)
            if card is None:
                log(f"[SKIP] {front_ref} (not in pack {pack})")
                skipped += 1
                continue
            if how == "title" and front_number in by_number:
                log(
                    f"[INFO] {front_ref}: position {front_number} is "
                    f"{by_number[front_number]['Title']!r} in the catalog, "
                    f"matched on title instead"
                )
            card_id = normalize_title(card["Slug"])

            copies = front_files if args.all_copies else front_files[:1]
            for _copy, filename in copies:
                emit(str(folder / filename), card_id, pack_id, is_back=False)

            if back_ref in SHARED_BACKS:
                continue

            back_files = scans.get(scan_key(back_ref))
            if not back_files:
                log(f"[SKIP] {back_ref} (back of {front_ref}, no scan)")
                skipped += 1
                continue

            back_src = str(folder / back_files[0][1])
            if (card_id, pack_id, True) in seen:
                log(
                    f"[WARN] {front_ref} has more than one back in the manifest; "
                    f"keeping the first and ignoring {back_ref!r}"
                )
            emit(back_src, card_id, pack_id, is_back=True)

            # A numbered back with no stage letter is a card in its own right
            # (The Drowned Ruins pairs Jagged Cavern with Overgrown Passage,
            # and both are catalog cards), so write the pair the other way round
            # too. A "2B" back is the reverse of the front's own card and must
            # not become a front.
            if back_number and not back_is_reverse:
                back_card, _how = pick_card(back_number, back_title, by_number, by_title)
                if back_card is not None:
                    back_id = normalize_title(back_card["Slug"])
                    emit(back_src, back_id, pack_id, is_back=False)
                    emit(str(folder / front_files[0][1]), back_id, pack_id, is_back=True)

    return written, skipped, audit_rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", help="The 'LOTR LCG Nightmare Cards - Remastered' folder")
    parser.add_argument("-o", "--output", default=".", help="Where to create lotrlcg-nightmare/")
    parser.add_argument("--dry-run", action="store_true", help="Preview without writing images")
    parser.add_argument(
        "--all-copies",
        action="store_true",
        help="Write every numbered copy of a card, not just the first. Produces "
        "names Proxy Nexus cannot yet index -- for inspecting the archive only.",
    )
    args = parser.parse_args()

    root = pathlib.Path(args.archive)
    if not root.is_dir():
        sys.exit(f"No such folder: {root}")

    out_folder = os.path.abspath(os.path.join(args.output, "lotrlcg-nightmare"))
    os.makedirs(out_folder, exist_ok=True)

    pack_lookup, by_pack = build_lookups(load_catalog())

    log_handle = open(os.path.join(out_folder, "migrate.log"), "w", encoding="utf-8")
    rename.set_log_file(log_handle)
    try:
        log(f"--- Scanning {'(DRY RUN) ' if args.dry_run else ''}{root} ---")
        written, skipped, audit_rows = process_archive(
            root, pack_lookup, by_pack, out_folder, args
        )

        if not args.dry_run and written:
            audit_path = os.path.join(out_folder, "migration_audit_log.csv")
            with open(audit_path, "w", newline="", encoding="utf-8") as f:
                csv.writer(f).writerows(audit_rows)
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
