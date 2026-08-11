# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow", "unidecode"]
# ///
import os
import re
import argparse
import importlib.util
import pathlib
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
fetch_json = rename.fetch_json
log = rename.log

# Fallback repairs applied to a filename's title fragment when it can't be found
# in the live API catalog by name. Most of these are mangled non-ASCII
# characters (macrons, umlauts, etc.) that get dropped or garbled during
# filename generation upstream (e.g. "Th_odwyn" -> "Theodwyn").
#
# The "Ring Wall [ringa]" / "Ring Wall [ringb]" entries were previously present
# only in the front-face copy of this dict (the two loops had already drifted).
# Checked against the real archive (ALeP - Fangs in the Dark.English/back_official):
# both "046-1-Ring Wall [ringa]-2o.png" and "047-1-Ring Wall [ringb]-2o.png"
# exist as back images, so the back-face path *can* in principle need these
# fixes too. In practice both files are exactly 1693758 bytes -- a size already
# in GENERIC_BACK_FILE_SIZES below -- so today they get skipped by the
# generic-back dedup check before ever reaching this lookup. Keeping a single
# shared dict (rather than two copies) means that stays true by construction
# instead of by accident, and any future archive where the byte-size shortcut
# doesn't catch them will still resolve correctly.
ENCODING_FIXES = {
    "th_odwyn": "theodwyn",
    "e_omer": "eomer",
    "e_owyn": "eowyn",
    "And ril": "Anduril",
    "Gr ma": "Grima",
    "Durbatul k": "Durbatuluk",
    "Thrakatul k": "Thrakatuluk",
    "D in": "Dain",
    "D nedain": "Dunedain",
    "Dr -buri-Dr ": "Dru-buri-Dru",
    "Dr edain": "Druedain",
    "Felar f": "Felarof",
    "G lm d": "Galmod",
    "Gh n-buri-Gh n": "Ghan-buri-Ghan",
    "Nauglam r": "Nauglamir",
    "Gh l": "Ghul",
    "P kel": "Pukel",
    "Rh n": "Rhun",
    "Sharp-eyed Dr ": "Sharp-eyed Dru",
    "Th odwyn": "Theodwyn",
    "Nazg l": "Nazgul",
    "Il vatar": "Iluvatar",
    "Gorg n": "Gorgun",
    "Manw ": "Manwe",
    "Ring Wall [ringa]": "Ring Wall A",
    "Ring Wall [ringb]": "Ring Wall B",
}

# Byte sizes of the small handful of generic/shared card-back images that
# appear across many ALeP packs (e.g. the stock "encounter card back" reused
# for single-sided locations/treacheries). Deduping by exact file size is
# brittle -- a re-export or re-compression of the source art would silently
# stop matching -- but it is what this script was tuned against, so the
# behaviour is preserved as-is. Confirmed against the real archive: e.g. the
# "Ring Wall [ringa]"/"[ringb]" back images in
# "ALeP - Fangs in the Dark.English/back_official/" are both exactly 1693758
# bytes, one of the sizes below.
GENERIC_BACK_FILE_SIZES = {1670115, 1828547, 1675019, 1693758}

def build_alep_catalog():
    print("--- Fetching ALeP Catalog from APIs ---")
    cards = []

    # 1. Fetch from Hall of Beorn ALeP export
    try:
        hob_alep = fetch_json("http://hallofbeorn.com/Export/ALeP")
        for c in hob_alep:
            c['source'] = 'hob'
            cards.append(c)
    except Exception as e:
        print(f"Failed to fetch HoB ALeP: {e}")

    # 2. Fetch from RingsDB
    try:
        ringsdb_cards = fetch_json("https://ringsdb.com/api/public/cards/")
        for rc in ringsdb_cards:
            pack_name = rc.get("pack_name", "")
            if "ALeP" in pack_name or rc.get("pack_code") in ["CoE", "TAP", "TSotS", "FotE", "TGoR", "TGC", "MotR", "BitI", "TNaA", "TSoEr", "TSR", "THo", "SNiB", "FitD", "TMoG", "TBP"]:
                # Convert RingsDB format to our intermediate format
                c = {
                    "name": rc.get("name"),
                    "pack_name": pack_name,
                    "source": "ringsdb",
                    "type_code": rc.get("type_code", ""),
                }
                cards.append(c)
    except Exception as e:
        print(f"Failed to fetch RingsDB cards: {e}")

    # Build lookup
    card_lookup = {}
    for c in cards:
        title = c.get('name', '')
        if not title:
            continue

        pack_name = c.get('pack_name', '').replace("ALeP - ", "").replace(".English", "")
        clean_pack = normalize_title(pack_name)

        clean_name = clean_for_match(title)
        if clean_pack not in card_lookup:
            card_lookup[clean_pack] = {}

        if clean_name not in card_lookup[clean_pack]:
            card_lookup[clean_pack][clean_name] = []

        card_copy = c.copy()
        target_id = normalize_title(f"{title}-{clean_pack}")
        card_copy['target_id'] = target_id

        card_lookup[clean_pack][clean_name].append(card_copy)

    return card_lookup

def resolve_target_id(raw_title, clean_pack, pack_cards):
    """Resolve a scan's title fragment to a target_id: an exact match in the
    live-catalog lookup for this pack, or the ENCODING_FIXES fallback applied
    to the raw title, normalized against the pack. Shared by both the front-
    and back-face loops in main() so the two paths can't drift again.
    """
    clean_name = clean_for_match(raw_title)
    matched = pack_cards.get(clean_name)

    if not matched:
        for bad, good in ENCODING_FIXES.items():
            raw_title = raw_title.replace(bad, good)
        return normalize_title(f"{raw_title}-{clean_pack}")
    return matched[0]['target_id']

def parse_alep_filename(filename):
    """Parse an ALeP scan filename shaped like 'PREFIX-1-Card Title-1o.png'.

    Returns (copy_num, raw_title) or None if the filename doesn't match the
    expected shape.
    """
    m = re.match(r'^[^-\s]+-(\d+)-(.+?)-\w+\.png$', filename)
    if not m:
        return None
    return m.group(1), m.group(2)

def main():
    parser = argparse.ArgumentParser(description="Migrate ALeP LotR LCG images.")
    parser.add_argument("input", help="GenericPNG folder holding the ALeP pack directories")
    parser.add_argument("-o", "--output", default="lotrlcg-alep", help="Destination folder")
    parser.add_argument("--dry-run", action="store_true", help="Preview changes without converting")
    args = parser.parse_args()

    input_dir = args.input
    output_dir = os.path.abspath(args.output)
    os.makedirs(output_dir, exist_ok=True)

    log_path = os.path.join(output_dir, "migrate_alep.log")
    log_file = open(log_path, "w", encoding="utf-8")
    rename.set_log_file(log_file)

    try:
        card_lookup = build_alep_catalog()

        log(f"Scanning {input_dir}...")
        copied = 0
        written = {}

        for item in sorted(os.listdir(input_dir)):
            pack_path = os.path.join(input_dir, item)
            if not os.path.isdir(pack_path):
                continue

            pack_name = item.replace("ALeP - ", "").replace(".English", "")
            clean_pack = normalize_title(pack_name)
            pack_cards = card_lookup.get(clean_pack, {})

            # "back_official" holds the reverse faces; everything from it gets
            # the ~back part suffix and is deduped against the shared generic
            # backs that repeat across packs.
            for face_dir, part_suffix in (("front", ""), ("back_official", "~back")):
                face_path = os.path.join(pack_path, face_dir)
                if not os.path.exists(face_path):
                    continue

                for filename in sorted(os.listdir(face_path)):
                    if not filename.endswith(".png"):
                        continue

                    parsed = parse_alep_filename(filename)
                    if not parsed:
                        log(f"[SKIP] Unknown format: {filename}")
                        continue

                    copy_num, raw_title = parsed
                    if copy_num != "1":
                        continue

                    in_file = os.path.join(face_path, filename)

                    if part_suffix == "~back":
                        try:
                            if os.path.getsize(in_file) in GENERIC_BACK_FILE_SIZES:
                                continue
                        except OSError:
                            pass

                    target_id = resolve_target_id(raw_title, clean_pack, pack_cards)
                    out_name = f"{target_id}@{clean_pack}{part_suffix}.bleed.jpg"
                    out_file = os.path.join(output_dir, out_name)

                    if out_name in written:
                        log(f"[WARN] {filename} overwrites {out_name}, already written from {written[out_name]}")
                    written[out_name] = filename

                    if args.dry_run:
                        log(f"[DRY]  {filename} -> {out_name}")
                        copied += 1
                        continue

                    try:
                        with Image.open(in_file) as img:
                            rgb_im = img.convert('RGB')
                            rgb_im.save(out_file, "JPEG", quality=90)
                        copied += 1
                        log(f"Converted {filename} -> {out_name}")
                    except Exception as e:
                        log(f"[ERR] Failed to convert {filename}: {e}")

        log(f"\nDone! Processed {copied} images.")
    finally:
        log_file.close()

if __name__ == "__main__":
    main()
