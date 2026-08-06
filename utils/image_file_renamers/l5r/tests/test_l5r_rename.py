import ast
import importlib.util
import pathlib
from collections import Counter

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "l5r_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)

GAME_DIR = pathlib.Path(__file__).resolve().parent.parent


def _typo_fixes_source_keys():
    """The TYPO_FIXES keys as literally written in the source.

    Read from the AST rather than the imported dict, because Python collapses
    duplicate keys on the way in and that is exactly what we're looking for.
    """
    tree = ast.parse((GAME_DIR / "rename.py").read_text(encoding="utf-8"))
    for node in ast.walk(tree):
        if (
            isinstance(node, ast.Assign)
            and isinstance(node.value, ast.Dict)
            and any(getattr(t, "id", None) == "TYPO_FIXES" for t in node.targets)
        ):
            return [k.value for k in node.value.keys if isinstance(k, ast.Constant)]
    raise AssertionError("TYPO_FIXES assignment not found in rename.py")


def test_typo_fixes_has_no_duplicate_keys():
    """Three keys were once defined twice, and Python kept only the last.

    The shadowed entries were dead for as long as they existed, which no
    amount of reading the dict catches.
    """
    dupes = {k: n for k, n in Counter(_typo_fixes_source_keys()).items() if n > 1}
    assert dupes == {}, f"TYPO_FIXES defines the same key twice: {dupes}"


def test_card_name_is_taken_after_the_last_metadata_marker():
    """The FFG scans put the card name last but vary what precedes it."""
    for base_name, expected in [
        ("Phoenix_D_9_Kaito Temple Protector", "Kaito Temple Protector"),
        ("127_Neutral_D_SuddenTempest", "SuddenTempest"),
        ("Children of the Empire_Neutral_C_80_Stay Your Hand", "Stay Your Hand"),
        ("The Core Set_Dragon_D_56_Agasha Swordsmith", "Agasha Swordsmith"),
    ]:
        card_name, _, _ = rename.split_prefix_and_name(base_name)
        assert card_name == expected, base_name


def test_both_side_b_spellings_are_detected():
    """"Side B" on the role cards, "B side" on the Shadowlands warlords.

    Matching only one spelling makes those cards overwrite each other.
    """
    for base_name, expected in [
        ("newRole Card_Side B_214_Keeper Of Air", True),
        ("Shadowlands_Warlord_B side_1_Akuma no Oni", True),
        ("newRole Card_Side A_214_Keeper Of Air", False),
        ("Shadowlands_Warlord_A side_1_Akuma no Oni", False),
    ]:
        _, _, is_side_b = rename.split_prefix_and_name(base_name)
        assert is_side_b is expected, base_name


def test_side_b_marker_does_not_fire_on_card_names_containing_side():
    """Roadside Inn and Countryside Trader must not read as B sides."""
    for base_name in ("Crab_D_29_Roadside Inn", "Unicorn_C_22_Countryside Trader"):
        _, _, is_side_b = rename.split_prefix_and_name(base_name)
        assert is_side_b is False, base_name


def test_side_b_selects_the_other_card_not_a_back_face():
    """Both sides of these scans are named after the A-side card.

    Side B of a role card is the Seeker, not the back of the Keeper; side B of
    a warlord is the challenge version, not the back of the cooperative one.
    """
    assert rename.side_b_card_id("keeper-of-air") == "seeker-of-air"
    assert rename.side_b_card_id("akuma-no-oni-coop") == "akuma-no-oni-challenge"
    assert rename.side_b_card_id("fine-katana") is None


def test_numbers_in_name_come_from_the_whole_filename():
    """Documented, not fixed: this is tuned behaviour, don't "correct" it.

    Positions are matched against every digit run anywhere in the filename, so
    a number embedded in a set name can spuriously select a printing.
    """
    _, numbers, _ = rename.split_prefix_and_name("Set3_Crane_D_56_Agasha Swordsmith")
    assert numbers == ["3", "56"]
