# Concat Studio: long videos from a topic

[العربية](README.ar.md)

A pipeline that takes a topic and leaves behind a finished video and the
Concat project it was cut in:

```
topic ──script──▶ storyboard.json ──prompts──▶ prompts.md
                         │
                         ├──voice────▶ voice/<scene>.wav|mp3
                         ├──visuals──▶ visuals/<scene>_<n>.mp4|png
                         ▼
                       edit ──▶ concat/<title>/  (open it in Concat)
                            └─▶ final.mp4
```

- **script**: Claude writes the video chapter by chapter: an outline first,
  then each chapter with the outline and the previous chapter's ending in
  hand, so an hour of narration stays coherent. Every scene gets narration,
  an optional on-screen title, and one English text-to-video prompt per
  4–8 s shot. A single visual-style line is appended to every prompt so the
  shots look like one film.
- **prompts**: every shot's prompt on one sheet, with the file name to save
  it as, for generating shots by hand in whatever tool you like.
- **voice**: the narration, one file a scene. ElevenLabs, or any local TTS
  through a command template (Piper, Kokoro, …), or none.
- **visuals**: the shots. Any Replicate text-to-video model, any local
  generator through a command template (ComfyUI, Wan, LTX-Video, …),
  gradient placeholders for an animatic, or by hand.
- **edit**: the montage, done by Concat through its API (`concat-cli api`):
  shots back to back with cross-fades, each one trimmed, slowed or looped to
  fill its scene; the narration under them; a looped music bed at low
  volume with a fade-out; chapter titles; captions timed to the narration;
  a slow push-in or pull-out on stills; a fade up from and down to black.
  Then the export.

Every step skips what already exists, so it can be re-run after a failure,
and any file can be replaced by hand: regenerate one bad shot by deleting
it, or drop in your own footage under the same name. The storyboard is
plain JSON (see `example/storyboard.json`) and can be written or edited
by hand too.

## Requirements

- Python 3.10+, standard library only.
- `concat-cli`, built from `src/`:
  `cargo build --profile quick -p concat-cli` (see `src/README.md` for the
  FFmpeg it needs). Found on `PATH`, through `CONCAT_CLI`, `--concat-cli`, or
  under `src/target/`.
- `ANTHROPIC_API_KEY` for `script`.
- Whatever your chosen voice and shot providers need
  (`ELEVENLABS_API_KEY`, `REPLICATE_API_TOKEN`, a local tool).
- `ffmpeg` on `PATH` only for `--shots placeholder`.

## Use

All at once:

```sh
python3 tools/longform/longform.py run jobs/desert \
  --topic "The lost cities of the Arabian desert" --minutes 12 --language ar \
  --voice elevenlabs --voice-id <id> \
  --shots replicate --replicate-model <owner>/<text-to-video-model> \
  --replicate-input '{"prompt": "{prompt}", "aspect_ratio": "{aspect}"}' \
  --music bed.mp3
```

Or step by step, checking each step's output before paying for the next:

```sh
L=tools/longform/longform.py
python3 $L script  jobs/desert --topic "..." --minutes 12 --language ar
# read and edit jobs/desert/storyboard.json
python3 $L prompts jobs/desert
python3 $L voice   jobs/desert --voice command \
  --voice-cmd 'piper -m ar_JO-kareem-medium.onnx -f {out} < {text_file}'
python3 $L visuals jobs/desert --shots placeholder   # an animatic first
python3 $L edit    jobs/desert                       # jobs/desert/final.mp4
# happy with the pacing: delete the placeholders, make the real shots
rm jobs/desert/visuals/*
python3 $L visuals jobs/desert --shots command \
  --shot-cmd 'my-generator --prompt {prompt} --seconds {seconds} --size {width}x{height} --out {out}'
python3 $L edit    jobs/desert --music bed.mp3 --force
```

`edit` refuses to overwrite a project that is already there, since it may
hold your hand edits; `--force` rebuilds it.

### Options worth knowing

| Option | Step | Default |
|---|---|---|
| `--aspect 16:9\|9:16\|1:1\|4:5` | script | 16:9 |
| `--model` | script | `claude-sonnet-5-5` |
| `--audience`, `--notes` | script | |
| `--transition <id>\|none` | edit | `cross-fade`; also the built-ins `fade-black`, `fade-white`, `push`, `zoom`, `wipe-left`, `wipe-right`, or any id from `concat-cli api '{"method":"catalogue.list","kind":"transition"}'` |
| `--no-captions`, `--no-titles`, `--no-ken-burns` | edit | |
| `--music-volume` | edit | 0.12 |
| `--output` | edit | `JOB/final.mp4` |
| `--verbose` | edit | prints every API call |

### Command templates

Fields are shell-quoted before they are substituted:

- voice: `{text}` `{text_file}` `{out}` `{scene}`
- shots: `{prompt}` `{out}` `{seconds}` `{width}` `{height}` `{aspect}` `{scene}` `{shot}`

## The timeline

| Lane | What |
|---|---|
| T1 | shots |
| T2 | narration |
| T3 | music |
| T4 | chapter titles |
| top (minted) | captions |

A scene lasts as long as its narration plus 0.6 s (at least 3 s); with no
narration, its `seconds`. Its shots share that time in proportion to their
own lengths. A shot longer than its slot is trimmed; a shorter one is
slowed down to half speed, then loops from its start.

## Tests

```sh
python3 -m unittest discover tools/longform/tests
```
