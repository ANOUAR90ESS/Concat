# LongCut roadmap

What is planned for Long video (File ▸ Long video) and what is done. Kept in
the repository so the plan outlives any one conversation: whoever picks the
work up - a person or an assistant - reads this first, and updates it in the
same pull request that moves an item.

Status: ✅ done · 🚧 in progress · ⏳ planned · ❌ decided against

## Done

- ✅ Script editor: the storyboard scene by scene, edited before a shot is
  paid for (#10).
- ✅ Image to video: each clip moves a still made first from its prompt;
  a cost estimate and a spend limit (#11).
- ✅ Claude's answer read as given, and asked again for what it left out
  (#12).
- ✅ Captions without narration (#13); empty lanes taken away (#14).
- ✅ A run cut short carries on where it stopped without paying again; the
  running view shows the last shot, the steps and what was spent (#15).

## In progress

- 🚧 **Make one shot again** (phase 1, this pull request): a "Make again"
  button under every shot in the script editor, with its picture, and on
  a generated clip selected on the timeline (the phone's clip tools, the
  desk's clip menu). Only that shot is made again, as a new take in
  `takes/`; it replaces the old one in place as one undo step, and no file
  already made is moved or overwritten.
  - Known gap: after Undo the timeline shows the earlier take, but the
    script editor's picture and a later full re-cut use the newest take.

## Planned

- ⏳ **Make one shot again, phase 2**: "Suggest another" - Claude writes a
  better prompt for that one shot before it is made again (costs almost
  nothing).
- ⏳ **Step 5: animated captions and Shorts** - word-by-word captions in
  the style of short-form video, and a 9:16 cut of the best minute of a
  long video.
- ⏳ **Real test with paid keys**: a full run with Replicate (images and
  clips) and ElevenLabs on the phone, by the owner; fix what it shows.
- ⏳ **Spanish and other interface strings**: many Long video lines are
  still English in es.json and the other locales.
- ⏳ **Keep a run alive in the background on Android**: a foreground
  service so leaving the app does not pause a run (today it resumes when
  reopened, which loses nothing but time).

## Decided against

- ❌ **HeyGen presenter avatar**: too expensive per minute (about $1-4).
  Revisit only if a much cheaper talking-head service appears.

## Notes for whoever continues

- Releases: tag `v0.2.8-alpha.N` on `main` as a pre-release; the Release
  workflow builds everything and signs the Android APK with the owner's
  key (repository secrets). Never create a new signing key: updates would
  stop installing over the old app.
- The owner tests on an Android phone and writes in Arabic.
