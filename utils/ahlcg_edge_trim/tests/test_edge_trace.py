"""Strip detection against images built with a known ground width."""
import os, sys
import numpy as np
import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from edge_trace import strip_depth, trim_box, EDGES

W, H = 750, 1050
rng = np.random.default_rng(0)


def card(colour=(150, 160, 170), texture=6):
    """A plain card with a little print texture, so no edge is perfectly flat."""
    img = np.zeros((H, W, 3), np.uint8)
    img[:] = colour
    noise = rng.normal(0, texture, (H, W, 3))
    return np.clip(img.astype(np.float32) + noise, 0, 255).astype(np.uint8)


def with_strip(img, edge, depth, colour=(0, 0, 0)):
    img = img.copy()
    if edge == "top":
        img[:depth] = colour
    elif edge == "bottom":
        img[-depth:] = colour
    elif edge == "left":
        img[:, :depth] = colour
    else:
        img[:, -depth:] = colour
    return img


@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize("depth", [1, 2, 3, 4])
def test_finds_a_black_ground_on_any_edge(edge, depth):
    assert strip_depth(with_strip(card(), edge, depth), edge) == depth


@pytest.mark.parametrize("edge", EDGES)
def test_finds_a_white_ground(edge):
    dark = card(colour=(30, 30, 30))
    assert strip_depth(with_strip(dark, edge, 2, colour=(255, 255, 255)), edge) == 2


def test_leaves_a_clean_card_alone():
    assert trim_box(card()) == {e: 0 for e in EDGES}


def test_leaves_a_dark_card_alone():
    """The failure that matters: a card whose own border is a solid dark line is not ground."""
    assert trim_box(card(colour=(20, 22, 24))) == {e: 0 for e in EDGES}


def test_leaves_a_flat_border_alone():
    """A border that is one colour all the way in has no boundary, so nothing to cut."""
    img = card(colour=(120, 120, 120), texture=2)
    assert trim_box(img) == {e: 0 for e in EDGES}


def test_ignores_a_gradient_running_to_the_edge():
    """Art that ramps is not a strip: it has no abrupt, edge-wide boundary."""
    img = card()
    ramp = np.linspace(0.15, 1.0, 40)[None, :, None]
    img[:, :40] = (img[:, :40] * ramp).astype(np.uint8)
    assert strip_depth(img, "left") == 0


def test_only_the_edge_that_has_one_is_cut():
    box = trim_box(with_strip(card(), "left", 3))
    assert box == {"left": 3, "top": 0, "bottom": 0, "right": 0}


def test_ground_wider_than_the_cap_is_capped():
    """Damage is bounded even when the measurement is wrong."""
    assert strip_depth(with_strip(card(), "left", 40), "left") <= int(W * 0.008)


def test_corner_wedges_do_not_count_as_an_edge_strip():
    """corner_infill's job; sampling skips the ends of each edge so they cannot trigger one."""
    img = card()
    n = int(W * 0.10)
    for ys, xs in ((slice(0, n), slice(0, n)), (slice(0, n), slice(W - n, W)),
                   (slice(H - n, H), slice(0, n)), (slice(H - n, H), slice(W - n, W))):
        img[ys, xs] = 0
    assert trim_box(img) == {e: 0 for e in EDGES}


def test_scales_with_resolution():
    """The cap is a fraction of the width, so twice the pixels allows twice the cut."""
    big = np.repeat(np.repeat(with_strip(card(), "left", 4), 2, axis=0), 2, axis=1)
    assert strip_depth(big, "left") == 8
