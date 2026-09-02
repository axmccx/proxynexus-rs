import importlib.util
import pathlib

import cv2
import numpy as np
import pytest
from PIL import Image

# Load ../lotr_despeckle.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "lotr_despeckle", pathlib.Path(__file__).resolve().parent.parent / "lotr_despeckle.py"
)
assert _spec and _spec.loader, "could not load lotr_despeckle.py"
despeckle = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(despeckle)

SCREEN_PERIOD = 3.5      # px, the rosette these four carry at 1468x2080


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
    assert screen_energy(out) < screen_energy(src)


def test_the_denoise_takes_more_off_than_the_resample_alone():
    """The two passes are a pair. The resample takes the screen down on its own,
    and the denoise is what the default strength is chosen against, so measure
    it where it is applied: on top of the resample."""
    src = screened(1468, 2080)
    resampled = despeckle.resample(src, long_side=1038)
    both = despeckle.process(src, long_side=1038)
    assert screen_energy(both) < screen_energy(resampled)


def test_the_edge_survives_the_averaging():
    src = screened()
    out = despeckle.despeckle(src)
    assert edge_contrast(out) > edge_contrast(src) * 0.9


def test_a_higher_strength_averages_harder():
    src = screened()
    assert screen_energy(despeckle.despeckle(src, 8)) < \
           screen_energy(despeckle.despeckle(src, 2))


def test_the_edge_survives_every_strength_the_tool_offers():
    """Edge energy was flat across 3 to 8 on the real cards; the strength is
    chosen on the screen alone, which only holds while the type is untouched."""
    src = screened()
    for strength in (3, 5, 8):
        assert edge_contrast(despeckle.despeckle(src, strength)) > \
               edge_contrast(src) * 0.9


def test_the_default_strength_actually_moves_a_crisp_screen():
    """The failure this guards is a setting that reads well on a soft screen and
    does nothing to a crisp one, which is what strength 3 alone did here."""
    src = screened()
    assert screen_energy(despeckle.process(src)) < screen_energy(src) * 0.5


# --- the notch -------------------------------------------------------------

def test_the_notch_takes_a_periodic_screen_out():
    """A halftone is periodic and stands as sharp spikes in the spectrum, which
    picture detail does not. This is what does most of the work."""
    src = screened()
    assert screen_energy(despeckle.descreen(src)) < screen_energy(src) * 0.5


def test_the_notch_beats_non_local_means_alone_on_a_crisp_screen():
    """Non-local means preserves a regular rosette, because it averages from
    patches that look alike and a screen is the most self-similar thing there
    is. The notch is the tool that suits this; the ordering is the finding."""
    src = screened()
    assert screen_energy(despeckle.descreen(src)) < \
           screen_energy(despeckle.despeckle(src, 8))


def test_the_notch_leaves_a_hard_edge_alone():
    src = screened()
    assert edge_contrast(despeckle.descreen(src)) > edge_contrast(src) * 0.9


def test_the_notch_correction_is_bounded():
    """Unbounded, the notch rings against a hard edge and the overshoot clips --
    on the real cards that put black speckle along the metal ornament, and 1.8%
    of pixels clipped. No pixel may move further than the limit."""
    src = screened()
    moved = np.abs(despeckle.descreen(src).astype(int) - src.astype(int))
    assert moved.max() <= despeckle.CORRECTION_LIMIT + 1


def test_a_tighter_limit_moves_pixels_less():
    src = screened()
    assert np.abs(despeckle.descreen(src, limit=4).astype(int) - src.astype(int)).max() <= 5


def test_an_image_with_no_screen_survives_the_notch():
    """Nothing periodic to find means nothing should be taken out."""
    rng = np.random.default_rng(0)
    flat = np.full((256, 256, 3), 180, np.uint8)
    flat = np.clip(flat + rng.normal(0, 3, flat.shape), 0, 255).astype(np.uint8)
    out = despeckle.descreen(flat)
    assert np.abs(out.astype(int) - flat.astype(int)).mean() < 2.0


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


def test_neither_order_costs_the_type_anything():
    """The Call of Cthulhu pass denoises first because shrinking first was
    measurably softer there. That does not reproduce on these four -- the two
    orders came within 0.25% of each other on the type -- so the order is kept
    for consistency, not for a gain, and the test records that rather than
    asserting a difference that is not there."""
    src = screened(1468, 2080)
    denoise_first = despeckle.process(src, long_side=1038)
    shrink_first = despeckle.despeckle(despeckle.resample(src, long_side=1038))
    assert edge_contrast(denoise_first) == pytest.approx(
        edge_contrast(shrink_first), rel=0.02)


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


# --- the resolution these four are kept at ---------------------------------

def test_the_default_leaves_the_resolution_alone():
    """The rest of lotrlcg sits at about 578dpi and these arrive at 592dpi.
    Downscaling only these to MPC's 300dpi cut line, the way the Call of Cthulhu
    pass does for a collection built that way, would make them the one set of
    cards at half everyone else's resolution."""
    assert despeckle.TARGET_LONG_SIDE == 0

    src = screened(1468, 2080)
    assert despeckle.process(src).shape[:2] == src.shape[:2]


def test_a_long_side_can_still_be_asked_for():
    out = despeckle.resample(screened(1468, 2080), long_side=1038)
    assert out.shape[0] == 1038
    assert out.shape[1] / out.shape[0] == pytest.approx(1468 / 2080, abs=0.001)


# --- the bleed images in the same folder -----------------------------------

def test_a_bleed_copy_is_told_apart_by_name():
    """`lotrlcg-gap-fills/` also holds cards copied out of lotrlcg-enhanced,
    which came from the Enhanced Proxies and were descreened upstream."""
    assert despeckle.is_not_for_this_pass(
        'race_to_rivendell_tfotr@the_fellowship_of_the_ring.bleed.jpg')
    assert despeckle.is_not_for_this_pass(
        'race_to_rivendell_tfotr@the_fellowship_of_the_ring~back.bleed.jpg')
    assert not despeckle.is_not_for_this_pass(
        'abandoned_camp_tdom@the_dark_of_mirkwood.jpg')


def test_a_bleed_copy_is_skipped(tmp_path):
    import subprocess, sys
    src = tmp_path / 'in'
    src.mkdir()
    despeckle.save(screened(400, 560), str(src / 'plain@pack.jpg'))
    despeckle.save(screened(400, 560), str(src / 'bled@pack.bleed.jpg'))

    script = pathlib.Path(__file__).resolve().parent.parent / 'lotr_despeckle.py'
    out = tmp_path / 'out'
    subprocess.run([sys.executable, str(script), str(src), '-o', str(out)],
                   check=True, capture_output=True)

    assert [p.name for p in out.iterdir()] == ['plain@pack.jpg']


def test_the_size_summary_is_right_when_writing_over_the_input(tmp_path):
    """-o may be the folder being read from, which is how the four in
    `lotrlcg-gap-fills/` are replaced. The source size has to be taken before
    the write or the summary reports the output against itself."""
    import subprocess, sys, re
    folder = tmp_path / 'cards'
    folder.mkdir()
    despeckle.save(screened(1468, 2080), str(folder / 'card@pack.jpg'))
    before = (folder / 'card@pack.jpg').stat().st_size

    script = pathlib.Path(__file__).resolve().parent.parent / 'lotr_despeckle.py'
    done = subprocess.run([sys.executable, str(script), str(folder), '-o', str(folder),
                           '--long-side', '520'],
                          check=True, capture_output=True, text=True)

    after = (folder / 'card@pack.jpg').stat().st_size
    assert after < before, 'the resample should have shrunk it'
    reported = re.search(r'Size: (\d+)MB -> (\d+)MB', done.stdout)
    assert reported, done.stdout
    assert int(reported.group(1)) == round(before / 1e6)


def test_a_zero_long_side_leaves_the_image_alone_rather_than_collapsing_it():
    """`resample` is called with the module default by `process`, so a zero that
    only `main` knew how to handle would shrink a card to one pixel."""
    src = screened(300, 420)
    assert despeckle.resample(src, long_side=0).shape == src.shape
