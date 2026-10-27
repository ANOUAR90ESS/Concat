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
- ✅ Make one shot again, phase 1: Make again under every shot in the
  script editor, with its picture, and on a generated clip on the
  timeline; a new take in `takes/`, swapped in place as one undo step
  (#16). Known gap: after Undo the timeline shows the earlier take, but the
  script editor's picture and a later full re-cut use the newest take.
- ✅ Make one shot again, phase 2: Suggest another - the writer proposes a
  new picture for one shot to read, edit and Make again. Editing a shot's
  words keeps what was paid for (marked in `stale.txt`, made again as a
  take); a clip the timeline will not take is left out and named rather
  than ending the cut; no Open folder on a phone (#17).

## In progress

- 🚧 **Step 5a: animated captions** (this pull request) - a Captions
  style choice in the sheet: Classic (a sentence on a dark plate) or
  Animated (two or three words at a time, big and bold, each card
  popping in, every other one in the accent yellow), in the style of
  short-form video.

## Planned

- ⏳ **Step 5b: Shorts** - a 9:16 cut of the best minute of a long video,
  its pictures reframed to fill the tall frame, with animated captions.
- ⏳ **Captions timed to the voice word by word**: today a card's time is
  shared out by its length in letters; reading the narration back with
  the local transcriber would place each word where it is said.
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
