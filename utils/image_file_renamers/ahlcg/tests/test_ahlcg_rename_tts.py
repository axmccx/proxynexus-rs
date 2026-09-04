import importlib.util
import json
import pathlib

from PIL import Image

# Load ../rename_tts.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "ahlcg_rename_tts", pathlib.Path(__file__).resolve().parent.parent / "rename_tts.py"
)
assert _spec and _spec.loader, "could not load rename_tts.py"
tts = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(tts)


def entry(path='campaign/A/x.json', width=10, height=7, slot=0,
          unique_back=True, back='back-url', face='face-url'):
    return {'path': path, 'nickname': 'X', 'face': face, 'back': back,
            'unique_back': unique_back, 'width': width, 'height': height, 'slot': slot}


def card(code, name='A Card', pack='core', type_code='location', **extra):
    value = {'code': code, 'name': name, 'pack_code': pack, 'type_code': type_code}
    value.update(extra)
    return value


class TestScedId:
    def test_a_card_is_found_by_its_own_code(self):
        assert tts.sced_id(card('83009'), {'83009': []}, {})[0] == '83009'

    def test_a_card_split_in_two_is_found_by_the_half_arkhamdb_hides(self):
        # ArkhamDB indexes the pair under 03325b and hides 03325; SCED names its
        # one object after the hidden half.
        identity, how = tts.sced_id(card('03325b'), {'03325': []}, {'03325b': '03325'})
        assert identity == '03325'
        assert 'hidden' in how

    def test_the_face_suffix_arkhamdb_adds_is_dropped(self):
        identity, how = tts.sced_id(card('04128a'), {'04128': []}, {})
        assert identity == '04128'
        assert 'ArkhamDB adds' in how

    def test_the_per_copy_suffix_sced_adds_is_tried(self):
        # ArkhamDB has one Alkaline Rail; SCED gives each printed copy an object.
        identity, how = tts.sced_id(card('10512'), {'10512a': [], '10512b': []}, {})
        assert identity == '10512a'
        assert 'SCED adds' in how

    def test_a_reprint_falls_back_to_the_card_it_duplicates(self):
        # The English mod holds Switchblade once, under the Core Set code.
        identity, how = tts.sced_id(
            card('60307', duplicate_of_code='01044'), {'01044': []}, {})
        assert identity == '01044'
        assert how == tts.DUPLICATE_RULE

    def test_the_direct_hit_wins_over_every_fallback(self):
        index = {'60307': [], '01044': []}
        assert tts.sced_id(card('60307', duplicate_of_code='01044'), index, {})[0] == '60307'

    def test_a_card_sced_does_not_hold_resolves_to_nothing(self):
        assert tts.sced_id(card('99999'), {}, {})[0] is None

    def test_a_code_that_is_not_a_number_is_not_stemmed(self):
        # Only a plain number with one trailing letter carries a face suffix.
        assert tts.sced_id(card('09022-t-c'), {'09022': []}, {})[0] is None


class TestPick:
    def test_a_translation_never_reaches_pick(self):
        # build_index drops them, so an empty list is what pick sees.
        assert tts.pick([]) is None

    def test_an_official_product_beats_a_fan_rework_of_it(self):
        chosen = tts.pick([entry(path='campaign/Unofficial Return to X/a.json', width=1, height=1),
                           entry(path='campaign/X/b.json')])
        assert chosen['path'] == 'campaign/X/b.json'

    def test_the_smallest_grid_wins_because_its_cells_are_biggest(self):
        chosen = tts.pick([entry(path='campaign/X/a.json', width=10, height=7),
                           entry(path='campaign/X/b.json', width=6, height=3)])
        assert chosen['path'] == 'campaign/X/b.json'

    def test_a_fan_rework_is_used_when_it_is_all_there_is(self):
        only = entry(path='campaign/Unofficial X/a.json')
        assert tts.pick([only]) is only

    def test_a_sheet_known_to_be_gone_is_stepped_over(self):
        dead = entry(path='campaign/X/a.json', face='gone', width=1, height=1)
        live = entry(path='campaign/X/b.json', face='alive')
        assert tts.pick([dead, live]) is dead
        assert tts.pick([dead, live], avoid={'gone'}) is live

    def test_nothing_is_left_when_every_sheet_is_gone(self):
        only = entry(face='gone')
        assert tts.pick([only], avoid={'gone'}) is None

    def test_ranking_puts_the_best_first_and_keeps_the_rest(self):
        rework = entry(path='campaign/Unofficial X/a.json', width=1, height=1)
        big = entry(path='campaign/X/a.json', width=10, height=7)
        small = entry(path='campaign/X/b.json', width=6, height=3)
        assert tts.rank([rework, big, small]) == [small, big, rework]


class TestCell:
    def test_a_slot_is_counted_row_major_from_zero(self):
        sheet = Image.new('RGB', (100, 70))
        sheet.paste(Image.new('RGB', (10, 10), 'red'), (30, 10))
        assert tts.cell(sheet, 10, 7, 13).getpixel((0, 0)) == (255, 0, 0)

    def test_a_deck_of_one_card_is_the_whole_picture(self):
        sheet = Image.new('RGB', (100, 140))
        assert tts.cell(sheet, 1, 1, 0).size == (100, 140)


class TestBacks:
    def test_a_unique_back_is_the_cards_own(self):
        assert tts.has_own_back(entry(unique_back=True))

    def test_a_deck_of_one_shares_a_back_with_nothing_else(self):
        assert tts.has_own_back(entry(unique_back=False, width=1, height=1))

    def test_a_grid_sharing_one_picture_is_the_generic_card_back(self):
        assert not tts.has_own_back(entry(unique_back=False, width=10, height=7))

    def test_arkhamdb_giving_a_back_image_means_a_second_face(self):
        assert tts.wants_back(card('01125', backimagesrc='/x.png'), {})

    def test_a_linked_half_counts_as_a_second_face(self):
        by_code = {'03076b': {'imagesrc': '/y.png'}}
        assert tts.wants_back(card('03076a', linked_to_code='03076b'), by_code)

    def test_a_card_with_the_standard_back_wants_none(self):
        assert not tts.wants_back(card('01044'), {})


class TestAlreadyHeld:
    def test_both_faces_of_a_card_report_the_one_id(self, tmp_path):
        for name in ('01001@core.jpg', '01001@core~back.jpg', '03076a@ptc.png'):
            (tmp_path / name).touch()
        assert tts.already_held(str(tmp_path)) == {'01001', '03076a'}

    def test_a_file_that_is_not_a_card_is_ignored(self, tmp_path):
        (tmp_path / 'notes.txt').touch()
        assert tts.already_held(str(tmp_path)) == set()

    def test_no_directory_holds_nothing(self):
        assert tts.already_held(None) == set()


class TestSheetJobs:
    def test_both_faces_are_grouped_under_the_sheet_they_come_from(self):
        job = {'code': '83009', 'pack': 'guardians', 'back': True,
               'entry': entry(face='F', back='B')}
        grouped = tts.sheet_jobs([job])
        assert grouped['F'] == [(job, '')]
        assert grouped['B'] == [(job, '~back')]

    def test_a_single_sided_card_names_only_its_face_sheet(self):
        job = {'code': '01044', 'pack': 'core', 'back': False,
               'entry': entry(face='F', back='B')}
        assert set(tts.sheet_jobs([job])) == {'F'}

    def test_cards_sharing_a_sheet_are_cut_together(self):
        first = {'code': 'a', 'pack': 'p', 'back': False, 'entry': entry(face='F')}
        second = {'code': 'b', 'pack': 'p', 'back': False, 'entry': entry(face='F')}
        assert len(tts.sheet_jobs([first, second])['F']) == 2


class TestBestCorrelation:
    def test_a_card_stored_a_quarter_turn_out_still_matches(self):
        # An act is printed landscape and stored turned into a portrait frame,
        # so it only matches its landscape reference once turned back.
        reference = Image.new('RGB', (140, 100))
        reference.paste(Image.new('RGB', (40, 100), 'white'), (0, 0))
        turned = reference.rotate(90, expand=True)
        assert tts.best_correlation(tts.orientation.squared(turned), reference) > 0.9

    def test_a_different_picture_does_not_match(self):
        # A quarter white against a half white: no turn of one becomes the other.
        one = Image.new('RGB', (100, 140), 'black')
        one.paste(Image.new('RGB', (50, 70), 'white'), (0, 0))
        other = Image.new('RGB', (100, 140), 'black')
        other.paste(Image.new('RGB', (100, 70), 'white'), (0, 0))
        assert tts.best_correlation(tts.orientation.squared(one), other) < 0.9


class TestReferenceFetcher:
    def _fetcher(self, answers):
        fetcher = tts.orientation.ReferenceFetcher(limit=3)
        calls = []

        def fake(url):
            calls.append(url)
            return answers.get(url)

        fetcher_module = tts.orientation.fetch_reference
        tts.orientation.fetch_reference = fake
        try:
            return fetcher, calls, [fetcher(u) for u in sorted(answers)]
        finally:
            tts.orientation.fetch_reference = fetcher_module

    def test_a_run_of_failures_stops_it_asking(self):
        fetcher, calls, _ = self._fetcher({'a': None, 'b': None, 'c': None, 'd': None})
        assert fetcher.given_up
        assert calls == ['a', 'b', 'c']       # 'd' was never asked for

    def test_a_success_clears_the_run(self):
        fetcher, calls, results = self._fetcher(
            {'a': None, 'b': None, 'c': '/hit', 'd': None})
        assert not fetcher.given_up
        assert len(calls) == 4
        assert results[2] == '/hit'

    def test_what_is_already_cached_is_served_after_giving_up(self, tmp_path, monkeypatch):
        # Handing back a file already on disk costs nothing, so giving up on the
        # network must not also give up on the cache.
        monkeypatch.setattr(tts.orientation, 'REF_CACHE', str(tmp_path))
        (tmp_path / 'hit.png').write_bytes(b'x')
        fetcher = tts.orientation.ReferenceFetcher(limit=1)
        monkeypatch.setattr(tts.orientation, 'fetch_reference', lambda url: None)
        assert fetcher('/bundles/cards/miss.png') is None
        assert fetcher.given_up
        assert fetcher('/bundles/cards/hit.png') == str(tmp_path / 'hit.png')

    def test_all_answered_never_gives_up(self):
        fetcher, _calls, results = self._fetcher({'a': '/1', 'b': '/2'})
        assert not fetcher.given_up
        assert results == ['/1', '/2']


class TestCardObjects:
    def _write(self, root, name, obj, notes=None):
        (root / f'{name}.json').write_text(json.dumps(obj))
        if notes is not None:
            (root / f'{name}.gmnotes').write_text(json.dumps(notes))

    def test_the_id_comes_from_the_gmnotes_file_beside_the_object(self, tmp_path):
        self._write(tmp_path, 'CairoBazaar.d2de0e', {
            'CardID': 270108,
            'CustomDeck': {'2701': {'FaceURL': 'F', 'NumWidth': 10, 'NumHeight': 5}},
            'GMNotes_path': 'Any/Path/CairoBazaar.d2de0e.gmnotes',
        }, {'id': '83009'})
        (_path, identity, _obj, sheet, slot) = next(tts.card_objects(str(tmp_path)))
        assert identity == '83009'
        assert slot == 8
        assert sheet['NumWidth'] == 10

    def test_the_card_id_names_which_deck_it_indexes(self, tmp_path):
        self._write(tmp_path, 'Two', {
            'CardID': 330201,
            'CustomDeck': {'2701': {'FaceURL': 'wrong'}, '3302': {'FaceURL': 'right'}},
        })
        (_path, _identity, _obj, sheet, slot) = next(tts.card_objects(str(tmp_path)))
        assert sheet['FaceURL'] == 'right'
        assert slot == 1

    def test_an_object_with_no_gmnotes_carries_no_id(self, tmp_path):
        self._write(tmp_path, 'ExplorationMap.6abc69', {
            'CardID': 271802, 'CustomDeck': {'2718': {'FaceURL': 'F'}}})
        assert next(tts.card_objects(str(tmp_path)))[1] is None

    def test_something_that_is_not_a_card_is_skipped(self, tmp_path):
        (tmp_path / 'Bag.json').write_text(json.dumps({'Name': 'Bag'}))
        assert list(tts.card_objects(str(tmp_path))) == []
