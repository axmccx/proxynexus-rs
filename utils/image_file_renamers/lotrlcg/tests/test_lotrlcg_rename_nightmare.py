import importlib.util
import pathlib

_spec = importlib.util.spec_from_file_location(
    "lotrlcg_rename_nightmare",
    pathlib.Path(__file__).resolve().parent.parent / "rename_nightmare.py",
)
assert _spec and _spec.loader, "could not load rename_nightmare.py"
nm = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(nm)


def card(number, title, slug, card_set="Passage Through Mirkwood Nightmare"):
    return {"Number": number, "Title": title, "Slug": slug, "CardSet": card_set}


# --- manifest parsing --------------------------------------------------------

MANIFEST = """Card List
01 - Core Set : Shadows of Mirkwood - Nightmare -- 01 - Passage Through Mirkwood

Every card, in the order it prints. Each card takes two pages: the front,
then its back. "encounter back" is the shared card back.

==============================================================================
01 - Passage Through Mirkwood
==============================================================================

    10 different cards, 21 to print, 42 printed sides
    19 with the shared encounter back, 2 double-sided

    Qty  Type            Front                           Back
    ---------------------------------------------------------
      1  Pack Cover      000 - Passage Through Mirkwood  000 - Introduction
      1  Nightmare Mode  001 - Passage Through Mirkwood  001 - Setup
      3  Enemy           003 - Ungoliant's Brood         encounter back
"""


def test_parse_manifest_reads_the_four_columns():
    rows = nm.parse_manifest(MANIFEST)
    assert rows == [
        (1, "Pack Cover", "000 - Passage Through Mirkwood", "000 - Introduction"),
        (1, "Nightmare Mode", "001 - Passage Through Mirkwood", "001 - Setup"),
        (3, "Enemy", "003 - Ungoliant's Brood", "encounter back"),
    ]


def test_parse_manifest_ignores_prose_headers_and_rules():
    # The summary lines ("10 different cards, 21 to print, ...") and the "-----"
    # rule sit in the same block as the rows and must not be read as cards.
    assert all(r[1] != "Type" for r in nm.parse_manifest(MANIFEST))
    assert not any(r[2].startswith("-") for r in nm.parse_manifest(MANIFEST))


# --- card references ---------------------------------------------------------

def test_parse_card_ref_numbered():
    assert nm.parse_card_ref("004 - Jagged Cavern") == (4, "Jagged Cavern", False)


def test_parse_card_ref_pack_cover_is_position_zero():
    # Position 0 is the pack cover, which Hall of Beorn does not list. It is
    # dropped rather than written; see EXTRA_CARDS.md.
    number, _title, _reverse = nm.parse_card_ref("000 - Passage Through Mirkwood")
    assert number == 0


def test_parse_card_ref_quest_front_is_not_a_reverse():
    assert nm.parse_card_ref("008 - 2A - Through the Marsh") == (8, "Through the Marsh", False)


def test_parse_card_ref_quest_back_is_a_reverse():
    assert nm.parse_card_ref("008 - 2B - An Arduous Journey") == (8, "An Arduous Journey", True)


def test_parse_card_ref_shared_back_has_no_number():
    # "Lost Island" backs seven different locations in The Fate of Númenor and
    # is not a card of its own.
    assert nm.parse_card_ref("Lost Island") == (None, "Lost Island", False)


def test_parse_card_ref_unnumbered_quest_front():
    # Flight from Moria prints one 2A face on three cards; the front carries no
    # number, so the card has to be identified by the reverse it pairs with.
    assert nm.parse_card_ref("2A - Search for an Exit") == (None, "Search for an Exit", False)


# --- copy suffixes and scan keys ---------------------------------------------

def test_strip_copy_suffix_splits_the_em_dash_form():
    assert nm.strip_copy_suffix("003 - Forest Flies — 06 of 20") == ("003 - Forest Flies", 6)


def test_strip_copy_suffix_defaults_to_copy_one():
    assert nm.strip_copy_suffix("001 - Setup") == ("001 - Setup", 1)


def test_scan_key_matches_apostrophe_to_underscore():
    # The manifest writes "001 - Shelob's Lair"; the file is "001 - Shelob_s Lair.jpg".
    assert nm.scan_key("001 - Shelob's Lair") == nm.scan_key("001 - Shelob_s Lair")


def test_scan_key_strips_errata_qualifier():
    # The City of Corsairs' Patrol Ship scan carries "(errata)", the manifest does not.
    assert nm.scan_key("002 - Patrol Ship (errata)") == nm.scan_key("002 - Patrol Ship")


def test_scan_key_keeps_position_so_shared_titles_stay_distinct():
    assert nm.scan_key("002 - Cursed Temple") != nm.scan_key("004 - Cursed Temple")


def test_index_scans_groups_copies_in_order():
    index = nm.index_scans([
        "003 - Forest Flies — 08 of 20.jpg",
        "003 - Forest Flies — 06 of 20.jpg",
        "003 - Forest Flies — 07 of 20.jpg",
        "Card list.txt",
    ])
    assert list(index) == [nm.scan_key("003 - Forest Flies")]
    copies = index[nm.scan_key("003 - Forest Flies")]
    assert [c for c, _ in copies] == [6, 7, 8]
    assert copies[0][1] == "003 - Forest Flies — 06 of 20.jpg"


def test_index_scans_ignores_non_images():
    assert nm.index_scans(["Card list.txt", "notes.md"]) == {}


# --- card resolution ---------------------------------------------------------

def test_pick_card_prefers_position_when_hall_of_beorn_has_a_typo():
    # Hall of Beorn stores the Road to Rivendell enemy as "Gobline Trapper".
    # The scan says "Goblin Trapper" and position 4 is authoritative.
    by_number = {4: card(4, "Gobline Trapper", "Gobline-Trapper-RtRN")}
    picked, how = nm.pick_card(4, "Goblin Trapper", by_number, {})
    assert picked["Slug"] == "Gobline-Trapper-RtRN"
    assert how == "position-mismatch"


def test_pick_card_prefers_title_when_hall_of_beorn_transposed_the_numbers():
    # Hall of Beorn has Intruders in Chetwood #5 and #6 swapped; the printed
    # cards read 5 = Outskirts of Archet, 6 = Greenway Path.
    greenway = card(5, "Greenway Path", "Greenway-Path-IiCN")
    outskirts = card(6, "Outskirts of Archet", "Outskirts-of-Archet-IiCN")
    by_number = {5: greenway, 6: outskirts}
    by_title = {
        nm.clean_for_match("Greenway Path"): greenway,
        nm.clean_for_match("Outskirts of Archet"): outskirts,
    }

    picked, how = nm.pick_card(5, "Outskirts of Archet", by_number, by_title)
    assert picked["Slug"] == "Outskirts-of-Archet-IiCN"
    assert how == "title"

    picked, how = nm.pick_card(6, "Greenway Path", by_number, by_title)
    assert picked["Slug"] == "Greenway-Path-IiCN"
    assert how == "title"


def test_pick_card_uses_position_when_the_title_is_ambiguous():
    # Flight from Moria has three cards titled "Search for an Exit", separated
    # only by slug. build_lookups drops the title, leaving position to decide.
    by_number = {
        2: card(2, "Search for an Exit", "Search-for-an-Exit-Pursued-By-Shadow-FfMN"),
        3: card(3, "Search for an Exit", "Search-for-an-Exit-Blocked-by-Flame-FfMN"),
    }
    picked, _ = nm.pick_card(3, "Search for an Exit", by_number, {})
    assert picked["Slug"] == "Search-for-an-Exit-Blocked-by-Flame-FfMN"


def test_pick_card_returns_none_when_nothing_matches():
    assert nm.pick_card(99, "Not A Card", {}, {}) == (None, None)


def test_build_lookups_drops_titles_that_collide_within_a_pack():
    cards = [
        card(2, "Search for an Exit", "Search-for-an-Exit-A-FfMN", "Flight from Moria Nightmare"),
        card(3, "Search for an Exit", "Search-for-an-Exit-B-FfMN", "Flight from Moria Nightmare"),
        card(4, "Swarming Goblins", "Swarming-Goblins-FfMN", "Flight from Moria Nightmare"),
    ]
    _packs, by_pack = nm.build_lookups(cards)
    by_number, by_title = by_pack["Flight from Moria Nightmare"]
    assert nm.clean_for_match("Search for an Exit") not in by_title
    assert nm.clean_for_match("Swarming Goblins") in by_title
    assert set(by_number) == {2, 3, 4}


# --- pack resolution ---------------------------------------------------------

PACKS = {
    nm.clean_for_match(p): p
    for p in (
        "Passage Through Mirkwood Nightmare",
        "Intruders in Chetwood Nightmare",
        "The Hobbit: Over Hill and Under Hill",
        "We Must Away, Ere Break of Day Nightmare",
    )
}


def test_resolve_pack_appends_nightmare_to_the_scenario_folder():
    folder = pathlib.Path("01 - Core Set - Shadows of Mirkwood - Nightmare/01 - Passage Through Mirkwood")
    assert nm.resolve_pack(folder, PACKS) == "Passage Through Mirkwood Nightmare"


def test_resolve_pack_repairs_the_invaders_in_chetwood_typo():
    # Hall of Beorn, RingsDB and the card itself all read "Intruders".
    folder = pathlib.Path("05 - The Lost Realm - Angmar Awakened - Nightmare/01 - Invaders in Chetwood")
    assert nm.resolve_pack(folder, PACKS) == "Intruders in Chetwood Nightmare"


def test_resolve_pack_handles_the_extra_saga_nesting():
    folder = pathlib.Path("07 - The Hobbit/01 - Over Hill and Under Hill - Nightmare/We Must Away, Ere Break of Day")
    assert nm.resolve_pack(folder, PACKS) == "We Must Away, Ere Break of Day Nightmare"


def test_resolve_pack_returns_none_for_an_unknown_folder():
    assert nm.resolve_pack(pathlib.Path("99 - Not A Real Scenario"), PACKS) is None


# --- bleed trimming ----------------------------------------------------------

def test_trim_box_leaves_the_card_on_the_cut_line():
    # Cutting a trimmed scan at MPC's 744/816 and 1038/1110 has to land on the
    # card edge: anything left over prints as a dark margin.
    left, top, right, bottom = nm.trim_box(3432, 4680)
    assert (left, top, right, bottom) == (94, 91, 3338, 4589)

    w, h = right - left, bottom - top
    rx = (nm.MPC_BLEED_W - nm.MPC_CUT_W) / 2 / nm.MPC_BLEED_W
    ry = (nm.MPC_BLEED_H - nm.MPC_CUT_H) / 2 / nm.MPC_BLEED_H
    cut_w, cut_h = w * (1 - 2 * rx), h * (1 - 2 * ry)
    assert abs(cut_w / cut_h - nm.CARD_W_MM / nm.CARD_H_MM) < 0.001


def test_trim_box_takes_more_off_the_sides():
    # The cut fractions differ per axis, 4.41% of width against 3.24% of height,
    # while the source bleed is 5mm all round.
    left, top, _r, _b = nm.trim_box(3432, 4680)
    assert left > top


def test_trim_box_matches_after_a_landscape_scan_is_turned_upright():
    # The 35 sideways quest scans are 4680x3432 and rotate to the portrait size.
    assert nm.trim_box(3432, 4680) == nm.trim_box(*sorted((4680, 3432)))


def test_landscape_rotation_is_clockwise():
    # Counter-clockwise comes out mirrored against every other LotR collection.
    from PIL import Image
    assert nm.LANDSCAPE_ROTATION == Image.Transpose.ROTATE_270
