# /// script
# requires-python = ">=3.10"
# dependencies = ["Pillow", "unidecode"]
# ///
"""Report which LotR LCG card sets a set of collection folders can print.

Answers one question per card: if you asked Proxy Nexus to print this pack, what
image would come out? Every card lands in one of four states.

    own      this printing has an image of its own
    filled   another printing of the same card supplies it -- prints correctly,
             carrying the other set's frame and icon
    twin     the second Hall of Beorn entry for a flip card whose other face
             prints; the physical card comes out, this id just never gets art
    wrong    what resolves is a different card
    blank    nothing resolves; the card does not print

A set is complete when every card is `own`, `filled` or `twin`. None of the
three is a gap. `filled` is right because two printings that share an identity
are the same card. `twin` is right because Hall of Beorn lists a card that flips
twice -- Eithiliant and Eithiliant-Upgraded are one piece of cardboard -- and
`rename.py` writes the second face as the first's `~back`, so asking for art
under the second id is asking for a card that does not exist.

Nothing here is fuzzy. Two printings are the same card when
`proxynexus-core/src/games/lotrlcg/identity.rs` says their identity strings are
equal -- title, card type, sphere and both faces' stats, rules text and
subtitle, compared exactly. This file mirrors that function, `card_titles`
beside it, `file_naming.rs::parse_filename`,
`collection_manager.rs::resolve_card_and_version` and
`card_store.rs::select_printing`. Those five are the contract: change one in the
Rust and this script drifts, which is what `tests/` pins.

`normalize_title` comes from rename.py so the audit normalizes ids the same way
the renamers wrote the filenames. It uses `unidecode` where the Rust uses
`deunicode`; both are ports of Text::Unidecode and agree on this catalog.

Pack release dates come from RingsDB, the adapter's own source, cached beside
this script. They are not cosmetic: the earliest printing's slug becomes the
card id, so a wrong or missing date moves ids and silently changes the answer.
"""

import argparse
import collections
import csv
import importlib.util
import json
import pathlib
import sys

# Matching and id normalization are shared with rename.py. Loaded by explicit
# path rather than `import rename`: all four games ship a rename.py, so a bare
# import resolves through sys.path and can pick up another game's module.
_spec = importlib.util.spec_from_file_location(
    "lotrlcg_rename_helpers", pathlib.Path(__file__).resolve().parent / "rename.py"
)
assert _spec and _spec.loader, "could not load rename.py"
rename = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rename)

normalize_title = rename.normalize_title
load_catalog = rename.load_catalog

# The adapter derives pack dates from RingsDB, and those dates decide which
# printing of a card is the earliest and so which slug becomes the card id.
# Reading them from anywhere else risks assigning different ids to 845 of the
# 5309 printings, so this fetches the same endpoint and caches it here.
RINGSDB_PACKS_URL = "https://ringsdb.com/api/public/packs/"
PACK_DATES_PATH = pathlib.Path(__file__).resolve().parent / "lotrlcg_ringsdb_pack_dates.json"

# identity.rs treats a pack with no release date as printed last.
UNDATED = "9999-99-99"

# HobCardStats, in the order identity.rs joins them.
STAT_FIELDS = [
    "Threat",
    "ThreatCost",
    "ResourceCost",
    "Willpower",
    "Attack",
    "Defense",
    "HitPoints",
    "QuestPoints",
    "EngagementCost",
    "StageNumber",
]

IMAGE_SUFFIXES = {".jpg", ".jpeg", ".png"}

OWN, FILLED, TWIN, WRONG, BLANK = "own", "filled", "twin", "wrong", "blank"
STATES = [OWN, FILLED, TWIN, WRONG, BLANK]

# `games::lotrlcg::adapter`'s card-type to back-group mapping. Two entries at
# one position in one pack are the two faces of a flip card only when they sit
# on the same back: a promo hero numbered 1 alongside quest stage 1 is two
# separate cards, and must stay countable as two.
PLAYER_TYPES = {"Ally", "Attachment", "Contract", "Event", "Hero",
                "Player_Side_Quest", "Treasure"}
QUEST_TYPES = {"Quest", "Campaign", "GenCon_Setup", "Nightmare_Setup"}


def back_group(card_type):
    if card_type in PLAYER_TYPES:
        return "player"
    if card_type in QUEST_TYPES:
        return "quest"
    return "encounter"


# --- identity.rs -----------------------------------------------------------


def canonical_pack_name(ringsdb_name):
    """Mirror of `games::lotrlcg::canonical_pack_name`."""
    cleaned = ringsdb_name.replace("ALeP - ", "").replace(".English", "")
    if cleaned in ("Over Hill and Under Hill", "On the Doorstep"):
        return f"The Hobbit: {cleaned}"
    return cleaned


def load_pack_dates(refresh=False):
    """`{normalized pack id: release date}`, as the adapter builds it."""
    if PACK_DATES_PATH.exists() and not refresh:
        return json.loads(PACK_DATES_PATH.read_text())
    dates = {
        normalize_title(canonical_pack_name(pack["name"])): pack["available"]
        for pack in rename.fetch_json(RINGSDB_PACKS_URL)
    }
    PACK_DATES_PATH.write_text(json.dumps(dates, indent=1, sort_keys=True))
    return dates


def normalized_rules_text(paragraphs):
    out = []
    pending_separator = False
    for ch in rename.unidecode(" ".join(paragraphs or [])).lower():
        if ch.isalnum():
            if pending_separator and out:
                out.append("_")
            out.append(ch)
            pending_separator = False
        else:
            pending_separator = True
    return "".join(out)


def face_key(face):
    if not face:
        return ""
    stats = face.get("Stats")
    joined = (
        ",".join(stats.get(field) or "" for field in STAT_FIELDS)
        if stats is not None
        else ""
    )
    text = normalized_rules_text(face.get("Text"))
    return f"{joined}|{text}|{face.get('Subtitle') or ''}"


def card_identity(card):
    return "|".join(
        [
            normalize_title(card.get("Title") or ""),
            card.get("CardType") or "",
            card.get("Sphere") or "",
            face_key(card.get("Front")),
            face_key(card.get("Back")),
        ]
    )


def printing_card_ids(cards, pack_dates):
    """Each printing's slug mapped to its card's id: the earliest printing's slug."""
    earliest = {}
    for card in cards:
        pack = normalize_title(card["CardSet"])
        candidate = (pack_dates.get(pack, UNDATED), pack, normalize_title(card["Slug"]))
        identity = card_identity(card)
        if identity not in earliest or candidate < earliest[identity]:
            earliest[identity] = candidate
    return {
        normalize_title(card["Slug"]): earliest[card_identity(card)][2] for card in cards
    }


def slug_suffix(slug, title):
    wanted = normalize_title(title)
    end = None
    for i in range(1, len(slug)):
        if normalize_title(slug[:i]) == wanted:
            end = i
            break
    if end is None:
        return None
    rest = slug[end:].lstrip("-").replace("-", " ")
    return rest or None


def card_titles(cards, card_ids):
    """Each card id mapped to its `cards.title`, suffixed when a title names several."""
    raw_slug, title_of = {}, {}
    for card in cards:
        slug = normalize_title(card["Slug"])
        raw_slug.setdefault(slug, card["Slug"])
        card_id = card_ids.get(slug)
        if card_id is not None:
            title_of.setdefault(card_id, card["Title"])

    cards_per_title = collections.defaultdict(set)
    for card_id, title in title_of.items():
        cards_per_title[normalize_title(title)].add(card_id)

    titles = {}
    for card_id, title in title_of.items():
        suffix = None
        if len(cards_per_title[normalize_title(title)]) > 1:
            suffix = slug_suffix(raw_slug.get(card_id, ""), title)
        titles[card_id] = f"{title} ({suffix})" if suffix else title
    return titles


# --- file_naming.rs --------------------------------------------------------


def parse_filename(path):
    """Split `{card_id}@{printing}[~{side}][.bleed]` into its parts."""
    stem = path.stem
    has_bleed = stem.endswith(".bleed")
    if has_bleed:
        stem = stem[: -len(".bleed")]
    if "@" not in stem:
        return None
    card_id, rest = stem.split("@", 1)
    if "@" in rest:
        return None
    if "~" in rest:
        printing, side = rest.split("~", 1)
        if "~" in side:
            return None
    else:
        printing, side = rest, "front"
    return card_id, printing, side, has_bleed


# --- the model Proxy Nexus builds ------------------------------------------


class Printing:
    """One image as `card_store.rs` sees it, after collection_manager linked it."""

    __slots__ = (
        "card_id",
        "collection",
        "date_release",
        "is_official",
        "named",
        "pack_id",
        "position",
        "variant",
        "version",
    )

    def __init__(self, **kw):
        for slot in self.__slots__:
            setattr(self, slot, kw.get(slot))


def resolve_card_and_version(named_id, named_printing, by_printing_id, by_card_pack):
    """Mirror of collection_manager.rs::resolve_card_and_version."""
    printing_hit = by_printing_id.get(named_id)
    card_pack_hit = by_card_pack.get((named_id, named_printing))
    if printing_hit and printing_hit["pack_id"] == named_printing:
        return printing_hit["card_id"], printing_hit
    if card_pack_hit is not None:
        return named_id, card_pack_hit
    if printing_hit:
        return printing_hit["card_id"], None
    return named_id, None


def select_printing(request, printings):
    """Mirror of card_store.rs::select_printing. Returns the winner, or None."""

    def key(p):
        printing_miss = (
            request["printing"] is not None
            and request["printing"] != p.pack_id
            and request["printing"] != p.variant
        )
        collection_miss = (
            request["collection"] is not None and request["collection"] != p.collection
        )
        position_miss = (
            request["position"] is not None and request["position"] != p.position
        )
        id_miss = p.card_id != request["id"]
        return (
            printing_miss,
            collection_miss,
            position_miss,
            id_miss,
            not p.is_official,
            p.date_release is None,
            p.date_release or "",
        )

    return min(printings, key=key) if printings else None


# --- the audit -------------------------------------------------------------


def scan_collections(folders):
    """Every image file in each folder, as (named id, named printing, folder name)."""
    files = []
    for folder in folders:
        if not folder.is_dir():
            print(f"[WARN] no such folder: {folder}")
            continue
        for path in sorted(folder.iterdir()):
            if path.suffix.lower() not in IMAGE_SUFFIXES:
                continue
            parsed = parse_filename(path)
            if parsed is None:
                print(f"[WARN] unparseable filename: {folder.name}/{path.name}")
                continue
            named_id, named_printing, side, _ = parsed
            if side != "front":
                continue  # a back is part of its front's printing, not a card of its own
            files.append((named_id, named_printing, folder.name))
    return files


def build(catalog, pack_dates, files):
    card_ids = printing_card_ids(catalog, pack_dates)
    titles = card_titles(catalog, card_ids)

    versions = {}     # (slug, pack) -> version dict
    by_card_pack = {} # (card id, pack) -> version dict
    packs = collections.defaultdict(list)
    for card in catalog:
        slug = normalize_title(card["Slug"])
        pack = normalize_title(card["CardSet"])
        if (slug, pack) in versions:
            continue
        version = {
            "slug": slug,
            "pack_id": pack,
            "pack_name": card["CardSet"],
            "card_id": card_ids[slug],
            "position": card["Number"],
            "date_release": pack_dates.get(pack),
            "number": card["Number"],
            "type": card["CardType"],
            "back_group": back_group(card["CardType"]),
            "raw_title": card["Title"],
        }
        versions[(slug, pack)] = version
        by_card_pack.setdefault((version["card_id"], pack), version)
        packs[card["CardSet"]].append(version)

    by_printing_id = {}
    for (slug, _), version in versions.items():
        by_printing_id.setdefault(slug, version)

    printings = collections.defaultdict(list)  # title_normalized -> [Printing]
    for named_id, named_printing, collection in files:
        card_id, version = resolve_card_and_version(
            named_id, named_printing, by_printing_id, by_card_pack
        )
        title = titles.get(card_id)
        if title is None:
            continue  # an image for a card this catalog does not list
        printings[normalize_title(title)].append(
            Printing(
                card_id=card_id,
                pack_id=version["pack_id"] if version else None,
                variant=None if version else named_printing,
                position=version["position"] if version else None,
                is_official=version is not None,
                date_release=version["date_release"] if version else None,
                collection=collection,
                named=f"{named_id}@{named_printing}",
                version=version,
            )
        )
    return packs, titles, printings


def mark_twins(states, versions):
    """Re-label a blank that is a flip card's second entry.

    Its other face prints, and the two share one piece of cardboard, so the card
    is not missing -- only Hall of Beorn's second id for it is.
    """
    printed = {
        (v["position"], v["back_group"])
        for v in versions
        if states[v["slug"]][0] in (OWN, FILLED)
    }
    for version in versions:
        state, winner = states[version["slug"]]
        if state == BLANK and (version["position"], version["back_group"]) in printed:
            states[version["slug"]] = (TWIN, winner)


def classify(version, titles, printings):
    """What would print for this card, and which image supplies it."""
    request = {
        "id": version["card_id"],
        "printing": version["pack_id"],
        "collection": None,
        "position": version["position"],
    }
    candidates = printings.get(normalize_title(titles[version["card_id"]]), [])
    winner = select_printing(request, candidates)
    if winner is None:
        return BLANK, None
    if winner.card_id != version["card_id"]:
        return WRONG, winner
    # `own` is this pack's own art. The file may name either the printing slug
    # or the card id plus this pack, and resolve_card_and_version links both to
    # this version, so the version is what decides -- not the filename.
    return (OWN if winner.version is version else FILLED), winner


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("collections", help="Folder holding the lotrlcg-* collection folders")
    parser.add_argument("--include", action="append", default=[], metavar="DIR",
                        help="An extra collection folder, e.g. one not added yet. Repeatable.")
    parser.add_argument("--exclude", action="append", default=[], metavar="NAME",
                        help="Skip a folder under `collections` by name. Repeatable.")
    parser.add_argument("--set", dest="only", metavar="NAME",
                        help="Print every card of one set instead of the summary")
    parser.add_argument("--csv", metavar="PATH", help="Write the per-card rows to a CSV")
    parser.add_argument("--all", action="store_true",
                        help="List complete sets too, not just those with gaps")
    parser.add_argument("--refresh-dates", action="store_true",
                        help="Re-fetch the RingsDB pack dates instead of using the cache")
    args = parser.parse_args()

    root = pathlib.Path(args.collections).expanduser()
    folders, skipped = [], []
    if root.is_dir():
        for folder in sorted(p for p in root.iterdir() if p.is_dir()):
            # A backup sitting beside the collections would be scanned as one,
            # and its images would cover cards the real collection is missing.
            if folder.name in args.exclude or folder.name.endswith(("_bak", "-bak", ".bak", "~")):
                skipped.append(folder.name)
            else:
                folders.append(folder)
    folders += [pathlib.Path(p).expanduser() for p in args.include]
    if not folders:
        sys.exit(f"No collection folders under {root}")

    pack_dates = load_pack_dates(args.refresh_dates)
    catalog = [c for c in load_catalog() if c.get("Slug") and c.get("CardSet")]

    print(f"collections: {', '.join(f.name for f in folders)}")
    if skipped:
        print(f"skipped:     {', '.join(skipped)}")
    files = scan_collections(folders)
    packs, titles, printings = build(catalog, pack_dates, files)
    print(f"{len(files)} front images, {len(catalog)} printings, "
          f"{len(set(titles))} cards, {len(packs)} sets\n")

    rows = []
    summary = {}
    for pack_name, versions in packs.items():
        states = {v["slug"]: classify(v, titles, printings) for v in versions}
        mark_twins(states, versions)
        counts = collections.Counter()
        for version in versions:
            state, winner = states[version["slug"]]
            counts[state] += 1
            rows.append({
                "set": pack_name,
                "number": version["number"],
                "card": version["raw_title"],
                "type": version["type"],
                "card_id": version["card_id"],
                "state": state,
                "printed_from": winner.named if winner else "",
            })
        summary[pack_name] = counts

    if args.only:
        wanted = [r for r in rows if r["set"].lower() == args.only.lower()]
        if not wanted:
            sys.exit(f"No set named {args.only!r}")
        for r in sorted(wanted, key=lambda r: r["number"]):
            flag = "" if r["state"] in (OWN, FILLED, TWIN) else "  <-- "
            print(f"  #{r['number']:<5}{r['card'][:34]:36s}{r['type'][:18]:20s}"
                  f"{r['state']:8s}{flag}{r['printed_from']}")
    else:
        def group(pred):
            return {p: c for p, c in summary.items() if pred(c)}

        misprints = group(lambda c: c[WRONG])
        gaps = group(lambda c: not c[WRONG] and c[BLANK])
        borrowed = group(lambda c: not c[WRONG] and not c[BLANK] and c[FILLED] and not c[OWN])
        part = group(lambda c: not c[WRONG] and not c[BLANK] and c[FILLED] and c[OWN])
        green = group(lambda c: not c[WRONG] and not c[BLANK])

        def show(title, packs):
            if not packs:
                return
            print(f"{title} ({len(packs)})")
            width = max(len(p) for p in packs)
            for pack in sorted(packs, key=lambda p: (-summary[p][WRONG], -summary[p][BLANK], p)):
                c = summary[pack]
                prints = c[OWN] + c[FILLED]
                bits = " ".join(f"{c[s]} {s}" for s in (WRONG, BLANK, FILLED) if c[s])
                print(f"   {pack:{width}s}  {prints:4d} of {sum(c.values()):<4d} print   {bits}")
            print()

        # A set is named by what is wrong with it, never by its worst card: a
        # pack missing one objective still prints the other 83.
        show("SETS PRINTING A WRONG CARD", misprints)
        show("sets with cards that do not print", gaps)
        show("prints in full, entirely from other sets", borrowed)
        show("prints in full, partly from other sets", part)
        if args.all:
            show("complete", green)
        else:
            print(f"complete ({len(green)}) — pass --all to list them\n")

        total = collections.Counter()
        for c in summary.values():
            total.update(c)
        print("all sets: " + "  ".join(f"{total[s]} {s}" for s in STATES))

    if args.csv:
        with open(args.csv, "w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
            writer.writeheader()
            writer.writerows(rows)
        print(f"\nper-card rows written to {args.csv}")


if __name__ == "__main__":
    main()
