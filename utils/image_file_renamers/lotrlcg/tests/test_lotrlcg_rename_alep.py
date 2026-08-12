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


# --- Mangled non-ASCII titles ------------------------------------------------
#
# The archives replace each non-ASCII character with one non-alphanumeric
# placeholder, which clean_for_match then drops along with the real separators.
# "Írensaga" and "Smeóhbrand Rogue of Orthanc" shipped in lotrlcg-alep as
# _rensaga / sme_hbrand_rogue_of_orthanc: no ENCODING_FIXES entry existed, so
# resolve_target_id fabricated an id from the mangled spelling and it matched
# no catalog card. The wildcard pass places these without a per-card entry.

def _pack_cards(*names):
    return {
        rename_alep.clean_for_match(n): [
            {"name": n, "target_id": rename_alep.normalize_title(f"{n}-mustering_of_the_rohirrim")}
        ]
        for n in names
    }


def test_mangled_leading_character_resolves_to_catalog_card():
    pack_cards = _pack_cards("Írensaga", "Herubrand")
    for mangled in (" rensaga", "?rensaga", "_rensaga"):
        assert rename_alep.resolve_target_id(
            mangled, "mustering_of_the_rohirrim", pack_cards
        ) == "irensaga_mustering_of_the_rohirrim"


def test_mangled_interior_character_resolves_to_catalog_card():
    pack_cards = _pack_cards("Smeóhbrand Rogue of Orthanc", "Herubrand")
    for mangled in ("Sme hbrand Rogue of Orthanc", "Sme?hbrand Rogue of Orthanc"):
        assert rename_alep.resolve_target_id(
            mangled, "mustering_of_the_rohirrim", pack_cards
        ) == "smeohbrand_rogue_of_orthanc_mustering_of_the_rohirrim"


def test_wildcard_pass_leaves_leading_the_titles_matchable():
    # clean_for_match strips a leading "the"; the pattern must too, or every
    # "The ..." title stops matching its own key.
    pack_cards = _pack_cards("The Gríma Problem")
    assert rename_alep.resolve_target_id(
        "The Gr ma Problem", "mustering_of_the_rohirrim", pack_cards
    ) == rename_alep.normalize_title("The Gríma Problem-mustering_of_the_rohirrim")


def test_ambiguous_wildcard_match_is_refused():
    # "ring wall a"/"ring wall b" differ only where the placeholder sits, so the
    # fragment is too lossy to place; it must not silently pick one.
    pack_cards = _pack_cards("Ring Wall A", "Ring Wall B")
    assert rename_alep.find_wildcard_match("Ring Wall ", pack_cards) is None


def test_unmatched_title_is_logged(capsys):
    rename_alep.resolve_target_id(
        "Totally Not A Card", "mustering_of_the_rohirrim", _pack_cards("Herubrand")
    )
    assert "[UNMATCHED]" in capsys.readouterr().out


def test_empty_pack_catalog_does_not_log_unmatched(capsys):
    # A pack absent from the catalog (e.g. a release the APIs don't carry yet)
    # takes the ENCODING_FIXES path for every card; that is expected, not a gap.
    rename_alep.resolve_target_id("Ring Wall [ringa]", "fangs_in_the_dark", {})
    assert "[UNMATCHED]" not in capsys.readouterr().out
