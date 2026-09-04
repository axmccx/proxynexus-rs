# /// script
# requires-python = ">=3.9"
# dependencies = ["Pillow"]
# ///
"""
Cuts Arkham Horror LCG card images out of the SCED Tabletop Simulator mod and
names them to the current Proxy Nexus naming convention.

SCED holds one JSON object per card. The object names the sheet its picture sits
on and its slot in that sheet's grid, and a sibling `.gmnotes` file gives the
ArkhamDB id of the card it is:

    TheEternalSlumber.9ff406/CairoBazaar.d2de0e.json     CardID, CustomDeck
    TheEternalSlumber.9ff406/CairoBazaar.d2de0e.gmnotes  {"id": "83009", ...}

So a card is found by its id rather than by its filename, and its picture by
slicing the sheet it names. Sheets are downloaded once and cached; the
repositories are never modified.

See README.md for the mapping rules and known limitations.
"""

import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import urllib.error
import urllib.request
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor

from PIL import Image

# The catalog loader is shared with rename.py. Loaded by explicit path rather
# than `import rename`: every game ships a rename.py, so a bare import resolves
# through sys.path and can pick up another game's module when the whole test
# suite runs in one process.
_spec = importlib.util.spec_from_file_location(
    'ahlcg_rename_helpers', pathlib.Path(__file__).resolve().parent / 'rename.py')
assert _spec and _spec.loader, 'could not load rename.py'
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)

load_catalog = rename.load_catalog

_faces = importlib.util.spec_from_file_location(
    'ahlcg_face_helpers', pathlib.Path(__file__).resolve().parent / 'fix_orientation.py')
assert _faces and _faces.loader, 'could not load fix_orientation.py'
orientation = importlib.util.module_from_spec(_faces)
_faces.loader.exec_module(orientation)

HERE = os.path.dirname(os.path.abspath(__file__))
SHEET_CACHE = os.path.join(HERE, 'sced_sheet_cache')

# The two repositories the mod is split across: SCED holds the player cards it
# always has loaded, SCED-downloads every campaign and scenario.
REPOS = ('SCED', 'SCED-downloads')

# The ArkhamDB packs making up the Chapter 1 card pool. ArkhamDB files a card
# under exactly one pack and it is always the original cycle pack rather than
# one of the 2024 expansion repackagings, so the six classic cycles are named
# here by their deluxe box and mythos packs.
CHAPTER1_PACKS = set("""
    core rcore
    dwl tmm tece bota uau wda litas
    ptc eotp tuo apot tpm bsr dca
    tfa tof tbb hote tcoa tdoy sha
    tcu tsn wos fgg uad icc bbt
    tde sfk tsh dsm pnr wgd woc
    tic itd def hhg lif lod itm
    eoep eoec tskp tskc fhvp fhvc
    nat har win jac ste
    cotr coh lol guardians hotel blob wog mtt fof tmg
    rtnotz rtdwl rtptc rttfa rttcu
""".split())

# Types printed landscape. SCED stores them upright in a portrait frame, the way
# the collection does, but not all facing the same way, so they are reported for
# fix_orientation.py rather than turned on a guess here.
LANDSCAPE_TYPES = {'investigator', 'act', 'agenda'}

# Types whose two pictures SCED does not reliably hold the way round ArkhamDB
# does, so which of the pair is the front is measured per card rather than
# assumed. A location is the one that varies: it is laid on the table unrevealed
# side up, and often though not always authored that way round. Acts, agendas,
# investigators, scenarios and stories came back the right way round every time.
# See README.md for the measurement.
CHECKED_TYPES = {'location'}

# Objects under here are a translation of the mod, not another printing.
TRANSLATED = 'language-pack'

# Fan reworks of an official product, which sit beside it holding the same cards.
UNOFFICIAL = re.compile(r'[/\\]Unofficial', re.I)

OUTPUT_NAME = re.compile(r'^(.+?)@([a-z_0-9]+)(~back)?\.(?:jpg|jpeg|png)$', re.I)

# A card code with the single letter ArkhamDB uses to tell one printed face, or
# one same-numbered card, from another: `03182b`, `04128a`.
FACE_SUFFIX = re.compile(r'^(\d+)[a-z]?$')

DUPLICATE_RULE = 'the card ArkhamDB calls it a duplicate of'

# How much better the reversed pairing has to correlate before a card's two
# pictures are swapped. Below this the two sides are too alike to tell apart.
FACE_MARGIN = 0.02

# ArkhamDB rate limits by address, and a handful of parallel readers is enough to
# make it stop answering for minutes at a time. The Steam CDN the sheets come
# from has no such limit, so the two downloads are given separate budgets.
REFERENCE_WORKERS = 2

# How many times a dead sheet may send its cards to another copy.
RETRY_ROUNDS = 3


def read_json(path):
    try:
        with open(path, encoding='utf-8') as handle:
            return json.load(handle)
    except (OSError, ValueError):
        return None


def card_objects(root):
    """Yield every TTS card object under `root`, with the id it declares.

    A card object is one carrying both a `CardID` and the `CustomDeck` the id
    indexes into. Its ArkhamDB id lives in a `.gmnotes` file beside it; objects
    without one are the mod's own custom content and are yielded with `None`.
    """
    for dirpath, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d != '.git']
        for name in sorted(files):
            if not name.endswith('.json'):
                continue
            path = os.path.join(dirpath, name)
            obj = read_json(path)
            if not isinstance(obj, dict) or 'CardID' not in obj or 'CustomDeck' not in obj:
                continue
            notes = obj.get('GMNotes_path')
            identity = None
            if notes:
                identity = read_json(os.path.join(dirpath, os.path.basename(notes)))
            elif isinstance(obj.get('GMNotes'), str) and obj['GMNotes'].startswith('{'):
                try:
                    identity = json.loads(obj['GMNotes'])
                except ValueError:
                    identity = None
            card_id = obj['CardID']
            deck = obj['CustomDeck']
            # CardID is the deck's own id followed by two digits of slot, so the
            # deck it indexes is named rather than assumed to be the only one.
            sheet = deck.get(str(int(card_id) // 100)) or next(iter(deck.values()))
            yield path, (identity or {}).get('id'), obj, sheet, int(card_id) % 100


def build_index(root):
    """Index the English card objects in the repositories by their ArkhamDB id.

    Translations are dropped here rather than when a card is picked. A card the
    English mod does not hold separately -- a starter deck reprinting a Core Set
    card, say -- still has an object in every translation, and leaving those in
    makes the id look present and stops the fallbacks that would find the
    English card it duplicates.
    """
    index = defaultdict(list)
    unidentified = translated = 0
    for repo in REPOS:
        base = os.path.join(root, repo)
        if not os.path.isdir(base):
            raise SystemExit(f'{base} is not a directory. Clone SCED and SCED-downloads into '
                             f'{root} first; see README.md.')
        for path, identity, obj, sheet, slot in card_objects(base):
            relative = os.path.relpath(path, root)
            if TRANSLATED in relative:
                translated += 1
                continue
            if not identity:
                unidentified += 1
                continue
            if not sheet.get('FaceURL'):
                continue
            index[identity].append({
                'path': relative,
                'nickname': obj.get('Nickname'),
                'face': sheet.get('FaceURL'),
                'back': sheet.get('BackURL'),
                'unique_back': bool(sheet.get('UniqueBack')),
                'width': int(sheet.get('NumWidth') or 1),
                'height': int(sheet.get('NumHeight') or 1),
                'slot': slot,
            })
    return index, unidentified, translated


def sced_id(card, index, hidden_of):
    """The SCED id holding this ArkhamDB card, and how it was reached.

    SCED names a card after the face it puts on the table, which is not always
    the code ArkhamDB indexes it under, so four fallbacks follow the direct hit.
    """
    code = card['code']
    if code in index:
        return code, 'its ArkhamDB code'
    hidden = hidden_of.get(code)
    if hidden and hidden in index:
        # A card whose two faces ArkhamDB splits into a visible code and a
        # hidden one: SCED names the object after the hidden face.
        return hidden, 'the hidden half ArkhamDB links to'
    stem = FACE_SUFFIX.match(code)
    if stem:
        number = stem.group(1)
        if number in index:
            # ArkhamDB suffixes the faces of some cards `a`/`b`; SCED does not.
            return number, 'the face suffix ArkhamDB adds'
        for suffix in ('a', 'b'):
            if number + suffix in index:
                # The reverse: SCED gives each printed copy its own object.
                return number + suffix, 'the per-copy suffix SCED adds'
    duplicate = card.get('duplicate_of_code')
    if duplicate and duplicate in index:
        # A reprint carrying the same art as an earlier card; SCED holds it once.
        return duplicate, DUPLICATE_RULE
    return None, 'no SCED object'


def rank(entries):
    """Everything SCED holds for one id, best first.

    An official product before a fan rework of it, and then the smallest grid,
    which for a given sheet is the biggest cell.
    """
    ordered = sorted(entries, key=lambda e: (bool(UNOFFICIAL.search(e['path'])),
                                             e['width'] * e['height'], e['path']))
    return ordered


def pick(entries, avoid=()):
    """The entry to cut a card from, skipping any sheet known to be gone."""
    for entry in rank(entries):
        if entry['face'] not in avoid:
            return entry
    return None


def cache_path(url):
    return os.path.join(SHEET_CACHE, hashlib.sha1(url.encode()).hexdigest())


def fetch_sheet(url):
    """Download a sheet once and keep it. Returns the cached path, or None."""
    path = cache_path(url)
    if os.path.exists(path) and os.path.getsize(path) > 0:
        return path
    os.makedirs(SHEET_CACHE, exist_ok=True)
    request = urllib.request.Request(url, headers={'User-Agent': 'proxynexus-rs'})
    for _attempt in range(3):
        try:
            with urllib.request.urlopen(request, timeout=240) as response:
                data = response.read()
            if data:
                with open(path, 'wb') as handle:
                    handle.write(data)
                return path
        except (urllib.error.URLError, OSError, ValueError):
            pass
    return None


def cell(sheet, width, height, slot):
    """The card at `slot` of a `width` x `height` sheet, row-major from 0."""
    if width * height <= 1:
        return sheet
    cw, ch = sheet.size[0] // width, sheet.size[1] // height
    row, column = divmod(slot, width)
    return sheet.crop((column * cw, row * ch, (column + 1) * cw, (row + 1) * ch))


def has_own_back(entry):
    """Whether an entry's BackURL is this card's own second face.

    A `UniqueBack` deck gives every slot its own back, so the back is a grid to
    be cut the same way the face is. Without it the deck shares one picture --
    which is the generic card back on a deck of many cards, but on a deck of one
    is simply that card's back, and SCED models most double-sided encounter
    cards as a deck of one.
    """
    return entry['unique_back'] or entry['width'] * entry['height'] == 1


def wants_back(card, by_code):
    """Whether ArkhamDB gives this card a second face of its own."""
    if card.get('backimagesrc'):
        return True
    linked = card.get('linked_to_code')
    return bool(linked and by_code.get(linked, {}).get('imagesrc'))


def already_held(directory):
    """Card ids a collection already covers, so this one can fill the rest."""
    held = set()
    if not directory:
        return held
    for name in os.listdir(directory):
        match = OUTPUT_NAME.match(name)
        if match:
            held.add(match.group(1))
    return held


def plan(cards, index, by_code, hidden_of, held, checked, reports):
    """Work out, for every card in scope, which sheet slot to cut it from."""
    jobs, how_counts, claimed = [], defaultdict(int), defaultdict(list)
    for card in sorted(cards, key=lambda c: c['code']):
        code = card['code']
        if code in held:
            reports['Cards the excluded collection already holds'].append(code)
            continue
        identity, how = sced_id(card, index, hidden_of)
        candidates = rank(index[identity]) if identity else []
        entry = candidates[0] if candidates else None
        if entry is None:
            reports['Cards SCED has no English object for'].append(
                f"{code}  {card['name']}  ({card['pack_code']})")
            continue
        how_counts[how] += 1
        claimed[identity].append((code, how))
        back = wants_back(card, by_code)
        if back and not (entry['back'] and has_own_back(entry)):
            reports['Cards ArkhamDB gives a second face that SCED does not hold'].append(
                f"{code}  {card['name']}  ({card['pack_code']})")
            back = False
        jobs.append({
            'code': code,
            'pack': card['pack_code'],
            'card': card,
            'entry': entry,
            'candidates': candidates,
            'back': back,
            'check': back and card['type_code'] in checked,
            'landscape': card['type_code'] in LANDSCAPE_TYPES,
        })
    for identity, claims in sorted(claimed.items()):
        if len(claims) < 2:
            continue
        # Two cards cut from one SCED object print as the same picture. That is
        # the point for a reprint ArkhamDB flags a duplicate -- the art really is
        # the same, and both codes want an image. Any other pair is a fallback
        # having reached too far, so the two are reported apart.
        codes = ', '.join(code for code, _how in claims)
        expected = all(how == DUPLICATE_RULE for _code, how in claims[1:])
        heading = ('Cards sharing a picture with the card they are a duplicate of'
                   if expected else 'SCED objects claimed by more than one card')
        reports[heading].append(f'{identity}  <-  {codes}')
    return jobs, how_counts


def sheet_jobs(jobs):
    """Group the cuts by the sheet they come from, so each is opened once.

    Both pictures are written the way SCED holds them, front to front. Cards
    whose pair turns out to be the other way round are swapped afterwards, by
    renaming the two files rather than by opening their sheets a second time.
    """
    by_sheet = defaultdict(list)
    for job in jobs:
        entry = job['entry']
        by_sheet[entry['face']].append((job, ''))
        if job['back']:
            by_sheet[entry['back']].append((job, '~back'))
    return by_sheet


def write_sheet(url, cuts, output, quality, reports):
    """Cut every card taken from one sheet, then let it go."""
    path = cache_path(url)
    written = []
    try:
        with Image.open(path) as sheet:
            sheet.load()
            if sheet.mode != 'RGB':
                sheet = sheet.convert('RGB')
            for job, side in cuts:
                entry = job['entry']
                # Both sheets carry the same grid: a unique back is cut slot for
                # slot with the face, and a deck of one card is a 1x1 grid whose
                # only cell is the whole picture.
                image = cell(sheet, entry['width'], entry['height'], entry['slot'])
                name = f"{job['code']}@{job['pack']}{side}.jpg"
                image.save(os.path.join(output, name), 'JPEG',
                           quality=quality, optimize=True, subsampling=0)
                written.append((name, image.size))
    except (OSError, ValueError) as error:
        reports['Sheets that could not be read'].append(f'{url}  ({error})')
    return written


def best_correlation(signature, reference):
    """Correlation over the four right-angle turns.

    A card printed landscape is stored turned into a portrait frame, so it has to
    be turned back before it will match a landscape reference at all.
    """
    target = orientation.squared(reference)
    if len(signature) != len(target):
        return 0.0
    side = int(len(signature) ** 0.5)
    best = orientation.correlation(signature, target)
    grid = [signature[row * side:(row + 1) * side] for row in range(side)]
    for _turn in range(3):
        grid = [list(row) for row in zip(*grid[::-1])]
        turned = [value for row in grid for value in row]
        best = max(best, orientation.correlation(turned, target))
    return best


def settle_faces(jobs, by_code, output, workers, reports):
    """Put each checked card's two pictures the way round ArkhamDB has them.

    SCED authors a location with whichever side sits on the table as its face,
    and that is the revealed side for some scenarios and the unrevealed side for
    others. ArkhamDB's own two pictures settle it; a pair that turns out
    reversed is swapped by renaming the two files.

    The comparison reads the written files rather than the sheets they came
    from, so it is idempotent -- a pair already the right way round is left --
    and a run cut short by ArkhamDB going quiet can be finished later with
    --settle-only, without cutting anything again.
    """
    checked = [job for job in jobs if job['check']
               and os.path.exists(os.path.join(output, f"{job['code']}@{job['pack']}.jpg"))
               and os.path.exists(os.path.join(output, f"{job['code']}@{job['pack']}~back.jpg"))]
    urls = sorted({url for job in checked
                   for url in (orientation.reference_url(job['card'], False, by_code),
                               orientation.reference_url(job['card'], True, by_code))
                   if url})
    if not urls:
        return []
    print(f'\nsettling {len(checked)} cards against {len(urls)} ArkhamDB references...')
    fetcher = orientation.ReferenceFetcher()
    with ThreadPoolExecutor(workers) as pool:
        paths = dict(zip(urls, pool.map(fetcher, urls)))
    if fetcher.given_up:
        print(f'  ArkhamDB stopped answering after {fetcher.limit} reads in a row; settling the '
              f'{sum(1 for path in paths.values() if path)} references already in hand. Run again '
              f'with --settle-only to pick up the rest.')

    swapped = []
    for job in checked:
        front_url = orientation.reference_url(job['card'], False, by_code)
        back_url = orientation.reference_url(job['card'], True, by_code)
        front_path, back_path = paths.get(front_url), paths.get(back_url)
        if not front_path or not back_path:
            reports['Cards left as SCED had them, no reference yet'].append(job['code'])
            continue
        try:
            with Image.open(front_path) as image:
                front_reference = image.convert('RGB').copy()
            with Image.open(back_path) as image:
                back_reference = image.convert('RGB').copy()
        except (OSError, ValueError):
            reports['Cards left as SCED had them, no reference yet'].append(job['code'])
            continue
        stem = os.path.join(output, f"{job['code']}@{job['pack']}")
        try:
            with Image.open(f'{stem}.jpg') as image:
                face = orientation.squared(image)
            with Image.open(f'{stem}~back.jpg') as image:
                back = orientation.squared(image)
        except (OSError, ValueError):
            reports['Cards whose written pictures could not be read'].append(job['code'])
            continue
        straight = (best_correlation(face, front_reference)
                    + best_correlation(back, back_reference))
        reversed_ = (best_correlation(face, back_reference)
                     + best_correlation(back, front_reference))
        if reversed_ <= straight:
            continue
        if abs(reversed_ - straight) < FACE_MARGIN:
            # Both sides of some locations are near enough the same picture that
            # the comparison cannot separate them, and either way round prints
            # the same card. Left alone rather than turned on noise.
            reports['Cards whose two pictures are too alike to tell apart'].append(job['code'])
            continue
        os.replace(f'{stem}.jpg', f'{stem}.swap')
        os.replace(f'{stem}~back.jpg', f'{stem}.jpg')
        os.replace(f'{stem}.swap', f'{stem}~back.jpg')
        swapped.append(f"{job['code']}@{job['pack']}")
    return swapped


def main():
    parser = argparse.ArgumentParser(
        description='Cut Arkham Horror LCG cards out of the SCED Tabletop Simulator mod.')
    parser.add_argument('input', help='Directory holding the SCED and SCED-downloads clones.')
    parser.add_argument('-o', '--output', default='ahlcg_tts_out', help='Output directory.')
    parser.add_argument('--exclude', help='A collection directory whose cards to skip, so this '
                                          'one only fills what that is missing.')
    parser.add_argument('--packs', default='chapter1',
                        help='Pack codes to build, comma separated. Defaults to the Chapter 1 '
                             'pool; pass "all" for every pack ArkhamDB lists.')
    parser.add_argument('--quality', type=int, default=92, help='JPEG quality (default 92).')
    parser.add_argument('--workers', type=int, default=6,
                        help='Parallel sheet downloads from the Steam CDN.')
    parser.add_argument('--reference-workers', type=int, default=REFERENCE_WORKERS,
                        help=f'Parallel reference downloads from ArkhamDB (default '
                             f'{REFERENCE_WORKERS}).')
    parser.add_argument('--check-faces', default=','.join(sorted(CHECKED_TYPES)),
                        help='Card types whose two pictures to put the way round ArkhamDB has '
                             'them, comma separated. Defaults to locations, the only type '
                             'measured to vary. "all" checks every double-sided card and costs a '
                             'reference download per face; "none" keeps SCED\'s own order.')
    parser.add_argument('--dry-run', action='store_true', help='Report without downloading.')
    parser.add_argument('--settle-only', action='store_true',
                        help='Skip cutting and only put the faces of an existing output the way '
                             'round ArkhamDB has them. Safe to repeat: it reads the files as they '
                             'stand, so a run ArkhamDB cut short is finished by running again.')
    parser.add_argument('--refresh-catalog', action='store_true',
                        help='Re-download the ArkhamDB catalog.')
    args = parser.parse_args()

    root = os.path.abspath(os.path.expanduser(args.input))
    cards, _packs = load_catalog(args.refresh_catalog)
    by_code = {c['code']: c for c in cards}
    # ArkhamDB splits a card whose back is a card in its own right into a
    # visible code and a hidden one, and the visible half carries the link.
    hidden_of = {c['code']: c['linked_to_code'] for c in cards if c.get('linked_to_code')}

    if args.packs == 'all':
        packs = {c['pack_code'] for c in cards}
    elif args.packs == 'chapter1':
        packs = CHAPTER1_PACKS
    else:
        packs = {p.strip() for p in args.packs.split(',') if p.strip()}

    wanted = [c for c in cards if c['pack_code'] in packs and not c.get('hidden')]
    held = already_held(args.exclude)

    if args.check_faces == 'none':
        checked = set()
    elif args.check_faces == 'all':
        checked = {c['type_code'] for c in cards}
    else:
        checked = {t.strip() for t in args.check_faces.split(',') if t.strip()}

    print(f'indexing {root}...')
    index, unidentified, translated = build_index(root)
    print(f'{len(index)} ArkhamDB ids in SCED; skipped {unidentified} English objects '
          f'carrying no id and {translated} translated ones')

    reports = defaultdict(list)
    jobs, how_counts = plan(wanted, index, by_code, hidden_of, held, checked, reports)

    faces = len(jobs)
    backs = sum(1 for job in jobs if job['back'])
    by_sheet = sheet_jobs(jobs)
    print(f'\n{len(wanted)} cards in scope, {len(held & {c["code"] for c in wanted})} already held')
    print(f'{faces} fronts and {backs} backs from {len(by_sheet)} sheets')
    for how, count in sorted(how_counts.items(), key=lambda kv: -kv[1]):
        print(f'  {count:5}  found by {how}')

    landscape = sorted(f"{j['code']}@{j['pack']}" for j in jobs if j['landscape'])
    if landscape:
        reports['Landscape cards, for fix_orientation.py'] = landscape

    if not args.dry_run:
        os.makedirs(args.output, exist_ok=True)
        if not args.settle_only:
            fetched, gone = {}, set()
            # A sheet can simply be gone from the CDN. Where SCED holds the card
            # more than once that is recoverable, so a dead sheet sends its
            # cards back to pick() to be cut from the next copy instead.
            for _round in range(RETRY_ROUNDS):
                by_sheet = sheet_jobs(jobs)
                urls = [url for url in sorted(by_sheet) if url and url not in fetched]
                if not urls:
                    break
                print(f'\ndownloading {len(urls)} sheets...')
                with ThreadPoolExecutor(args.workers) as pool:
                    fetched.update(zip(urls, pool.map(fetch_sheet, urls)))
                dead = {url for url in urls if fetched[url] is None} - gone
                if not dead:
                    break
                gone |= dead
                moved = 0
                for job in jobs:
                    if job['entry']['face'] in gone:
                        other = pick(job['candidates'], gone)
                        if other is not None and other is not job['entry']:
                            job['entry'] = other
                            moved += 1
                print(f'  {len(dead)} sheets gone from the CDN; {moved} cards sent to another copy')
                if not moved:
                    break
            by_sheet = sheet_jobs(jobs)
            for job in jobs:
                if fetched.get(job['entry']['face']) is None:
                    reports['Cards whose every SCED sheet is gone from the CDN'].append(
                        f"{job['code']}  {job['card']['name']}  ({job['pack']})")

            sizes, written = defaultdict(int), 0
            urls = sorted(by_sheet)
            for url in urls:
                if fetched.get(url) is None:
                    continue
                for _name, size in write_sheet(url, by_sheet[url], args.output,
                                               args.quality, reports):
                    sizes[size] += 1
                    written += 1
            print(f'\nwrote {written} files to {args.output}')
            for size, count in sorted(sizes.items(), key=lambda kv: -kv[1]):
                print(f'  {count:5}  {size[0]}x{size[1]}')

        swapped = settle_faces(jobs, by_code, args.output, args.reference_workers, reports)
        if swapped:
            reports['Cards whose two pictures SCED held the other way round'] = sorted(swapped)
        print(f'{len(swapped)} cards turned front to back')

    for heading in sorted(reports):
        rows = reports[heading]
        print(f'\n{heading} ({len(rows)}):')
        for row in rows[:200]:
            print(f'  {row}')
        if len(rows) > 200:
            print(f'  ... and {len(rows) - 200} more')


if __name__ == '__main__':
    main()
