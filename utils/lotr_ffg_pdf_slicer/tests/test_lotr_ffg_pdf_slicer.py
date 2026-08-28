import importlib.util
import pathlib

# Load ../lotr_ffg_pdf_slicer.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "lotr_ffg_pdf_slicer",
    pathlib.Path(__file__).resolve().parent.parent / "lotr_ffg_pdf_slicer.py",
)
assert _spec and _spec.loader, "could not load lotr_ffg_pdf_slicer.py"
slicer = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(slicer)

CARD = slicer.CARD
STOCK_BACK = slicer.STOCK_BACK
NOTICE = slicer.NOTICE
FRONT = slicer.FRONT
BACK = slicer.BACK


def roles_from(shape):
    """Build a page->role map from a compact string: c=card, s=stock back, n=notice."""
    letters = {"c": CARD, "s": STOCK_BACK, "n": NOTICE}
    return {page: letters[letter] for page, letter in enumerate(shape, start=1)}


# --- COLLECTOR_NUMBER ------------------------------------------------------


def test_collector_number_matches_printed_numbers():
    for text in ["1", "44", "129", "158a", "158b"]:
        assert slicer.COLLECTOR_NUMBER.match(text), text


def test_collector_number_rejects_anything_longer():
    for text in ["1234", "12ab", "Setup", "+1", "129."]:
        assert not slicer.COLLECTOR_NUMBER.match(text), text


# --- pair_faces ------------------------------------------------------------


def test_front_first_file_pairs_each_front_with_the_page_after_it():
    faces, note = slicer.pair_faces(roles_from("cscscs"))
    assert note == "backs come after their fronts"
    assert faces == {
        1: (FRONT, 2),
        2: (BACK, 1),
        3: (FRONT, 4),
        4: (BACK, 3),
        5: (FRONT, 6),
        6: (BACK, 5),
    }


def test_back_first_file_pairs_each_front_with_the_page_before_it():
    faces, note = slicer.pair_faces(roles_from("scsc"))
    assert note == "backs come before their fronts"
    assert faces == {1: (BACK, 2), 2: (FRONT, 1), 3: (BACK, 4), 4: (FRONT, 3)}


def test_a_card_page_on_the_back_parity_is_a_cards_own_back():
    # A quest back carries text, so it reads as a card; its position is what
    # makes it a back.
    faces, _ = slicer.pair_faces(roles_from("cccs"))
    assert faces[1] == (FRONT, 2)
    assert faces[2] == (BACK, 1)
    assert faces[3] == (FRONT, 4)


def test_the_notice_page_is_dropped_and_does_not_shift_the_parity():
    trailing = slicer.pair_faces(roles_from("cscsn"))[0]
    leading = slicer.pair_faces(roles_from("ncscs"))[0]
    assert trailing[1] == (FRONT, 2) and 5 not in trailing
    assert leading[2] == (FRONT, 3) and 1 not in leading


def test_a_file_with_no_stock_back_is_read_as_single_sided_fronts():
    faces, note = slicer.pair_faces(roles_from("nccccc"))
    assert note.startswith("no stock backs")
    assert faces == {page: (FRONT, None) for page in range(2, 7)}


def test_stock_backs_on_both_parities_turn_pairing_off():
    faces, note = slicer.pair_faces(roles_from("cscss"))
    assert note.startswith("[WARN]")
    assert all(side == FRONT and partner is None for side, partner in faces.values())


def test_a_front_with_no_page_after_it_has_no_back():
    faces, _ = slicer.pair_faces(roles_from("cscsc"))
    assert faces[5] == (FRONT, None)


# --- signatures_match ------------------------------------------------------


def test_signatures_a_few_levels_apart_are_the_same_card():
    one = bytes(range(256))
    other = bytes(min(255, value + slicer.SIGNATURE_TOLERANCE) for value in one)
    assert slicer.signatures_match(one, other)


def test_signatures_further_apart_are_different_cards():
    one = bytes(range(256))
    other = bytearray(one)
    other[100] = one[100] + slicer.SIGNATURE_TOLERANCE + 1
    assert not slicer.signatures_match(one, bytes(other))


def test_a_face_with_a_back_never_matches_one_without():
    one = bytes(range(256))
    assert not slicer.signatures_match(one, None)
    assert not slicer.signatures_match(None, one)
    assert slicer.signatures_match(None, None)
