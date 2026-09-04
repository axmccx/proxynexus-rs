# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow"]
# ///
"""
Makes Arkham Horror LCG scans face the same way, against ArkhamDB's own images.

Investigators, acts and agendas are printed landscape. Like AGOT's plot cards
they are still *stored portrait*, so every card in a collection is the same
shape -- see ../agot/rotate_horizontal.py. The archive already does that, and
nothing here comes out landscape.

What the archive does not do is store them facing consistently. A Core Set
investigator's back is a quarter turn one way and its Carcosa equivalent a
quarter turn the other, so half read by turning the card clockwise and half
anticlockwise. Printed as-is, some of the acts in a campaign come out upside
down relative to the rest.

Rather than guess, each scan is compared against the card's picture on ArkhamDB.
That settles which way the art actually faces; a scan facing the wrong way is
turned 180, which keeps it portrait. Backs are covered too: ArkhamDB carries a
back image for every double-sided card, and a card whose back is printed as a
card of its own has its own picture.

References are cached, so a second run costs nothing. Output goes to a separate
directory; the input is never modified.
"""

import argparse
import json
import os
import re
import shutil
import threading
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

from PIL import Image, JpegImagePlugin

HERE = os.path.dirname(os.path.abspath(__file__))
CATALOG_CACHE = os.path.join(HERE, 'ahlcg_catalog_cache.json')
REF_CACHE = os.path.join(HERE, 'arkhamdb_refs')

BASE = 'https://arkhamdb.com'
COMPARE = 96          # both images are squared to this before correlating
MARGIN = 0.02         # how much the winning orientation must beat the other by
TIMEOUT = 12          # seconds to wait on a reference before giving up on it
RETRIES = 3           # attempts per reference, with a short backoff between

# ArkhamDB does not refuse a request outright when it rate limits; it stops
# answering, so every read costs its full timeout. Once this many in a row have
# gone that way it is not going to serve the rest of this run either, and going
# on would spend hours to learn nothing.
CONSECUTIVE_FAILURES = 12


def load_catalog():
    with open(CATALOG_CACHE, encoding='utf-8') as handle:
        return json.load(handle)['cards']


def reference_url(card, is_back, by_code):
    """Where ArkhamDB keeps the picture of this face."""
    if not is_back:
        return card.get('imagesrc')
    if card.get('backimagesrc'):
        return card['backimagesrc']
    linked = card.get('linked_to_code')
    if linked and linked in by_code:
        return by_code[linked].get('imagesrc')
    return None


def cached_reference(url):
    """The reference's path if it is already downloaded, else None."""
    path = os.path.join(REF_CACHE, url.strip('/').split('/')[-1])
    return path if os.path.exists(path) and os.path.getsize(path) > 0 else None


def fetch_reference(url):
    """Download a reference once and keep it.

    ArkhamDB rate limits by address: past some rate it simply stops answering,
    and a reader that waits a full minute on each of several attempts spends
    minutes achieving nothing while holding a connection open. So the timeout is
    short and the attempts few -- a card whose reference never arrives is
    reported and left alone, and running again picks up where this left off,
    because whatever did arrive is still in the cache.

    An HTTP status is an answer rather than a rate limit, and retrying a 404
    only wastes the budget, so it is not retried.
    """
    cached = cached_reference(url)
    if cached:
        return cached
    path = os.path.join(REF_CACHE, url.strip('/').split('/')[-1])
    os.makedirs(REF_CACHE, exist_ok=True)
    request = urllib.request.Request(BASE + url, headers={'User-Agent': 'proxynexus-rs'})
    for attempt in range(RETRIES):
        try:
            with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
                data = response.read()
            if data:
                with open(path, 'wb') as handle:
                    handle.write(data)
                return path
        except urllib.error.HTTPError:
            return None
        except Exception:
            pass
        if attempt + 1 < RETRIES:
            time.sleep(2 ** attempt)
    return None


class ReferenceFetcher:
    """Fetches references, and stops asking once ArkhamDB stops answering.

    What it did fetch is kept, so a run cut short here is not wasted: the cards
    it could check are checked, the rest are reported, and running again takes
    up where this left off.
    """

    def __init__(self, limit=CONSECUTIVE_FAILURES):
        self.limit = limit
        self.failures = 0
        self.given_up = False
        self._lock = threading.Lock()

    def __call__(self, url):
        # A reference already downloaded costs nothing to hand back, so it is
        # served whether or not the network side has been given up on.
        cached = cached_reference(url)
        if cached:
            return cached
        with self._lock:
            if self.given_up:
                return None
        path = fetch_reference(url)
        with self._lock:
            if path:
                self.failures = 0
            else:
                self.failures += 1
                self.given_up = self.failures >= self.limit
        return path


def squared(image, size=COMPARE):
    """Grayscale, mean-centred and squashed to a fixed square.

    Aspect is deliberately thrown away: the two images being compared are the
    same card, so only the picture inside has to line up.
    """
    small = image.convert('L').resize((size, size), Image.BILINEAR)
    pixels = list(small.getdata())
    mean = sum(pixels) / len(pixels)
    return [p - mean for p in pixels]


def correlation(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na = sum(x * x for x in a) ** 0.5
    nb = sum(y * y for y in b) ** 0.5
    return dot / (na * nb) if na and nb else 0.0


def facing(scan, reference, clockwise=True):
    """Whether the scan needs turning 180 to face the way the collection stores.

    The reference says which way the art actually faces. The scan stays portrait
    either way: for landscape art a quarter turn either way is portrait, and the
    two possibilities differ by exactly a half turn, so only the 180 is ever in
    question.
    """
    target = squared(reference)
    if reference.width > reference.height:
        # Storing art to be read by turning the card clockwise means the quarter
        # turn anticlockwise is the one that stands it upright.
        upright = 90 if clockwise else 270
        candidates = {0: upright, 180: (upright + 180) % 360}
    else:
        candidates = {0: 0, 180: 180}
    scored = sorted(
        ((correlation(squared(scan.rotate(check, expand=True)), target), turn)
         for turn, check in candidates.items()), reverse=True)
    (best_score, best_turn), (next_score, _) = scored[0], scored[1]
    return best_turn, best_score, best_score - next_score


def save_settings(image, quality):
    """Reproduce the source's own JPEG encoding, so a turn costs no quality."""
    if image.format != 'JPEG':
        return {'format': image.format or 'JPEG'}
    if quality is not None:
        return {'format': 'JPEG', 'quality': quality}
    settings = {'format': 'JPEG', 'qtables': image.quantization}
    sampling = JpegImagePlugin.get_sampling(image)
    if sampling in (0, 1, 2):
        settings['subsampling'] = sampling
    for key in ('exif', 'icc_profile'):
        if key in image.info:
            settings[key] = image.info[key]
    return settings


def process(job):
    path, out_path, ref_path, clockwise, quality = job
    name = os.path.basename(path)
    if ref_path is None:
        shutil.copy2(path, out_path)
        return name, 0, 'unchecked'
    with Image.open(path) as scan:
        scan.load()
        with Image.open(ref_path) as reference:
            reference.load()
            if reference.mode == 'RGBA':
                flat = Image.new('RGB', reference.size, 'white')
                flat.paste(reference, mask=reference.split()[3])
                reference = flat
            turn, score, margin = facing(scan, reference, clockwise)
        if turn:
            settings = save_settings(scan, quality)
            scan.transpose(Image.Transpose.ROTATE_180).save(out_path, **settings)
    if not turn:
        shutil.copy2(path, out_path)
    status = 'ok' if margin >= MARGIN else f'unsure ({score:.2f}, margin {margin:.3f})'
    return name, turn, status


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[1])
    parser.add_argument('input', help='Directory of renamed scans.')
    parser.add_argument('-o', '--output', help="Output directory. Defaults to '<input>-faced'.")
    parser.add_argument('--workers', type=int, default=4, help='Parallel downloads.')
    parser.add_argument('--ccw', action='store_true',
                        help='Store landscape art to be read by turning the card anticlockwise. '
                             'The default is clockwise, matching ../agot/rotate_horizontal.py.')
    parser.add_argument('--quality', type=int,
                        help="JPEG quality for the re-encode. Omit to reuse the source's own "
                             'quantization tables, which is what you almost always want.')
    parser.add_argument('--types', default='investigator,act,agenda',
                        help='Card types to check, comma separated. These are the ones printed '
                             'landscape; everything else is copied through. Pass "all" to check '
                             'the lot, which costs a reference download per card.')
    args = parser.parse_args()

    source = os.path.abspath(args.input.rstrip(os.sep))
    dest = args.output or f'{source}-faced'
    os.makedirs(dest, exist_ok=True)

    by_code = {c['code']: c for c in load_catalog()}
    wanted = None if args.types == 'all' else set(args.types.split(','))

    jobs, unknown = [], []
    for name in sorted(os.listdir(source)):
        match = re.match(r'^(.+?)@([a-z_0-9]+)(~back)?\.jpg$', name)
        if not match:
            continue
        card = by_code.get(match.group(1))
        checked = card is not None and (wanted is None or card['type_code'] in wanted)
        url = reference_url(card, bool(match.group(3)), by_code) if checked else None
        if checked and url is None:
            unknown.append(name)
        jobs.append((os.path.join(source, name), os.path.join(dest, name), url))

    urls = sorted({url for _, _, url in jobs if url})
    print(f'{len(jobs)} scans, {len(urls)} references')
    fetcher = ReferenceFetcher()
    with ThreadPoolExecutor(args.workers) as pool:
        paths = dict(zip(urls, pool.map(fetcher, urls)))
    failed = sorted(url for url, path in paths.items() if path is None)
    if fetcher.given_up:
        print(f'ArkhamDB stopped answering after {fetcher.limit} reads in a row. '
              f'{len(urls) - len(failed)} references in hand; the cards behind the other '
              f'{len(failed)} are left facing as they were. Run again to pick up the rest.')

    jobs = [(src, out, paths.get(url), not args.ccw, args.quality) for src, out, url in jobs]
    turned, unsure = [], []
    with ThreadPoolExecutor(args.workers) as pool:
        for name, turn, status in pool.map(process, jobs):
            if turn:
                turned.append(name)
            if status.startswith('unsure'):
                unsure.append(f'{name}  {status}')

    print(f'{len(turned)} turned 180, {len(jobs) - len(turned)} left as they were')
    for heading, rows in (('Turned 180', turned),
                          ('Low confidence, check these', unsure),
                          ('No reference on ArkhamDB', unknown),
                          ('Reference download failed, these were left alone', failed)):
        if rows:
            print(f'\n{heading} ({len(rows)}):')
            for row in rows:
                print(f'  {row}')
    print(f'\nwrote {len(jobs)} files to {dest}')


if __name__ == '__main__':
    main()
