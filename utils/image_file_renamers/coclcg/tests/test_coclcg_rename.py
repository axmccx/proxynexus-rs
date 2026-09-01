import importlib.util
import json
import pathlib
import struct
import zlib

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "coclcg_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)


def png(path, width, height):
    def chunk(tag, data):
        body = tag + data
        return struct.pack('>I', len(data)) + body + struct.pack('>I', zlib.crc32(body))

    header = struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IEND', b''))


def card(unique_id, name, versions, card_type='Character'):
    return {'unique_id': unique_id, 'name': name, 'subtitle': None, 'type': card_type,
            'faction': 'Miskatonic', 'back_group': 'card',
            'versions': [{'pack_code': pack, 'number': number, 'quantity': 3}
                         for pack, number in versions]}


def index(*cards):
    """The per-pack lookups `load_catalog` builds, for one pack."""
    pack = {'by_squash': {}, 'by_number': {}, 'ids': set()}
    for entry in cards:
        pack['by_squash'].setdefault(rename.squash(rename.title_only(entry)), []).append(entry)
        pack['ids'].add(entry['unique_id'])
        for version in entry['versions']:
            pack['by_number'][version['number']] = entry
    return pack


def write_catalog(tmp_path, cards, packs):
    folder = tmp_path / 'catalog'
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'coc_cards.json').write_text(json.dumps(cards))
    (folder / 'coc_packs.json').write_text(json.dumps(packs))
    return str(folder)


# --- name normalisation ----------------------------------------------------

def test_slug_drops_apostrophes_rather_than_hyphenating_them():
    assert rename.slug("Danny O'Bannion's Crony") == 'danny-obannions-crony'


def test_squash_reads_an_underscore_and_an_apostrophe_the_same_way():
    assert rename.squash('Neil_s Curiosity Shop') == rename.squash("Neil's Curiosity Shop")
    assert rename.squash('The Path to Y_ha-nthlei') == rename.squash("The Path to Y'ha-nthlei")


def test_squash_reads_an_ellipsis_spelled_either_way():
    assert rename.squash('The Greatest Fear...') == rename.squash('The Greatest Fear…')
    assert rename.squash('Torch the Joint') == rename.squash('Torch the Joint!')


def test_the_suffix_telling_two_cards_apart_is_not_part_of_the_title():
    assert rename.title_only({'name': 'The Necronomicon (Al Azif)'}) == 'The Necronomicon'
    assert rename.title_only({'name': 'Thomas F. Malone'}) == 'Thomas F. Malone'


# --- packs -----------------------------------------------------------------

def test_a_folder_naming_a_pack_resolves_to_its_id(tmp_path):
    pack_ids = {'kingsport-dreams': 'kingsport-dreams'}
    path = str(tmp_path / '02 - Forgotten Lore' / '02 - Kingsport Dreams' / '021 - Nodens.jpg')
    assert rename.resolve_pack(path, str(tmp_path), pack_ids) == ('kingsport-dreams', None)


def test_the_cycle_folder_above_a_pack_is_walked_past(tmp_path):
    pack_ids = {'ancient-horrors': 'ancient-horrors'}
    path = str(tmp_path / '02 - Forgotten Lore' / '06 - Ancient Horrors' / '101 - Safari.jpg')
    assert rename.resolve_pack(path, str(tmp_path), pack_ids)[0] == 'ancient-horrors'


def test_a_folder_named_differently_from_its_pack_still_finds_it(tmp_path):
    pack_ids = {'the-mountains-of-madness': 'the-mountains-of-madness'}
    path = str(tmp_path / '05 - At the Mountains of Madness' / '089 - Penguin.jpg')
    pack_id, guess = rename.resolve_pack(path, str(tmp_path), pack_ids)
    assert pack_id == 'the-mountains-of-madness'
    assert guess == ('05 - At the Mountains of Madness', 'the-mountains-of-madness')


def test_a_file_outside_any_pack_folder_resolves_to_nothing(tmp_path):
    path = str(tmp_path / 'Card Reverse.jpg')
    assert rename.resolve_pack(path, str(tmp_path), {'core-set': 'core-set'}) == (None, None)


# --- cards -----------------------------------------------------------------

def test_a_title_naming_one_card_resolves_without_a_guess():
    pack = index(card('hastur', 'Hastur', [('core-set', 81)]))
    found, guess, reason = rename.resolve_card(pack, 81, 'Hastur')
    assert found['unique_id'] == 'hastur'
    assert guess is None and reason is None


def test_punctuation_alone_is_not_counted_as_a_spelling_match():
    pack = index(card('neils-curiosity-shop', "Neil's Curiosity Shop", [('kingsport-dreams', 22)]))
    found, guess, _ = rename.resolve_card(pack, 22, 'Neil_s Curiosity Shop')
    assert found['unique_id'] == 'neils-curiosity-shop' and guess is None


def test_the_number_picks_between_cards_a_pack_prints_under_one_title():
    pack = index(
        card('the-necronomicon-al-azif', 'The Necronomicon (Al Azif)',
             [('the-unspeakable-pages', 90)], 'Support'),
        card('the-necronomicon-owlswick-translation', 'The Necronomicon (Owlswick Translation)',
             [('the-unspeakable-pages', 85)], 'Support'),
    )
    found, guess, _ = rename.resolve_card(pack, 85, 'The Necronomicon')
    assert found['unique_id'] == 'the-necronomicon-owlswick-translation' and guess is None


def test_a_title_several_cards_share_and_no_number_reaches_is_reported():
    pack = index(
        card('the-necronomicon-al-azif', 'The Necronomicon (Al Azif)',
             [('the-unspeakable-pages', 90)], 'Support'),
        card('the-necronomicon-owlswick-translation', 'The Necronomicon (Owlswick Translation)',
             [('the-unspeakable-pages', 85)], 'Support'),
    )
    found, _, reason = rename.resolve_card(pack, 99, 'The Necronomicon')
    assert found is None and 'names 2 cards' in reason


def test_a_misspelled_title_still_finds_its_card():
    pack = index(card('tcho-tcho-tribe', 'Tcho-Tcho Tribe', [('ancient-horrors', 116)]))
    found, guess, reason = rename.resolve_card(pack, 116, 'Tch-Tcho Tribe')
    assert found['unique_id'] == 'tcho-tcho-tribe'
    assert guess[0] == 'spelling' and reason is None


def test_a_title_naming_no_card_falls_back_to_the_number():
    # `030 - Reading the Star Signs.jpg` is a scan of Itinerant Scholar, whose
    # number it carries and whose title it does not.
    pack = index(card('itinerant-scholar', 'Itinerant Scholar', [('core-set', 30)]))
    found, guess, reason = rename.resolve_card(pack, 30, 'Reading the Star Signs')
    assert found['unique_id'] == 'itinerant-scholar'
    assert guess == ('number', 'itinerant-scholar', 'Reading the Star Signs')
    assert reason is None


def test_a_title_and_a_number_that_both_name_nothing_are_reported():
    pack = index(card('hastur', 'Hastur', [('core-set', 81)]))
    found, _, reason = rename.resolve_card(pack, 900, 'Hedge Fund')
    assert found is None and reason


def test_a_title_two_cards_are_equally_close_to_is_reported():
    pack = index(
        card('blood-chamber', 'Blood Chamber', [('core-set', 1)]),
        card('brood-chamber', 'Brood Chamber', [('core-set', 2)]),
    )
    found, _, reason = rename.resolve_card(pack, 9, 'Bzood Chamber')
    assert found is None and 'as close to' in reason


# --- promos ----------------------------------------------------------------

def test_a_promo_resolves_against_the_cards_with_a_promo_printing():
    promos = {'azathoth': [card('azathoth', 'Azathoth (The Blind Idiot God)',
                                [('secrets-of-arkham', 46), ('promos', None)])]}
    found, guess, reason = rename.resolve_promo(promos, 'Azathoth')
    assert found['unique_id'] == 'azathoth' and guess is None and reason is None


def test_a_promo_naming_no_card_is_reported_rather_than_guessed():
    promos = {'azathoth': [card('azathoth', 'Azathoth', [('promos', None)])]}
    found, _, reason = rename.resolve_promo(promos, 'Nyarlathotep')
    assert found is None and reason


# --- choosing between scans ------------------------------------------------

def test_the_larger_of_two_scans_of_one_card_is_the_one_named(tmp_path):
    small, large = tmp_path / 'a.jpg', tmp_path / 'b.jpg'
    png(small, 1480, 2084)
    png(large, 2960, 4168)

    renames, passed_over = rename.plan({('kingsport-dreams', 'nodens'): [str(small), str(large)]})
    assert renames == [(str(large), 'nodens@kingsport-dreams.jpg')]
    assert passed_over == [str(small)]


# --- end to end ------------------------------------------------------------

def test_an_archive_resolves_end_to_end(tmp_path, monkeypatch):
    monkeypatch.setattr(rename, 'CATALOG_DIR', write_catalog(
        tmp_path,
        [card('hastur', 'Hastur', [('core-set', 81)]),
         card('nodens', 'Nodens', [('kingsport-dreams', 36)]),
         card('daybreak', 'Daybreak!', [('in-memory-of-day', 23), ('promos', None)], 'Event')],
        [{'code': 'core-set', 'name': 'Core Set', 'date_release': '2008-10-01'},
         {'code': 'kingsport-dreams', 'name': 'Kingsport Dreams', 'date_release': None},
         {'code': 'in-memory-of-day', 'name': 'In Memory of Day', 'date_release': None},
         {'code': 'promos', 'name': 'Promos', 'date_release': None}]))

    source = tmp_path / 'scans'
    png(source / '01 - Core Set' / '081 - Hastur.jpg', 1480, 2084)
    png(source / '01 - Core Set' / 'Story Card Reverse.jpg', 1480, 2084)
    png(source / '02 - Forgotten Lore' / '02 - Kingsport Dreams' / '036 - Nodens.jpg', 1480, 2084)
    png(source / 'Promos' / 'Daybreak!.jpg', 1480, 2084)
    png(source / 'Lore' / 'CT30 - Sleep of the Dead.jpg', 1480, 2084)

    catalog, by_pack, pack_ids = rename.load_catalog()
    groups, unresolved, _, _, _, _, backs, _ = rename.collect(
        str(source), catalog, by_pack, pack_ids)
    renames, _ = rename.plan(groups)

    assert sorted(name for _, name in renames) == [
        'daybreak@promos.jpg', 'hastur@core-set.jpg', 'nodens@kingsport-dreams.jpg']
    assert unresolved == []
    assert backs == 1, 'the card back scan belongs to the adapter, not the collection'
