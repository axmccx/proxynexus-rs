# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow", "unidecode"]
# ///
import os
import pathlib
import json
import re
import csv
import argparse
import urllib.request
import shutil
from unidecode import unidecode
from PIL import Image

CATALOG_URLS = [
    "http://hallofbeorn.com/Export/PlayerCards",
    "http://hallofbeorn.com/Export/EncounterCards",
    "http://hallofbeorn.com/Export/QuestCards",
]

CACHE_PATH = pathlib.Path(__file__).resolve().parent / "lotrlcg_catalog_cache.json"

# Source archives in the order they are searched. A card is taken from the first
# folder that has it, so this order is the quality preference.
#
# `has_bleed` says whether that archive's scans already carry a print-bleed
# border. It has to be declared per archive rather than measured: the trimmed
# scans in "Lord of the Rings LCG RAW" and the bled quest cards in "Lord of the
# Rings LCG" overlap in aspect ratio, so no threshold separates them.
SOURCE_FOLDERS = [
    ("Enhanced Proxies", True),
    ("Lord of the Rings LCG", True),
    ("Lord of the Rings LCG RAW", False),
]

log_file_handle = None

def log(msg):
    print(msg)
    if log_file_handle:
        log_file_handle.write(msg + "\n")

def set_log_file(handle):
    """Point log() at an open file. rename_alep.py reuses log(), so it needs a
    way to redirect the handle from outside this module."""
    global log_file_handle
    log_file_handle = handle

def fetch_json(url):
    print(f"Fetching: {url}")
    req = urllib.request.Request(url, headers={'User-Agent': 'ProxyNexus-ImageMigrator/1.0'})
    with urllib.request.urlopen(req) as response:
        return json.loads(response.read().decode('utf-8'))

def load_catalog():
    """Return the combined Hall of Beorn player/encounter/quest exports.

    Cached next to this script; delete the cache to pick up catalog changes.
    """
    if CACHE_PATH.exists():
        with open(CACHE_PATH, "r", encoding="utf-8") as f:
            return json.load(f)

    cards = []
    for url in CATALOG_URLS:
        cards.extend(fetch_json(url))
    with open(CACHE_PATH, "w", encoding="utf-8") as f:
        json.dump(cards, f)
    return cards

def clean_for_match(text):
    text = unidecode(text).lower()
    text = text.replace("_", "")
    norm = "".join([c for c in text if c.isalnum()])
    if norm.startswith("the"):
        norm = norm[3:]
    return norm

def normalize_title(title):
    text = unidecode(title).lower()
    return "".join([c if c.isalnum() else "_" for c in text])

# Filenames whose spelling disagrees with the Hall of Beorn title for that pack.
# Card ids are derived from the HoB slug, so the file has to be matched against
# HoB's spelling even when HoB is the one with the typo (e.g. "The Wizards's
# Voice", "Watter-logged Halls"). Keyed by pack: the same wording is often
# correct in another pack, so a global replace silently loses that card.
PACK_TITLE_FIXES = {
    ("A Shadow in the East", "Treacherour Easterling"): "Treacherous Easterling",
    ("Heirs of Númenor", "Lost Company"): "Lost Companion",
    ("Heirs of Númenor", "Watcher in the Woods"): "Watcher in the Wood",
    ("Khazad-dûm", "Chieftan of the Pit"): "Chieftain of the Pit",
    ("Khazad-dûm", "Veteran of Nanduhiron"): "Veteran of Nanduhirion",
    ("Return to Mirkwood", "Rumor from the Earth"): "Rumour from the Earth",
    ("Return to Mirkwood", "To the Elven King's Halls"): "To the Elvin King's Halls",
    ("Shadow and Flame", "A Elbereth! Gilthonial!"): "A Elbereth! Gilthoniel!",
    ("The Battle of Carn Dûm", "Vile Affliction"): "Vile Afflication",
    ("The Black Riders", "Bill Fenry"): "Bill Ferny",
    ("The Black Riders", "Sticken Dumb"): "Striken Dumb",
    ("The Blood of Gondor", "Tome of Atanator"): "Tome of Atanatar",
    ("The Crossings of Poros", "Battle at the Crossroads"): "Battle at the Crossings",
    ("The Crossings of Poros", "Magic Rings"): "Magic Ring",
    ("The Dead Marshes Nightmare", "Swarming Mosquitoes"): "Swarming Mosquitos",
    ("The Drowned Ruins", "Water-logged Halls"): "Watter-logged Halls",
    ("The Fate of Wilderland", "Gwahir's Debt"): "Gwaihir's Debt",
    ("The Grey Havens", "The Havens Burns"): "The Havens Burn",
    ("The Hunt for Gollum", "Mustering the Rohirim"): "Mustering the Rohirrim",
    ("The Mountain of Fire", "The Battle of Morannon"): "The Battle of the Morannon",
    ("The Nîn-in-Eilph Nightmare", "An Arduous Journey"): "Through the Marsh",
    ("The Nîn-in-Eilph Nightmare", "Deadly Waters"): "Lost in the Swanfleet",
    ("The Old Forest", "The Wicked Willow"): "The Wiched Willow",
    ("The Road Darkens", "The Argonath"): "The Argonauth",
    ("The Sands of Harad", "Mordor Wargs"): "Mordor Warg",
    ("The Voice of Isengard", "The Wizard's Voice"): "The Wizards's Voice",
    ("The Watcher in the Water Nightmare", "Writhing Tentacle"): "Writing Tentacle",
    ("Under the Ash Mountains", "Ash Mountain Werewofl"): "Ash Mountain Werewolf",
    ("Under the Ash Mountains", "Buring Reek"): "Burning Reek",
}

# Matched on the cleaned name so the table can be written with real punctuation
# regardless of how the scan spells it ("Gwahir_s Debt" vs "Gwahir's Debt").
PACK_TITLE_FIXES_CLEAN = {
    (pack, clean_for_match(name)): fixed
    for (pack, name), fixed in PACK_TITLE_FIXES.items()
}

def parse_filename(base_name):
    """Parse a scan filename (without extension) into (position_str, text_name, is_back).

    Handles filename shapes seen in the real archive:
      '001 - Aragorn'
      '047a - A Perilous Voyage'
      '011 - 1B - The Hunt Begins'
      '- - 2A - Lost in the Swanfleet'  (normalized to '000 - ...' first)

    Returns None if base_name could not be parsed at all (mirrors the original
    inline `continue` in main()'s per-file loop).
    """
    # Normalize "- - " to "000 - " to fix parsing for Nightmare front replacements
    if base_name.startswith("- - "):
        filename_to_parse = "000 - " + base_name[4:]
    else:
        filename_to_parse = base_name

    # Extract position and name: '001 - Aragorn' or '047a - A Perilous Voyage'
    if " - " in filename_to_parse:
        position_str, text_name = filename_to_parse.split(" - ", 1)
        secondary_match = re.match(r'^(\d+[A-Za-z])\s*[-_.]\s*(.*)$', text_name)
        if secondary_match:
            side_str, text_name = secondary_match.groups()
            position_str = side_str
    else:
        match = re.match(r'^(\d+[A-Za-z]?)?\s*(?:[-_.]\s*)?(.*)$', base_name)
        if not match:
            return None
        position_str, text_name = match.groups()

    is_back = False

    # Check position_str for side indicators (e.g. if the file is just '1B - The Hunt Begins')
    if position_str:
        pos_match = re.match(r'^\d+([A-Za-z])$', position_str)
        if pos_match and pos_match.group(1).upper() in ('B', 'D', 'F', 'H'):
            is_back = True

    # Check text_name for side indicators (e.g. if the file is '011 - 1B - The Hunt Begins')
    side_match = re.match(r'^(\d+)([A-Za-z])\s*[-_.]\s*(.*)$', text_name)
    if side_match:
        if side_match.group(2).upper() == 'B':
            is_back = True

    # Now strip the prefix if it exists
    text_name = re.sub(r'^\d+[A-Za-z]\s*[-_.]\s*', '', text_name)

    # Handle Explicit Back-Faces and Side indicators
    text_name_lower = text_name.lower()
    if "(side b)" in text_name_lower:
        is_back = True
        text_name = re.sub(r'(?i)\s*\(side b\)', '', text_name)
    elif "(side a)" in text_name_lower:
        text_name = re.sub(r'(?i)\s*\(side a\)', '', text_name)
    elif "reverse" in text_name_lower:
        is_back = True
        text_name = re.sub(r'(?i)\s*\(?reverse\)?', '', text_name)
    # Removed the harmful "back" stripping logic here
    # Strip (errata) from filenames
    text_name = re.sub(r'(?i)\s*\(errata\)', '', text_name)

    # Strip trailing deduplication numbers (e.g. "Dark Pools 3" -> "Dark Pools")
    text_name = re.sub(r'\s+\d+$', '', text_name)

    return position_str, text_name, is_back

def find_orphaned_backs(folder_file_lists):
    """Validate '~back' files against their front within EACH folder independently.

    folder_file_lists: {folder_path_or_name: iterable_of_filenames}

    A back file's matching front must live in the SAME output folder. A back in
    lotrlcg-nightmare/ whose front happens to have landed in lotrlcg-enhanced/
    (or vice versa) is genuinely orphaned within its own collection and must be
    reported, even though the union of both folders would contain the front.

    Returns {folder: [orphaned_back_filenames]} for folders with at least one
    orphan.
    """
    orphans_by_folder = {}
    for folder, files in folder_file_lists.items():
        file_set = set(files)
        folder_orphans = []
        for filename in file_set:
            if "~back" in filename:
                front_filename = filename.replace("~back", "")
                if front_filename not in file_set:
                    folder_orphans.append(filename)
        if folder_orphans:
            orphans_by_folder[folder] = folder_orphans
    return orphans_by_folder

def process_folders(input_folders, pack_lookup, card_lookup, output_folder, args):
    """Walk all input folders, match scans against the catalog, and copy/crop
    them into output_folder. Returns (copied, skipped, audit_rows).

    Nightmare packs belong to rename_nightmare.py and are skipped here.
    """
    processed_cards = set() # Track (target_id, pack_code, is_back)
    copied = 0
    skipped = 0

    # We will buffer the audit rows in case it's a dry run, or just write them if it's not
    audit_rows = [["Source Path", "Bleed Output", "Crop Output"]]

    for input_folder, has_bleed in input_folders:
        if not os.path.exists(input_folder):
            print(f"[WARN] Input folder not found: {input_folder}")
            continue

        bleed_suffix = ".bleed" if has_bleed else ""

        log(f"\nScanning: {input_folder}")
        for root, _, files in os.walk(input_folder):

            # The folder name might be '01 - The Hunt for Gollum'
            folder_name = os.path.basename(root)
            folder_name_stripped = re.sub(r'^[\d\s-]+', '', folder_name)
            clean_folder = clean_for_match(folder_name_stripped)

            pack_code = None
            if "Nightmare" in root:
                pack_code = pack_lookup.get(clean_for_match(folder_name_stripped + " Nightmare"))
                if not pack_code:
                    parent_folder = os.path.basename(os.path.dirname(root))
                    parent_stripped = re.sub(r'^[\d\s-]+', '', parent_folder)
                    pack_code = pack_lookup.get(clean_for_match(parent_stripped + " Nightmare"))

            if not pack_code:
                pack_code = pack_lookup.get(clean_folder)

            # If not found, try the parent folder (e.g. if we are in 'Player' subfolder)
            if not pack_code:
                parent_folder = os.path.basename(os.path.dirname(root))
                parent_stripped = re.sub(r'^[\d\s-]+', '', parent_folder)
                clean_parent = clean_for_match(parent_stripped)
                pack_code = pack_lookup.get(clean_parent)

                if not pack_code:
                    pack_code = pack_lookup.get(clean_for_match(parent_stripped + " Nightmare"))

                # Check grandparent folder (e.g. for Two-Player Starter/Encounter/The Oath)
                if not pack_code:
                    grandparent_folder = os.path.basename(os.path.dirname(os.path.dirname(root)))
                    grandparent_folder_stripped = re.sub(r'^[\d\s-]+', '', grandparent_folder)
                    pack_code = pack_lookup.get(clean_for_match(grandparent_folder_stripped))

                # Last resort fallback: aggressively guess it's a nightmare pack
                if not pack_code:
                    pack_code = pack_lookup.get(clean_for_match(folder_name_stripped + " Nightmare"))

            if not pack_code:
                # Optionally log a debug message so we don't silently ignore folders we should process
                # log(f"[DEBUG] Silently skipping folder: {root}")
                continue

            # Skipping on the resolved pack rather than on the path keeps the
            # rule in one place, and catches the Nightmare/ subfolders that sit
            # inside every cycle of "Lord of the Rings LCG".
            if "nightmare" in pack_code.lower():
                continue

            for filename in files:
                if not filename.lower().endswith(('.jpg', '.jpeg', '.png')):
                    continue

                base_name = os.path.splitext(filename)[0]

                parsed = parse_filename(base_name)
                if parsed is None:
                    continue
                position_str, text_name, is_back = parsed

                # Fix known FFG typos
                text_name = text_name.replace("The Gathering of the Clouds", "The Gathering of Clouds")
                text_name = text_name.replace("Provisons", "Provisions")
                text_name = text_name.replace("Hobbitt-sense", "Hobbit-sense")
                text_name = text_name.replace("Raven Claw Druid", "Raven Clan Druid")
                text_name = text_name.replace("The Hunt fo Gollum", "The Hunt for Gollum")
                text_name = text_name.replace("The Trail Grows Cold", "The Trail Goes Cold")
                text_name = text_name.replace("Goblin Trapper", "Gobline Trapper")
                text_name = text_name.replace("Blinding the Blizzard", "Blinding Blizzard")
                text_name = text_name.replace("Swarms of Mosquitos", "Swarms of Mosquitoes")
                text_name = text_name.replace("Arched Tunnels", "Arched Tunnel")
                text_name = text_name.replace("Rob & Bob", "Rob and Bob")
                # NOT mojibake damage in this script: Hall of Beorn's own export
                # really does store this title (and slug) with "L≤rien" for this
                # specific card — verified in EncounterCards.json, CardSet "The
                # Dead Marshes Nightmare", Slug "Lost-Soul-of-L≤rien-TDMN". Since
                # card ids are derived from the HoB slug (see PACK_TITLE_FIXES
                # above), the filename has to be rewritten INTO HoB's mangled
                # spelling to match, not the correct one.
                text_name = text_name.replace("Lost Soul of Lorien", "Lost Soul of L≤rien")
                text_name = text_name.replace("Venemous Spider", "Venomous Spider")

                clean_name = clean_for_match(text_name)
                # Pack-scoped corrections, applied last so they win over the blanket ones
                clean_name = clean_for_match(
                    PACK_TITLE_FIXES_CLEAN.get((pack_code, clean_name), clean_name)
                )

                pack_cards = card_lookup.get(pack_code, {})
                matched_cards = pack_cards.get(clean_name, [])

                if not matched_cards:
                    matched_cards = pack_cards.get(clean_for_match(base_name), [])

                # Fallback to matching against the slug if title match fails
                if not matched_cards:
                    for cards_list in pack_cards.values():
                        for card in cards_list:
                            slug = card.get('Slug', '').replace('-', '').replace("'", "").lower()
                            if clean_name in slug:
                                matched_cards.append(card)

                if not matched_cards:
                    log(f"[SKIP] {filename} (Card '{text_name}' not found in pack {pack_code})")
                    skipped += 1
                    continue

                selected_cards = []
                if len(matched_cards) == 1:
                    selected_cards = [matched_cards[0]]
                else:
                    # Collision! Fallback to position matching
                    selected_card = None

                    is_shared_front = False
                    if not is_back and len(matched_cards) > 1:
                        # Are they all identical stages?
                        enc_info = matched_cards[0].get('EncounterInfo') or {}
                        first_stage = enc_info.get('StageNumber')
                        first_letter = enc_info.get('StageLetter')
                        if first_stage and first_letter:
                            if all((mc.get('EncounterInfo') or {}).get('StageNumber') == first_stage and
                                   (mc.get('EncounterInfo') or {}).get('StageLetter') == first_letter
                                   for mc in matched_cards):
                                is_shared_front = True

                    if not is_shared_front and position_str:
                        try:
                            num_str = re.sub(r'[A-Za-z]+$', '', position_str)
                            pos_int = int(num_str)
                            for mc in matched_cards:
                                if mc.get('position') == pos_int:
                                    selected_card = mc
                                    break
                        except ValueError:
                            pass

                    if selected_card:
                        selected_cards = [selected_card]
                    else:
                        if not is_back:
                            # It's an A face (front) and we couldn't resolve by position.
                            # Map this front face to ALL matching cards!
                            log(f"[INFO] Mapping front face {filename} to all {len(matched_cards)} matched cards.")
                            selected_cards = matched_cards
                        else:
                            # Print warning for back faces
                            log(f"[WARN] Number mismatch on collision for {filename}, defaulting to {matched_cards[0]['target_id']}")
                            selected_cards = [matched_cards[0]]

                for selected_card in selected_cards:
                    target_id = selected_card['target_id']
                    unique_key = (target_id, pack_code, is_back)

                    if unique_key in processed_cards:
                        continue
                    processed_cards.add(unique_key)

                    clean_pack_code = normalize_title(pack_code)

                    part_suffix = "~back" if is_back else ""
                    bleed_name = f"{target_id}@{clean_pack_code}{part_suffix}{bleed_suffix}.jpg"
                    old_path = os.path.join(root, filename)
                    bleed_path = os.path.join(output_folder, bleed_name)

                    log(f"{'[DRY] ' if args.dry_run else '[OK]  '} {filename} -> {bleed_name}")
                    audit_rows.append([old_path, bleed_name, ""])

                    if not args.dry_run:
                        try:
                            # Process version using Pillow to compress to 90% quality
                            with Image.open(old_path) as img:
                                # Handle transparent PNG to JPEG conversion for bleed
                                bleed_img = img
                                if bleed_img.mode in ("RGBA", "P"):
                                    bleed_img = bleed_img.convert("RGB")
                                bleed_img.save(bleed_path, format="JPEG", quality=90)

                            copied += 1
                        except Exception as e:
                            print(f"[ERR]  {filename}: {e}")
                    else:
                        copied += 1

                    # Now add the double-sided mapping for Na'asiyah and Sahír
                    if target_id in ("na_asiyah_enemy_tgh", "na_asiyah_objective_ally_tgh", "captain_sahir_enemy_tgh", "captain_sahir_objective_ally_tgh"):
                        alt_id = ""
                        if target_id == "na_asiyah_enemy_tgh": alt_id = "na_asiyah_objective_ally_tgh"
                        elif target_id == "na_asiyah_objective_ally_tgh": alt_id = "na_asiyah_enemy_tgh"
                        elif target_id == "captain_sahir_enemy_tgh": alt_id = "captain_sahir_objective_ally_tgh"
                        elif target_id == "captain_sahir_objective_ally_tgh": alt_id = "captain_sahir_enemy_tgh"

                        alt_key = (alt_id, pack_code, True) # we are writing the back side
                        if alt_key not in processed_cards:
                            processed_cards.add(alt_key)
                            alt_bleed_name = f"{alt_id}@{clean_pack_code}~back{bleed_suffix}.jpg"
                            alt_bleed_path = os.path.join(output_folder, alt_bleed_name)

                            log(f"{'[DRY] ' if args.dry_run else '[OK]  '} {filename} -> {alt_bleed_name} (double-sided link)")
                            audit_rows.append([old_path, alt_bleed_name, ""])
                            if not args.dry_run:
                                try:
                                    shutil.copy2(bleed_path, alt_bleed_path)
                                    copied += 1
                                except Exception as e:
                                    print(f"[ERR] Double-sided link {filename}: {e}")
                            else:
                                copied += 1

    return copied, skipped, audit_rows

def main():
    parser = argparse.ArgumentParser(description="Migrate and crop LotR LCG images.")
    parser.add_argument("archive", help="Folder holding the source archives named in SOURCE_FOLDERS")
    parser.add_argument("-o", "--output", default=".", help="Folder to create lotrlcg-enhanced/ in")
    parser.add_argument("--dry-run", action="store_true", help="Preview changes without copying or cropping")
    args = parser.parse_args()

    archive_root = pathlib.Path(args.archive)
    input_folders = [(str(archive_root / name), has_bleed) for name, has_bleed in SOURCE_FOLDERS]

    enhanced_folder = os.path.abspath(os.path.join(args.output, "lotrlcg-enhanced"))

    all_cards = load_catalog()

    # 1. Map cleaned pack names to exact CardSet strings
    pack_lookup = {}
    for c in all_cards:
        card_set = c.get('CardSet', '')
        if not card_set:
            continue
        clean_p = clean_for_match(card_set)
        pack_lookup[clean_p] = card_set

    # Inject aliases for Hobbit sagas where the folder name drops "The Hobbit"
    pack_lookup["overhillandunderhill"] = "The Hobbit: Over Hill and Under Hill"
    pack_lookup["onthedoorstep"] = "The Hobbit: On the Doorstep"

    # 2. Count name collisions per pack to align with our ProxyNexus adapter logic
    name_pack_counts = {}
    for c in all_cards:
        title = c.get('Title', '')
        card_set = c.get('CardSet', '')
        norm = normalize_title(title)
        key = (norm, card_set)
        name_pack_counts[key] = name_pack_counts.get(key, 0) + 1

    # 3. Build Card Lookup: card_set -> clean_for_match(title) -> list of cards
    card_lookup = {}
    for c in all_cards:
        title = c.get('Title', '')
        card_set = c.get('CardSet', '')
        if not title or not card_set:
            continue

        clean_name = clean_for_match(title)
        norm = normalize_title(title)

        if card_set not in card_lookup:
            card_lookup[card_set] = {}
        if clean_name not in card_lookup[card_set]:
            card_lookup[card_set][clean_name] = []

        slug = c.get('Slug', '')
        if not slug:
            continue
        target_id = normalize_title(slug)

        # Create a localized card dict for this specific printing
        card_copy = c.copy()
        card_copy['target_id'] = target_id
        card_copy['position'] = c.get('Number')
        card_copy['pack_code'] = card_set
        card_lookup[card_set][clean_name].append(card_copy)

    global log_file_handle
    log_path = os.path.join(enhanced_folder, "migrate.log")
    os.makedirs(enhanced_folder, exist_ok=True)
    log_file_handle = open(log_path, "w", encoding="utf-8")

    try:
        log(f"\n--- Scanning {'(DRY RUN) ' if args.dry_run else ''}---")

        copied, skipped, audit_rows = process_folders(
            input_folders, pack_lookup, card_lookup, enhanced_folder, args
        )

        audit_log_path = os.path.join(enhanced_folder, "migration_audit_log.csv")
        if not args.dry_run and copied > 0:
            with open(audit_log_path, 'w', newline='', encoding='utf-8') as f:
                writer = csv.writer(f)
                writer.writerows(audit_rows)
            print(f"\nAudit log written to: {audit_log_path}")

        if not args.dry_run and copied > 0:
            # A back file's front must exist in the SAME folder as the back.
            # Each output folder becomes its own collection, so a back whose
            # front lives in another one is orphaned within its own even though
            # the union of the folders has it -- hence the mapping.
            print("\nValidating generated images...")
            folder_file_lists = {
                enhanced_folder: os.listdir(enhanced_folder),
            }
            orphans_by_folder = find_orphaned_backs(folder_file_lists)
            total_orphans = sum(len(v) for v in orphans_by_folder.values())

            if total_orphans:
                log(f"\n[ERROR] Validation failed! Found {total_orphans} back images with no front image:")
                for folder, orphaned_backs in orphans_by_folder.items():
                    for ob in orphaned_backs:
                        log(f"  Missing front for: {ob} (in {os.path.basename(folder)})")
            else:
                log("\n[OK] Validation passed: All back images have a corresponding front image.")

        log(f"\nSummary: {copied} unique printings processed, {skipped} files skipped.")
    finally:
        log_file_handle.close()

if __name__ == "__main__":
    main()
