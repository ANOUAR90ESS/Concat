"""The storyboard: what the video says and shows, scene by scene.

A storyboard is plain JSON, so it can be written by Claude (`script`),
by hand, or by Claude and then by hand:

    {
      "title": "...",
      "language": "ar",
      "aspect": "16:9",
      "style": {"visual": "cinematic, 35mm, warm light", "voice": "calm"},
      "chapters": [
        {"title": "...", "scenes": [
          {"id": "c01s01",
           "narration": "what the voice says over this scene",
           "onScreenText": "a title over the scene's start, or null",
           "seconds": 8,
           "shots": [{"prompt": "what the camera sees"}, ...]}
        ]}
      ]
    }

A scene is as long as its narration takes to say; its shots share that
time. The visual style is appended to every shot's prompt, so a hundred
shots generated one at a time still look like one film.
"""
from __future__ import annotations

import json
import pathlib

ASPECTS = {"16:9": (1920, 1080), "9:16": (1080, 1920), "1:1": (1080, 1080), "4:5": (1080, 1350)}


def load(path: pathlib.Path) -> dict:
    board = json.loads(path.read_text(encoding="utf-8"))
    validate(board)
    return board


def save(board: dict, path: pathlib.Path) -> None:
    validate(board)
    path.write_text(json.dumps(board, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def validate(board: dict) -> None:
    """Fills ids and refuses a storyboard the later steps could not use."""
    if board.get("aspect", "16:9") not in ASPECTS:
        raise ValueError(f"aspect must be one of {', '.join(ASPECTS)}")
    chapters = board.get("chapters")
    if not chapters:
        raise ValueError("storyboard has no chapters")
    seen = set()
    for c, chapter in enumerate(chapters, 1):
        for s, scene in enumerate(chapter.get("scenes") or [], 1):
            scene.setdefault("id", f"c{c:02d}s{s:02d}")
            if scene["id"] in seen:
                raise ValueError(f"scene id {scene['id']} is used twice")
            seen.add(scene["id"])
            if not scene.get("shots"):
                raise ValueError(f"scene {scene['id']} has no shots")
            for shot in scene["shots"]:
                if not shot.get("prompt"):
                    raise ValueError(f"scene {scene['id']} has a shot with no prompt")


def scenes(board: dict):
    """Every scene in order, with its chapter."""
    for chapter in board["chapters"]:
        for scene in chapter["scenes"]:
            yield chapter, scene


def shot_prompt(board: dict, shot: dict) -> str:
    """The prompt a generator gets: the shot, then the film's look."""
    style = (board.get("style") or {}).get("visual")
    return f"{shot['prompt'].rstrip('. ')}. {style}" if style else shot["prompt"]


def frame(board: dict) -> tuple[int, int]:
    return ASPECTS[board.get("aspect", "16:9")]


def prompt_sheet(board: dict) -> str:
    """Markdown: every shot's prompt under its scene, with the file name the
    visuals step looks for, for generating shots by hand."""
    lines = [f"# {board.get('title', 'Storyboard')}", ""]
    lines.append(f"Aspect {board.get('aspect', '16:9')}. Save each shot as the file named, "
                 "in the job's `visuals/` folder (.mp4, .mov, .webm, .png or .jpg).")
    for chapter in board["chapters"]:
        lines += ["", f"## {chapter.get('title', '')}"]
        for scene in chapter["scenes"]:
            lines += ["", f"### {scene['id']} ({scene.get('seconds', '?')} s)", ""]
            if scene.get("narration"):
                lines += [f"> {scene['narration']}", ""]
            for n, shot in enumerate(scene["shots"], 1):
                lines.append(f"- `{scene['id']}_{n}`: {shot_prompt(board, shot)}")
    return "\n".join(lines) + "\n"
