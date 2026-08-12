# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow"]
# ///
"""
Downloads every Netrunner Reboot Project card image, plus the alt arts.

    uv run download.py ~/Downloads/netrunner-reboot-raw
    uv run rename.py ~/Downloads/netrunner-reboot-raw -o ~/Downloads/netrunner-reboot

Card images land under their NRDB code (`01001.jpg`) and alt arts under their own
name (`01001-alt.jpg`); rename.py turns those into Proxy Nexus filenames. Keeping
the two steps apart means the catalog can be refreshed and the rename re-run
without refetching a gigabyte of images.

Every file is named for the image format actually served rather than the URL's
extension, because reteki serves the alt arts as JPEG from a .png path.

Safe to re-run: images already downloaded are left alone, so an interrupted run
resumes where it stopped.
"""

import os
import sys
import time
import argparse
import http.client
import urllib.error
import urllib.request

# rename.py owns the catalog and the id normalization; importing it keeps this
# script from carrying a second copy that could drift. Python puts a script's
# own directory on sys.path, so this resolves however the script is invoked.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rename  # noqa: E402

USER_AGENT = 'ProxyNexus-ImageMigrator/1.0'

REQUEST_DELAY = 0.25


def wanted(catalog):
    """Every image to fetch, as (stem, url, description).

    Card images come from nrdb.reteki.fun rather than media.reteki.fun, where
    the alt arts live. The two serve identical bytes for every ordinary card,
    but a flip card's faces only come as one image from nrdb.reteki.fun;
    media.reteki.fun has the front alone, and for the few cards in MEDIA_FRONTS
    that front is the newer render, so it is fetched as well.
    """
    by_code = rename.cards_by_code(catalog.cards)

    items = [(card['code'],
              catalog.image_url_template.format(code=card['code']),
              f"{card['title']} ({card['pack_code']})")
             for card in catalog.cards]

    for art in catalog.alt_arts:
        card = by_code.get(art['code'])
        title = card['title'] if card else f"card {art['code']}"
        items.append((art['stem'],
                      rename.ALT_IMAGE_URL.format(stem=art['stem']),
                      f"{title} ({art['label']})"))

    for code in sorted(rename.MEDIA_FRONTS):
        card = by_code.get(code)
        title = card['title'] if card else f"card {code}"
        items.append((f"{code}-{rename.FRONT_VARIANT}",
                      rename.ALT_IMAGE_URL.format(stem=code),
                      f"{title} (front)"))
    return items


def existing_image(folder, stem):
    """The already-downloaded image for `stem`, whatever extension it landed
    under, or None.

    A file that isn't a readable image counts as absent, so anything a previous
    run left behind broken gets refetched rather than passing as present.
    """
    for extension in rename.IMAGE_EXTS:
        path = os.path.join(folder, stem + extension)
        if os.path.exists(path) and rename.image_kind(path) is not None:
            return path
    return None


def download(url, folder, stem):
    """Fetch `url` into `folder`, named for the image format it turns out to be.

    Writes through a temp file, so a failed or unreadable download leaves
    nothing behind. Returns (path, (width, height)), or None if what arrived
    isn't an image this pipeline can use.
    """
    req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
    with urllib.request.urlopen(req) as response:
        data = response.read()

    tmp_path = os.path.join(folder, stem + '.part')
    try:
        with open(tmp_path, 'wb') as f:
            f.write(data)

        kind = rename.image_kind(tmp_path)
        if kind is None:
            os.remove(tmp_path)
            return None

        extension, size = kind
        path = os.path.join(folder, stem + extension)
        os.replace(tmp_path, path)
        return path, size
    except BaseException:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)
        raise


def main():
    parser = argparse.ArgumentParser(
        description="Download every Netrunner Reboot card image and alt art from reteki.")
    parser.add_argument("folder", help="Where to write the images")
    parser.add_argument("--dry-run", action="store_true", help="List what would be fetched")
    args = parser.parse_args()

    folder = os.path.abspath(args.folder)
    if not args.dry_run:
        os.makedirs(folder, exist_ok=True)

    catalog = rename.load_catalog()
    items = wanted(catalog)
    pending = [item for item in items if existing_image(folder, item[0]) is None]

    print(f"--- {len(pending)} of {len(items)} image(s) to fetch "
          f"{'(DRY RUN) ' if args.dry_run else ''}---")

    fetched = 0
    missing = []
    failed = []

    for stem, url, description in pending:
        if args.dry_run:
            print(f"[DRY]  {description} -> {stem}   {url}")
            fetched += 1
            continue

        try:
            result = download(url, folder, stem)
        except urllib.error.HTTPError as e:
            # reteki's card database lists cards it has no art for.
            if e.code == 404:
                print(f"[404]  {stem}: no image for '{description}'")
                missing.append((stem, description))
            else:
                print(f"[ERR]  {stem}: {e}")
                failed.append(stem)
            continue
        except (urllib.error.URLError, http.client.HTTPException, OSError) as e:
            # A connection cut mid-body raises IncompleteRead, which is an
            # HTTPException rather than an OSError.
            print(f"[ERR]  {stem}: {e}")
            failed.append(stem)
            continue

        if result is None:
            print(f"[ERR]  {stem}: what arrived is not a usable image")
            failed.append(stem)
            continue

        path, size = result
        print(f"[OK]   {description} -> {os.path.basename(path)}   {size[0]}x{size[1]}")
        fetched += 1
        time.sleep(REQUEST_DELAY)

    verb = "would fetch" if args.dry_run else "fetched"
    print(f"\nSummary: {verb} {fetched} of {len(pending)}.")
    if missing:
        print(f"{len(missing)} image(s) reteki does not have:")
        for stem, description in missing:
            print(f"  {stem}  {description}")
    if failed:
        print(f"{len(failed)} image(s) failed; re-run to retry them.")


if __name__ == "__main__":
    main()
