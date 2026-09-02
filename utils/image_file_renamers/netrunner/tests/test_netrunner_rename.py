import importlib.util
import pathlib

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "netrunner_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)


# --- canonical_part() ----------------------------------------------------

def test_no_part_and_an_explicit_front_are_both_the_front():
    assert rename.canonical_part(None) == ""
    assert rename.canonical_part("") == ""
    assert rename.canonical_part("front") == ""
    assert rename.canonical_part("front1") == ""


def test_the_first_back_loses_its_index():
    assert rename.canonical_part("back") == "back"
    assert rename.canonical_part("back1") == "back"


def test_higher_backs_keep_their_number():
    assert rename.canonical_part("back2") == "back2"
    assert rename.canonical_part("back3") == "back3"


def test_a_face_is_the_older_spelling_of_a_back():
    """face1 would have been the front, so face2 is the first back."""
    assert rename.canonical_part("face2") == "back"
    assert rename.canonical_part("face3") == "back2"
    assert rename.canonical_part("face4") == "back3"


# --- is_repeated_front() -------------------------------------------------

def test_a_further_front_is_a_repeat():
    assert rename.is_repeated_front("front2")
    assert rename.is_repeated_front("front3")


def test_the_only_front_is_not_a_repeat():
    for part in (None, "", "front", "front1", "back", "back2", "face2"):
        assert not rename.is_repeated_front(part), part


# --- build_proxynexus_name() ---------------------------------------------

def test_jinteki_biotechs_faces_become_backs():
    for part, expected in [("face2", "back"), ("face3", "back2"), ("face4", "back3")]:
        assert (
            rename.build_proxynexus_name("jinteki_biotech", None, "the_valley", part, "jpg")
            == f"jinteki_biotech@the_valley~{expected}.jpg"
        )
