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
- ✅ Step 5a: animated captions - Caption style Classic or Animated (two
  or three words a card, big and outlined, popping in, white and yellow)
  (#18).
- ✅ Step 5b: Shorts - Make a Short, once every shot of a video is made:
  the writer picks the minute most worth a Short (scenes in a row, 60 s
  at most) and a hook, or the opening minute is taken; cut 9:16 from the
  files already made, pictures filling the tall frame, captions animated
  (#19).
- ✅ A run goes on in the background on Android: a foreground service
  under a notification, with a wake lock; CI declares it in the APK
  (#20). Back still closes the app and ends the run.
- ✅ Fixes from the phone test: Make a Short on a line of its own; the
  script editor's thumbnails read by content and handed over six at a
  time; a stood-in colour card kept and the shot made again as a take,
  so a cut using it still exports (#21).

## In progress

- 🚧 **Captions and pictures shown together all through** (this pull
  request): from the fifth shot or so of a run of cross-fades, a caption
  showed over black or a picture with no caption, on the monitor and in
  the export alike. Each cross-fade puts the incoming shot a lane above
  the one it crosses, and the export spread a track over two lanes only,
  so a long run of shots climbed onto the captions' lane and over it. A
  track now gets as many lanes as it has cuts, plus one.

## Planned

- ⏳ **Captions timed to the voice word by word**: today a card's time is
  shared out by its length in letters; reading the narration back with
  the local transcriber would place each word where it is said.
- ⏳ **Real test with paid keys**: a full run with Replicate (images and
  clips) and ElevenLabs on the phone, by the owner; fix what it shows.
- ⏳ **Spanish and other interface strings**: many Long video lines are
  still English in es.json and the other locales.

## Decided against

- ❌ **HeyGen presenter avatar**: too expensive per minute (about $1-4).
  Revisit only if a much cheaper talking-head service appears.

## Notes for whoever continues

- Releases: tag `v0.2.8-alpha.N` on `main` as a pre-release; the Release
  workflow builds everything and signs the Android APK with the owner's
  key (repository secrets). Never create a new signing key: updates would
  stop installing over the old app.
- The owner tests on an Android phone and writes in Arabic.
