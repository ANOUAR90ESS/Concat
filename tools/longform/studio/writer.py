"""The script: a topic becomes a storyboard, written by Claude.

A long video is written the way a writers' room would: an outline first,
then each chapter on its own, every call carrying the outline and how the
chapter before it ended. One call could not hold an hour of narration in
its output, and chapter by chapter keeps each call short enough to be
good. Each call answers through a forced tool, so what comes back is the
schema's JSON and never prose to be parsed.
"""
from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request

API = "https://api.anthropic.com/v1/messages"
DEFAULT_MODEL = "claude-sonnet-5-5"

# Narration speed, words a second, for sizing a scene from its seconds.
WORDS_PER_SECOND = 2.4

OUTLINE_TOOL = {
    "name": "outline",
    "description": "The video's outline: its look, its voice, and its chapters.",
    "input_schema": {
        "type": "object",
        "properties": {
            "title": {"type": "string"},
            "visual_style": {
                "type": "string",
                "description": "One line in English appended to every shot prompt so all "
                               "shots look like one film: medium, lens, light, palette, grade.",
            },
            "voice_style": {"type": "string", "description": "How the narrator sounds."},
            "chapters": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "summary": {"type": "string", "description": "What this chapter covers and why it comes here."},
                        "seconds": {"type": "number"},
                    },
                    "required": ["title", "summary", "seconds"],
                },
            },
        },
        "required": ["title", "visual_style", "voice_style", "chapters"],
    },
}

SCENES_TOOL = {
    "name": "scenes",
    "description": "One chapter's scenes, in order.",
    "input_schema": {
        "type": "object",
        "properties": {
            "scenes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "narration": {"type": "string"},
                        "onScreenText": {
                            "type": ["string", "null"],
                            "description": "A short title shown over the scene's start, or null.",
                        },
                        "seconds": {"type": "number"},
                        "shots": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {"prompt": {"type": "string"}},
                                "required": ["prompt"],
                            },
                        },
                    },
                    "required": ["narration", "seconds", "shots"],
                },
            },
            "ending": {"type": "string", "description": "One sentence: where this chapter leaves the viewer."},
        },
        "required": ["scenes", "ending"],
    },
}

SYSTEM = """You write long-form videos for YouTube: documentaries, explainers, stories.
Narration is written to be heard, not read: short sentences, concrete images,
a hook in the first ten seconds, a question or a turn that pulls the viewer
into the next chapter. Never pad.

Shot prompts are for a text-to-video model and are always in English, whatever
the narration's language. Each prompt is one shot of 4 to 8 seconds: subject,
action, setting, camera move, framing, light. No text, logos or captions in
the picture, no real people's names. Describe; do not tell the model what to
avoid."""


def call(tool: dict, prompt: str, model: str, max_tokens: int = 16000) -> dict:
    """One Messages API call answered through `tool`; the tool's input."""
    key = os.environ.get("ANTHROPIC_API_KEY")
    if not key:
        raise SystemExit("ANTHROPIC_API_KEY is not set (or write storyboard.json by hand)")
    body = {
        "model": model,
        "max_tokens": max_tokens,
        "system": SYSTEM,
        "tools": [tool],
        "tool_choice": {"type": "tool", "name": tool["name"]},
        "messages": [{"role": "user", "content": prompt}],
    }
    request = urllib.request.Request(
        API,
        data=json.dumps(body).encode(),
        headers={
            "content-type": "application/json",
            "x-api-key": key,
            "anthropic-version": "2023-06-01",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=600) as response:
            reply = json.load(response)
    except urllib.error.HTTPError as error:
        raise SystemExit(f"Claude API: {error.code} {error.read().decode(errors='replace')}")
    for block in reply.get("content", []):
        if block.get("type") == "tool_use":
            return block["input"]
    raise SystemExit(f"Claude API: no {tool['name']} in the reply: {reply}")


def write(topic: str, minutes: float, language: str, aspect: str, audience: str | None,
          notes: str | None, model: str = DEFAULT_MODEL) -> dict:
    """A whole storyboard for `topic`, about `minutes` long."""
    brief = (f"Topic: {topic}\nLength: about {minutes:g} minutes\n"
             f"Narration language: {language}\nAspect ratio: {aspect}\n")
    if audience:
        brief += f"Audience: {audience}\n"
    if notes:
        brief += f"Notes from the producer: {notes}\n"

    chapters = max(3, round(minutes * 60 / 150))
    print(f"script: outlining {chapters} chapters", file=sys.stderr)
    outline = call(OUTLINE_TOOL, brief + f"\nOutline the video in about {chapters} chapters "
                   "whose seconds add up to the length.", model)

    board = {
        "title": outline["title"],
        "language": language,
        "aspect": aspect,
        "style": {"visual": outline["visual_style"], "voice": outline["voice_style"]},
        "chapters": [],
    }
    summary = "\n".join(f"{i}. {c['title']} ({c['seconds']:g} s): {c['summary']}"
                        for i, c in enumerate(outline["chapters"], 1))
    ending = "This is the first chapter: open with the hook."
    for i, chapter in enumerate(outline["chapters"], 1):
        print(f"script: chapter {i}/{len(outline['chapters'])}: {chapter['title']}", file=sys.stderr)
        words = round(chapter["seconds"] * WORDS_PER_SECOND)
        got = call(SCENES_TOOL, brief + (
            f"\nThe outline:\n{summary}\n\nVisual style: {outline['visual_style']}\n"
            f"Narrator: {outline['voice_style']}\n\n"
            f"Write chapter {i}, \"{chapter['title']}\": about {chapter['seconds']:g} seconds, "
            f"so about {words} words of narration in {language}, split into scenes of 6 to 20 "
            "seconds. Give each scene one shot per 4 to 8 seconds of its length. "
            f"Where the previous chapter left off: {ending}"
        ), model)
        board["chapters"].append({"title": chapter["title"], "scenes": got["scenes"]})
        ending = got["ending"]
    return board
