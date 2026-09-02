import importlib.util
import pathlib

# Load ../rename_ffg_pdfs.py directly. It's a standalone script, not an
# installed package, so there's nothing to import by name.
_spec = importlib.util.spec_from_file_location(
    "lotrlcg_rename_ffg_pdfs",
    pathlib.Path(__file__).resolve().parent.parent / "rename_ffg_pdfs.py",
)
assert _spec and _spec.loader, "could not load rename_ffg_pdfs.py"
ffg = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ffg)


def card(slug, number, card_set="Test Set", title="Title", **face):
    return {
        "Slug": slug,
        "Number": number,
        "CardSet": card_set,
        "Title": title,
        "Front": face,
    }


# --- words / overlap -------------------------------------------------------


def test_words_splits_on_punctuation_and_transliterates():
    assert ffg.words("Iârion's Pendant!") == {"iarion", "s", "pendant"}


def test_catalog_words_takes_every_field_of_the_face():
    entry = card(
        "X",
        1,
        title="Ballista",
        Subtitle="Sub",
        Text=["Attached Ship gets +1."],
        Traits=["Grey Havens."],
        Keywords=["Sentinel."],
    )
    assert {"ballista", "sub", "attached", "ship", "gets", "1", "grey", "havens", "sentinel"} == (
        ffg.catalog_words(entry)
    )


def test_overlap_is_the_share_of_the_catalog_wording_found_on_the_page():
    entry = card("X", 1, title="Ballista", Text=["Attached Ship gets."])
    assert ffg.overlap(entry, "Ballista Attached Ship gets") == 1.0
    assert ffg.overlap(entry, "Ballista Attached") == 0.5
    assert ffg.overlap(entry, "nothing here") == 0.0


def test_a_card_with_no_wording_at_all_scores_zero():
    assert ffg.overlap(card("X", 1, title=""), "any page text") == 0.0


# --- build_lookup ----------------------------------------------------------


def test_build_lookup_keys_by_set_then_number():
    lookup = ffg.build_lookup([card("Aragorn-RevCore", 1, "Revised Core Set")])
    assert [c["Slug"] for c in lookup["Revised Core Set"][1]] == ["Aragorn-RevCore"]


def test_a_slug_listed_twice_is_kept_once():
    # Hall of Beorn lists every Ered Mithrin treasure twice under one slug.
    entries = [card("Masterwork-Bow-EMC", 204), card("Masterwork-Bow-EMC", 204)]
    assert len(ffg.build_lookup(entries)["Test Set"][204]) == 1


def test_two_slugs_on_one_number_are_both_kept():
    # One physical card with a different card on each face.
    entries = [card("Protect-the-Innocent-AAC", 158), card("Arnor-Ravaged-AAC", 158)]
    assert len(ffg.build_lookup(entries)["Test Set"][158]) == 2


def test_entries_missing_a_set_number_or_slug_are_dropped():
    entries = [
        {"Slug": "A", "Number": 1, "CardSet": None},
        {"Slug": "B", "Number": None, "CardSet": "Test Set"},
        {"Slug": "", "Number": 2, "CardSet": "Test Set"},
    ]
    assert ffg.build_lookup(entries) == {}


# --- pack_for --------------------------------------------------------------


def test_pack_for_reads_a_single_set_pdf():
    assert ffg.pack_for("core_set_campaign_cards", 129) == "Revised Core Set"


def test_pack_for_reads_a_pdf_that_spans_two_sets():
    stem = "mec104-mec105_replacement_cards"
    assert ffg.pack_for(stem, 21) == "Elves of Lórien"
    assert ffg.pack_for(stem, 10) == "Defenders of Gondor"
    assert ffg.pack_for(stem, 999) is None


def test_pack_for_an_undeclared_pdf_is_none():
    assert ffg.pack_for("something_else", 1) is None


# --- assign_faces ----------------------------------------------------------


def test_assign_faces_gives_each_card_the_page_it_fits_better():
    base = card("Armor-Plating-DCC", 172, title="Armor Plating", Text=["Attached Ship gets."])
    upgraded = card(
        "Armor-Plating-Upgraded-DCC",
        172,
        title="Armor Plating",
        Text=["Attached Ship gets.", "Response discard shadow."],
    )
    front, back = ffg.assign_faces(
        [base, upgraded], "Armor Plating Attached Ship gets", "Armor Plating Attached Ship gets Response discard shadow"
    )
    assert front["Slug"] == "Armor-Plating-DCC"
    assert back["Slug"] == "Armor-Plating-Upgraded-DCC"


def test_assign_faces_does_not_depend_on_the_order_it_is_given():
    one = card("One", 1, title="One", Text=["alpha"])
    two = card("Two", 1, title="Two", Text=["beta"])
    assert ffg.assign_faces([one, two], "One alpha", "Two beta")[0]["Slug"] == "One"
    assert ffg.assign_faces([two, one], "One alpha", "Two beta")[0]["Slug"] == "One"


def test_two_cards_that_fit_both_pages_equally_are_left_unassigned():
    one = card("One", 1, title="Same", Text=["same"])
    two = card("Two", 1, title="Same", Text=["same"])
    assert ffg.assign_faces([one, two], "Same same", "Same same") is None


# --- the declared tables ---------------------------------------------------


def test_every_number_offset_names_a_declared_card_set():
    declared = set()
    for pack in ffg.PACKS.values():
        declared.update(pack.values() if isinstance(pack, dict) else [pack])
    assert set(ffg.NUMBER_OFFSETS) <= declared


def test_a_multi_set_pdf_maps_collector_numbers_not_pages():
    for stem, pack in ffg.PACKS.items():
        if isinstance(pack, dict):
            assert all(isinstance(number, int) for number in pack), stem


# --- how a doubled collector number is written -----------------------------


def face(title, slug, text, card_type="Attachment"):
    return {
        "Title": title,
        "Slug": slug,
        "Number": 1,
        "CardSet": "Test Set",
        "CardType": card_type,
        "Front": {"Text": [text], "Traits": [], "Stats": None, "Subtitle": None},
    }


def test_an_upgradable_card_is_one_id_with_the_upgraded_side_as_its_back():
    # FFG prints the Upgraded side on the back of the base side, so the two
    # catalog entries are one card. Writing both would file the same two images
    # under a second id and print the card twice.
    base = face("Armor Plating", "Armor-Plating-DCC", "Attached Ship gets +1 defense.")
    upgraded = face("Armor Plating", "Armor-Plating-Upgraded-DCC",
                    "Attached Ship gets +1 defense. Response: discard a shadow card.")
    ordered = ffg.assign_faces(
        [base, upgraded],
        "Armor Plating Attached Ship gets +1 defense.",
        "Armor Plating Attached Ship gets +1 defense. Response: discard a shadow card.",
    )
    assert [c["Slug"] for c in ordered] == ["Armor-Plating-DCC", "Armor-Plating-Upgraded-DCC"]
    assert ffg.normalize_title(ordered[0]["Title"]) == ffg.normalize_title(ordered[1]["Title"])


def test_two_differently_named_cards_on_one_card_stay_two_ids():
    # Protect the Innocent and Arnor Ravaged are 158a and 158b of one card, but
    # they are different cards, so a decklist naming either has to resolve.
    front = face("Protect the Innocent", "Protect-the-Innocent-AAC",
                 "Forced: After an attack damages a character, place 1 damage.",
                 card_type="Encounter_Side_Quest")
    back = face("Arnor Ravaged", "Arnor-Ravaged-AAC",
                "Setup: Add Arnor Ravaged to the staging area with damage.",
                card_type="Objective")
    ordered = ffg.assign_faces(
        [front, back],
        "Protect the Innocent Forced: After an attack damages a character, place 1 damage.",
        "Arnor Ravaged Setup: Add Arnor Ravaged to the staging area with damage.",
    )
    assert ffg.normalize_title(ordered[0]["Title"]) != ffg.normalize_title(ordered[1]["Title"])
