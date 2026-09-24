import importlib.util
import pathlib
import sys
from collections import defaultdict

import numpy as np

_here = pathlib.Path(__file__).resolve().parent.parent
# The scripts sit beside each other and import each other by name, which is
# what running them from their own folder gives them.
sys.path.insert(0, str(_here))


def _load(name):
    spec = importlib.util.spec_from_file_location(name, _here / f"{name}.py")
    assert spec and spec.loader, f"could not load {name}.py"
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


fix_orientation = _load("fix_orientation")
rename_proxies = _load("rename_proxies")
report_coverage = _load("report_coverage")


def title_on_right(seed):
    """A profile with a bright title band on its right and some art noise."""
    rng = np.random.default_rng(seed)
    profile = 0.4 + 0.1 * rng.random(150)
    profile[-20:-5] += 0.4
    profile[30:60] += 0.2 * rng.random()
    return profile


class TestClassify:
    def test_each_image_is_placed_by_which_side_its_title_band_is_on(self):
        profiles = {f"r{i}": title_on_right(i) for i in range(6)}
        profiles.update({f"l{i}": title_on_right(10 + i)[::-1] for i in range(4)})
        right, scores = fix_orientation.classify(profiles)
        assert all(right[n] for n in profiles if n.startswith("r"))
        assert not any(right[n] for n in profiles if n.startswith("l"))
        assert all(abs(s) > fix_orientation.UNSURE for s in scores.values())

    def test_the_title_side_is_found_when_the_first_guess_is_wrong(self):
        # A dark title band inverts the brightness guess; the template still
        # sides with the majority it builds from.
        profiles = {f"r{i}": title_on_right(i) for i in range(6)}
        odd = title_on_right(99)
        odd[-20:-5] -= 0.6
        odd[5:20] += 0.3
        profiles["odd"] = odd
        right, _ = fix_orientation.classify(profiles)
        assert all(right[f"r{i}"] for i in range(6))


class TestPrintedFace:
    def test_a_back_shows_the_linked_face(self):
        by_code = {
            "01097a": {"code": "01097a", "type_code": "main_scheme", "linked_to_code": "01097b"},
            "01097b": {"code": "01097b", "type_code": "main_scheme"},
        }
        assert fix_orientation.printed_face("01097a@core~back.jpg", by_code)["code"] == "01097b"
        assert fix_orientation.printed_face("01097a@core.jpg", by_code)["code"] == "01097a"

    def test_a_bleed_name_is_read_the_same_way(self):
        by_code = {
            "01097a": {"code": "01097a", "type_code": "main_scheme", "linked_to_code": "01097b"},
            "01097b": {"code": "01097b", "type_code": "main_scheme"},
        }
        assert fix_orientation.printed_face("01097a@core.bleed.jpg", by_code)["code"] == "01097a"
        assert (fix_orientation.printed_face("01097a@core~back.bleed.jpg", by_code)["code"]
                == "01097b")



class TestProxyNames:
    def test_a_front_and_a_back_of_one_card(self):
        assert rename_proxies.output_name("13001a", "front", "wsp") == "13001a@wsp.bleed.jpg"
        assert rename_proxies.output_name("13001a", "back", "wsp") == "13001a@wsp~back.bleed.jpg"

    def test_the_generic_backs_and_second_copies_are_left_out(self, tmp_path):
        by_code = {"13001a": {"code": "13001a", "pack_code": "wsp"}}
        (tmp_path / "Wasp").mkdir()
        (tmp_path / "_Shared Backs").mkdir()
        for name in ("Wasp/13001a_front.png", "Wasp/13001a_front(1).png",
                     "Wasp/00000_front.png", "Wasp/notes.txt",
                     "_Shared Backs/Hero Back.png"):
            (tmp_path / name).write_bytes(b"")
        reports = defaultdict(list)

        written = rename_proxies.collect(str(tmp_path), by_code, reports)

        assert set(written) == {"13001a@wsp.bleed.jpg"}
        assert reports["Extra copies of one file"] == ["Wasp/13001a_front(1).png"]
        assert reports["Codes MarvelCDB does not list"] == ["Wasp/00000_front.png"]
        assert reports["Card backs, which live in the adapter"] == ["_Shared Backs/Hero Back.png"]

    def test_a_two_faced_card_with_only_a_front_is_reported(self):
        by_code = {
            "04160a": {"code": "04160a", "pack_code": "trors", "linked_to_code": "04160b"},
            "26002": {"code": "26002", "pack_code": "vision", "back_name": "Dense"},
            "04164": {"code": "04164", "pack_code": "trors"},
            "13001a": {"code": "13001a", "pack_code": "wsp", "linked_to_code": "13001b"},
        }
        written = {"04160a@trors.bleed.jpg": "a", "26002@vision.bleed.jpg": "b",
                   "04164@trors.bleed.jpg": "c",
                   "13001a@wsp.bleed.jpg": "d", "13001a@wsp~back.bleed.jpg": "e"}
        reports = defaultdict(list)

        rename_proxies.report_one_sided(written, by_code, reports)

        # The single-faced card and the complete pair say nothing.
        assert reports["Two-faced cards the source has the front of only"] == [
            "04160a@trors", "26002@vision"]

    def test_an_alt_art_is_a_printing_of_its_own(self):
        assert rename_proxies.alt_printing("Gambit") == "alt_gambit"
        assert rename_proxies.alt_printing("Age of Apocalypse") == "alt_age_of_apocalypse"
        match = rename_proxies.ALT_FACE.match("08013_front Stealth Strike (Gambit)")
        assert match.groups() == ("08013", "front", "Gambit")
        assert rename_proxies.output_name("08013", "front", "alt_gambit") \
            == "08013@alt_gambit.bleed.jpg"



class TestWantedFaces:
    def test_a_face_is_wanted_per_side_and_only_for_out_of_print_packs(self):
        cards = [
            {'code': '04160a', 'pack_code': 'trors', 'linked_to_code': '04160b'},
            {'code': '04164', 'pack_code': 'trors'},
            {'code': '26002', 'pack_code': 'vision', 'back_name': 'Dense'},
            {'code': '50001a', 'pack_code': 'aos', 'linked_to_code': '50001b'},
        ]

        faces = report_coverage.wanted_faces(cards)

        assert set(faces) == {'04160a@trors', '04160a@trors~back', '04164@trors',
                              '26002@vision', '26002@vision~back'}
        assert faces['26002@vision~back'][1] == 'back'

    def test_the_faces_wanted_are_the_ones_printed_as_cards(self):
        cards = [
            {'code': '01097', 'pack_code': 'gob', 'hidden': False},
            {'code': '01097a', 'pack_code': 'gob', 'hidden': False, 'linked_to_code': '01097b'},
            {'code': '01097b', 'pack_code': 'gob', 'hidden': True},
            {'code': '31002a', 'pack_code': 'spdr', 'hidden': True, 'linked_to_code': '31002b'},
            {'code': '31002b', 'pack_code': 'spdr', 'hidden': True},
        ]

        printed = [c['code'] for c in report_coverage.printed_cards(cards)]

        assert printed == ['01097a', '31002a']
