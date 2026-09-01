import csv
import importlib.util
import json
import pathlib

import pytest

# Load ../build_catalog.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "coclcg_build_catalog", pathlib.Path(__file__).resolve().parent.parent / "build_catalog.py"
)
assert _spec and _spec.loader, "could not load build_catalog.py"
build_catalog = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(build_catalog)

COLUMNS = ['DD', 'MM', 'YY', 'Cycle', 'Set Name', '#', 'EXP', 'ID', 'IF',
           'Faction', 'Type', 'Title', 'Subtitle', 'Rarity', 'Have', 'Need']


def row(set_name, card_id, title, subtitle='', faction='Miskatonic', card_type='Character',
        rarity=3, released=('0', '0', '0'), cycle='Core Set'):
    day, month, year = released
    return {'DD': day, 'MM': month, 'YY': year, 'Cycle': cycle, 'Set Name': set_name,
            '#': str(card_id), 'EXP': '0', 'ID': str(card_id), 'IF': str(card_id),
            'Faction': faction, 'Type': card_type, 'Title': title, 'Subtitle': subtitle,
            'Rarity': str(rarity), 'Have': '', 'Need': str(rarity)}


def write_csv(tmp_path, rows):
    path = tmp_path / 'coc.csv'
    with open(path, 'w', encoding='utf-8', newline='') as handle:
        writer = csv.writer(handle)
        writer.writerow(['Release', '', '', '', '', 'Catalogue'])
        writer.writerow(COLUMNS)
        for entry in rows:
            writer.writerow([entry[column] for column in COLUMNS])
    return str(path)


def build(tmp_path, rows):
    packs, cards = build_catalog.build(write_csv(tmp_path, rows))
    return packs, {card['unique_id']: card for card in cards}


@pytest.fixture(autouse=True)
def no_fix_tables(monkeypatch):
    """Both tables name real cards, which a made-up CSV does not hold, and
    build() rejects an entry it cannot place. A test that wants one sets it."""
    monkeypatch.setattr(build_catalog, 'PROMOS', [])
    monkeypatch.setattr(build_catalog, 'POSITION_FIXES', {})


# --- ids and titles --------------------------------------------------------

def test_a_card_whose_title_is_its_own_keeps_the_bare_title(tmp_path):
    _, cards = build(tmp_path, [row('Core Set', 1, 'Thomas F. Malone', 'Haunted Police Detective')])
    assert cards['thomas-f-malone']['name'] == 'Thomas F. Malone'
    assert cards['thomas-f-malone']['subtitle'] == 'Haunted Police Detective'


def test_cards_sharing_a_title_are_told_apart_by_subtitle(tmp_path):
    _, cards = build(tmp_path, [
        row('Core Set', 41, 'Cthulhu', 'The Great Old One'),
        row('Core Set', 64, 'Cthulhu', "Lord of R'lyeh"),
    ])
    assert set(cards) == {'cthulhu-the-great-old-one', 'cthulhu-lord-of-rlyeh'}
    assert cards['cthulhu-lord-of-rlyeh']['name'] == "Cthulhu (Lord of R'lyeh)"


def test_the_card_with_no_subtitle_keeps_the_bare_title(tmp_path):
    _, cards = build(tmp_path, [
        row('Core Set', 81, 'Hastur'),
        row('Core Set', 82, 'Hastur', 'Lord of Carcosa'),
    ])
    assert set(cards) == {'hastur', 'hastur-lord-of-carcosa'}
    assert cards['hastur']['name'] == 'Hastur'


def test_a_title_and_subtitle_two_cards_share_falls_to_the_faction(tmp_path):
    _, cards = build(tmp_path, [
        row('Core Set', 158, 'Opening Night', faction='Story', card_type='Story'),
        row('Core Set', 35, 'Opening Night', faction='Hastur', card_type='Conspiracy'),
    ])
    assert set(cards) == {'opening-night-story', 'opening-night-hastur'}
    assert cards['opening-night-hastur']['name'] == 'Opening Night (Hastur)'


def test_a_title_the_spreadsheet_misspells_is_corrected(tmp_path):
    _, cards = build(tmp_path, [row('Core Set', 81, 'Hasur')])
    assert cards['hastur']['name'] == 'Hastur'


def test_a_subtitle_the_spreadsheet_ran_into_the_title_is_split_back_out(tmp_path):
    _, cards = build(tmp_path, [row('The Twilight Horror', 19, 'The Rays of Dawn Cleansing Light')])
    assert cards['the-rays-of-dawn']['subtitle'] == 'Cleansing Light'


# --- printings -------------------------------------------------------------

def test_a_reprint_is_one_card_with_a_printing_in_each_pack(tmp_path):
    _, cards = build(tmp_path, [
        row('Core Set', 18, 'Torch the Joint!', faction='Agency', card_type='Event'),
        row('Spawn of Madness', 2, 'Torch the Joint!', faction='Agency', card_type='Event'),
    ])
    assert list(cards) == ['torch-the-joint']
    assert [(v['pack_code'], v['number']) for v in cards['torch-the-joint']['versions']] == [
        ('core-set', 18), ('spawn-of-madness', 2)]


def test_the_copies_a_pack_holds_come_from_the_rarity_column(tmp_path):
    _, cards = build(tmp_path, [row('Secrets of Arkham', 1, 'Agency Medic', rarity=4)])
    assert cards['agency-medic']['versions'][0]['quantity'] == 4


def test_a_number_the_spreadsheet_has_wrong_is_corrected(tmp_path, monkeypatch):
    monkeypatch.setattr(build_catalog, 'POSITION_FIXES', {'core-set': {'Moving the Scenery': 147}})
    _, cards = build(tmp_path, [row('Core Set', 141, 'Moving the Scenery', card_type='Support')])
    assert cards['moving-the-scenery']['versions'][0]['number'] == 147


def test_a_position_fix_naming_a_card_that_is_not_there_is_an_error(tmp_path, monkeypatch):
    monkeypatch.setattr(build_catalog, 'POSITION_FIXES', {'core-set': {'No Such Card': 1}})
    with pytest.raises(SystemExit):
        build(tmp_path, [row('Core Set', 1, 'Thomas F. Malone')])


# --- packs -----------------------------------------------------------------

def test_a_dated_set_takes_its_date_from_the_spreadsheet(tmp_path):
    packs, _ = build(tmp_path, [
        row('Secrets of Arkham', 1, 'Agency Medic', released=('27', '5', '2010'))])
    assert packs[0] == {'code': 'secrets-of-arkham', 'name': 'Secrets of Arkham',
                        'date_release': '2010-05-27'}


def test_a_set_the_spreadsheet_leaves_undated_falls_back_to_the_release_table(tmp_path):
    packs, _ = build(tmp_path, [row('Core Set', 1, 'Thomas F. Malone')])
    assert packs[0]['date_release'] == build_catalog.RELEASE_DATES['core-set']


def test_a_set_in_neither_source_has_no_date(tmp_path):
    packs, _ = build(tmp_path, [row('Some Unknown Set', 1, 'Thomas F. Malone')])
    assert packs[0]['date_release'] is None


def test_every_run_carries_the_promos_pack(tmp_path):
    packs, _ = build(tmp_path, [row('Core Set', 1, 'Thomas F. Malone')])
    assert packs[-1] == {'code': 'promos', 'name': 'Promos', 'date_release': None}


# --- promos ----------------------------------------------------------------

def test_a_promo_adds_a_printing_to_the_card_it_names(tmp_path, monkeypatch):
    monkeypatch.setattr(build_catalog, 'PROMOS', [('Laboratory Assistant', '')])
    _, cards = build(tmp_path, [row('Core Set', 29, 'Laboratory Assistant')])
    assert {v['pack_code'] for v in cards['laboratory-assistant']['versions']} == {
        'core-set', 'promos'}


def test_a_promo_naming_no_card_is_an_error(tmp_path, monkeypatch):
    monkeypatch.setattr(build_catalog, 'PROMOS', [('Nyarlathotep', '')])
    with pytest.raises(SystemExit):
        build(tmp_path, [row('Core Set', 29, 'Laboratory Assistant')])


def test_a_promo_naming_two_cards_is_an_error(tmp_path, monkeypatch):
    monkeypatch.setattr(build_catalog, 'PROMOS', [('Cthulhu', '')])
    with pytest.raises(SystemExit):
        build(tmp_path, [
            row('Core Set', 41, 'Cthulhu', faction='Cthulhu'),
            row('Core Set', 42, 'Cthulhu', faction='Neutral'),
        ])


# --- card backs ------------------------------------------------------------

def test_a_story_card_names_the_back_of_the_pack_it_is_in(tmp_path):
    _, cards = build(tmp_path, [
        row('Core Set', 158, 'Opening Night', faction='Story', card_type='Story')])
    assert cards['opening-night']['back_group'] == 'story-core-set'


def test_every_other_card_carries_the_standard_back(tmp_path):
    _, cards = build(tmp_path, [
        row('Core Set', 155, 'The Bootleg Whiskey Cover-up', faction='Neutral',
            card_type='Conspiracy')])
    assert cards['the-bootleg-whiskey-cover-up']['back_group'] == 'card'


# --- the file the adapter reads --------------------------------------------

def test_the_generated_catalog_is_the_one_checked_in():
    """The two JSON files are generated, so they are only ever as current as
    the last run of this script."""
    catalog_dir = pathlib.Path(build_catalog.OUTPUT_DIR)
    cards = json.loads((catalog_dir / 'coc_cards.json').read_text(encoding='utf-8'))
    packs = json.loads((catalog_dir / 'coc_packs.json').read_text(encoding='utf-8'))

    build_catalog.check(packs, cards)
    assert {pack['code'] for pack in packs} >= {'core-set', 'promos'}
