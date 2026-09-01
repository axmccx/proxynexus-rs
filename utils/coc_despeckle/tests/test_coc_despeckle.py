import importlib.util
import pathlib

import cv2
import numpy as np
import pytest
from PIL import Image

# Load ../coc_despeckle.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "coc_despeckle", pathlib.Path(__file__).resolve().parent.parent / "coc_despeckle.py"
)
assert _spec and _spec.loader, "could not load coc_despeckle.py"
despeckle = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(despeckle)

SCREEN_PERIOD = 3.5      # px, the rosette on these scans


def screened(width=300, height=420, amplitude=26):
    """Flat paper carrying a print screen, with a hard edge across it."""
    y, x = np.mgrid[0:height, 0:width]
    screen = np.sin(2 * np.pi * x / SCREEN_PERIOD) * np.sin(2 * np.pi * y / SCREEN_PERIOD)
    paper = 210 + amplitude * screen
    paper[:, width // 2:] -= 90          # an edge, standing in for type
    return cv2.cvtColor(paper.clip(0, 255).astype(np.uint8), cv2.COLOR_GRAY2BGR)


def screen_energy(img):
    """How much of the screen is left, measured away from the edge."""
    g = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY).astype(float)
    flat = g[:, :g.shape[1] // 2 - 10]
    return float((flat - cv2.GaussianBlur(flat, (0, 0), 2.0)).std())


def edge_contrast(img):
    """How much of the hard edge survives."""
    g = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY).astype(float)
    mid = g.shape[1] // 2
    return float(g[:, mid - 25:mid - 5].mean() - g[:, mid + 5:mid + 25].mean())


# --- the screen ------------------------------------------------------------

def test_the_screen_is_reduced():
    src = screened()
    out = despeckle.despeckle(src)
    assert screen_energy(out) < screen_energy(src) / 2


def test_the_edge_survives_the_averaging():
    src = screened()
    out = despeckle.despeckle(src)
    assert edge_contrast(out) > edge_contrast(src) * 0.9


def test_a_higher_strength_averages_harder():
    src = screened()
    assert screen_energy(despeckle.despeckle(src, 14)) < \
           screen_energy(despeckle.despeckle(src, 4))


# --- the resample ----------------------------------------------------------

def test_the_long_side_lands_on_the_target():
    out = despeckle.resample(screened(300, 420), long_side=210)
    assert max(out.shape[:2]) == 210


def test_the_aspect_is_kept():
    src = screened(300, 420)
    out = despeckle.resample(src, long_side=210)
    assert out.shape[1] / out.shape[0] == pytest.approx(300 / 420, abs=0.01)


def test_the_long_side_is_the_long_one_whichever_way_up():
    portrait = despeckle.resample(screened(300, 420), long_side=210)
    landscape = despeckle.resample(screened(420, 300), long_side=210)
    assert portrait.shape[0] == 210 and landscape.shape[1] == 210


def test_an_image_already_smaller_than_the_target_is_left_alone():
    src = screened(300, 420)
    assert despeckle.resample(src, long_side=900).shape == src.shape


def test_resampling_by_area_does_not_alias_the_screen_into_moire():
    """A sampling resize would fold the screen into a coarse beat. Averaging
    over the whole box is what stops that."""
    src = screened()
    area = despeckle.resample(src, long_side=210)
    naive = cv2.resize(src, (150, 210), interpolation=cv2.INTER_NEAREST)
    assert screen_energy(area) < screen_energy(naive)


# --- writing ---------------------------------------------------------------

def test_a_jpeg_is_written_without_chroma_subsampling(tmp_path):
    path = str(tmp_path / 'card.jpg')
    despeckle.save(screened(), path)
    with Image.open(path) as img:
        from PIL import JpegImagePlugin
        assert JpegImagePlugin.get_sampling(img) == 0, 'small type wants 4:4:4'


def test_a_png_stays_a_png(tmp_path):
    path = str(tmp_path / 'card.png')
    despeckle.save(screened(), path)
    with Image.open(path) as img:
        assert img.format == 'PNG'


# --- running in parallel ---------------------------------------------------

def test_working_in_parallel_gives_the_same_result_as_one_at_a_time(tmp_path):
    """The pool is there for speed only; nothing about a card depends on the
    others, so the output must not depend on how many run at once."""
    import subprocess, sys, os
    src = tmp_path / 'in'
    src.mkdir()
    for i in range(4):
        despeckle.save(screened(180, 250), str(src / f'card{i}.jpg'))

    script = pathlib.Path(__file__).resolve().parent.parent / 'coc_despeckle.py'
    outs = []
    for jobs in (1, 4):
        out = tmp_path / f'out{jobs}'
        subprocess.run([sys.executable, str(script), str(src), '-o', str(out),
                        '--jobs', str(jobs)], check=True, capture_output=True)
        outs.append(sorted((p.name, p.read_bytes()) for p in out.iterdir()))
    assert outs[0] == outs[1]


# --- the resolution the collection is built at -----------------------------

def test_the_default_resamples_to_mpcs_300dpi_sheet():
    """Proxy Nexus never downscales -- it only scales an undersized image up --
    so an image handed to it larger than the cut line reaches MPC oversized and
    is resampled there, by a filter nobody here chose. Downscaling at this step
    is what keeps that decision. 1038 is the height Proxy Nexus lays a card out
    at inside MPC's 816x1110 sheet."""
    assert despeckle.TARGET_LONG_SIDE == 1038
    out = despeckle.process(screened(1439, 2034))
    assert out.shape[0] == 1038


def test_the_resample_can_be_turned_off_for_an_archival_copy():
    src = screened(1439, 2034)
    out = despeckle.resample(src, long_side=max(src.shape[:2]))
    assert out.shape == src.shape
