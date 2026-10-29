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

- ✅ **Stage 1: export quality** (on the branch). A phone's encoder ran
  its VBR far under the target on stills - 0.83 Mbit/s for a 60 s 1080p
  video asked for 9: the quality setting now runs as CBR on a phone, at
  three fifths of what the CRF implies (5.5 Mbit/s at 1080p30). A still
  is enlarged with Lanczos rather than bilinear when it is decoded - once,
  so it costs nothing - which keeps the edges of a 1344x768 picture
  brought up to 1080p (about a sixth more edge detail, measured).
- ✅ **Stage 2: sound** (on the branch). That video had no audio stream:
  the owner narrates with ElevenLabs only (no new free voices). The Music
  list gained "Made for this video": with real shots on Replicate, the
  writer describes a bed from the moods, title and first lines, and
  MusicGen (`meta/musicgen`, about $0.08) makes 30 s of it as
  `music.mp3` in the job's folder, made once and kept; the cut loops it
  under the whole video, ducked. A model that is not an official one on
  Replicate now falls back to its latest version, and a file's real kind
  (mp3, wav, m4a ...) is read from its bytes. Checked in the phone shell
  with a stand-in file; the paid MusicGen call waits on the owner's test.
- ✅ **Stage 3: no repeated pictures** (on the branch). Two shots of one
  scene came out near the same picture (scenes 3-4, 9-10). The writer is
  told that within a scene each shot shows something the others do not.
  Every picture made is read for its likeness (a 64-bit difference hash
  and its overall colour; colour cards and other washes are passed
  over); one too like the shot before it is marked "Too like the shot
  before" in the script editor, where Suggest and Make again give it
  another, and a run that made any says so when it ends. Nothing is
  made again on its own: that is paid for.
- ✅ **Stage 4: the voice and its captions** (on the branch). The
  owner's 29 Oct video stopped for about a second between paragraphs:
  each ElevenLabs file's own silence at both ends plus the 0.6 s pad.
  ElevenLabs is now asked with timestamps (the same price), which tells
  when every letter is said; the cut starts each narration just before
  its first word and ends it just after its last, the pause between
  paragraphs is 0.35 s, and each caption card comes up as its first
  word is said (`voice/<scene>.words.json`). A voice with no timings -
  the built-in ones, or ElevenLabs lines made before this - is trimmed
  to where its waveform sounds. Each line is also sent with the lines
  either side (`previous_text` / `next_text`), so one paragraph runs on
  from the last in the same voice rather than starting afresh.
- ✅ **Stage 5: publishing** (on the branch). "Write for YouTube" in the
  Long video sheet asks the writer (about $0.01) for a title, a
  description, 10-15 tags in the video's language and English, and a
  thumbnail's words and picture. The description gets the chapters'
  start times from the cut, as YouTube reads them (three or more, the
  first at 0:00, none under ten seconds). Each has a Copy button, and
  all of it is kept as `publishing.txt` in the video's folder. The
  thumbnail is laid on a timeline of its own, 1280x720: its picture
  made for it on Replicate (one still's price) or, without Replicate,
  the video's first picture, with the words large in yellow over it -
  editable like any title; Save Video Frame As makes the picture.
- ✅ **Stage 6: interface strings** (on the branch). Arabic is complete
  (0 lines left in English); Spanish and French have every line
  translated but words that are the same in both (Audio, Normal,
  Idea...). The owner chose to stop there for now.
- ⏳ **Later: the other 21 locales.** The Long video sheet and the script
  editor are still English in them: 110 lines each.

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
