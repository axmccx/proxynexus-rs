import importlib.util
import json
import pathlib
import struct
import zlib

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "whconquest_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
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


def card(unique_id, name, pack_code='core-set', card_type='Army'):
    return {'unique_id': unique_id, 'name': name, 'pack_code': pack_code,
            'type': card_type, 'faction': 'Ork', 'card_number': 1, 'card_quantity': 3}


def write_catalog(tmp_path, cards, packs):
    folder = tmp_path / 'catalog'
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'whc_cards.json').write_text(json.dumps(cards))
    (folder / 'whc_packs.json').write_text(json.dumps(packs))
    return str(folder)


# --- name normalisation ----------------------------------------------------

def test_slug_drops_apostrophes_rather_than_hyphenating_them():
    assert rename.slug("Zogwort's Curse") == 'zogworts-curse'
    assert rename.slug("Y'varn") == 'yvarn'


def test_squash_reads_a_space_and_an_apostrophe_the_same_way():
    assert rename.squash('Straken_s_Cunning') == rename.squash("Straken's Cunning")
    assert rename.squash('23rd_Mechanised_Battalion') == rename.squash('23rd Mechanised Battalion')
    assert rename.squash('bleed_Y_varn') == rename.squash("bleed Y'varn")


def test_copy_number_is_split_off_the_card_name():
    assert rename.split_copy_index('bleed_Brood Warriors (3)') == ('Brood Warriors', 3)


def test_a_name_with_no_copy_number_is_the_first_copy():
    assert rename.split_copy_index('bleed_Omega Zero Command') == ('Omega Zero Command', 1)


# --- what a scan is worth --------------------------------------------------

def test_a_bleed_scans_resolution_is_measured_across_the_card_not_the_sheet():
    # 816px over the 2.72in bleed sheet is 300dpi across the 2.5in card.
    assert round(rename.card_dpi(816, bleed=True)) == 300
    assert round(rename.card_dpi(750, bleed=False)) == 300


def test_bleed_is_read_off_the_image_not_the_filename(tmp_path):
    bleedless = tmp_path / 'bleed_Acid Maw.png'
    png(bleedless, 1464, 2087)
    assert not rename.describe(str(bleedless))['bleed']

    bordered = tmp_path / 'Acid Maw.png'
    png(bordered, *rename.BLEED_SIZE)
    assert rename.describe(str(bordered))['bleed']


def scan(path, dpi, index=1, bleed=False, lossless=False, card_row=None):
    return {'path': path, 'ext': '.png' if lossless else '.jpg', 'bleed': bleed, 'dpi': dpi,
            'lossless': lossless, 'index': index, 'card': card_row or card('acid-maw', 'Acid Maw')}


def test_the_sharpest_scan_of_a_card_is_the_one_used():
    low = scan('low.png', 300, bleed=True, lossless=True)
    high = scan('high.jpg', 585)
    assert rename.sharpest([low, high]) is high


def test_a_bleed_border_only_breaks_a_tie_on_resolution():
    plain = scan('plain.jpg', 300)
    bordered = scan('bordered.png', 300, bleed=True)
    assert rename.sharpest([plain, bordered]) is bordered


def test_a_lossless_file_breaks_a_tie_no_bleed_border_settles():
    jpeg = scan('a.jpg', 300)
    lossless = scan('b.png', 300, lossless=True)
    assert rename.sharpest([jpeg, lossless]) is lossless


# --- naming ----------------------------------------------------------------

def test_a_single_scan_is_named_as_a_front():
    groups = {('decree-of-ruin', 'acid-maw'): [scan('a.jpg', 585)]}
    renames, passed_over, half = rename.plan(groups)
    assert renames == [('a.jpg', 'acid-maw@decree-of-ruin.jpg')]
    assert passed_over == [] and half == []


def test_a_bleed_scan_keeps_the_bleed_suffix():
    groups = {('decree-of-ruin', 'acid-maw'): [scan('a.png', 300, bleed=True, lossless=True)]}
    renames, _, _ = rename.plan(groups)
    assert renames == [('a.png', 'acid-maw@decree-of-ruin.bleed.png')]


def test_a_warlords_second_side_is_its_bloodied_reverse():
    row = card('nahumekh', 'Nahumekh', 'legions-of-death', 'Warlord')
    groups = {('legions-of-death', 'nahumekh'): [
        scan('front.jpg', 585, index=1, card_row=row),
        scan('back.jpg', 585, index=2, card_row=row),
    ]}
    renames, passed_over, half = rename.plan(groups)
    assert renames == [
        ('front.jpg', 'nahumekh@legions-of-death.jpg'),
        ('back.jpg', 'nahumekh@legions-of-death~back.jpg'),
    ]
    assert passed_over == [] and half == []


def test_each_side_of_a_warlord_picks_its_own_sharpest_scan():
    row = card('nahumekh', 'Nahumekh', 'legions-of-death', 'Warlord')
    groups = {('legions-of-death', 'nahumekh'): [
        scan('front-low.png', 300, index=1, bleed=True, lossless=True, card_row=row),
        scan('front-high.jpg', 585, index=1, card_row=row),
        scan('back-high.jpg', 585, index=2, card_row=row),
    ]}
    renames, passed_over, _ = rename.plan(groups)
    assert [name for _, name in renames] == [
        'nahumekh@legions-of-death.jpg', 'nahumekh@legions-of-death~back.jpg']
    assert passed_over == ['front-low.png']


def test_a_warlord_with_only_one_side_is_reported():
    row = card('nahumekh', 'Nahumekh', 'legions-of-death', 'Warlord')
    _, _, half = rename.plan({('legions-of-death', 'nahumekh'): [
        scan('front.jpg', 585, index=1, card_row=row)]})
    assert half == ['nahumekh']


def test_copies_of_an_ordinary_card_are_all_one_face():
    # Only warlords are double-sided, so a second copy of an Army card is
    # another scan of the same face, never a reverse.
    row = card('hate', 'Hate', 'legions-of-death', 'Event')
    groups = {('legions-of-death', 'hate'): [
        scan('a.jpg', 585, index=1, card_row=row),
        scan('b.jpg', 300, index=2, card_row=row),
    ]}
    renames, passed_over, half = rename.plan(groups)
    assert renames == [('a.jpg', 'hate@legions-of-death.jpg')]
    assert passed_over == ['b.jpg'] and half == []


# --- pack archive ----------------------------------------------------------

def test_a_folder_naming_a_pack_resolves_to_its_id(tmp_path):
    pack_ids = {'decree-of-ruin': 'decree-of-ruin'}
    path = str(tmp_path / 'Planetfall Cycle' / 'Decree of Ruin' / 'bleed_Acid Maw.png')
    assert rename.resolve_pack(path, str(tmp_path), pack_ids) == ('decree-of-ruin', None)


def test_folders_between_the_card_and_its_pack_are_walked_past(tmp_path):
    pack_ids = {'core-set': 'core-set'}
    path = str(tmp_path / 'Core Set' / 'Warlords' / 'Zarathur' / 'bleed_Mark of Chaos.png')
    assert rename.resolve_pack(path, str(tmp_path), pack_ids)[0] == 'core-set'


def test_a_misspelled_folder_still_finds_its_pack(tmp_path):
    pack_ids = {'the-descendants-of-isha': 'the-descendants-of-isha'}
    path = str(tmp_path / 'Warlord Cycle' / 'Descendants of Isha' / 'bleed_Doom Siren.png')
    pack_id, guess = rename.resolve_pack(path, str(tmp_path), pack_ids)
    assert pack_id == 'the-descendants-of-isha'
    assert guess == ('Descendants of Isha', 'the-descendants-of-isha')


def test_a_file_outside_any_pack_folder_resolves_to_nothing(tmp_path):
    path = str(tmp_path / 'bleed_40k Back.png')
    assert rename.resolve_pack(path, str(tmp_path), {'core-set': 'core-set'}) == (None, None)


def test_a_misspelled_filename_still_finds_its_card():
    cards = rename.card_index({'acid-maw': card('acid-maw', 'Acid Maw')})
    found, guess, reason = rename.resolve_card('Avid Maw', cards)
    assert found['unique_id'] == 'acid-maw' and guess[1] == 'acid-maw' and reason is None


def test_an_underscore_standing_for_an_apostrophe_still_matches():
    cards = rename.card_index({'catos-stronghold': card('catos-stronghold', "Cato's Stronghold")})
    found, guess, _ = rename.resolve_card('Cato_s Stronghold', cards)
    assert found['unique_id'] == 'catos-stronghold'
    assert guess is None, 'punctuation alone should not count as a spelling match'


def test_a_name_matching_nothing_is_reported():
    cards = rename.card_index({'acid-maw': card('acid-maw', 'Acid Maw')})
    found, _, reason = rename.resolve_card('Hedge Fund', cards)
    assert found is None and reason


def test_a_name_two_cards_are_equally_close_to_is_reported():
    cards = rename.card_index({
        'blood-chamber': card('blood-chamber', 'Blood Chamber'),
        'brood-chamber': card('brood-chamber', 'Brood Chamber'),
    })
    found, _, reason = rename.resolve_card('Bzood Chamber', cards)
    assert found is None and 'as close to' in reason


def test_a_pack_archive_resolves_end_to_end(tmp_path, monkeypatch):
    monkeypatch.setattr(rename, 'CATALOG_DIR', write_catalog(
        tmp_path,
        [card('nahumekh', 'Nahumekh', 'legions-of-death', 'Warlord'),
         card('gauss-flayer', 'Gauss Flayer', 'legions-of-death', 'Attachment')],
        [{'code': 'legions-of-death', 'name': 'Legions of Death', 'date_release': None}]))

    source = tmp_path / 'scans' / 'Legions of Death'
    png(source / 'bleed_Guass Flayer.png', *rename.BLEED_SIZE)
    png(source / 'Warlords' / 'Nahumekh' / 'bleed_Nahumekh (1).png', *rename.BLEED_SIZE)
    png(source / 'Warlords' / 'Nahumekh' / 'bleed_Nahumekh (2).png', *rename.BLEED_SIZE)

    by_pack, pack_ids = rename.load_catalog()
    groups, unresolved, _, guesses = rename.collect_pack_archive(
        str(tmp_path / 'scans'), by_pack, pack_ids)
    renames, passed_over, half = rename.plan(groups)

    assert unresolved == [] and passed_over == [] and half == []
    assert [name for _, name in renames] == [
        'gauss-flayer@legions-of-death.bleed.png',
        'nahumekh@legions-of-death.bleed.png',
        'nahumekh@legions-of-death~back.bleed.png',
    ]
    assert [guess[1] for guess in guesses] == ['gauss-flayer']


# --- faction archive -------------------------------------------------------

def faction_cards():
    return rename.card_index({
        'doom': card('doom', 'Doom', 'core-set', 'Event'),
        'nahumekh': card('nahumekh', 'Nahumekh', 'legions-of-death', 'Warlord'),
        'strakens-cunning': card('strakens-cunning', "Straken's Cunning", 'core-set', 'Event'),
        'shrieking-exarch': card('shrieking-exarch', 'Shrieking Exarch'),
    })


def test_the_pack_comes_from_the_card_when_no_folder_names_one(tmp_path):
    png(tmp_path / 'src' / 'Eldar' / 'Event' / 'Doom.jpg', 1463, 2088)
    groups, foreign, _ = rename.collect_faction_archive(str(tmp_path / 'src'), faction_cards())
    assert list(groups) == [('core-set', 'doom')] and foreign == 0


def test_an_underscore_serving_as_both_space_and_apostrophe_still_matches(tmp_path):
    png(tmp_path / 'src' / 'Astra Militarum' / 'Straken_s_Cunning.jpg', 1463, 2088)
    groups, foreign, guesses = rename.collect_faction_archive(
        str(tmp_path / 'src'), faction_cards())
    assert list(groups) == [('core-set', 'strakens-cunning')]
    assert foreign == 0 and guesses == []


def test_a_bloodied_file_is_the_warlords_reverse(tmp_path):
    src = tmp_path / 'src' / 'Necrons' / 'Warlords' / 'Nahumekh'
    png(src / 'Nahumekh.jpg', 1463, 2088)
    png(src / 'Nahumekh_bloodied.jpg', 1463, 2088)
    groups, _, _ = rename.collect_faction_archive(str(tmp_path / 'src'), faction_cards())
    renames, _, half = rename.plan(groups)
    assert [name for _, name in renames] == [
        'nahumekh@legions-of-death.jpg', 'nahumekh@legions-of-death~back.jpg']
    assert half == []


def test_a_fan_reworking_of_a_card_is_not_used(tmp_path):
    png(tmp_path / 'src' / 'Eldar' / 'Army' / 'Shrieking_Exarch_apoka.jpg', 1463, 2088)
    groups, foreign, _ = rename.collect_faction_archive(str(tmp_path / 'src'), faction_cards())
    assert groups == {} and foreign == 1


def test_folders_of_card_parts_and_tokens_are_skipped(tmp_path):
    png(tmp_path / 'src' / 'Blanked' / 'Eldar' / 'Doom.jpg', 1463, 2088)
    png(tmp_path / 'src' / 'Tokens' / 'Doom.jpg', 1463, 2088)
    groups, foreign, _ = rename.collect_faction_archive(str(tmp_path / 'src'), faction_cards())
    assert groups == {} and foreign == 0


def test_a_card_outside_the_catalog_is_counted_not_guessed_at(tmp_path):
    png(tmp_path / 'src' / 'Eldar' / 'Army' / 'Some_Fan_Made_Card.jpg', 1463, 2088)
    groups, foreign, _ = rename.collect_faction_archive(str(tmp_path / 'src'), faction_cards())
    assert groups == {} and foreign == 1


# --- merging both archives -------------------------------------------------

def test_the_sharper_archive_wins_where_both_hold_a_card():
    key = ('core-set', 'doom')
    merged = rename.merge({key: [scan('pack.png', 300, bleed=True, lossless=True)]},
                          {key: [scan('faction.jpg', 585)]})
    renames, passed_over, _ = rename.plan(merged)
    assert renames == [('faction.jpg', 'doom@core-set.jpg')]
    assert passed_over == ['pack.png']


def test_a_card_only_one_archive_holds_still_comes_through():
    merged = rename.merge({('core-set', 'doom'): [scan('pack.png', 300, bleed=True)]},
                          {('core-set', 'raid'): [scan('faction.jpg', 585)]})
    renames, _, _ = rename.plan(merged)
    assert sorted(name for _, name in renames) == ['doom@core-set.bleed.jpg', 'raid@core-set.jpg']


# --- coverage --------------------------------------------------------------

def test_cards_a_touched_pack_has_no_scan_for_are_reported():
    by_pack = {'core-set': {'acid-maw': card('acid-maw', 'Acid Maw'),
                            'avid-maw': card('avid-maw', 'Avid Maw')}}
    assert rename.report_gaps({('core-set', 'acid-maw'): []}, by_pack) == [
        ('core-set', 2, ['avid-maw'])]


def test_a_pack_no_scan_reached_is_not_reported_as_a_gap():
    by_pack = {'champions': {'doom': card('doom', 'Doom', 'champions')}}
    assert rename.report_gaps({}, by_pack) == []


def test_a_name_the_catalog_spells_with_a_flattened_symbol_still_matches():
    cards = rename.card_index({
        'subject-o-x62113': card('subject-o-x62113', '"Subject: O-X62113"',
                                 'what-lurks-below', 'Warlord')})
    found, guess, _ = rename.resolve_card('Subject_Omega-X62113', cards)
    assert found['unique_id'] == 'subject-o-x62113'
    assert guess is None
