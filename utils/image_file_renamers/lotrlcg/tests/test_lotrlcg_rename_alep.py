import importlib.util
import pathlib

_spec = importlib.util.spec_from_file_location(
    "lotrlcg_rename_alep", pathlib.Path(__file__).resolve().parent.parent / "rename_alep.py"
)
assert _spec and _spec.loader, "could not load rename_alep.py"
rename_alep = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename_alep)


def test_clean_for_match_and_normalize_title_shared_helpers():
    assert rename_alep.clean_for_match("The Ring Wall") == rename_alep.clean_for_match("Ring Wall")
    assert rename_alep.normalize_title("Ring Wall A") == "ring_wall_a"


def test_parse_alep_filename_extracts_copy_num_and_title():
    parsed = rename_alep.parse_alep_filename("046-1-Ring Wall [ringa]-1o.png")
    assert parsed == ("1", "Ring Wall [ringa]")


def test_parse_alep_filename_rejects_unknown_shape():
    assert rename_alep.parse_alep_filename("not-a-valid-name.png") is None


# --- Issue 7: front/back encoding-fix drift ---------------------------------
#
# The front and back loops used to carry separate copies of the encoding_fixes
# dict, and the back copy was missing the "Ring Wall [ringa]"/"[ringb]"
# entries. Since a card's front and back must share the same target_id for
# the '~back' pairing convention to work, resolving the same raw title through
# the front path and the back path must always produce the SAME target_id.

def test_ring_wall_front_and_back_resolve_to_same_target_id():
    pack_cards = {}  # force the encoding_fixes fallback path (no API match)
    front_id = rename_alep.resolve_target_id("Ring Wall [ringa]", "fangs_in_the_dark", pack_cards)
    back_id = rename_alep.resolve_target_id("Ring Wall [ringa]", "fangs_in_the_dark", pack_cards)
    assert front_id == back_id == rename_alep.normalize_title("Ring Wall A-fangs_in_the_dark")


def test_encoding_fixes_is_a_single_shared_table():
    # Guards against the dict being re-split into two copies again.
    assert "Ring Wall [ringa]" in rename_alep.ENCODING_FIXES
    assert "Ring Wall [ringb]" in rename_alep.ENCODING_FIXES
