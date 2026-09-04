import importlib.util
import pathlib

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "ahlcg_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)


def card(code, name, pack='ptc', **extra):
    entry = {'code': code, 'name': name, 'pack_code': pack, 'type_code': 'location'}
    entry.update(extra)
    return entry


def catalog(*cards, pack='ptc'):
    return rename.Catalog(list(cards), pack)


class TestSquash:
    def test_an_apostrophe_written_as_underscore_collapses_to_the_same_thing(self):
        assert rename.squash('Skids O_toole') == rename.squash('"Skids" O\'Toole')

    def test_accents_are_folded(self):
        assert rename.squash('Umôrdhoth') == 'umordhoth'


class TestParseName:
    def test_a_side_suffix_is_read_and_removed(self):
        info = rename.parse_name('10_Foyer_Location_Side B')
        assert info['side'] == 'back'
        assert info['body'] == 'Foyer_Location'

    def test_an_act_writes_its_side_into_the_label(self):
        front = rename.parse_name('Act 1A_Trapped')
        back = rename.parse_name('Act 1B_The Door on the Floor')
        assert front['act'] == back['act'] == ('act', 1)
        assert (front['side'], back['side']) == ('front', 'back')

    def test_the_two_faces_of_an_act_share_a_group(self):
        assert (rename.group_key(rename.parse_name('Act 1A_Trapped'))
                == rename.group_key(rename.parse_name('Act 1B_The Door on the Floor')))

    def test_a_position_range_keeps_both_ends(self):
        info = rename.parse_name('16 to 18_Backstage Doorway_Location')
        assert (info['first'], info['last']) == (16, 18)

    def test_a_level_suffix_is_not_part_of_the_title(self):
        assert rename.parse_name('Seeker_Occult Lexicon_Asset_L3')['body'] \
            == 'Seeker_Occult Lexicon_Asset'

    def test_sides_of_an_unnumbered_card_share_a_group(self):
        assert (rename.group_key(rename.parse_name('Location_Attic'))
                == rename.group_key(rename.parse_name('Location_Attic_Side B')))


class TestCandidates:
    def test_a_title_is_found_anywhere_in_the_filename(self):
        found, how = rename.candidates('Survivor_Cunning Distraction_Event',
                                       catalog(card('03001', 'Cunning Distraction')))
        assert [c['code'] for c in found] == ['03001']
        assert how == 'title'

    def test_the_encounter_set_name_does_not_beat_the_cards(self):
        """`16_The Devourer Below_Umordhoth_Enemy` names Umordhoth, not the
        scenario it is written under -- which is a card of its own."""
        found, _ = rename.candidates(
            'The Devourer Below_Umordhoth_Enemy',
            catalog(card('01157', 'Umôrdhoth'), card('01142', 'The Devourer Below')))
        assert [c['code'] for c in found] == ['01157']

    def test_a_longer_title_ending_in_the_same_place_wins(self):
        found, _ = rename.candidates(
            'Basic Weakness_Silver Twilight Acolyte_Enemy',
            catalog(card('01102', 'Silver Twilight Acolyte'), card('01169', 'Acolyte')))
        assert [c['code'] for c in found] == ['01102']

    def test_a_misspelling_still_resolves(self):
        found, how = rename.candidates('4_Handman_s Brook_Location',
                                       catalog(card('54037', "Hangman's Brook")))
        assert [c['code'] for c in found] == ['54037']
        assert how == 'spelling'

    def test_a_subtitle_separates_cards_sharing_a_name(self):
        cards = [card('03084a', 'Whispers in Your Head', subname='Dismay'),
                 card('03084b', 'Whispers in Your Head', subname='Dread')]
        found, _ = rename.candidates('Whispers in Your Head (Dread)_Treachery',
                                     catalog(*cards))
        assert [c['code'] for c in found] == ['03084b']

    def test_type_separates_cards_sharing_a_name(self):
        cards = [card('03076a', 'Constance Dumaine', type_code='asset'),
                 card('03059', 'Constance Dumaine', type_code='enemy')]
        found, _ = rename.candidates('5_Constance Dumaine_Enemy', catalog(*cards))
        assert [c['code'] for c in found] == ['03059']

    def test_a_renamed_card_resolves_through_the_fix_table(self):
        found, how = rename.candidates('Maniac_Enemy',
                                       catalog(card('03095', 'Seer of the Sign')))
        assert [c['code'] for c in found] == ['03095']
        assert 'ArkhamDB' in how

    def test_an_unknown_title_matches_nothing(self):
        found, how = rename.candidates('Something Else Entirely',
                                       catalog(card('03001', 'Cunning Distraction')))
        assert found == []
        assert how == 'no title matched'


class TestCatalog:
    def test_a_hidden_back_points_at_the_front_it_is_linked_from(self):
        index = catalog(card('03076a', 'Constance Dumaine', linked_to_code='03076b'),
                        card('03076b', "Engram's Oath", hidden=True))
        assert index.front_of == {'03076b': '03076a'}


class TestResolveFolder:
    def test_the_folder_order_separates_two_cards_of_one_name(self):
        """Return to the Wages of Sin prints two Heretics. Nothing in the
        filenames tells them apart, so the position they carry does."""
        index = catalog(card('54038', 'Heretic', type_code='enemy'),
                        card('54039', 'Heretic', type_code='enemy'))
        entries = []
        for position in (5, 6):
            info = rename.parse_name(f'{position}_Heretic_Enemy_Side A')
            info.update(rel=f'{position}.tif', path=f'{position}.tif',
                        stem=f'{position}_Heretic_Enemy_Side A')
            entries.append(info)
        resolved, problems, extras, _levels, _backs = rename.resolve_folder(entries, index)
        assert not problems and not extras
        assert [(e['first'], c['code']) for e, c, _ in resolved] == [(5, '54038'), (6, '54039')]

    def test_a_second_scan_of_one_card_is_reported_not_matched_twice(self):
        index = catalog(card('01111', 'Crypt Chill', type_code='treachery'))
        entries = []
        for position in (1, 2):
            stem = f'{position}_Chilling Cold_Crypt Chill_Treachery'
            info = rename.parse_name(stem)
            info.update(rel=f'{position}.tif', path=f'{position}.tif', stem=stem)
            entries.append(info)
        resolved, _problems, extras, _levels, _backs = rename.resolve_folder(entries, index)
        assert len(resolved) == 1
        assert len(extras) == 1


class TestLevels:
    def test_a_recorded_level_decides_between_two_printings_of_one_card(self):
        """The Core Set prints Beat Cop at level 0 and level 2, and the archive
        holds one scan of it. The filename says nothing, so LEVEL_FIXES does."""
        index = catalog(card('01018', 'Beat Cop', pack='core', type_code='asset', xp=0),
                        card('01028', 'Beat Cop', pack='core', type_code='asset', xp=2),
                        pack='core')
        found, _ = rename.candidates('Guardian_Beat Cop_Asset', index)
        assert [c['code'] for c in found] == ['01028']

    def test_an_unrecorded_level_is_left_ambiguous_rather_than_guessed(self):
        index = catalog(card('01099', 'Some Card', pack='core', type_code='asset', xp=0),
                        card('01100', 'Some Card', pack='core', type_code='asset', xp=2),
                        pack='core')
        found, _ = rename.candidates('Guardian_Some Card_Asset', index)
        assert len(found) == 2
        assert rename.levels_differ(found)

    def test_one_scan_of_a_card_printed_at_two_levels_is_reported(self):
        index = catalog(card('01099', 'Some Card', pack='core', type_code='asset', xp=0),
                        card('01100', 'Some Card', pack='core', type_code='asset', xp=2),
                        pack='core')
        info = rename.parse_name('Guardian_Some Card_Asset')
        info.update(rel='x.tif', path='x.tif', stem='Guardian_Some Card_Asset')
        _resolved, _problems, _extras, unlevelled, _backs = rename.resolve_folder([info], index)
        assert len(unlevelled) == 1
