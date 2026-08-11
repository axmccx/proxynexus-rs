import ast
import importlib.util
import pathlib
from collections import Counter

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "agot_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)

GAME_DIR = pathlib.Path(__file__).resolve().parent.parent


# --- normalize_title() ---------------------------------------------------

def test_normalize_title_lowercases_and_underscores_punctuation():
    assert rename.normalize_title("The Kingsroad") == "the_kingsroad"


def test_normalize_title_non_alnum_becomes_underscore():
    assert rename.normalize_title("Beneath the Gold, the Bitter Steel") == (
        "beneath_the_gold__the_bitter_steel"
    )


def test_normalize_title_transliterates_accents():
    """Card ids come from this, and the Rust side runs the text through
    deunicode first. ThronesDB's catalog is pure ASCII today, so nothing
    exercises this in practice -- but if the two sides stop agreeing on
    accented names the ids diverge silently at collection-build time."""
    assert rename.normalize_title("Löthar Frey") == "lothar_frey"


def test_normalize_title_transliterates_letters_nfkd_leaves_alone():
    """NFKD decomposes accents but passes these through unchanged, so they
    need the explicit table or they'd normalize to '_'."""
    assert rename.normalize_title("Ærø") == "aero"


# --- clean_for_match() ----------------------------------------------------

def test_clean_for_match_strips_punctuation_and_underscores():
    assert rename.clean_for_match("Bear_Island Scout!", {}) == "bearislandscout"


def test_clean_for_match_strips_leading_the():
    assert rename.clean_for_match("The Kingsroad", {}) == "kingsroad"


def test_clean_for_match_applies_the_table_it_is_given():
    """The two archives carry separate tables. Passing the wrong one has to be
    impossible to do silently, so the table is an argument rather than a
    module-level global the function reaches for."""
    assert rename.clean_for_match("Rhaelgal", rename.FFG_TYPO_FIXES) == "rhaegal"
    assert rename.clean_for_match("Bonifer", rename.COMMUNITY_TYPO_FIXES) == "boniferthegood"
    # Each table is inert against the other archive's names.
    assert rename.clean_for_match("Rhaelgal", rename.COMMUNITY_TYPO_FIXES) == "rhaelgal"


# --- TYPO_FIXES table integrity -------------------------------------------

def _typo_fixes_source_keys(name):
    """The table's keys as literally written in the source.

    Read from the AST rather than the imported dict, because Python collapses
    duplicate keys on the way in and that is exactly what we're looking for.
    """
    tree = ast.parse((GAME_DIR / "rename.py").read_text(encoding="utf-8"))
    for node in ast.walk(tree):
        if (isinstance(node, ast.Assign) and isinstance(node.value, ast.Dict)
                and any(getattr(t, "id", None) == name for t in node.targets)):
            return [k.value for k in node.value.keys if isinstance(k, ast.Constant)]
    raise AssertionError(f"{name} assignment not found in rename.py")


def test_typo_fixes_have_no_duplicate_keys():
    """A key written twice is silently dropped, leaving a fix that reads as
    present but never fires."""
    for name in ("FFG_TYPO_FIXES", "COMMUNITY_TYPO_FIXES"):
        dupes = [k for k, n in Counter(_typo_fixes_source_keys(name)).items() if n > 1]
        assert dupes == [], f"duplicate keys in {name}: {dupes}"


def test_typo_fixes_have_no_noop_entries():
    """corncorncorn / familydutyhonor / beneaththegoldthebittersteel used to
    map to themselves, doing nothing. None of the three source names start
    with "the", so they were never at risk from the leading-"the" stripping in
    clean_for_match() either -- they were plain no-ops."""
    for name in ("FFG_TYPO_FIXES", "COMMUNITY_TYPO_FIXES"):
        table = getattr(rename, name)
        noops = [k for k, v in table.items() if k == v]
        assert noops == [], f"no-op entries in {name}: {noops}"


# --- archive dispatch -----------------------------------------------------

def test_classify_dir_separates_the_two_archives():
    """FFG pack folders are numbered, agot.cards folders are not. Checked
    against both archives in full: all 45 FFG folders and none of the 9 fan
    folders match."""
    for name in ("00_core", "23_tak", "44_dote"):
        assert rename.classify_dir(name) == "ffg"
    for name in ("AHAH_TIFF_ENG", "R_R_TIFF_ENG", "TSOW_TIFF_ENG"):
        assert rename.classify_dir(name) == "community"


# --- FFG resolution -------------------------------------------------------

def test_resolve_ffg_handles_numbered_filenames():
    """The DotE pack names its scans '039.LysaArryn' rather than 'Lysa Arryn'."""
    card = {"label": "Lysa Arryn"}
    lookup = {"lysaarryn": card}
    assert rename.resolve_ffg("039.LysaArryn", lookup, "TAK")[0] is card
    assert rename.resolve_ffg("Lysa Arryn", lookup, "TAK")[0] is card


def test_resolve_ffg_reports_a_reason_when_unmatched():
    card, pack, reason = rename.resolve_ffg("Not A Card", {}, "TAK")
    assert card is None and pack is None
    assert "not found in pack TAK" in reason


# --- fan-pack filename parsing --------------------------------------------

def test_strip_community_filename_leading_index():
    assert rename.strip_community_filename("01_Gendry") == "Gendry"


def test_strip_community_filename_leading_index_and_language_suffix():
    assert rename.strip_community_filename("1_Bonifer_ENG") == "Bonifer"


def test_strip_community_filename_multi_word_name():
    assert rename.strip_community_filename("07_Raider_from_Pyke") == "Raider from Pyke"


# --- pack code resolution -------------------------------------------------

PACK_CODE_MAP = {"ahah": "AHaH", "tsow": "TSoW", "ftr": "FtR",
                 "btb": "BtB", "chos": "ChoS", "hmw": "HMW", "r": "R"}


def test_resolve_pack_guess_remaps_cos_to_chos():
    assert rename.resolve_pack_guess("CoS_SomeFaction", PACK_CODE_MAP) == "ChoS"


def test_resolve_pack_guess_recovers_thronesdb_letter_case():
    """Folder prefixes are upper-cased on disk but ThronesDB pack codes are
    mixed-case, so a direct comparison never matched for AHAH/BTB/FTR/TSOW.
    Those files fell through to "first match wins", which filed The Balerion
    under FH instead of AHaH and Mace Tyrell under R instead of TSoW."""
    assert rename.resolve_pack_guess("AHAH_TIFF_ENG", PACK_CODE_MAP) == "AHaH"
    assert rename.resolve_pack_guess("TSOW_TIFF_ENG", PACK_CODE_MAP) == "TSoW"


def test_resolve_pack_guess_passes_through_unknown_packs():
    assert rename.resolve_pack_guess("HMW_Baratheon", PACK_CODE_MAP) == "HMW"
    assert rename.resolve_pack_guess("ZZZ_Nothing", PACK_CODE_MAP) == "ZZZ"


# --- collision detection --------------------------------------------------

def test_check_collision_flags_two_sources_writing_same_destination():
    """This guard lived in the FFG renamer and not the fan-pack one back when
    they were separate scripts, which is how 20 files came to overwrite each
    other silently."""
    seen = {}
    assert rename.check_collision(seen, "iron_mines__r_@R.tif", "R_R/08.tif") is None
    prior = rename.check_collision(seen, "iron_mines__r_@R.tif", "Redesigns/08.tif")
    assert prior == "R_R/08.tif"


def test_check_collision_same_source_is_not_a_collision():
    seen = {}
    rename.check_collision(seen, "iron_mines__r_@R.tif", "R_R/08.tif")
    assert rename.check_collision(seen, "iron_mines__r_@R.tif", "R_R/08.tif") is None


# --- image dimensions, bleed detection, corrupt-scan guard ----------------

def _tiff_bytes(width, height, order="little"):
    """A header-only TIFF. image_size() reads the first IFD and stops."""
    endian = b"II\x2a\x00" if order == "little" else b"MM\x00\x2a"

    def num(value, size):
        return value.to_bytes(size, order)

    def entry(tag, value):
        # type 3 = SHORT, count 1, value left-packed into the 4-byte field.
        return num(tag, 2) + num(3, 2) + num(1, 4) + num(value, 2) + b"\x00\x00"

    return (endian + num(8, 4) + num(2, 2)
            + entry(256, width) + entry(257, height) + num(0, 4))


def _png_bytes(width, height):
    return (b"\x89PNG\r\n\x1a\n" + (13).to_bytes(4, "big") + b"IHDR"
            + width.to_bytes(4, "big") + height.to_bytes(4, "big"))


def test_image_size_reads_tiff_both_byte_orders(tmp_path):
    for order in ("little", "big"):
        path = tmp_path / f"{order}.tif"
        path.write_bytes(_tiff_bytes(822, 1122, order))
        assert rename.image_size(str(path)) == (822, 1122)


def test_image_size_reads_png(tmp_path):
    path = tmp_path / "a.png"
    path.write_bytes(_png_bytes(750, 1050))
    assert rename.image_size(str(path)) == (750, 1050)


def test_image_size_is_none_for_zero_byte_file(tmp_path):
    """The FFG archive ships 02_trtw/Syrio Forel.jpg as 0 bytes. Copied through
    unchecked it becomes a card the collection builder can't read, so a missing
    size is what makes the renamer skip it."""
    path = tmp_path / "Syrio Forel.jpg"
    path.write_bytes(b"")
    assert rename.image_size(str(path)) is None


def test_image_size_is_none_for_non_image(tmp_path):
    path = tmp_path / "truncated.jpg"
    path.write_bytes(b"not an image at all")
    assert rename.image_size(str(path)) is None


def test_has_bleed_is_orientation_independent():
    """The eight HMW plot scans are landscape. Comparing dimensions in order
    would read them as un-bled and drop the .bleed suffix from exactly the
    cards that also need rotating."""
    assert rename.has_bleed((822, 1122))
    assert rename.has_bleed((1122, 822))


def test_output_name_parses_ids_packs_and_bleed():
    for name, expect in [
        ("iron_mines__r_@R.bleed.jpg", ("iron_mines__r_", "R", "jpg")),
        ("a_clash_of_kings@Core.jpg", ("a_clash_of_kings", "Core", "jpg")),
        ("_hands_of_gold_@PoS.jpg", ("_hands_of_gold_", "PoS", "jpg")),
        ("the_prince_who_came_too_late@DotE.png", ("the_prince_who_came_too_late", "DotE", "png")),
    ]:
        m = rename.OUTPUT_NAME.match(name)
        assert m, name
        assert (m.group('id'), m.group('pack'), m.group('ext')) == expect


def test_has_bleed_false_for_trimmed_and_ffg_scans():
    assert not rename.has_bleed((750, 1050))
    # The FFG archive is 492x699 and must never pick up a .bleed suffix.
    assert not rename.has_bleed((492, 699))
    assert not rename.has_bleed(None)
