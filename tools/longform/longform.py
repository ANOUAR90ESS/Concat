#!/usr/bin/env python3
"""A long video from a topic: script, prompts, voice, shots, then a Concat cut.

    longform.py script  JOB --topic "..." --minutes 10 --language ar
    longform.py prompts JOB                       # JOB/prompts.md, one prompt a shot
    longform.py voice   JOB --voice elevenlabs --voice-id ...
    longform.py visuals JOB --shots replicate --replicate-model owner/name
    longform.py edit    JOB [--music bed.mp3]     # JOB/concat/<title>, JOB/final.mp4
    longform.py run     JOB --topic "..." ...     # all of the above, in order

JOB is a folder; every step reads what the one before it left there and
skips what is already made, so a step can be re-run, and any file in it
replaced by hand, at any point. See README.md beside this file.
"""
from __future__ import annotations

import argparse
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from studio import editor, providers, storyboard, timeline, writer  # noqa: E402


def board_path(job: pathlib.Path) -> pathlib.Path:
    return job / "storyboard.json"


def step_script(args) -> None:
    path = board_path(args.job)
    if path.exists() and not args.force:
        print(f"script: {path} exists, keeping it (--force to rewrite)", file=sys.stderr)
        return
    if not args.topic:
        raise SystemExit("script needs --topic")
    board = writer.write(args.topic, args.minutes, args.language, args.aspect,
                         args.audience, args.notes, args.model)
    args.job.mkdir(parents=True, exist_ok=True)
    storyboard.save(board, path)
    count = sum(1 for _ in storyboard.scenes(board))
    shots = sum(len(s["shots"]) for _, s in storyboard.scenes(board))
    print(f"script: {count} scenes, {shots} shots -> {path}", file=sys.stderr)


def step_prompts(args) -> None:
    board = storyboard.load(board_path(args.job))
    out = args.job / "prompts.md"
    out.write_text(storyboard.prompt_sheet(board), encoding="utf-8")
    print(f"prompts: {out}", file=sys.stderr)


def step_voice(args) -> None:
    board = storyboard.load(board_path(args.job))
    providers.voices(board, args.job / "voice", args.voice, args)


def step_visuals(args) -> None:
    board = storyboard.load(board_path(args.job))
    missing = providers.shots(board, args.job / "visuals", args.shots, args)
    if missing:
        print(f"visuals: {len(missing)} shots to make by hand (see prompts.md), "
              f"into {args.job / 'visuals'}:", file=sys.stderr)
        for stem in missing:
            print(f"  {stem}", file=sys.stderr)
        raise SystemExit(1)


def step_edit(args) -> None:
    board = storyboard.load(board_path(args.job))
    options = timeline.Options(
        transition=None if args.transition == "none" else args.transition,
        captions=not args.no_captions,
        titles=not args.no_titles,
        ken_burns=not args.no_ken_burns,
        music_volume=args.music_volume,
    )
    out = editor.edit(args.job, board, options, args.music, args.output or args.job / "final.mp4",
                      args.concat_cli, args.force, args.verbose)
    print(f"done: {out}")


def step_run(args) -> None:
    step_script(args)
    step_prompts(args)
    step_voice(args)
    step_visuals(args)
    args.force = True
    step_edit(args)


def parser() -> argparse.ArgumentParser:
    top = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = top.add_subparsers(dest="step", required=True)

    def common(p):
        p.add_argument("job", type=pathlib.Path, help="the job folder")
        p.add_argument("--force", action="store_true", help="rewrite what is already there")

    def script_args(p):
        p.add_argument("--topic")
        p.add_argument("--minutes", type=float, default=10)
        p.add_argument("--language", default="en", help="narration language, e.g. ar, en, fr")
        p.add_argument("--aspect", default="16:9", choices=list(storyboard.ASPECTS))
        p.add_argument("--audience")
        p.add_argument("--notes", help="anything else the writer should know")
        p.add_argument("--model", default=writer.DEFAULT_MODEL)

    def voice_args(p):
        p.add_argument("--voice", default="none", choices=["none", "command", "elevenlabs"])
        p.add_argument("--voice-cmd", help="shell template: {text} {text_file} {out} {scene}")
        p.add_argument("--voice-ext", default=".wav")
        p.add_argument("--voice-id")
        p.add_argument("--voice-model", default="eleven_multilingual_v2")

    def visual_args(p):
        p.add_argument("--shots", default="manual",
                       choices=["manual", "placeholder", "command", "replicate"])
        p.add_argument("--shot-cmd", help="shell template: {prompt} {out} {seconds} {width} {height} {aspect}")
        p.add_argument("--shot-ext", default=".mp4")
        p.add_argument("--replicate-model", help="owner/name of a text-to-video model")
        p.add_argument("--replicate-input", default='{"prompt": "{prompt}"}',
                       help="the model's input as JSON; strings may use {prompt} {seconds} {aspect}")

    def edit_args(p):
        p.add_argument("--music", type=pathlib.Path, help="a music bed, looped under the whole video")
        p.add_argument("--music-volume", type=float, default=0.12)
        p.add_argument("--transition", default="cross-fade", help="a transition id, or none")
        p.add_argument("--no-captions", action="store_true")
        p.add_argument("--no-titles", action="store_true")
        p.add_argument("--no-ken-burns", action="store_true")
        p.add_argument("--output", type=pathlib.Path)
        p.add_argument("--concat-cli", help="the concat-cli binary")
        p.add_argument("--verbose", action="store_true", help="print every API call")

    for name, func, extra in (
        ("script", step_script, [script_args]),
        ("prompts", step_prompts, []),
        ("voice", step_voice, [voice_args]),
        ("visuals", step_visuals, [visual_args]),
        ("edit", step_edit, [edit_args]),
        ("run", step_run, [script_args, voice_args, visual_args, edit_args]),
    ):
        p = sub.add_parser(name)
        common(p)
        for add in extra:
            add(p)
        p.set_defaults(func=func)
    return top


def main() -> None:
    args = parser().parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
