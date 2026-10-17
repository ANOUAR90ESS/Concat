"""The edit: every file on the timeline, through the Concat API, then the
export.

The project is left behind as a Concat project folder, so whatever the
automatic cut gets wrong is fixed in the editor, not by running this again.
"""
from __future__ import annotations

import pathlib
import re
import shutil
import sys

from . import providers, storyboard, timeline
from .concat_api import Concat, ConcatError

# How far a shot shorter than its slot may be slowed to fill it. Slower
# than this looks like a mistake, so the shot is held on its last frame.
SLOWEST = 0.5

# Sizes are fractions of the frame's height, widths of its width, as
# Concat's titles measure them.
CAPTION = {
    "fontSize": 0.048, "fontWeight": 700, "color": "#ffffff",
    "background": "#000000a6", "backgroundRadius": 0.01,
    "backgroundPaddingX": 0.022, "backgroundPaddingY": 0.012, "shadow": False,
}
TITLE = {"fontSize": 0.075, "fontWeight": 800, "color": "#ffffff", "shadow": True, "maxWidth": 0.86}


def media_lengths(api: Concat, files: list[pathlib.Path]) -> dict[str, float | None]:
    """Each file's length in seconds, None for a still, by Concat's own probe."""
    out = {}
    for path in files:
        summary = api.call("media.probe", path=str(path))
        out[str(path)] = None if summary["kind"] == "image" else float(summary["duration"] or 0)
    return out


def edit(job: pathlib.Path, board: dict, options: timeline.Options, music: pathlib.Path | None,
         output: pathlib.Path, cli: str | None, force: bool, verbose: bool) -> pathlib.Path:
    name = re.sub(r'[\\/:*?"<>|]+', " ", board.get("title") or "Long video").strip()[:80]
    project = job / "concat" / name
    if project.exists():
        if not force:
            raise SystemExit(f"{project} exists: it may hold hand edits. Pass --force to rebuild it.")
        shutil.rmtree(project)
    project.parent.mkdir(parents=True, exist_ok=True)

    with Concat(cli, verbose) as api:
        voice_files, shot_files = {}, {}
        for _chapter, scene in storyboard.scenes(board):
            found = providers.existing(job / "voice", scene["id"], providers.VOICE_SUFFIXES)
            if found:
                voice_files[scene["id"]] = found
            shot_files[scene["id"]] = []
            for n in range(1, len(scene["shots"]) + 1):
                path = providers.existing(job / "visuals", f"{scene['id']}_{n}", providers.SHOT_SUFFIXES)
                if not path:
                    raise SystemExit(f"missing shot {scene['id']}_{n} in {job / 'visuals'}: run `visuals` first")
                shot_files[scene["id"]].append(path)

        everything = list(voice_files.values()) + [p for ps in shot_files.values() for p in ps]
        if music:
            everything.append(music)
        print(f"edit: probing {len(everything)} files", file=sys.stderr)
        lengths = media_lengths(api, everything)

        placements = timeline.plan(
            board,
            {sid: (str(p), lengths[str(p)]) for sid, p in voice_files.items()},
            {sid: [(str(p), lengths[str(p)]) for p in ps] for sid, ps in shot_files.items()},
            options,
            (str(music), lengths[str(music)]) if music else None,
        )
        width, height = storyboard.frame(board)
        api.call("project.create", location=str(project.parent), name=name,
                 video={"width": width, "height": height, "rateNum": 30, "rateDen": 1})
        cut(api, str(project), placements, options)
        api.call("project.save", path=str(project))
        print(f"edit: project saved to {project}", file=sys.stderr)

        output.parent.mkdir(parents=True, exist_ok=True)
        started = api.call("export.run", path=str(project), output=str(output.resolve()))
        total = timeline.total_length(placements)
        print(f"edit: exporting {total / 60:.1f} min to {output}", file=sys.stderr)

        def progress(params):
            if params.get("total"):
                print(f"\r  {params.get('stage', '')} {100 * params['frame'] / params['total']:5.1f}%",
                      end="", file=sys.stderr)

        try:
            api.wait_for(started["job"], progress)
        finally:
            print(file=sys.stderr)
    return output


def cut(api: Concat, project: str, placements: list[timeline.Placement], options: timeline.Options) -> None:
    media: dict[str, str] = {}

    def media_id(file: str) -> str:
        if file not in media:
            media[file] = api.call("media.import", path=project, file=file)["createdId"]
        return media[file]

    shots = [p for p in placements if p.kind == "shot"]
    # Media first, words last: a caption lands on the first lane free above
    # whatever is under it, so laid before the music it would take the
    # music's lane and push the bed off the end.
    order = ("shot", "voice", "music", "title", "caption")
    placements = sorted(placements, key=lambda p: order.index(p.kind))
    captions = None
    for i, p in enumerate(placements):
        if p.kind in ("shot", "voice", "music"):
            clip = api.edit(project, {"op": "addClip", "mediaId": media_id(p.file),
                                      "trackId": p.track, "start": p.start})
            fit(api, project, clip, p, media_id(p.file))
            patch = {}
            if p.kind == "shot":
                n = shots.index(p)
                if n and options.transition:
                    patch["transitionIn"] = {"id": options.transition, "duration": options.transition_seconds}
                if p.still and options.ken_burns:
                    ken_burns(api, project, clip, n)
                if n == 0 or n == len(shots) - 1:
                    fade(api, project, clip, p.length, options.fade_seconds, first=n == 0,
                         last=n == len(shots) - 1)
            elif p.kind == "music":
                patch["volume"] = options.music_volume
                if p is [m for m in placements if m.kind == "music"][-1]:
                    patch["fadeOut"] = min(3.0, p.length)
            if patch:
                api.edit(project, {"op": "updateClip", "clipId": clip, "patch": patch})
        elif p.kind == "title":
            api.edit(project, {"op": "addTextClip", "trackId": p.track, "start": p.start,
                               "duration": p.length, "offsetY": -0.3,
                               "style": {"content": p.text, **TITLE}})
        elif p.kind == "caption":
            if captions is None:
                # A lane of their own on top, so every caption sits over
                # everything and the lane can be hidden in one click.
                captions = api.edit(project, {"op": "addTrack"})
            api.edit(project, {"op": "addTextClip", "trackId": captions, "start": p.start,
                               "duration": p.length, "offsetY": 0.38,
                               "style": {"content": p.text, **CAPTION}})
        if i % 25 == 0:
            print(f"edit: {i + 1}/{len(placements)} clips", file=sys.stderr)


def fit(api: Concat, project: str, clip: str, p: timeline.Placement, media: str) -> None:
    """Makes a clip exactly as long as its slot: trimmed when the file is
    longer, stretched when it is a still, and slowed (down to SLOWEST) when
    it is a shorter video - which then plays again from its top for
    whatever is still left, the way an editor loops b-roll."""
    natural = p.source_length if p.source_length else 5.0
    if p.kind == "shot" and not p.still and natural < p.length:
        speed = max(natural / p.length, SLOWEST)
        api.edit(project, {"op": "setClipSpeed", "clipId": clip, "speed": speed})
        natural /= speed
    delta = p.length - natural
    if delta < -1e-3 or (p.still and abs(delta) > 1e-3):
        api.edit(project, {"op": "trimClip", "clipId": clip, "edge": "end", "delta": delta})
    elif delta > 0.05 and p.kind == "shot":
        again = api.edit(project, {"op": "addClip", "mediaId": media, "trackId": p.track,
                                   "start": p.start + natural})
        fit(api, project, again, timeline.Placement(p.kind, p.track, p.start + natural, delta,
                                                    p.file, p.source_length), media)


def ken_burns(api: Concat, project: str, clip: str, n: int) -> None:
    """A slow push in or pull out on a still, alternating, so a run of
    stills reads as camera moves."""
    a, b = (1.0, 1.12) if n % 2 == 0 else (1.12, 1.0)
    for at, value in ((0.0, a), (1.0, b)):
        api.edit(project, {"op": "setClipKey", "clipId": clip, "property": "scale", "at": at, "value": value})


def fade(api: Concat, project: str, clip: str, length: float, seconds: float, first: bool, last: bool) -> None:
    """The film's fade up from black and down to it, as opacity keys."""
    edge = min(seconds / length, 0.45) if length > 0 else 0
    keys = []
    if first:
        keys += [(0.0, 0.0), (edge, 1.0)]
    if last:
        keys += [(1.0 - edge, 1.0), (1.0, 0.0)]
    for at, value in keys:
        api.edit(project, {"op": "setClipKey", "clipId": clip, "property": "opacity", "at": at, "value": value})


__all__ = ["edit", "ConcatError"]
