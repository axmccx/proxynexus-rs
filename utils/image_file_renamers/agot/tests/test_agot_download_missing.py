import importlib.util
import pathlib
import sys

GAME_DIR = pathlib.Path(__file__).resolve().parent.parent

# download_missing.py does `import rename` for the catalog and id normalization.
# When it runs as a script Python supplies its own directory on sys.path; loading
# it by file path here does not, so put it there before exec_module.
sys.path.insert(0, str(GAME_DIR))

_spec = importlib.util.spec_from_file_location(
    "agot_download_missing", GAME_DIR / "download_missing.py"
)
assert _spec and _spec.loader, "could not load download_missing.py"
download_missing = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(download_missing)


CATALOG = [
    {"label": "A Clash of Kings", "pack_code": "Core", "name": "A Clash of Kings",
     "image_url": "https://thronesdb.com/images/cards/GT01_1.jpg"},
    {"label": "Melisandre (Core)", "pack_code": "Core", "name": "Melisandre",
     "image_url": "https://thronesdb.com/images/cards/GT01_47.jpg"},
    {"label": "Some Draft Card", "pack_code": "ToJ", "name": "Some Draft Card",
     "image_url": "https://thronesdb.com/images/cards/GT99_1.jpg"},
]


def test_missing_from_finds_the_gap(tmp_path):
    (tmp_path / "a_clash_of_kings@Core.jpg").write_bytes(b"x")
    gaps = download_missing.missing_from(str(tmp_path), CATALOG)
    assert [f for _, f in gaps] == ["melisandre__core_@Core.jpg"]


def test_missing_from_ignores_packs_the_folder_has_no_cards_from(tmp_path):
    """Scope is limited to packs already represented. Without that, filling an
    FFG rebuild would also try to pull down every fan pack and all 281 Tower of
    Joy draft cards."""
    (tmp_path / "a_clash_of_kings@Core.jpg").write_bytes(b"x")
    packs = {c["pack_code"] for c, _ in download_missing.missing_from(str(tmp_path), CATALOG)}
    assert "ToJ" not in packs


def test_missing_from_is_empty_once_the_gap_is_filled(tmp_path):
    """Re-running must be a no-op rather than re-downloading everything."""
    (tmp_path / "a_clash_of_kings@Core.jpg").write_bytes(b"x")
    (tmp_path / "melisandre__core_@Core.jpg").write_bytes(b"x")
    assert download_missing.missing_from(str(tmp_path), CATALOG) == []


def test_missing_from_treats_a_zero_byte_file_as_absent(tmp_path):
    """A card whose file is empty is not a card. Counting it as present would
    let an interrupted copy pass as done."""
    (tmp_path / "a_clash_of_kings@Core.jpg").write_bytes(b"x")
    (tmp_path / "melisandre__core_@Core.jpg").write_bytes(b"")
    gaps = [f for _, f in download_missing.missing_from(str(tmp_path), CATALOG)]
    assert gaps == ["melisandre__core_@Core.jpg"]


def test_missing_from_matches_a_bleed_card_to_its_catalog_entry(tmp_path):
    """A .bleed suffix is part of the filename but not of the card's identity,
    so a bled scan must not read as a missing card."""
    catalog = [{"label": "Iron Mines (R)", "pack_code": "R", "name": "Iron Mines",
                "image_url": "https://thronesdb.com/images/cards/GT00_8.jpg"}]
    (tmp_path / "iron_mines__r_@R.bleed.tif").write_bytes(b"x")
    assert download_missing.missing_from(str(tmp_path), catalog) == []


def test_missing_from_takes_the_extension_from_the_image_url(tmp_path):
    """The DotE pack is PNG on ThronesDB and JPEG elsewhere; writing the wrong
    extension would produce a file the collection builder can't decode."""
    catalog = [{"label": "Drogon", "pack_code": "DotE", "name": "Drogon",
                "image_url": "https://thronesdb.com/images/cards/GT44_2.png"}]
    (tmp_path / "rhaegal@DotE.png").write_bytes(b"x")
    assert [f for _, f in download_missing.missing_from(str(tmp_path), catalog)] == ["drogon@DotE.png"]
