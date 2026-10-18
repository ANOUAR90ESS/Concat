"""The cut, planned: where every file lands on the timeline.

Planning is pure arithmetic over durations, kept apart from the API so it
can be read and tested on its own. The edit step then turns each
placement into Concat commands.

Lanes, bottom to top:

    T1  the shots, back to back, a cross-fade on each cut
    T2  the narration, one file a scene
    T3  the music bed, looped and ducked under the voice
    T4  scene titles
    t.  captions, on a lane of their own minted on top
"""
from __future__ import annotations

import math
import re
from dataclasses import dataclass, field

from . import storyboard

VISUALS, VOICE, MUSIC, TITLES = "T1", "T2", "T3", "T4"

STILL_SUFFIXES = (".png", ".jpg", ".jpeg", ".webp")


@dataclass
class Options:
    """The house style of the cut."""

    transition: str | None = "cross-fade"
    transition_seconds: float = 0.5
    # Silence after each scene's narration, so a cut never clips a word.
    scene_pad: float = 0.6
    # A scene with no narration, or a very short one, lasts at least this.
    min_scene: float = 3.0
    captions: bool = True
    caption_words: int = 7
    titles: bool = True
    title_seconds: float = 3.0
    ken_burns: bool = True
    music_volume: float = 0.12
    fade_seconds: float = 1.0


@dataclass
class Placement:
    """One clip to put on the timeline."""

    kind: str  # "shot", "voice", "music", "title", "caption"
    track: str | None
    start: float
    length: float
    file: str | None = None
    source_length: float | None = None
    text: str | None = None
    still: bool = False
    extras: dict = field(default_factory=dict)


def caption_chunks(text: str, words: int) -> list[str]:
    """The narration as caption cards: sentences, then at most `words`
    words a card, so a card is read in a glance."""
    chunks = []
    for sentence in re.split(r"(?<=[.!?؟。])\s+", text.strip()):
        tokens = sentence.split()
        if not tokens:
            continue
        cards = math.ceil(len(tokens) / words)
        per = math.ceil(len(tokens) / cards)
        chunks += [" ".join(tokens[i:i + per]) for i in range(0, len(tokens), per)]
    return chunks


def plan(board: dict, voice: dict[str, tuple[str, float]], shots: dict[str, list[tuple[str, float | None]]],
         options: Options, music: tuple[str, float] | None = None) -> list[Placement]:
    """Every placement, in timeline order.

    `voice` maps a scene id to its narration file and that file's length
    in seconds (absent: no narration). `shots` maps a scene id to its files in shot order, each
    with its length (None for a still). `music` is a file and its length.
    """
    out: list[Placement] = []
    t = 0.0
    for _chapter, scene in storyboard.scenes(board):
        sid = scene["id"]
        voice_path, spoken = voice.get(sid, (None, None))
        if spoken:
            length = max(spoken + options.scene_pad, options.min_scene)
        else:
            length = max(float(scene.get("seconds") or 0), options.min_scene)
        files = shots.get(sid) or []
        if not files:
            raise ValueError(f"scene {sid} has no shot files")

        # The scene's time shared out over its shots, weighted by how long
        # each shot runs, so a 10 s clip is not squeezed to match a 4 s one.
        weights = [src if src else length / len(files) for _, src in files]
        total = sum(weights)
        at = t
        for i, (path, src) in enumerate(files):
            slot = length - (at - t) if i == len(files) - 1 else length * weights[i] / total
            out.append(Placement("shot", VISUALS, at, slot, file=path, source_length=src,
                                 still=src is None, extras={"scene": sid, "index": len(out)}))
            at += slot

        if spoken:
            out.append(Placement("voice", VOICE, t, spoken, file=voice_path))
            if options.captions and scene.get("narration"):
                cards = caption_chunks(scene["narration"], options.caption_words)
                chars = sum(len(card) for card in cards)
                at = t
                for card in cards:
                    span = spoken * len(card) / chars
                    out.append(Placement("caption", None, at, span, text=card))
                    at += span
        if options.titles and scene.get("onScreenText"):
            out.append(Placement("title", TITLES, t, min(options.title_seconds, length),
                                 text=scene["onScreenText"]))
        t += length

    if music:
        path, src = music
        at = 0.0
        while at < t - 0.05:
            out.append(Placement("music", MUSIC, at, min(src, t - at), file=path, source_length=src))
            at += src
    return out


def total_length(placements: list[Placement]) -> float:
    return max((p.start + p.length for p in placements), default=0.0)
