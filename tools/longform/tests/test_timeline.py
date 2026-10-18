"""The plan's arithmetic: `python3 -m unittest discover tools/longform/tests`."""
from __future__ import annotations

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

from studio import storyboard, timeline  # noqa: E402


def board():
    return {
        "title": "Test",
        "aspect": "16:9",
        "style": {"visual": "warm light"},
        "chapters": [
            {"title": "One", "scenes": [
                {"narration": "First sentence here. Second one.", "onScreenText": "Chapter one",
                 "seconds": 6, "shots": [{"prompt": "a"}, {"prompt": "b"}]},
                {"narration": "", "seconds": 2, "shots": [{"prompt": "c"}]},
            ]},
        ],
    }


class Plan(unittest.TestCase):
    def setUp(self):
        self.board = board()
        storyboard.validate(self.board)

    def plan(self, **options):
        voice = {"c01s01": ("v1.wav", 5.0)}
        shots = {"c01s01": [("a.mp4", 4.0), ("b.mp4", 6.0)], "c01s02": [("c.png", None)]}
        return timeline.plan(self.board, voice, shots, timeline.Options(**options), ("m.mp3", 4.0))

    def test_shots_fill_each_scene_back_to_back(self):
        shots = [p for p in self.plan() if p.kind == "shot"]
        # Scene one is the narration plus its pad; its time is shared by
        # the shots' own lengths, 4:6.
        self.assertAlmostEqual(shots[0].length, 5.6 * 0.4)
        self.assertAlmostEqual(shots[1].start, shots[0].length)
        self.assertAlmostEqual(shots[1].start + shots[1].length, 5.6)
        # Scene two has no narration: its seconds, floored at min_scene.
        self.assertAlmostEqual(shots[2].start, 5.6)
        self.assertAlmostEqual(shots[2].length, 3.0)
        self.assertTrue(shots[2].still)

    def test_captions_cover_the_narration_exactly(self):
        captions = [p for p in self.plan() if p.kind == "caption"]
        self.assertEqual([c.text for c in captions], ["First sentence here.", "Second one."])
        self.assertAlmostEqual(captions[0].start, 0.0)
        self.assertAlmostEqual(captions[-1].start + captions[-1].length, 5.0)

    def test_music_loops_to_the_end_and_no_further(self):
        music = [p for p in self.plan() if p.kind == "music"]
        self.assertEqual([m.start for m in music], [0.0, 4.0, 8.0])
        self.assertAlmostEqual(music[-1].start + music[-1].length, 8.6)

    def test_options_turn_words_off(self):
        kinds = {p.kind for p in self.plan(captions=False, titles=False)}
        self.assertEqual(kinds, {"shot", "voice", "music"})

    def test_a_scene_without_files_is_refused(self):
        with self.assertRaises(ValueError):
            timeline.plan(self.board, {}, {"c01s01": [("a.mp4", 4.0)]}, timeline.Options())


class Captions(unittest.TestCase):
    def test_long_sentences_split_evenly(self):
        cards = timeline.caption_chunks(" ".join(f"w{i}" for i in range(10)), 7)
        self.assertEqual([len(c.split()) for c in cards], [5, 5])

    def test_arabic_question_mark_ends_a_sentence(self):
        self.assertEqual(timeline.caption_chunks("ما هذا؟ نجم.", 7), ["ما هذا؟", "نجم."])


class Board(unittest.TestCase):
    def test_ids_are_filled_and_the_style_is_appended(self):
        b = board()
        storyboard.validate(b)
        self.assertEqual([s["id"] for _, s in storyboard.scenes(b)], ["c01s01", "c01s02"])
        self.assertEqual(storyboard.shot_prompt(b, {"prompt": "a dune."}), "a dune. warm light")
        self.assertIn("`c01s01_2`", storyboard.prompt_sheet(b))

    def test_a_scene_without_shots_is_refused(self):
        b = board()
        b["chapters"][0]["scenes"][0]["shots"] = []
        with self.assertRaises(ValueError):
            storyboard.validate(b)


if __name__ == "__main__":
    unittest.main()
