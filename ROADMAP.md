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
- ✅ A run of cross-fades stays under the track above it: from the fifth
  shot of a Long video the pictures climbed onto the captions' lane in
  the export (#22).

## In progress

The owner's plan after alpha.15 (28 Oct): the stages below are built one
after another on the branch, each tested locally and committed there, and
go up as **one** pull request and one release at the end - a branch push
runs no CI, so nothing is built until then.

- 🚧 **Stage 1: export quality.** The owner's alpha.15 export of a 60 s
  1080p video came out at 0.83 Mbit/s - soft, smeared detail. Raise what a
  Long video exports at, and make its pictures sharper: Replicate's
  stills are 1344x768, enlarged to 1920x1080 and then by the Ken Burns
  move.
- ⏳ **Stage 2: sound.** That video had no audio stream at all: make sure
  narration (the built-in voices, ElevenLabs) works on the phone, and add
  background music chosen to the mood.
- ⏳ **Stage 3: no repeated pictures.** Two shots of one scene came out
  near the same picture (scenes 3-4, 9-10): ask the writer for different
  framings, and offer Make again on a picture too like its neighbour.
- ⏳ **Stage 4: captions timed to the voice word by word**: today a
  card's time is shared out by its length in letters; reading the
  narration back with the local transcriber would place each word where
  it is said.
- ⏳ **Stage 5: publishing**: a thumbnail, and a title, description and
  tags for YouTube, written by the writer.
- ⏳ **Stage 6: interface strings**: many Long video lines are still
  English in ar.json, es.json and the other locales.

## Done since alpha.14

- ✅ Pictures saved as clips (#23): Replicate's image addresses carry no
  file type and a shot was saved as `.mp4`; a download is named for what
  its bytes hold, and a misnamed shot is renamed when the video is cut
  again.

## Planned

- ⏳ **Real test with paid keys**: a full run with Replicate (images and
  clips) and ElevenLabs on the phone, by the owner; fix what it shows.

## Decided against

- ❌ **HeyGen presenter avatar**: too expensive per minute (about $1-4).
  Revisit only if a much cheaper talking-head service appears.

## Notes for whoever continues

- Releases: tag `v0.2.8-alpha.N` on `main` as a pre-release; the Release
  workflow builds everything and signs the Android APK with the owner's
  key (repository secrets). Never create a new signing key: updates would
  stop installing over the old app.
- The owner tests on an Android phone and writes in Arabic.
