import importlib.util
import pathlib

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "marvel_champions_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)


def card(code, name, type_code='event', pack='core', position=1, **extra):
    entry = {'code': code, 'name': name, 'pack_code': pack, 'type_code': type_code,
             'position': position, 'hidden': False}
    entry.update(extra)
    return entry


def pair(front, back):
    front['linked_to_code'] = back['code']
    back['hidden'] = True
    return [front, back]


def resolve(stem, cards, pack='core', folders=(), packs=()):
    catalog = rename.Catalog(cards, packs)
    entry = {'pack': pack, 'sets': [rename.squash(f) for f in folders],
             'info': rename.parse_name(stem)}
    found, _how = rename.resolve(entry, catalog, {})
    return [rename.output_name(c, entry['info'], catalog) for c in found]


class TestSquash:
    def test_an_apostrophe_written_as_underscore_collapses_to_the_same_thing(self):
        assert rename.squash("Captain America_s Shield") == rename.squash("Captain America's Shield")

    def test_a_slash_written_as_bangs_collapses_to_the_same_thing(self):
        assert rename.squash('SP!!dr') == rename.squash('SP//dr')


class TestParseName:
    def test_type_stage_number_and_side_are_read_out_of_the_title(self):
        info = rename.parse_name('4B_None Shall Pass_Main Scheme_1B')
        assert info['type'] == 'main_scheme'
        assert info['numbers'] == {1, 4}
        assert info['stages'] == {'1B', '4B'}
        assert info['faces'] == {'b'}
        assert info['fields'] == ['None Shall Pass']

    def test_a_copy_count_is_not_a_number(self):
        info = rename.parse_name('Spider-Man_Webbed Up_Upgrade_9_14.15')
        assert info['numbers'] == {9}

    def test_a_roman_numeral_is_a_stage_and_still_a_title_field(self):
        info = rename.parse_name("Rocket Raccoon_I_ve Got a Plan_Event_30")
        assert 'I' in info['stages']
        assert 'I' in info['fields']

    def test_a_side_written_with_its_stage(self):
        assert rename.parse_name('0_Rogue_Hero_Side 1B')['side'] == 'back'

    def test_a_type_joined_by_a_hyphen(self):
        info = rename.parse_name('3_All Tied Up-Attachment')
        assert info['type'] == 'attachment'
        assert info['fields'] == ['All Tied Up']

    def test_a_range_of_collector_numbers(self):
        info = rename.parse_name('Mister Sinister_Sinister Experiment_Main Scheme_140a-142a')
        assert info['range'] == (140, 142)
        assert info['faces'] == {'a'}

    def test_a_range_written_out(self):
        info = rename.parse_name("Attack on Xavier_s_Main Scheme_2A_BAck for cards 126 to 129")
        assert info['range'] == (126, 129)

    def test_an_aspect_colour_is_not_a_title_field(self):
        info = rename.parse_name('Red_Combat Training_Upgrade_57')
        assert info['aspect'] == 'aggression'
        assert info['fields'] == ['Combat Training']


class TestResolve:
    def test_the_type_picks_between_cards_of_one_title(self):
        cards = [card('01', 'Hawkeye', 'hero'), card('02', 'Hawkeye', 'ally')]
        assert resolve('Leadership_Hawkeye_Ally_12', cards) == ['02@core.jpg']

    def test_the_stage_picks_between_villain_stages(self):
        cards = [card(f'0{i}', 'Rhino', 'villain', stage=s) for i, s in enumerate(['I', 'II', 'III'])]
        assert resolve('2_Rhino_Stage II_Villain', cards) == ['01@core.jpg']

    def test_a_hidden_face_is_written_as_the_back_of_its_front(self):
        cards = pair(card('01001a', 'Spider-Man', 'hero'), card('01001b', 'Peter Parker', 'alter_ego'))
        assert resolve('Peter Parker_Spider-Man_Alter-Ego_1b', cards) == ['01001a@core~back.jpg']

    def test_a_side_outranks_the_title_it_was_written_with(self):
        cards = pair(card('04160a', 'Basic Attack Upgrade', 'upgrade'),
                     card('04160b', 'Improved Attack Upgrade', 'upgrade'))
        assert resolve('1B_Basic_Basic Attack Upgrade_Upgrade', cards) == ['04160a@core~back.jpg']

    def test_a_face_letter_outranks_a_type_copied_from_the_other_face(self):
        cards = pair(card('40190a', 'Assemble the Team', 'player_side_scheme'),
                     card('40190b', 'Team Assembled', 'environment'))
        assert resolve('Team Assembled_Environment_190a', cards) == ['40190a@core.jpg']

    def test_hero_and_alter_ego_outrank_a_face_letter(self):
        cards = pair(card('16029a', 'Rocket Raccoon', 'hero'),
                     card('16029b', 'Rocket Raccoon', 'alter_ego'))
        assert resolve('Rocket Raccoon_Rocket Raccoon_Alter-Ego_29a', cards) \
            == ['16029a@core~back.jpg']

    def test_the_linked_pair_is_preferred_over_the_plain_duplicate(self):
        cards = [card('01097', 'The Break-In!', 'main_scheme', stage='1')] + pair(
            card('01097a', 'The Break-In!', 'main_scheme', stage='1A'),
            card('01097b', 'The Break-In!', 'main_scheme', stage='1B'))
        assert resolve('4_The Break-In!_Main Scheme_1A', cards) == ['01097a@core.jpg']

    def test_a_misspelt_card_after_an_exact_hero_name(self):
        cards = [card('01', 'Hulk', 'hero'), card('06', 'Unstoppable Force', 'event', position=6)]
        assert resolve('Hulk_Unstappable Force_6_9.15', cards) == ['06@core.jpg']

    def test_the_closest_spelling_over_every_field_wins(self):
        cards = [card('01', 'Protector', 'ally'), card('32', 'Defensive Stance', 'upgrade')]
        assert resolve('Protection_Defensive Stage_Upgrade_32', cards) == ['32@core.jpg']

    def test_type_and_number_place_a_title_too_misspelt_to_find(self):
        cards = [card('04082', 'Rocky Outcrop', 'environment', set_position=7,
                      card_set_name='Absorbing Man'),
                 card('04081', 'Snowy Hillside', 'environment', set_position=6,
                      card_set_name='Absorbing Man')]
        assert resolve('7_Rocky Outpost_Environment', cards, folders=['Absorbing Man']) \
            == ['04082@core.jpg']

    def test_a_shared_scan_is_written_once_per_card_in_its_range(self):
        cards = []
        for n in (140, 141, 142):
            cards += pair(card(f'40{n}a', f'Scheme {n}', 'main_scheme', position=n, stage='2A'),
                          card(f'40{n}b', f'Scheme {n}', 'main_scheme', position=n, stage='2B'))
        assert sorted(resolve('Mister Sinister_Sinister Experiment_Main Scheme_140a-142a', cards)) \
            == ['40140a@core.jpg', '40141a@core.jpg', '40142a@core.jpg']

    def test_a_field_naming_a_pack_picks_that_printing(self):
        cards = [card('06032', 'Teamwork', pack='thor'), card('33017', 'Teamwork', pack='cyclops')]
        packs = [{'code': 'thor', 'name': 'Thor'}, {'code': 'cyclops', 'name': 'Cyclops'}]
        assert resolve('Leadership_Teamwork_Event_Thor', cards, pack='*', packs=packs) \
            == ['06032@thor.jpg']


class TestHeroPack:
    def test_the_alter_ego_picks_between_heroes_of_one_name(self):
        cards = pair(card('01040a', 'Black Panther', 'hero'), card('01040b', "T'Challa", 'alter_ego')) \
            + pair(card('51001a', 'Black Panther', 'hero', pack='bp'),
                   card('51001b', 'Shuri', 'alter_ego', pack='bp'))
        assert rename.Catalog(cards).hero_pack('Shuri_Black Panther') == 'bp'

    def test_a_pack_named_after_the_hero_is_used_first(self):
        catalog = rename.Catalog([card('31001a', 'SP//dr Suit', 'hero', pack='spdr')],
                                 [{'code': 'spdr', 'name': 'SP//dr'}])
        assert catalog.hero_pack('Peni Parker_SP!!dr') == 'spdr'


class TestOtherSide:
    def test_a_front_turns_into_its_back_and_back_again(self):
        cards = pair(card('16091a', 'The Art of Evasion', 'main_scheme'),
                     card('16091b', 'The Art of Evasion', 'main_scheme'))
        catalog = rename.Catalog(cards)
        assert rename.other_side('16091a@core.jpg', cards[0], catalog) == '16091a@core~back.jpg'
        assert rename.other_side('16091a@core~back.jpg', cards[0], catalog) == '16091a@core.jpg'

    def test_a_single_sided_card_has_no_other_side(self):
        only = card('01009', 'Webbed Up', 'upgrade')
        assert rename.other_side('01009@core.jpg', only, rename.Catalog([only])) is None
