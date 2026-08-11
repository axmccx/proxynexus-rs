# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
Downloads cards ThronesDB has that a renamed scan folder is missing.

The official FFG one is short 18 cards and ships a nineteenth (Syrio Forel) as a zero-byte file.

    uv run rename.py <archive> -o ~/agot-rebuild
    uv run download_missing.py ~/agot-rebuild

Run it against rename.py's *output*, not a source archive. It reads the
Proxy Nexus filenames to work out what's already there. Safe to re-run; cards
already present are left alone.

ThronesDB's images are roughly 300x419, below the 492x699 of the FFG scans and
the 822x1122 of the community scans. See README.md.
"""

import os
import sys
import time
import argparse
import urllib.error
import urllib.request

# rename.py owns the catalog and the id normalization; importing it keeps this
# script from carrying a second copy that could drift. Python puts a script's
# own directory on sys.path, so this resolves however the script is invoked.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rename  # noqa: E402

USER_AGENT = 'ProxyNexus-ImageMigrator/1.0'

REQUEST_DELAY = 0.25


def missing_from(output_folder, cards):
    """Catalog cards absent from `output_folder`, as (card, filename) pairs.

    Scope is deliberately limited to packs the folder already has at least one
    card from. Without that, filling the gaps in an FFG rebuild would also try
    to pull down every community pack and all 281 Tower of Joy draft cards.
    """
    present = set()
    packs = set()
    for name in os.listdir(output_folder):
        match = rename.OUTPUT_NAME.match(name)
        if not match:
            continue
        packs.add(match.group('pack'))
        # A zero-byte file counts as absent, so an interrupted copy gets
        # refetched rather than passing as present. Only zero bytes: anything
        # with content is left alone, since this can't tell a format it doesn't
        # parse from a broken file, and overwriting someone's card is worse
        # than skipping it.
        if os.path.getsize(os.path.join(output_folder, name)) > 0:
            present.add((match.group('id'), match.group('pack')))

    gaps = []
    for card in cards:
        pack = card['pack_code']
        if pack not in packs:
            continue
        card_id = rename.normalize_title(card['label'])
        if (card_id, pack) in present:
            continue
        url = card.get('image_url') or ''
        ext = os.path.splitext(url)[1].lower() or '.jpg'
        gaps.append((card, f"{card_id}@{pack}{ext}"))
    return sorted(gaps, key=lambda gap: gap[1])


def download(url, path):
    """Fetch `url` to `path`, via a temp file so a failure leaves nothing behind."""
    req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
    with urllib.request.urlopen(req) as response:
        data = response.read()
    tmp_path = path + '.part'
    try:
        with open(tmp_path, 'wb') as f:
            f.write(data)
        os.replace(tmp_path, path)
    except BaseException:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)
        raise


def main():
    parser = argparse.ArgumentParser(
        description="Download cards ThronesDB has that a renamed scan folder is missing.")
    parser.add_argument("folder", help="A folder of renamed scans (rename.py's output)")
    parser.add_argument("--dry-run", action="store_true", help="List what would be fetched")
    args = parser.parse_args()

    folder: str = os.path.abspath(args.folder)
    if not os.path.isdir(folder):
        parser.error(f"not a folder: {folder}")

    cards, _ = rename.load_catalog()
    gaps = missing_from(folder, cards)

    if not gaps:
        print("No missing cards. Nothing to fetch.")
        return

    print(f"--- {len(gaps)} missing card(s) {'(DRY RUN) ' if args.dry_run else ''}---")
    fetched = 0
    for card, filename in gaps:
        url = card.get('image_url')
        if not url:
            print(f"[WARN] {filename}: ThronesDB has no image for '{card['name']}'")
            continue

        if args.dry_run:
            print(f"[DRY]  {card['name']} ({card['pack_code']}) -> {filename}   {url}")
            fetched += 1
            continue

        path = os.path.join(folder, filename)
        try:
            download(url, path)
        except (urllib.error.URLError, OSError) as e:
            print(f"[ERR]  {filename}: {e}")
            continue

        size = rename.image_size(path)
        dims = f"{size[0]}x{size[1]}" if size else "unreadable"
        print(f"[OK]   {card['name']} ({card['pack_code']}) -> {filename}   {dims}")
        fetched += 1
        time.sleep(REQUEST_DELAY)

    verb = "would fetch" if args.dry_run else "fetched"
    print(f"\nSummary: {verb} {fetched} of {len(gaps)}. ThronesDB serves roughly 300x419, "
          f"lower resolution than either scan archive.")


if __name__ == "__main__":
    main()
