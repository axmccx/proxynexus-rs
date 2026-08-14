import importlib.util
import pathlib

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "lotrlcg_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)


# --- clean_for_match / normalize_title -------------------------------------

def test_clean_for_match_strips_leading_the():
    assert rename.clean_for_match("The Hunt for Gollum") == rename.clean_for_match("Hunt for Gollum")


def test_clean_for_match_lowercases_and_strips_punctuation():
    assert rename.clean_for_match("Gwahir's Debt") == "gwahirsdebt"


def test_clean_for_match_transliterates_unicode():
    assert rename.clean_for_match("Khazad-dûm") == rename.clean_for_match("Khazad-dum")


def test_normalize_title_replaces_non_alnum_with_underscore():
    assert rename.normalize_title("A Perilous Voyage!") == "a_perilous_voyage_"


# --- parse_filename: position / name splitting ------------------------------

def test_parse_filename_simple_numbered_card():
    position_str, stage_str, text_name, is_back = rename.parse_filename("001 - Aragorn")
    assert position_str == "001"
    assert stage_str is None
    assert text_name == "Aragorn"
    assert is_back is False


def test_parse_filename_lettered_position_suffix():
    position_str, stage_str, text_name, is_back = rename.parse_filename("047a - A Perilous Voyage")
    assert position_str == "047a"
    assert text_name == "A Perilous Voyage"
    assert is_back is False


def test_parse_filename_keeps_card_number_and_stage_apart():
    # '011 - 1B - The Hunt Begins' carries both. The card number must survive:
    # folding the stage over it is what loses same-titled cards.
    position_str, stage_str, text_name, is_back = rename.parse_filename("011 - 1B - The Hunt Begins")
    assert position_str == "011"
    assert stage_str == "1B"
    assert text_name == "The Hunt Begins"
    assert is_back is True


def test_parse_filename_dash_dash_form_normalized():
    # '- - 2A - Lost in the Swanfleet' is normalized to '000 - 2A - ...' first.
    position_str, stage_str, text_name, is_back = rename.parse_filename("- - 2A - Lost in the Swanfleet")
    assert position_str == "000"
    assert stage_str == "2A"
    assert text_name == "Lost in the Swanfleet"
    assert is_back is False


# --- back-face detection -----------------------------------------------------

def test_parse_filename_back_suffix_letters():
    for letter in ("B", "D", "F", "H"):
        _, _, _, is_back = rename.parse_filename(f"012{letter} - Some Card")
        assert is_back is True, f"letter {letter} should mark a back face"


def test_parse_filename_side_b_marks_back():
    _, _, text_name, is_back = rename.parse_filename("012 - Some Card (side b)")
    assert is_back is True
    assert "(side b)" not in text_name.lower()


def test_parse_filename_side_a_does_not_mark_back():
    _, _, text_name, is_back = rename.parse_filename("012 - Some Card (side a)")
    assert is_back is False
    assert "(side a)" not in text_name.lower()


def test_parse_filename_reverse_marks_back():
    _, _, text_name, is_back = rename.parse_filename("012 - Some Card reverse")
    assert is_back is True
    assert "reverse" not in text_name.lower()


# --- (errata) stripping and trailing dedup-number stripping ------------------

def test_parse_filename_strips_errata_suffix():
    _, _, text_name, _ = rename.parse_filename("012 - Some Card (errata)")
    assert text_name == "Some Card"


def test_parse_filename_strips_trailing_dedup_number():
    _, _, text_name, _ = rename.parse_filename("012 - Dark Pools 3")
    assert text_name == "Dark Pools"


# --- PACK_TITLE_FIXES_CLEAN: pack-scoped, does not leak across packs ---------

def test_pack_title_fix_applies_only_within_its_own_pack():
    key_in_pack = ("The Black Riders", rename.clean_for_match("Bill Fenry"))
    assert rename.PACK_TITLE_FIXES_CLEAN[key_in_pack] == "Bill Ferny"

    # Same cleaned name, different pack: must NOT be present / must not resolve.
    other_pack_key = ("Some Other Pack", rename.clean_for_match("Bill Fenry"))
    assert other_pack_key not in rename.PACK_TITLE_FIXES_CLEAN


def test_pack_title_fix_does_not_leak_to_correctly_spelled_card_elsewhere():
    # "Mordor Wargs" is a deliberate HoB-matching misspelling fix scoped to
    # "The Sands of Harad". A card legitimately titled "Mordor Wargs" in a
    # different pack must resolve via .get() with a default to itself,
    # unaffected by the pack-scoped table.
    clean_name = rename.clean_for_match("Mordor Wargs")
    fixed = rename.PACK_TITLE_FIXES_CLEAN.get(("A Totally Different Pack", clean_name), clean_name)
    assert fixed == clean_name


# --- Issue 4: orphaned-back validation must be per-folder --------------------

def test_back_with_front_in_other_folder_is_reported_as_orphaned():
    folder_file_lists = {
        "lotrlcg-enhanced": ["aragorn@core.jpg"],
        "lotrlcg-nightmare": ["aragorn@core~back.jpg"],
    }
    orphans = rename.find_orphaned_backs(folder_file_lists)
    assert "lotrlcg-nightmare" in orphans
    assert "aragorn@core~back.jpg" in orphans["lotrlcg-nightmare"]


def test_back_with_front_in_same_folder_is_not_orphaned():
    folder_file_lists = {
        "lotrlcg-enhanced": ["aragorn@core.jpg", "aragorn@core~back.jpg"],
        "lotrlcg-nightmare": [],
    }
    orphans = rename.find_orphaned_backs(folder_file_lists)
    assert orphans == {}


# --- DQ4: '026 -1A' loses the space before the stage code --------------------

def test_parse_filename_stage_glued_to_card_number():
    # The Mumakil ships '026 -1A - Welcome to the Jungle.jpg'. The missing space
    # hid the stage code, so the 1B scan was written as a second front and
    # deduped away, leaving the card with no back.
    position_str, stage_str, text_name, is_back = rename.parse_filename("026 -1A - Welcome to the Jungle")
    assert position_str == "026"
    assert stage_str == "1A"
    assert text_name == "Welcome to the Jungle"
    assert is_back is False

    _, stage_str, _, is_back = rename.parse_filename("026 -1B - Welcome to the Jungle")
    assert stage_str == "1B"
    assert is_back is True


# --- DQ5: C/D-side quest cards must keep their own number --------------------

def test_parse_filename_c_and_d_sides():
    # Race Across Harad prints 'Setting Out' twice, at 1A/1B and again at 1C/1D.
    position_str, stage_str, _, is_back = rename.parse_filename("051 - 1C - Setting Out")
    assert position_str == "051"
    assert stage_str == "1C"
    assert is_back is False

    position_str, stage_str, _, is_back = rename.parse_filename("051 - 1D - Setting Out")
    assert position_str == "051"
    assert stage_str == "1D"
    assert is_back is True


# --- card_stage: Hall of Beorn's printed stage code --------------------------

def test_card_stage_reads_each_face():
    card = {
        "Front": {"Stats": {"StageNumber": "1C"}},
        "Back": {"Stats": {"StageNumber": "1D", "QuestPoints": "10"}},
    }
    assert rename.card_stage(card, is_back=False) == "1C"
    assert rename.card_stage(card, is_back=True) == "1D"


def test_card_stage_none_for_single_sided_and_unstaged():
    assert rename.card_stage({"Front": {"Stats": {}}, "Back": None}, is_back=True) is None
    assert rename.card_stage({"Front": {"Stats": {}}, "Back": None}, is_back=False) is None


# --- DQ6: promo cards filed under another product's set ----------------------

def _card(slug, card_set, rdb):
    return {"Slug": slug, "CardSet": card_set, "RingsDbCardId": rdb}


def test_find_promo_slugs_flags_foreign_pack_prefix():
    cards = [
        _card("Standard-Game-Mode-TSoA", "The Siege of Annuminas", "00001"),
        _card("Rebuild-the-Defenses-TSoA", "The Siege of Annuminas", "00003"),
        _card("Defend-the-City-TSoA", "The Siege of Annuminas", "00004"),
        _card("Lead-the-Sortie-TSoA", "The Siege of Annuminas", "00005"),
        _card("Faramir-TSoA", "The Siege of Annuminas", "06081"),
        _card("Boromir-TSoA", "The Siege of Annuminas", "02095"),
    ]
    assert rename.find_promo_slugs(cards) == {"Faramir-TSoA", "Boromir-TSoA"}


def test_find_promo_slugs_leaves_reprint_collections_alone():
    # Hero expansions are collections of reprints with no dominant prefix, so the
    # rule must not fire and strip the whole product.
    cards = [
        _card("Boromir-DoG", "Defenders of Gondor", "01001"),
        _card("Faramir-Ally-DoG", "Defenders of Gondor", "02010"),
        _card("Faramir-Hero-DoG", "Defenders of Gondor", "06028"),
        _card("Mablung-DoG", "Defenders of Gondor", "13003"),
    ]
    assert rename.find_promo_slugs(cards) == set()


def test_ringsdb_pack_prefix_drops_the_card_number():
    assert rename.ringsdb_pack_prefix({"RingsDbCardId": "02095"}) == "02"
    assert rename.ringsdb_pack_prefix({"RingsDbCardId": "148020"}) == "148"
    assert rename.ringsdb_pack_prefix({"RingsDbCardId": ""}) is None
    assert rename.ringsdb_pack_prefix({}) is None


# --- DQ1: backs must not be written onto single-sided cards ------------------

def test_has_split_sibling_detects_hob_two_entry_cards():
    # The Hunt for the Dreadnaught prints Eithiliant as one double-sided card,
    # 6a / 6b, but Hall of Beorn stores two entries with Back: null on both.
    basic = {"Title": "Eithiliant", "position": 6, "Slug": "Eithiliant-THftD", "Back": None}
    upgraded = {"Title": "Eithiliant", "position": 6, "Slug": "Eithiliant-Upgraded-THftD", "Back": None}
    pack_cards = {"eithiliant": [basic, upgraded]}
    assert rename.has_split_sibling(pack_cards, basic) is True
    assert rename.has_split_sibling(pack_cards, upgraded) is True


def test_has_split_sibling_false_for_a_lone_single_sided_card():
    # The Massing at Osgiliath's treachery is genuinely single-sided, so the
    # product back-cover scan must not be written as its back.
    treachery = {"Title": "Massing at Osgiliath", "position": 14,
                 "Slug": "Massing-at-Osgiliath-TMaO", "Back": None}
    other = {"Title": "Cut Off", "position": 12, "Slug": "Cut-Off-TMaO", "Back": None}
    pack_cards = {"massingatosgiliath": [treachery], "cutoff": [other]}
    assert rename.has_split_sibling(pack_cards, treachery) is False


def test_has_split_sibling_false_when_titles_match_but_numbers_differ():
    # Race Across Harad prints 'Setting Out' twice, at 47 and 51. Different
    # cards, not two faces of one.
    a = {"Title": "Setting Out", "position": 47, "Slug": "Setting-Out-RaH", "Back": {}}
    c = {"Title": "Setting Out", "position": 51, "Slug": "Setting-Out-C-RaH", "Back": {}}
    pack_cards = {"settingout": [a, c]}
    assert rename.has_split_sibling(pack_cards, a) is False
