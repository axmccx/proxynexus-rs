import importlib.util
import pathlib

# Load ../rename.py directly. It's a standalone script, not an installed
# package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "lotrlcg_rename", pathlib.Path(__file__).resolve().parent.parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)


# --- clean_for_match / normalize_title -------------------------------------

def test_clean_for_match_strips_leading_the():
    assert rename.clean_for_match("The Hunt for Gollum") == rename.clean_for_match("Hunt for Gollum")


def test_clean_for_match_lowercases_and_strips_punctuation():
    assert rename.clean_for_match("Gwahir's Debt") == "gwahirsdebt"


def test_clean_for_match_transliterates_unicode():
    assert rename.clean_for_match("Khazad-dûm") == rename.clean_for_match("Khazad-dum")


def test_normalize_title_replaces_non_alnum_with_underscore():
    assert rename.normalize_title("A Perilous Voyage!") == "a_perilous_voyage_"


# --- parse_filename: position / name splitting ------------------------------

def test_parse_filename_simple_numbered_card():
    position_str, stage_str, text_name, is_back = rename.parse_filename("001 - Aragorn")
    assert position_str == "001"
    assert stage_str is None
    assert text_name == "Aragorn"
    assert is_back is False


def test_parse_filename_lettered_position_suffix():
    position_str, stage_str, text_name, is_back = rename.parse_filename("047a - A Perilous Voyage")
    assert position_str == "047a"
    assert text_name == "A Perilous Voyage"
    assert is_back is False


def test_parse_filename_keeps_card_number_and_stage_apart():
    # '011 - 1B - The Hunt Begins' carries both. The card number must survive:
    # folding the stage over it is what loses same-titled cards.
    position_str, stage_str, text_name, is_back = rename.parse_filename("011 - 1B - The Hunt Begins")
    assert position_str == "011"
    assert stage_str == "1B"
    assert text_name == "The Hunt Begins"
    assert is_back is True


def test_parse_filename_dash_dash_form_normalized():
    # '- - 2A - Lost in the Swanfleet' is normalized to '000 - 2A - ...' first.
    position_str, stage_str, text_name, is_back = rename.parse_filename("- - 2A - Lost in the Swanfleet")
    assert position_str == "000"
    assert stage_str == "2A"
    assert text_name == "Lost in the Swanfleet"
    assert is_back is False


# --- back-face detection -----------------------------------------------------

def test_parse_filename_back_suffix_letters():
    for letter in ("B", "D", "F", "H"):
        _, _, _, is_back = rename.parse_filename(f"012{letter} - Some Card")
        assert is_back is True, f"letter {letter} should mark a back face"


def test_parse_filename_side_b_marks_back():
    _, _, text_name, is_back = rename.parse_filename("012 - Some Card (side b)")
    assert is_back is True
    assert "(side b)" not in text_name.lower()


def test_parse_filename_side_a_does_not_mark_back():
    _, _, text_name, is_back = rename.parse_filename("012 - Some Card (side a)")
    assert is_back is False
    assert "(side a)" not in text_name.lower()


def test_parse_filename_reverse_marks_back():
    _, _, text_name, is_back = rename.parse_filename("012 - Some Card reverse")
    assert is_back is True
    assert "reverse" not in text_name.lower()


# --- (errata) stripping and trailing dedup-number stripping ------------------

def test_parse_filename_strips_errata_suffix():
    _, _, text_name, _ = rename.parse_filename("012 - Some Card (errata)")
    assert text_name == "Some Card"


def test_parse_filename_strips_trailing_dedup_number():
    _, _, text_name, _ = rename.parse_filename("012 - Dark Pools 3")
    assert text_name == "Dark Pools"


# --- PACK_TITLE_FIXES_CLEAN: pack-scoped, does not leak across packs ---------

def test_pack_title_fix_applies_only_within_its_own_pack():
    key_in_pack = ("The Black Riders", rename.clean_for_match("Bill Fenry"))
    assert rename.PACK_TITLE_FIXES_CLEAN[key_in_pack] == "Bill Ferny"

    # Same cleaned name, different pack: must NOT be present / must not resolve.
    other_pack_key = ("Some Other Pack", rename.clean_for_match("Bill Fenry"))
    assert other_pack_key not in rename.PACK_TITLE_FIXES_CLEAN


def test_pack_title_fix_does_not_leak_to_correctly_spelled_card_elsewhere():
    # "Mordor Wargs" is a deliberate HoB-matching misspelling fix scoped to
    # "The Sands of Harad". A card legitimately titled "Mordor Wargs" in a
    # different pack must resolve via .get() with a default to itself,
    # unaffected by the pack-scoped table.
    clean_name = rename.clean_for_match("Mordor Wargs")
    fixed = rename.PACK_TITLE_FIXES_CLEAN.get(("A Totally Different Pack", clean_name), clean_name)
    assert fixed == clean_name


# --- Issue 4: orphaned-back validation must be per-folder --------------------

def test_back_with_front_in_other_folder_is_reported_as_orphaned():
    folder_file_lists = {
        "lotrlcg-enhanced": ["aragorn@core.jpg"],
        "lotrlcg-nightmare": ["aragorn@core~back.jpg"],
    }
    orphans = rename.find_orphaned_backs(folder_file_lists)
    assert "lotrlcg-nightmare" in orphans
    assert "aragorn@core~back.jpg" in orphans["lotrlcg-nightmare"]


def test_back_with_front_in_same_folder_is_not_orphaned():
    folder_file_lists = {
        "lotrlcg-enhanced": ["aragorn@core.jpg", "aragorn@core~back.jpg"],
        "lotrlcg-nightmare": [],
    }
    orphans = rename.find_orphaned_backs(folder_file_lists)
    assert orphans == {}


# --- DQ4: '026 -1A' loses the space before the stage code --------------------

def test_parse_filename_stage_glued_to_card_number():
    # The Mumakil ships '026 -1A - Welcome to the Jungle.jpg'. The missing space
    # hid the stage code, so the 1B scan was written as a second front and
    # deduped away, leaving the card with no back.
    position_str, stage_str, text_name, is_back = rename.parse_filename("026 -1A - Welcome to the Jungle")
    assert position_str == "026"
    assert stage_str == "1A"
    assert text_name == "Welcome to the Jungle"
    assert is_back is False

    _, stage_str, _, is_back = rename.parse_filename("026 -1B - Welcome to the Jungle")
    assert stage_str == "1B"
    assert is_back is True


# --- DQ5: C/D-side quest cards must keep their own number --------------------

def test_parse_filename_c_and_d_sides():
    # Race Across Harad prints 'Setting Out' twice, at 1A/1B and again at 1C/1D.
    position_str, stage_str, _, is_back = rename.parse_filename("051 - 1C - Setting Out")
    assert position_str == "051"
    assert stage_str == "1C"
    assert is_back is False

    position_str, stage_str, _, is_back = rename.parse_filename("051 - 1D - Setting Out")
    assert position_str == "051"
    assert stage_str == "1D"
    assert is_back is True


# --- card_stage: Hall of Beorn's printed stage code --------------------------

def test_card_stage_reads_each_face():
    card = {
        "Front": {"Stats": {"StageNumber": "1C"}},
        "Back": {"Stats": {"StageNumber": "1D", "QuestPoints": "10"}},
    }
    assert rename.card_stage(card, is_back=False) == "1C"
    assert rename.card_stage(card, is_back=True) == "1D"


def test_card_stage_none_for_single_sided_and_unstaged():
    assert rename.card_stage({"Front": {"Stats": {}}, "Back": None}, is_back=True) is None
    assert rename.card_stage({"Front": {"Stats": {}}, "Back": None}, is_back=False) is None


# --- slug matching must transliterate the slug, not just lowercase it -------

def test_slug_haystack_transliterates_and_flattens():
    # 'Nearing the Gate' ships its 3B face as
    # '007 - 3B - The Bridge of Khazad-dum.jpg', spelled with a plain u, while
    # the slug spells it with a circumflex. Cleaning only one side of the
    # comparison meant that back was never written.
    assert rename.clean_for_match("The Bridge of Khazad-dum") in rename.slug_haystack(
        "Nearing-the-Gate-The-Bridge-of-Khazad-d\u00fbm-EfKD"
    )


def test_slug_haystack_matches_hob_mangled_spelling():
    # Hall of Beorn's own export stores this title as 'L\u2264rien'. Both sides go
    # through unidecode, so the needle and the haystack agree on the mangling.
    assert rename.clean_for_match("Lost Soul of L\u2264rien") in rename.slug_haystack(
        "Lost-Soul-of-L\u2264rien-TDMN"
    )


def test_slug_haystack_still_matches_plain_ascii_subtitles():
    assert rename.clean_for_match("Last Lord of Moria") in rename.slug_haystack(
        "Nearing-the-Gate-Last-Lord-of-Moria-EfKD"
    )


# --- DQ1: backs must not be written onto single-sided cards ------------------

def test_has_split_sibling_detects_hob_two_entry_cards():
    # The Hunt for the Dreadnaught prints Eithiliant as one double-sided card,
    # 6a / 6b, but Hall of Beorn stores two entries with Back: null on both.
    basic = {"Title": "Eithiliant", "position": 6, "Slug": "Eithiliant-THftD", "Back": None}
    upgraded = {"Title": "Eithiliant", "position": 6, "Slug": "Eithiliant-Upgraded-THftD", "Back": None}
    pack_cards = {"eithiliant": [basic, upgraded]}
    assert rename.has_split_sibling(pack_cards, basic) is True
    assert rename.has_split_sibling(pack_cards, upgraded) is True


def test_has_split_sibling_false_for_a_lone_single_sided_card():
    # The Massing at Osgiliath's treachery is genuinely single-sided, so the
    # product back-cover scan must not be written as its back.
    treachery = {"Title": "Massing at Osgiliath", "position": 14,
                 "Slug": "Massing-at-Osgiliath-TMaO", "Back": None}
    other = {"Title": "Cut Off", "position": 12, "Slug": "Cut-Off-TMaO", "Back": None}
    pack_cards = {"massingatosgiliath": [treachery], "cutoff": [other]}
    assert rename.has_split_sibling(pack_cards, treachery) is False


def test_has_split_sibling_false_when_titles_match_but_numbers_differ():
    # Race Across Harad prints 'Setting Out' twice, at 47 and 51. Different
    # cards, not two faces of one.
    a = {"Title": "Setting Out", "position": 47, "Slug": "Setting-Out-RaH", "Back": {}}
    c = {"Title": "Setting Out", "position": 51, "Slug": "Setting-Out-C-RaH", "Back": {}}
    pack_cards = {"settingout": [a, c]}
    assert rename.has_split_sibling(pack_cards, a) is False


# --- one scan that is the shared back of several cards -----------------------

def _run_process_folders(tmp_path, pack, files, cards, dry_run=True):
    """Drive process_folders over a synthetic archive and return the output names.

    The Grey Havens ships six cards whose fronts are named individually and whose
    single common back is one file called 'Lost Island.jpg'. Nothing short of the
    real loop exercises that, so the fixture builds the folder rather than
    calling a helper.
    """
    import types

    archive = tmp_path / "Archive"
    folder = archive / f"01 - {pack}"
    folder.mkdir(parents=True)
    for name in files:
        (folder / name).write_bytes(b"")

    pack_lookup = {rename.clean_for_match(pack): pack}
    card_lookup = {pack: {}}
    for c in cards:
        c = dict(c, target_id=rename.normalize_title(c["Slug"]),
                 position=c.get("Number"), pack_code=pack)
        card_lookup[pack].setdefault(rename.clean_for_match(c["Title"]), []).append(c)

    args = types.SimpleNamespace(dry_run=dry_run)
    _, _, rows = rename.process_folders(
        [(str(archive), True, False)], pack_lookup, card_lookup, str(tmp_path / "out"), args
    )
    return sorted(row[1] for row in rows[1:])


def _lost_island(number, subtitle):
    return {
        "Title": subtitle,
        "Slug": f"{subtitle.replace(' ', '-')}-Lost-Island-TGH",
        "Number": number,
        "Front": {"Stats": {}},
        "Back": {"Stats": {}},
    }


def test_shared_back_is_written_for_every_card_that_prints_it(tmp_path):
    cards = [_lost_island(27, "Shrine to Morgoth"),
             _lost_island(30, "Lush Jungle"),
             _lost_island(31, "Forbidden Coast")]
    files = ["027 - Shrine to Morgoth.jpg", "030 - Lush Jungle.jpg",
             "031 - Forbidden Coast.jpg", "Lost Island.jpg"]

    written = _run_process_folders(tmp_path, "The Grey Havens", files, cards)

    assert written == sorted([
        "shrine_to_morgoth_lost_island_tgh@the_grey_havens.bleed.jpg",
        "lush_jungle_lost_island_tgh@the_grey_havens.bleed.jpg",
        "forbidden_coast_lost_island_tgh@the_grey_havens.bleed.jpg",
        "shrine_to_morgoth_lost_island_tgh@the_grey_havens~back.bleed.jpg",
        "lush_jungle_lost_island_tgh@the_grey_havens~back.bleed.jpg",
        "forbidden_coast_lost_island_tgh@the_grey_havens~back.bleed.jpg",
    ])


def test_subtitle_named_back_still_resolves_to_its_one_card(tmp_path):
    # The Fortress of Nurn's four 'Storm the Castle' backs are named
    # individually, one file per card. Relaxing the shared-back rule must not
    # smear each of them across all four.
    def storm(number, subtitle):
        return {
            "Title": "Storm the Castle",
            "Slug": f"Storm-the-Castle-{subtitle.replace(' ', '-')}-TFoN",
            "Number": number,
            "Front": {"Stats": {}},
            "Back": {"Stats": {}},
        }

    cards = [storm(161, "Castle Garrison"), storm(162, "Lethal Counterattack")]
    files = ["--- - Storm the Castle.jpg", "161 - Castle Garrison.jpg",
             "162 - Lethal Counterattack.jpg"]

    written = _run_process_folders(tmp_path, "The Fortress of Nurn", files, cards)

    assert written == sorted([
        "storm_the_castle_castle_garrison_tfon@the_fortress_of_nurn.bleed.jpg",
        "storm_the_castle_lethal_counterattack_tfon@the_fortress_of_nurn.bleed.jpg",
        "storm_the_castle_castle_garrison_tfon@the_fortress_of_nurn~back.bleed.jpg",
        "storm_the_castle_lethal_counterattack_tfon@the_fortress_of_nurn~back.bleed.jpg",
    ])


def test_diacritic_only_back_is_matched(tmp_path):
    cards = [{
        "Title": "Nearing the Gate",
        "Slug": "Nearing-the-Gate-The-Bridge-of-Khazad-dûm-EfKD",
        "Number": 7,
        "Front": {"Stats": {"StageNumber": "3A"}},
        "Back": {"Stats": {"StageNumber": "3B"}},
    }]
    files = ["006 - 3A - Nearing the Gate.jpg",
             "007 - 3B - The Bridge of Khazad-dum.jpg"]

    written = _run_process_folders(tmp_path, "Escape from Khazad-dûm", files, cards)

    assert written == sorted([
        "nearing_the_gate_the_bridge_of_khazad_dum_efkd@escape_from_khazad_dum.bleed.jpg",
        "nearing_the_gate_the_bridge_of_khazad_dum_efkd@escape_from_khazad_dum~back.bleed.jpg",
    ])


# --- the archive's Alt_Art_Heroes folder ------------------------------------

def test_alt_art_hero_folder_routes_each_file_to_its_own_set(tmp_path):
    # The folder sits at the archive root and resolves to no pack; each file
    # names a card in a different set, so the pack comes from the table.
    import types

    archive = tmp_path / "Archive"
    folder = archive / "Alt_Art_Heroes"
    folder.mkdir(parents=True)
    for name in ("Gimli_Alt_Art.jpg", "Glorfindel_Alt_Art.jpg", "Unmapped_Alt_Art.jpg"):
        (folder / name).write_bytes(b"")

    def hero(title, slug):
        return {"Title": title, "Slug": slug, "Number": 4,
                "Front": {"Stats": {}}, "Back": None}

    # Two Gimlis in the pack, to show the table picks the card and not the title.
    card_lookup = {
        "The Ruins of Belegost": {
            "gimli": [dict(hero("Gimli", "Gimli-TRoB"), target_id="gimli_trob",
                           position=4, pack_code="The Ruins of Belegost"),
                      dict(hero("Gimli", "Gimli-Other-TRoB"), target_id="gimli_other_trob",
                           position=9, pack_code="The Ruins of Belegost")],
        },
        "The Wizard's Quest": {
            "glorfindel": [dict(hero("Glorfindel", "Glorfindel-TWQ"),
                                target_id="glorfindel_twq", position=111,
                                pack_code="The Wizard's Quest")],
        },
    }
    pack_lookup = {rename.clean_for_match(p): p for p in card_lookup}

    args = types.SimpleNamespace(dry_run=True)
    copied, skipped, rows = rename.process_folders(
        [(str(archive), True, False)], pack_lookup, card_lookup, str(tmp_path / "out"), args
    )

    assert sorted(row[1] for row in rows[1:]) == sorted([
        "gimli_trob@the_ruins_of_belegost.bleed.jpg",
        "glorfindel_twq@the_wizard_s_quest.bleed.jpg",
    ])
    assert copied == 2
    assert skipped == 1  # the unmapped file, reported rather than walked past


def test_alt_art_table_names_cards_that_exist_in_the_catalog():
    # Guards the hand-written table against a catalog rename.
    catalog = rename.load_catalog()
    by_slug = {c["Slug"]: c for c in catalog}
    for filename, (card_set, slug) in rename.ALT_ART_HERO_FILES.items():
        assert slug in by_slug, f"{filename}: no card with slug {slug}"
        assert by_slug[slug]["CardSet"] == card_set, filename
        assert by_slug[slug]["CardType"] == "Hero", filename


# --- which archives carry the print screen -----------------------------------

def test_source_folders_declare_bleed_and_despeckle_per_archive():
    for entry in rename.SOURCE_FOLDERS:
        name, has_bleed, needs_despeckle = entry
        assert isinstance(name, str) and name
        assert isinstance(has_bleed, bool)
        assert isinstance(needs_despeckle, bool)
    names = [name for name, _, _ in rename.SOURCE_FOLDERS]
    assert len(set(names)) == len(names)


def test_only_the_enhanced_proxies_skip_the_despeckle():
    # They were denoised and sharpened before publication; the two scan sets are
    # flatbed scans of the printed cards and carry its screen.
    clean = [name for name, _, needs in rename.SOURCE_FOLDERS if not needs]
    assert clean == ["Enhanced Proxies"]


def test_despeckle_is_not_loaded_at_import():
    # It pulls in cv2 and numpy, which the test environment does not install.
    # Making this a top-level import would break every test in this file.
    assert rename._despeckle_module is None
