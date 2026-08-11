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
    position_str, text_name, is_back = rename.parse_filename("001 - Aragorn")
    assert position_str == "001"
    assert text_name == "Aragorn"
    assert is_back is False


def test_parse_filename_lettered_position_suffix():
    position_str, text_name, is_back = rename.parse_filename("047a - A Perilous Voyage")
    assert position_str == "047a"
    assert text_name == "A Perilous Voyage"
    assert is_back is False


def test_parse_filename_secondary_position_in_name():
    # '011 - 1B - The Hunt Begins' -> the secondary '1B -' overrides position_str
    # and marks it a back face.
    position_str, text_name, is_back = rename.parse_filename("011 - 1B - The Hunt Begins")
    assert position_str == "1B"
    assert text_name == "The Hunt Begins"
    assert is_back is True


def test_parse_filename_dash_dash_form_normalized():
    # '- - 2A - Lost in the Swanfleet' is normalized to '000 - 2A - ...' first.
    position_str, text_name, is_back = rename.parse_filename("- - 2A - Lost in the Swanfleet")
    assert position_str == "2A"
    assert text_name == "Lost in the Swanfleet"
    assert is_back is False


# --- back-face detection -----------------------------------------------------

def test_parse_filename_back_suffix_letters():
    for letter in ("B", "D", "F", "H"):
        _, _, is_back = rename.parse_filename(f"012{letter} - Some Card")
        assert is_back is True, f"letter {letter} should mark a back face"


def test_parse_filename_side_b_marks_back():
    _, text_name, is_back = rename.parse_filename("012 - Some Card (side b)")
    assert is_back is True
    assert "(side b)" not in text_name.lower()


def test_parse_filename_side_a_does_not_mark_back():
    _, text_name, is_back = rename.parse_filename("012 - Some Card (side a)")
    assert is_back is False
    assert "(side a)" not in text_name.lower()


def test_parse_filename_reverse_marks_back():
    _, text_name, is_back = rename.parse_filename("012 - Some Card reverse")
    assert is_back is True
    assert "reverse" not in text_name.lower()


# --- (errata) stripping and trailing dedup-number stripping ------------------

def test_parse_filename_strips_errata_suffix():
    _, text_name, _ = rename.parse_filename("012 - Some Card (errata)")
    assert text_name == "Some Card"


def test_parse_filename_strips_trailing_dedup_number():
    _, text_name, _ = rename.parse_filename("012 - Dark Pools 3")
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
