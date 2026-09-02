# /// script
# requires-python = ">=3.10"
# dependencies = ["Pillow", "unidecode"]
# ///
"""Build the LotR LCG pack release dates the adapter uses, from Hall of Beorn.

The adapter takes its dates from RingsDB's `/api/public/packs/`, and that field
is not a product release date. RingsDB dates The Dark of Mirkwood 2011-04-22 --
two days after the Core Set, for a pack FFG released in 2021 -- because it dates
a repackaged product by the era of the cards inside it. That is not cosmetic:
`printing_card_ids` makes the earliest printing's slug the card id, so a pack
wrongly dated to 2011 becomes the canonical source for every card it reprints.
The Dark of Mirkwood alone claims 77 card ids that belong to older packs.

Hall of Beorn publishes a real product release date, but only on each product's
own page, and only as prose: "Scenario Pack MEC102 01 December 2021". There is
no table and no JSON. `/LotR/Products?View=Chronological` carries no dates at
all -- it is a wall of product images -- but it does link every product page,
which is the only reliable way to get the slugs: they cannot be built from the
names. One is spelled `A-Joureny-to-Rhosgobel-Nightmare-Deck`, and the Nightmare
decks alternate between `-Nightmare` and `-Nightmare-Deck`.

So: read the index for its links, fetch each product once, and write the result
next to this script. Release dates are immutable history, so this is run when a
product is added, not on every import.

    uv run fetch_release_dates.py            # write lotrlcg_hob_release_dates.json
    uv run fetch_release_dates.py --report   # also show how it joins to the catalog
"""

import argparse
import html
import json
import pathlib
import re
import time
import urllib.parse
import urllib.request

from unidecode import unidecode

INDEX_URL = "https://www.hallofbeorn.com/LotR/Products?View=Chronological"
PRODUCT_URL = "https://www.hallofbeorn.com/LotR/Products/{}?View=Browse"
OUT_PATH = (
    pathlib.Path(__file__).resolve().parents[3]
    / "proxynexus-core/src/games/lotrlcg/ffg_release_dates.json"
)

# Card sets whose Hall of Beorn product is named differently. The GenCon promos
# carry the convention in their product name; Treachery of Rhudaur drops its
# leading "The".
ALIASES = {
    "The Battle of Lake-town": "The Battle of Lake town GenCon 2012",
    "The Massing at Osgiliath": "The Massing at Osgiliath GenCon 2011",
    "The Stone of Erech": "The Stone of Erech GenCon 2013",
    "The Treachery of Rhudaur": "Treachery of Rhudaur",
}

USER_AGENT = "ProxyNexus-ReleaseDates/1.0"
REQUEST_DELAY = 0.4

MONTHS = {
    m: i
    for i, m in enumerate(
        "January February March April May June July August September October "
        "November December".split(),
        start=1,
    )
}
DATE = re.compile(r"\b(\d{1,2}) (" + "|".join(MONTHS) + r") (\d{4})\b")
PRODUCT_CODE = re.compile(r"\bMEC\d+\b")
PRODUCT_LINK = re.compile(r"/LotR/Products/([^\"?]+)\?View=")


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read().decode("utf-8", "replace")


def product_slugs():
    """Every product Hall of Beorn links from its chronological index.

    The hrefs are HTML-escaped, so the six products with an apostrophe in the
    name arrive as `Celebrimbor&#x27;s-Secret` and 500 if requested that way.
    """
    return sorted({html.unescape(s) for s in PRODUCT_LINK.findall(fetch(INDEX_URL))})


def scrape_product(slug):
    """The release date and FFG code on one product page, or None."""
    body = fetch(PRODUCT_URL.format(slug))
    text = re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", " ", body)))
    found = DATE.search(text)
    if not found:
        return None
    day, month, year = found.groups()
    code = PRODUCT_CODE.search(body)
    return {
        "name": urllib.parse.unquote(slug).replace("-", " "),
        "code": code.group(0) if code else None,
        "released": f"{year}-{MONTHS[month]:02d}-{int(day):02d}",
    }


def scrape_all():
    slugs = product_slugs()
    print(f"{len(slugs)} products linked from the index")
    products = {}
    for n, slug in enumerate(slugs, start=1):
        try:
            found = scrape_product(slug)
        except Exception as e:  # noqa: BLE001 - report and keep going
            print(f"  [ERR]  {slug}: {type(e).__name__}")
            continue
        if found is None:
            print(f"  [WARN] {slug}: page has no date")
            continue
        products[urllib.parse.unquote(slug)] = found
        if n % 25 == 0:
            print(f"  {n}/{len(slugs)}")
        time.sleep(REQUEST_DELAY)
    return products


def match_key(text):
    """Collapse a product or card-set name to something joinable."""
    return "".join(c for c in unidecode(text).lower() if c.isalnum())


def load_card_sets():
    """Every card set in the Hall of Beorn catalog, via audit_coverage.py."""
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "audit", pathlib.Path(__file__).resolve().parent / "audit_coverage.py"
    )
    audit = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(audit)
    return sorted({c["CardSet"] for c in audit.load_catalog() if c.get("CardSet")}), audit


def join(products):
    """Map each card set to its product's release date.

    Nightmare sets are left out on purpose. Hall of Beorn sells the Nightmare
    decks per pack while the catalog names them per scenario, so the two do not
    line up, and it does not matter: no printing anywhere takes its card id from
    a Nightmare set, and shifting every Nightmare date by ten years moves no card
    id at all. They keep their RingsDB date, which only orders them in the set
    list.

    ALeP is left out for the same reason it is not in this catalog: it is still
    being published, so its dates are fetched live rather than frozen here.
    """
    card_sets, _ = load_card_sets()
    by_key = {}
    for product in products.values():
        key = match_key(product["name"])
        by_key.setdefault(key, product)
        # "... Nightmare Deck" and "... Nightmare Product" name the same thing
        for suffix in ("deck", "product"):
            if key.endswith(suffix):
                by_key.setdefault(key[: -len(suffix)], product)

    dates, unmatched = {}, []
    for card_set in card_sets:
        if "Nightmare" in card_set:
            continue
        product = by_key.get(match_key(ALIASES.get(card_set, card_set)))
        if product is None:
            unmatched.append(card_set)
            continue
        dates[card_set] = {"code": product["code"], "released": product["released"]}
    return dates, unmatched


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--reuse", action="store_true",
                        help="Report from the existing file instead of re-fetching")
    args = parser.parse_args()

    cache = OUT_PATH.with_name("lotrlcg_hob_products.json")
    if args.reuse and cache.exists():
        products = json.loads(cache.read_text())
    else:
        products = scrape_all()
        cache.write_text(json.dumps(products, indent=1, sort_keys=True, ensure_ascii=False))
        print(f"\n{len(products)} products cached in {cache.name}")

    dates, unmatched = join(products)
    OUT_PATH.write_text(json.dumps(dates, indent=1, sort_keys=True, ensure_ascii=False))
    print(f"{len(dates)} card sets dated in {OUT_PATH.name}")
    if unmatched:
        print(f"\n[WARN] {len(unmatched)} non-Nightmare card sets have no product page, "
              f"and keep their RingsDB date:")
        for card_set in unmatched:
            print(f"   {card_set}")


if __name__ == "__main__":
    main()
