"""Pins audit_coverage.py against the Rust it mirrors.

Most cases here are ported from the Rust tests of the functions being mirrored,
so that a change to `identity.rs`, `file_naming.rs`, `collection_manager.rs` or
`card_store.rs` that this script has not followed shows up as a failure here.
"""

import importlib.util
import pathlib

_spec = importlib.util.spec_from_file_location(
    "lotrlcg_audit_coverage",
    pathlib.Path(__file__).resolve().parent.parent / "audit_coverage.py",
)
assert _spec and _spec.loader, "could not load audit_coverage.py"
audit = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(audit)

OWN, FILLED, WRONG, BLANK = audit.OWN, audit.FILLED, audit.WRONG, audit.BLANK


def card(title, slug, pack, card_type="Hero", sphere=None, front=None, back=None, number=1):
    return {
        "Title": title,
        "Slug": slug,
        "CardSet": pack,
        "Number": number,
        "CardType": card_type,
        "Sphere": sphere,
        "Front": front,
        "Back": back,
    }


def face(text=None, **stats):
    return {"Text": [text] if text else None, "Stats": stats or None, "Subtitle": None}


def dates(**packs):
    return {audit.normalize_title(p): d for p, d in packs.items()}


def id_of(ids, slug):
    return ids[audit.normalize_title(slug)]


# --- identity.rs: printing_card_ids ----------------------------------------


def test_reprints_with_identical_identity_collapse_to_the_earliest_printing():
    cards = [
        card("Aragorn", "Aragorn-Core", "Core Set", sphere="Leadership", front=face("Sentinel.")),
        card("Aragorn", "Aragorn-RevCore", "Revised Core Set", sphere="Leadership", front=face("Sentinel.")),
    ]
    ids = audit.printing_card_ids(cards, dates(**{"Core Set": "2011-04-20", "Revised Core Set": "2022-01-01"}))
    assert id_of(ids, "Aragorn-Core") == "aragorn_core"
    assert id_of(ids, "Aragorn-RevCore") == "aragorn_core"


def test_different_card_type_or_sphere_keeps_a_shared_title_separate():
    types = [
        card("Gríma", "Grima-Hero", "Pack", card_type="Hero"),
        card("Gríma", "Grima-Objective-Ally", "Pack", card_type="Objective_Ally"),
    ]
    ids = audit.printing_card_ids(types, {})
    assert id_of(ids, "Grima-Hero") != id_of(ids, "Grima-Objective-Ally")

    spheres = [
        card("Faramir", "Faramir-A", "Pack", sphere="Lore"),
        card("Faramir", "Faramir-B", "Pack", sphere="Leadership"),
    ]
    ids = audit.printing_card_ids(spheres, {})
    assert id_of(ids, "Faramir-A") != id_of(ids, "Faramir-B")


def test_different_front_or_back_text_keeps_a_shared_title_separate():
    cards = [
        card("Armor Plating", "Armor-Plating", "Pack", front=face("Attach to a hero.")),
        card("Armor Plating", "Armor-Plating-Upgraded", "Pack",
             front=face("Attach to a hero. Gains +1 defense.")),
    ]
    ids = audit.printing_card_ids(cards, {})
    assert id_of(ids, "Armor-Plating") != id_of(ids, "Armor-Plating-Upgraded")

    stages = [
        card("Stage", "Stage-1", "Pack", back=face("2A setup.")),
        card("Stage", "Stage-2", "Pack", back=face("2B setup.")),
    ]
    ids = audit.printing_card_ids(stages, {})
    assert id_of(ids, "Stage-1") != id_of(ids, "Stage-2")


def test_differing_stats_keep_a_shared_title_separate():
    cards = [
        card("Ship", "Ship-A", "Pack", front=face("Sails.", Threat="3")),
        card("Ship", "Ship-B", "Pack", front=face("Sails.", Threat="5")),
    ]
    ids = audit.printing_card_ids(cards, {})
    assert id_of(ids, "Ship-A") != id_of(ids, "Ship-B")


def test_cosmetic_text_differences_do_not_split_identity():
    cards = [
        card("Dúnedain Lookout", "Lookout-A", "Pack",
             front=face("Response: after Dúnedain Lookout enters play,\ndraw a card.")),
        card("Dúnedain Lookout", "Lookout-B", "Pack",
             front=face("RESPONSE: After Dunedain Lookout enters play, draw a card.  ")),
    ]
    ids = audit.printing_card_ids(cards, {})
    assert id_of(ids, "Lookout-A") == id_of(ids, "Lookout-B")


def test_undated_packs_are_treated_as_printed_last():
    cards = [
        card("Aragorn", "Zz-Undated-Printing", "Unknown Pack"),
        card("Aragorn", "Aa-Dated-Printing", "Known Pack"),
    ]
    ids = audit.printing_card_ids(cards, dates(**{"Known Pack": "2015-01-01"}))
    assert id_of(ids, "Zz-Undated-Printing") == "aa_dated_printing"


def test_grouping_is_order_independent():
    cards = [
        card("Aragorn", "Aragorn-Core", "Core Set"),
        card("Aragorn", "Aragorn-RevCore", "Revised Core Set"),
        card("Gríma", "Grima-Hero", "Pack"),
    ]
    pack_dates = dates(**{"Core Set": "2011-04-20", "Revised Core Set": "2022-01-01"})
    assert audit.printing_card_ids(cards, pack_dates) == audit.printing_card_ids(
        list(reversed(cards)), pack_dates
    )


# --- identity.rs: card_titles ----------------------------------------------


def test_a_title_naming_one_card_stays_plain():
    cards = [card("Valor", "Valor-RevCore", "Pack")]
    ids = audit.printing_card_ids(cards, {})
    assert audit.card_titles(cards, ids)[id_of(ids, "Valor-RevCore")] == "Valor"


def test_a_title_naming_several_cards_gets_an_earliest_slug_suffix():
    cards = [
        card("Gríma", "Grima-Hero-VoI", "Pack", card_type="Hero"),
        card("Gríma", "Grima-Objective-Ally-VoI", "Pack", card_type="Objective_Ally"),
    ]
    ids = audit.printing_card_ids(cards, {})
    titles = audit.card_titles(cards, ids)
    assert titles[id_of(ids, "Grima-Hero-VoI")] == "Gríma (Hero VoI)"
    assert titles[id_of(ids, "Grima-Objective-Ally-VoI")] == "Gríma (Objective Ally VoI)"


def test_two_cards_sharing_a_title_never_share_a_normalized_title():
    # This is what stops a hero and an ally of one name competing in the title
    # lookup, and so what makes the `wrong` state rare.
    cards = [
        card("Faramir", "Faramir-Ally-DoG", "Pack", card_type="Ally"),
        card("Faramir", "Faramir-Hero-AoO", "Pack", card_type="Hero"),
    ]
    ids = audit.printing_card_ids(cards, {})
    titles = audit.card_titles(cards, ids)
    normalized = {audit.normalize_title(t) for t in titles.values()}
    assert len(normalized) == 2


# --- file_naming.rs --------------------------------------------------------


def test_parse_filename_variants():
    def parse(name):
        return audit.parse_filename(pathlib.Path(name))

    assert parse("hedge_fund@system_gateway.jpg") == ("hedge_fund", "system_gateway", "front", False)
    assert parse("sync@data_and_destiny~back.png") == ("sync", "data_and_destiny", "back", False)
    assert parse("hedge_fund@system_gateway~front.bleed.jpg") == (
        "hedge_fund", "system_gateway", "front", True)
    assert parse("hedge_fund@system_gateway.bleed.jpg") == (
        "hedge_fund", "system_gateway", "front", True)
    assert parse("hedge_fund~front.jpg") is None
    assert parse("hedge_fund@multiple@ats.jpg") is None
    assert parse("hedge_fund@dark~back~extra.png") is None


# --- collection_manager.rs -------------------------------------------------


def version(slug, pack, card_id, position=1):
    return {"slug": slug, "pack_id": pack, "card_id": card_id, "position": position}


def test_a_file_named_by_its_printings_own_id_links_to_that_version_and_card():
    v = version("aragorn_revcore", "revised_core_set", "aragorn_core")
    card_id, hit = audit.resolve_card_and_version(
        "aragorn_revcore", "revised_core_set", {"aragorn_revcore": v}, {})
    assert (card_id, hit) == ("aragorn_core", v)


def test_a_printing_id_in_an_unstored_pack_becomes_a_variant_of_its_card():
    v = version("aragorn_revcore", "revised_core_set", "aragorn_core")
    card_id, hit = audit.resolve_card_and_version(
        "aragorn_revcore", "enhanced", {"aragorn_revcore": v}, {})
    assert (card_id, hit) == ("aragorn_core", None)


def test_a_card_id_named_file_still_links_via_the_card_and_pack_fallback():
    v = version("hedge_fund", "system_gateway", "hedge_fund")
    card_id, hit = audit.resolve_card_and_version(
        "hedge_fund", "system_gateway", {}, {("hedge_fund", "system_gateway"): v})
    assert (card_id, hit) == ("hedge_fund", v)


def test_an_unrecognized_file_becomes_a_variant_of_its_own_id():
    card_id, hit = audit.resolve_card_and_version("mystery_card", "alt_art", {}, {})
    assert (card_id, hit) == ("mystery_card", None)


# --- card_store.rs: select_printing ----------------------------------------


def printing(**kw):
    kw.setdefault("is_official", True)
    kw.setdefault("variant", None)
    kw.setdefault("collection", "c")
    kw.setdefault("named", kw.get("card_id"))
    return audit.Printing(**kw)


def request(card_id="x", pack="p", position=1):
    return {"id": card_id, "printing": pack, "collection": None, "position": position}


def test_a_pack_match_outranks_everything_below_it():
    right_pack = printing(card_id="other", pack_id="p", position=9, date_release="2020-01-01")
    right_card = printing(card_id="x", pack_id="q", position=1, date_release="2011-01-01")
    assert audit.select_printing(request(), [right_card, right_pack]) is right_pack


def test_below_the_pack_the_position_then_the_card_id_decide():
    wrong_card = printing(card_id="other", pack_id="q", position=1, date_release="2011-01-01")
    right_card = printing(card_id="x", pack_id="q", position=1, date_release="2020-01-01")
    assert audit.select_printing(request(), [wrong_card, right_card]) is right_card


def test_an_official_printing_beats_a_variant_and_the_oldest_wins_last():
    variant = printing(card_id="x", pack_id=None, variant="alt", position=None, is_official=False)
    older = printing(card_id="x", pack_id="q", position=None, date_release="2011-01-01")
    newer = printing(card_id="x", pack_id="q", position=None, date_release="2020-01-01")
    assert audit.select_printing(request(position=None), [variant, newer, older]) is older


def test_no_candidates_resolves_to_nothing():
    assert audit.select_printing(request(), []) is None


# --- classify --------------------------------------------------------------


def classify_with(version_row, printings):
    titles = {version_row["card_id"]: "Title"}
    return audit.classify(version_row, titles, {"title": printings})[0]


def test_a_printing_with_its_own_image_is_own():
    v = version("aragorn_core", "core_set", "aragorn_core")
    p = printing(card_id="aragorn_core", pack_id="core_set", position=1, version=v)
    assert classify_with(v, [p]) == OWN


def test_a_file_naming_the_card_id_and_this_pack_is_still_its_own_image():
    # resolve_card_and_version links `aragorn_core@revised_core_set` to the
    # Revised Core version, so it is that printing's own art even though the
    # filename does not name its slug.
    v = version("aragorn_revcore", "revised_core_set", "aragorn_core")
    p = printing(card_id="aragorn_core", pack_id="revised_core_set", position=1, version=v)
    assert classify_with(v, [p]) == OWN


def test_a_printing_served_by_another_printing_of_the_same_card_is_filled():
    v = version("aragorn_revcore", "revised_core_set", "aragorn_core")
    other = version("aragorn_core", "core_set", "aragorn_core")
    p = printing(card_id="aragorn_core", pack_id="core_set", position=1, version=other)
    assert classify_with(v, [p]) == FILLED


def test_a_printing_served_by_a_different_card_is_wrong():
    v = version("faramir_hero", "dog", "faramir_hero")
    other = version("faramir_ally", "dog", "faramir_ally")
    p = printing(card_id="faramir_ally", pack_id="dog", position=1, version=other)
    assert classify_with(v, [p]) == WRONG


def test_a_printing_with_nothing_to_resolve_to_is_blank():
    v = version("navigation_tgh", "the_grey_havens", "navigation_tgh")
    assert classify_with(v, []) == BLANK


# --- mark_twins ------------------------------------------------------------


def flip(slug, pack, card_id, position, card_type="Ship_Objective"):
    return {
        "slug": slug,
        "pack_id": pack,
        "card_id": card_id,
        "position": position,
        "back_group": audit.back_group(card_type),
    }


def test_back_group_matches_the_adapters_mapping():
    assert audit.back_group("Hero") == "player"
    assert audit.back_group("Treasure") == "player"
    assert audit.back_group("Quest") == "quest"
    assert audit.back_group("Nightmare_Setup") == "quest"
    assert audit.back_group("Ship_Objective") == "encounter"
    assert audit.back_group("Treachery") == "encounter"


def test_a_flip_cards_second_entry_is_a_twin_not_a_gap():
    base = flip("eithiliant_thftd", "thftd", "eithiliant_thftd", 6)
    upgraded = flip("eithiliant_upgraded_thftd", "thftd", "eithiliant_upgraded_thftd", 6)
    states = {base["slug"]: (OWN, None), upgraded["slug"]: (BLANK, None)}

    audit.mark_twins(states, [base, upgraded])

    assert states[base["slug"]][0] == OWN
    assert states[upgraded["slug"]][0] == audit.TWIN


def test_a_card_at_its_own_position_stays_a_gap():
    printed = flip("a", "pack", "a", 6)
    missing = flip("b", "pack", "b", 7)
    states = {"a": (OWN, None), "b": (BLANK, None)}

    audit.mark_twins(states, [printed, missing])

    assert states["b"][0] == BLANK


def test_two_cards_sharing_a_number_on_different_backs_are_not_twins():
    # Attack on Dol Guldur numbers a promo hero 1 alongside quest stage 1.
    # They are two separate cards, so a missing one is still a gap.
    quest = flip("assault_aodg", "aodg", "assault_aodg", 1, card_type="Quest")
    hero = flip("celeborn_aodg", "aodg", "celeborn_aodg", 1, card_type="Hero")
    states = {quest["slug"]: (OWN, None), hero["slug"]: (BLANK, None)}

    audit.mark_twins(states, [quest, hero])

    assert states[hero["slug"]][0] == BLANK


def test_a_twin_of_a_filled_face_also_counts_as_printed():
    base = flip("a", "pack", "a", 6)
    twin = flip("b", "pack", "b", 6)
    states = {"a": (FILLED, None), "b": (BLANK, None)}

    audit.mark_twins(states, [base, twin])

    assert states["b"][0] == audit.TWIN
