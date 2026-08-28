# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
Renames Netrunner card scans to the current Proxy Nexus naming convention.

    legacy   01001_alt1.jpg
    current  hedge_fund@alt1.jpg

WARNING: renames IN PLACE via os.rename(). No output folder, no undo. Run it
against a disposable COPY of a scan folder.

See README.md for the mapping rules and known limitations.
"""

import os
import json
import re
import argparse
import urllib.request
import time

# 5-digit code, optional _variant, optional -part, optional dot, extension
FILENAME_PATTERN = re.compile(
    r'^(\d{5})(?:_(.*?))?(?:-(front|back|face\d|front\d|back\d))?\.?([a-zA-Z0-9]+)$'
)


def fetch_all_pages(base_url):
    """Fetches all pages of a JSON:API response."""
    items = []
    url = base_url
    while url:
        print(f"Fetching: {url}")
        with urllib.request.urlopen(url) as response:
            data = json.loads(response.read().decode('utf-8'))
            items.extend(data.get('data', []))
            links = data.get('links', {})
            url = links.get('next')
            if url:
                time.sleep(0.5)
    return items


def parse_legacy_filename(filename):
    """Returns (code, variant, part, ext), or None if the name doesn't match."""
    match = FILENAME_PATTERN.match(filename)
    if not match:
        return None
    return match.groups()


def is_repeated_front(part):
    """A printing has one front, so `front2` and up are copies of it."""
    match = re.fullmatch(r'front(\d+)', part or '')
    return bool(match) and int(match.group(1)) >= 2


def canonical_part(part):
    """The current part name for a legacy one.

    A printing is one front and a sequence of backs numbered from one. The
    front is the file with no part at all, so `back1` is just `back`, and
    `face2` is the first back -- `face1` would have been the front.
    """
    if not part:
        return ''
    if part == 'front' or part == 'front1':
        return ''
    match = re.fullmatch(r'back(\d+)', part)
    if match:
        index = int(match.group(1))
        return 'back' if index == 1 else part
    match = re.fullmatch(r'face(\d+)', part)
    if match and int(match.group(1)) >= 2:
        index = int(match.group(1)) - 1
        return 'back' if index == 1 else f'back{index}'
    return part


def build_proxynexus_name(card_id, variant, pack_id, part, ext):
    """A variant label from the filename wins over the catalog's pack_id."""
    part = canonical_part(part)
    printing = variant if variant else pack_id
    new_name = f"{card_id}@{printing}"
    if part:
        new_name += f"~{part}"
    new_name += f".{ext}"
    return new_name


def load_catalog(folder):
    """Read `printings_cache.json` from `folder`, downloading it there first
    if it isn't present yet."""
    printings_path = os.path.join(folder, 'printings_cache.json')

    if os.path.exists(printings_path):
        with open(printings_path, 'r', encoding='utf-8') as f:
            return json.load(f)

    print("Catalog cache not found. Downloading full printings catalog...")
    all_printings = fetch_all_pages("https://api.netrunnerdb.com/api/v3/public/printings?page[size]=1000")
    with open(printings_path, 'w', encoding='utf-8') as f:
        json.dump(all_printings, f)
    return all_printings


def main():
    parser = argparse.ArgumentParser(
        description=(
            "Renames legacy code-based scan filenames to the current Proxy Nexus "
            "convention, IN PLACE (os.rename). "
            "DESTRUCTIVE, no undo: run only against a disposable COPY of a scan folder."
        )
    )
    parser.add_argument("folder", type=str, help="Root folder for images and catalog cache (files are renamed IN PLACE here)")
    parser.add_argument("--dry-run", action="store_true", help="Preview changes without renaming")
    args = parser.parse_args()

    folder = os.path.abspath(args.folder)

    print("--- Verifying/Fetching Catalog ---")
    all_printings = load_catalog(folder)

    # The printing's `id` is the 5-digit code used in legacy filenames.
    catalog = {
        str(i.get('id', '')): {
            'card_id': i.get('attributes', {}).get('card_id'),
            'pack_id': i.get('attributes', {}).get('card_set_id')
        } for i in all_printings
    }

    print(f"\n--- Scanning {'(DRY RUN) ' if args.dry_run else ''}---")
    renamed, skipped, repeats = 0, 0, 0

    for root, _, files in os.walk(folder):
        for filename in files:
            if filename.endswith('.json') or filename.endswith('.py'):
                continue

            parsed = parse_legacy_filename(filename)
            if not parsed: continue

            code, variant, part, ext = parsed

            if is_repeated_front(part):
                print(f"[SKIP] {filename}: a printing has one front, so '{part}' is a repeat of it")
                repeats += 1
                continue

            entry = catalog.get(code)

            if not entry or not entry['card_id']:
                skipped += 1
                continue

            new_name = build_proxynexus_name(entry['card_id'], variant, entry['pack_id'], part, ext)

            old_path = os.path.join(root, filename)
            new_path = os.path.join(root, new_name)

            if old_path == new_path: continue

            print(f"{'[DRY] ' if args.dry_run else '[OK]  '} {filename} -> {new_name}")
            
            if not args.dry_run:
                try:
                    os.rename(old_path, new_path)
                    renamed += 1
                except Exception as e:
                    print(f"[ERR]  {filename}: {e}")
            else:
                renamed += 1

    print(f"\nSummary: {renamed} processed, {skipped} skipped, {repeats} repeated fronts left alone.")

if __name__ == "__main__":
    main()
