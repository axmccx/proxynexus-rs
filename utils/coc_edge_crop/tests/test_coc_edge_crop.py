import importlib.util
import pathlib

import numpy as np
import pytest
from PIL import Image

# Load ../coc_edge_crop.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "coc_edge_crop", pathlib.Path(__file__).resolve().parent.parent / "coc_edge_crop.py"
)
assert _spec and _spec.loader, "could not load coc_edge_crop.py"
edge_crop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(edge_crop)


def scan(width, height, rim_x=0, rim_y=None, border=240, ground=20, art=60):
    """A card: a bright border around darker art, with `rim` pixels of scanner
    ground around the outside."""
    rim_y = rim_x if rim_y is None else rim_y
    img = np.full((height, width, 3), border, dtype=np.uint8)
    inset_x, inset_y = int(width * 0.2), int(height * 0.2)
    img[inset_y:height - inset_y, inset_x:width - inset_x] = art

    if rim_y:
        img[:rim_y, :] = ground
        img[height - rim_y:, :] = ground
    if rim_x:
        img[:, :rim_x] = ground
        img[:, width - rim_x:] = ground

    return Image.fromarray(img)


def cropped(img):
    box = edge_crop.crop_box(img)
    return None if box is None else img.crop(box)


def edges(img):
    """The outermost ring, minus the corners the infill owns."""
    a = np.asarray(img.convert('L')).astype(int)
    h, w = a.shape
    return np.concatenate([a[0, int(w * .1):int(w * .9)], a[-1, int(w * .1):int(w * .9)],
                           a[int(h * .1):int(h * .9), 0], a[int(h * .1):int(h * .9), -1]])


# --- what gets trimmed -----------------------------------------------------

def test_the_rim_is_gone_from_every_edge():
    out = cropped(scan(740, 1040, rim_x=6))
    assert out is not None
    assert edges(out).min() >= edge_crop.BORDER_CUTOFF


def test_a_scan_with_no_rim_is_barely_touched():
    src = scan(740, 1040, rim_x=0)
    box = edge_crop.crop_box(src)
    assert box is None or box[0] <= edge_crop.MARGIN


def test_a_deeper_rim_costs_more():
    shallow = edge_crop.crop_box(scan(740, 1040, rim_x=4))
    deep = edge_crop.crop_box(scan(740, 1040, rim_x=20))
    assert deep[0] > shallow[0]


@pytest.mark.parametrize('width,height', [(740, 1040), (1040, 740)])
def test_each_axis_is_cropped_by_what_its_own_rim_needs(width, height):
    """A single depth scaled to both axes leaves rim on the long edges of a
    landscape scan, which is what this guards."""
    out = cropped(scan(width, height, rim_x=3, rim_y=18))
    assert edges(out).min() >= edge_crop.BORDER_CUTOFF

    out = cropped(scan(width, height, rim_x=18, rim_y=3))
    assert edges(out).min() >= edge_crop.BORDER_CUTOFF


@pytest.mark.parametrize('width,height', [(740, 1040), (1040, 740)])
def test_cropping_leaves_the_aspect_where_it_found_it(width, height):
    src = scan(width, height, rim_x=4, rim_y=16)
    out = cropped(src)
    assert out.width / out.height == pytest.approx(src.width / src.height, abs=0.002)


# --- what gets left alone --------------------------------------------------

def test_a_card_whose_border_is_as_dark_as_the_ground_is_left_alone():
    # A full-bleed promo: nothing distinguishes its edge from the ground, and
    # its bleed is meant to be dark.
    assert edge_crop.crop_box(scan(740, 1040, rim_x=6, border=15, art=15)) is None


def test_the_rounded_corners_are_not_measured_as_rim():
    # The ground runs deep into the corners by design; only the straight part
    # of each edge says how deep the rim is.
    src = scan(740, 1040, rim_x=4)
    a = np.asarray(src).copy()
    for y, x in ((0, 0), (0, 740 - 60), (1040 - 60, 0), (1040 - 60, 740 - 60)):
        a[y:y + 60, x:x + 60] = 20
    box = edge_crop.crop_box(Image.fromarray(a))
    assert box[0] <= 4 + edge_crop.MARGIN


# --- writing ---------------------------------------------------------------

def test_a_jpeg_is_written_back_with_its_own_encoding(tmp_path):
    path = tmp_path / 'card.jpg'
    scan(740, 1040, rim_x=6).save(path, quality=83, subsampling=2)

    with Image.open(path) as img:
        img.load()
        settings = edge_crop.save_settings(img)
        assert settings['format'] == 'JPEG'
        assert settings['qtables'] == img.quantization
