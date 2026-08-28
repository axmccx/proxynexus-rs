import importlib.util
import pathlib

from PIL import Image

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "netrunner_reboot_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)


def sheet(columns, rows):
    """A face sheet with each face a distinct solid colour, in reading order."""
    face_w, face_h = rename.FACE_SIZE
    img = Image.new("RGB", (columns * face_w, rows * face_h))
    for y in range(rows):
        for x in range(columns):
            index = y * columns + x
            img.paste((index + 1, 0, 0),
                      (x * face_w, y * face_h, (x + 1) * face_w, (y + 1) * face_h))
    return img


# --- normalize_title() ---------------------------------------------------

def test_normalize_title_lowercases_and_underscores_punctuation():
    assert rename.normalize_title("SYNC: Everything, Everywhere") == (
        "sync__everything__everywhere"
    )


def test_normalize_title_transliterates_accents():
    """Card ids come from this, and the Rust side runs the text through
    deunicode first. reteki's catalog has genuinely accented titles, so the two
    sides disagreeing here would silently misname real cards."""
    assert rename.normalize_title("Déjà Vu") == "deja_vu"
    assert rename.normalize_title("Tori Hanzō") == "tori_hanzo"


def test_normalize_title_transliterates_letters_nfkd_leaves_alone():
    """NFKD decomposes accents but passes these through unchanged, so they need
    the explicit table or they'd normalize to '_'."""
    assert rename.normalize_title("Ærø") == "aero"


# --- face_grid() ---------------------------------------------------------

def test_face_grid_plain_card_is_one_face():
    assert rename.face_grid(rename.FACE_SIZE) == (1, 1)


def test_face_grid_finds_a_side_by_side_flip_card():
    assert rename.face_grid((3440, 2400)) == (2, 1)


def test_face_grid_finds_a_two_by_two_sheet():
    """Jinteki Biotech ships all four of its forms in one image."""
    assert rename.face_grid((3440, 4800)) == (2, 2)


def test_face_grid_treats_an_undersized_image_as_one_face():
    """One draft identity is served at 508x709. Integer division would make
    that zero faces, and it has to come back as an ordinary single card."""
    assert rename.face_grid((508, 709)) == (1, 1)


def test_face_grid_requires_an_exact_multiple():
    """Only an exact tiling of FACE_SIZE is a sheet. Anything else is a card at
    an unexpected resolution, and cutting it would slice through the art."""
    assert rename.face_grid((3441, 2400)) == (1, 1)
    assert rename.face_grid((2600, 2400)) == (1, 1)


def test_face_grid_uses_the_table_for_a_shrunk_sheet():
    """Project Genesis holds four half-size faces in one card-sized image, so
    measurement reads it as an ordinary card and only the code gives it away."""
    assert rename.face_grid(rename.FACE_SIZE, '54019') == (2, 2)
    assert rename.face_grid(rename.FACE_SIZE, '01001') == (1, 1)


# --- sheet_contribution() ------------------------------------------------

def test_sheet_contribution_keeps_every_face_by_default():
    assert rename.sheet_contribution('09001', ['a', 'b'], ['', '~back']) == (
        ['a', 'b'], ['', '~back'])


def test_sheet_contribution_drops_a_front_the_media_host_renders_better():
    """Hype's front is stale in the mirror's sheet, so the sheet supplies only
    the back and the front is fetched separately."""
    assert rename.sheet_contribution('53024', ['a', 'b'], ['', '~back']) == (['b'], ['~back'])


# --- part_names() --------------------------------------------------------

def test_part_names_single_face_has_no_suffix():
    assert rename.part_names(1) == ['']


def test_part_names_two_faces_are_front_and_back():
    assert rename.part_names(2) == ['', '~back']


def test_part_names_beyond_two_are_numbered_backs():
    """A printing has one front, so Jinteki Biotech's three further faces are
    the backs of three physical cards that share it."""
    assert rename.part_names(4) == ['', '~back', '~back2', '~back3']


# --- faces() -------------------------------------------------------------

def test_faces_cuts_a_sheet_in_reading_order():
    """Left to right, then top to bottom. The part suffixes are assigned by
    position, so a transposed order would label the wrong face as the front."""
    cut = rename.faces(sheet(2, 2), (2, 2))
    assert [face.size for face in cut] == [rename.FACE_SIZE] * 4
    assert [face.getpixel((0, 0))[0] for face in cut] == [1, 2, 3, 4]


def test_faces_cuts_a_shrunk_sheet_into_equal_parts():
    """A shrunk sheet's cells are a fraction of FACE_SIZE rather than equal to
    it, so the cut has to divide the image rather than step through it."""
    cut = rename.faces(Image.new("RGB", rename.FACE_SIZE), (2, 2))
    assert [face.size for face in cut] == [(860, 1200)] * 4


def test_faces_returns_a_plain_card_untouched():
    img = sheet(1, 1)
    assert rename.faces(img, (1, 1)) == [img]


# --- output_name() -------------------------------------------------------

def test_output_name_joins_id_printing_and_part():
    card = {'title': 'Hedge Fund', 'pack_code': 'core'}
    assert rename.output_name(card, 'core', '', '.jpg') == "hedge_fund@core.jpg"
    assert rename.output_name(card, 'core', '~back', '.jpg') == "hedge_fund@core~back.jpg"


def test_output_name_takes_an_alt_art_label_as_the_printing():
    """Alt arts are a printing label rather than a pack, so the same card can
    appear under both without the two names colliding."""
    card = {'title': 'Hedge Fund', 'pack_code': 'core'}
    assert rename.output_name(card, 'alt', '', '.jpg') == "hedge_fund@alt.jpg"


# --- alt_arts() ----------------------------------------------------------

def test_alt_arts_reads_label_and_image_name_from_the_data():
    """Both the label and the image's own name come from the client data. Every
    entry reteki has today is {"alt": "<code>-alt"}, but deriving either from
    the card code would file a differently-named one wrongly."""
    assert rename.alt_arts([
        {'code': '01001', 'alt_art': {'alt': '01001-alt'}},
        {'code': '01003', 'alt_art': {'promo': 'special-thing'}},
        {'code': '01005'},
    ]) == [
        {'stem': '01001-alt', 'code': '01001', 'label': 'alt'},
        {'stem': 'special-thing', 'code': '01003', 'label': 'promo'},
    ]


def test_alt_arts_handles_a_card_with_several():
    arts = rename.alt_arts([{'code': '01001', 'alt_art': {'alt': '01001-alt',
                                                          'alt2': '01001-alt2'}}])
    assert [art['label'] for art in arts] == ['alt', 'alt2']


# --- image_kind() --------------------------------------------------------

def test_image_kind_names_the_format_not_the_extension(tmp_path):
    """reteki serves the alt arts as JPEG from a .png path. Trusting the URL
    would write filenames claiming a format the bytes aren't."""
    path = tmp_path / "01001-alt.png"
    Image.new("RGB", (60, 80)).save(path, format="JPEG")
    assert rename.image_kind(str(path)) == ('.jpg', (60, 80))


def test_image_kind_rejects_a_file_that_is_not_an_image(tmp_path):
    path = tmp_path / "truncated.jpg"
    path.write_bytes(b"")
    assert rename.image_kind(str(path)) is None


# --- image_url_template() ------------------------------------------------

def test_image_url_template_upgrades_to_https():
    """reteki advertises the template over plain HTTP."""
    assert rename.image_url_template(
        {'imageUrlTemplate': 'http://nrdb.reteki.fun/card_image/large/{code}.jpg'}
    ) == 'https://nrdb.reteki.fun/card_image/large/{code}.jpg'


def test_image_url_template_falls_back_when_absent():
    assert rename.image_url_template({}) == rename.FALLBACK_IMAGE_URL


# --- SOURCE_NAME ---------------------------------------------------------

def test_source_name_matches_a_bare_card_code():
    match = rename.SOURCE_NAME.match("01001")
    assert match.group('code') == "01001"
    assert match.group('variant') is None


def test_source_name_matches_an_alt_art_name():
    """The variant only marks the file as needing an alt art lookup; the label
    written into the filename comes from the catalog, not from this."""
    match = rename.SOURCE_NAME.match("01001-alt")
    assert (match.group('code'), match.group('variant')) == ("01001", "alt")


def test_source_name_rejects_anything_else():
    """Unrecognised names are reported rather than silently ignored, which the
    legacy Netrunner renamer got wrong."""
    assert rename.SOURCE_NAME.match("01001_alt1") is None
    assert rename.SOURCE_NAME.match("hedge_fund@core") is None
