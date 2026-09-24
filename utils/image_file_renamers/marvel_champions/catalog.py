# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
The MarvelCDB catalog, and which of its packs are out of print.

Shared by the scripts beside this one, which all resolve images against the
same catalog. It is cached on first download, because the scripts are run
repeatedly over the same source.
"""

import json
import os
import urllib.request

CATALOG_CACHE = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                             'marvel_champions_catalog_cache.json')

CARDS_URL = 'https://marvelcdb.com/api/public/cards/?encounter=1'
PACKS_URL = 'https://marvelcdb.com/api/public/packs/'

# Packs Fantasy Flight lists as out of print, which is what `--packs` defaults
# to. The Once and Future Kang is not listed: it left Asmodee's distribution
# catalogue in September 2026, which is where a pack usually goes out of print
# from, but Fantasy Flight has not marked it.
OUT_OF_PRINT_PACKS = {
    'trors', 'gmw', 'mts', 'sm', 'mut_gen', 'next_evol',
    'gob', 'hood', 'mojo', 'twc',
    'cap', 'thor', 'bkw', 'drs', 'hlk', 'wsp', 'qsv', 'scw', 'stld', 'gam', 'drax',
    'nebu', 'warm', 'valk', 'vision', 'nova', 'ironheart', 'spiderham', 'spdr',
    'cyclops', 'phoenix', 'wolv', 'storm', 'gambit', 'rogue', 'psylocke', 'x23',
    'deadpool',
}


def fetch_json(url):
    request = urllib.request.Request(url, headers={'User-Agent': 'proxynexus-rs'})
    with urllib.request.urlopen(request) as response:
        return json.load(response)


def load_catalog(refresh=False):
    """The catalog as (cards, packs), downloaded if it is not cached yet."""
    if refresh or not os.path.exists(CATALOG_CACHE):
        print('Downloading MarvelCDB catalog...')
        data = {'cards': fetch_json(CARDS_URL), 'packs': fetch_json(PACKS_URL)}
        with open(CATALOG_CACHE, 'w', encoding='utf-8') as handle:
            json.dump(data, handle)
    with open(CATALOG_CACHE, encoding='utf-8') as handle:
        data = json.load(handle)
    return data['cards'], data['packs']
