"""Where the narration and the shots come from.

Every provider writes one file to a path it is given and nothing else, so
any of them can be swapped for another, or for a file made by hand, without
the edit step noticing. A file already there is never made twice: a long
video's generation is resumable, and a shot you replaced by hand stays.

Voices:  none, command, elevenlabs
Shots:   manual, placeholder, command, replicate

`command` is the escape hatch for anything local - Piper or Kokoro for a
voice, ComfyUI, Wan or LTX-Video for shots: a shell template with the
fields below filled in, each one shell-quoted.

    voice:  {text} {text_file} {out} {scene}
    shots:  {prompt} {out} {seconds} {width} {height} {aspect} {scene} {shot}
"""
from __future__ import annotations

import json
import os
import pathlib
import shlex
import subprocess
import sys
import time
import urllib.request

from . import storyboard

SHOT_SUFFIXES = (".mp4", ".mov", ".webm", ".mkv", ".png", ".jpg", ".jpeg", ".webp")
VOICE_SUFFIXES = (".wav", ".mp3", ".m4a", ".flac", ".ogg")


def existing(folder: pathlib.Path, stem: str, suffixes) -> pathlib.Path | None:
    for suffix in suffixes:
        path = folder / f"{stem}{suffix}"
        if path.exists() and path.stat().st_size > 0:
            return path
    return None


def run_template(template: str, fields: dict) -> None:
    command = template.format(**{k: shlex.quote(str(v)) for k, v in fields.items()})
    print(f"  $ {command}", file=sys.stderr)
    subprocess.run(command, shell=True, check=True)


def fetch(url: str, out: pathlib.Path, headers: dict | None = None) -> None:
    request = urllib.request.Request(url, headers=headers or {})
    with urllib.request.urlopen(request, timeout=600) as response, open(out, "wb") as file:
        while chunk := response.read(1 << 20):
            file.write(chunk)


# Voices ---------------------------------------------------------------------

def voices(board: dict, folder: pathlib.Path, provider: str, args) -> None:
    """Speaks every scene's narration into `folder`/<scene>.<ext>."""
    if provider == "none":
        return
    folder.mkdir(parents=True, exist_ok=True)
    for _chapter, scene in storyboard.scenes(board):
        text = (scene.get("narration") or "").strip()
        if not text or existing(folder, scene["id"], VOICE_SUFFIXES):
            continue
        print(f"voice: {scene['id']}", file=sys.stderr)
        if provider == "command":
            out = folder / f"{scene['id']}{args.voice_ext}"
            text_file = folder / f"{scene['id']}.txt"
            text_file.write_text(text, encoding="utf-8")
            run_template(args.voice_cmd, {"text": text, "text_file": text_file,
                                          "out": out, "scene": scene["id"]})
        elif provider == "elevenlabs":
            elevenlabs(text, folder / f"{scene['id']}.mp3", args.voice_id, args.voice_model)
        else:
            raise SystemExit(f"unknown voice provider {provider}")


def elevenlabs(text: str, out: pathlib.Path, voice_id: str, model: str) -> None:
    key = os.environ.get("ELEVENLABS_API_KEY")
    if not key or not voice_id:
        raise SystemExit("elevenlabs needs ELEVENLABS_API_KEY and --voice-id")
    request = urllib.request.Request(
        f"https://api.elevenlabs.io/v1/text-to-speech/{voice_id}",
        data=json.dumps({"text": text, "model_id": model}).encode(),
        headers={"xi-api-key": key, "content-type": "application/json", "accept": "audio/mpeg"},
    )
    with urllib.request.urlopen(request, timeout=600) as response:
        out.write_bytes(response.read())


# Shots ----------------------------------------------------------------------

def shots(board: dict, folder: pathlib.Path, provider: str, args) -> list[str]:
    """Makes every shot into `folder`/<scene>_<n>.<ext>; the stems still
    missing afterwards (only `manual` leaves any)."""
    folder.mkdir(parents=True, exist_ok=True)
    width, height = storyboard.frame(board)
    missing = []
    for _chapter, scene in storyboard.scenes(board):
        count = len(scene["shots"])
        for n, shot in enumerate(scene["shots"], 1):
            stem = f"{scene['id']}_{n}"
            if existing(folder, stem, SHOT_SUFFIXES):
                continue
            seconds = round(float(scene.get("seconds") or 5) / count, 1)
            prompt = storyboard.shot_prompt(board, shot)
            if provider == "manual":
                missing.append(stem)
                continue
            print(f"shot: {stem}", file=sys.stderr)
            if provider == "placeholder":
                placeholder(folder / f"{stem}.mp4", seconds, width, height, n)
            elif provider == "command":
                run_template(args.shot_cmd, {
                    "prompt": prompt, "out": folder / f"{stem}{args.shot_ext}",
                    "seconds": seconds, "width": width, "height": height,
                    "aspect": board.get("aspect", "16:9"), "scene": scene["id"], "shot": n,
                })
            elif provider == "replicate":
                replicate(prompt, folder, stem, seconds, board.get("aspect", "16:9"), args)
            else:
                raise SystemExit(f"unknown shot provider {provider}")
    return missing


def placeholder(out: pathlib.Path, seconds: float, width: int, height: int, n: int) -> None:
    """A moving gradient, for cutting an animatic before paying for shots."""
    subprocess.run([
        "ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi",
        "-i", f"gradients=s={width}x{height}:d={max(seconds, 1)}:r=30:speed=0.02:seed={n * 7919}",
        "-pix_fmt", "yuv420p", "-c:v", "libx264", "-preset", "veryfast", str(out),
    ], check=True)


def replicate(prompt: str, folder: pathlib.Path, stem: str, seconds: float, aspect: str, args) -> None:
    """Any Replicate model: `--replicate-model owner/name`, and its input as
    a JSON template whose string values may hold {prompt}, {seconds} and
    {aspect}."""
    token = os.environ.get("REPLICATE_API_TOKEN")
    if not token or not args.replicate_model:
        raise SystemExit("replicate needs REPLICATE_API_TOKEN and --replicate-model")
    fields = {"prompt": prompt, "seconds": seconds, "aspect": aspect}

    def fill(value):
        if value == "{seconds}":
            # Most models take the length as a number, not a string.
            return seconds
        if isinstance(value, str):
            return value.format(**fields)
        if isinstance(value, dict):
            return {k: fill(v) for k, v in value.items()}
        if isinstance(value, list):
            return [fill(v) for v in value]
        return value

    headers = {"authorization": f"Bearer {token}", "content-type": "application/json"}
    body = {"input": fill(json.loads(args.replicate_input))}
    request = urllib.request.Request(
        f"https://api.replicate.com/v1/models/{args.replicate_model}/predictions",
        data=json.dumps(body).encode(), headers=headers)
    with urllib.request.urlopen(request, timeout=120) as response:
        prediction = json.load(response)
    while prediction["status"] not in ("succeeded", "failed", "canceled"):
        time.sleep(5)
        with urllib.request.urlopen(urllib.request.Request(prediction["urls"]["get"], headers=headers),
                                    timeout=120) as response:
            prediction = json.load(response)
    if prediction["status"] != "succeeded":
        raise SystemExit(f"replicate {stem}: {prediction['status']}: {prediction.get('error')}")
    output = prediction["output"]
    url = output[0] if isinstance(output, list) else output
    suffix = pathlib.PurePosixPath(url.split("?")[0]).suffix or ".mp4"
    fetch(url, folder / f"{stem}{suffix}")
