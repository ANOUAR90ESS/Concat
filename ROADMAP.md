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
- ✅ **Export on a phone** (on the branch). While a file is written the
  export sheet shows the picture, a progress ring, the stage and the time
  left instead of the settings, and a green tick once it is. A phone
  holds the run service while it exports - before, leaving the app froze
  the export - with the progress as a bar in the notification, told from
  the export's worker.
- ⏳ **Later: the other 21 locales.** The Long video sheet and the script
  editor are still English in them: 110 lines each.
- ⏳ **Ideas the owner raised (29 Oct), not started:** accounts and
  saved work in the cloud (Supabase-style: sign-in, scripts and settings
  synced, keys on a server; the videos stay on the phone or are an
  option); faster making (shots and voice lines made several at a time)
  and a measured look at where a phone's export spends its time; topic
  and content suggestions that learn from the owner's own videos and,
  once signed in to YouTube, from how they did.

## After alpha.16 (owner's test, 29 Oct)

Built on the branch overnight and sent up as one PR, as before. The
owner's 1:16 export came out at 0.79 Mbit/s where 5.5 were asked for -
the phone's chip ignored the CBR of stage 1 - and took about 7 minutes.

- ✅ **A: export quality on a phone.** The H.264/HEVC encoder is also
  given a quantiser ceiling (I/P/B at CRF+4/+6/+8: 24/26/28), a floor
  under the picture whatever the chip does with the bitrate (read from
  Android 12 on). The log says what the encoder was asked for; the done
  sheet shows the file's real size; and a phone file under 40% of the
  estimate is said to have come out soft. Needs the owner's phone to
  confirm.
- ✅ **B: making faster.** The shots asked of Replicate are made four
  at a time (each takes its price from a purse the run shares, so the
  spend limit still holds), ElevenLabs lines two at a time; what each
  came to is seen to in the order they finish. A failure that ends the
  run - the limit, a refused key - starts no more. Waits that took one
  after the other now overlap: about three to four times faster on
  Replicate.
- ✅ **D: Play and Share.** The done sheet has Play - the phone's own
  player on the video it just put in Movies (ACTION_VIEW), the default
  player on a desk - and, on a phone, Share, the system's sheet to send
  it to YouTube, a messenger or the cloud (ACTION_SEND).
- ✅ **F: the making sheet redesigned.** No empty picture box: while
  the script and the voice are made, the export's progress ring; once
  shots come, the last one made, large. The writer says which chapter
  it is writing by its title, shown under the stage; the steps sit in a
  card.
- ✅ **C: export faster (first part).** An export logs where its time
  goes every half minute and at the end - reading the sources, drawing,
  writing to the encoder, per frame. And a certain win: a still shown
  for seconds was scaled (Lanczos) and converted again for every one of
  its frames; it is now scaled once and handed out again. The rest waits
  on the owner's log from the phone.
- ✅ **E: keep using the app.** The making sheet and the export sheet
  have Hide while they run: a pill floats over the window - the editor,
  the list of projects - with the job and how far it is, and a press
  opens the sheet again. A Long video run belongs to its job's folder,
  not to the open project (`runs.rs`): it goes on when the person opens
  another project, holding the phone awake itself; the sheet opened again
  on that job shows it going on and will not start a second run, which
  would pay twice; when it ends elsewhere a toast says to open its
  project and press Create video, which cuts it without paying again.

## After alpha.17 (owner's test, 30 Oct)

alpha.17 works, but a 72 s 1080x1920 export still came out at 0.82
Mbit/s, as alpha.16 did: the owner's chip kept neither the CBR nor the
quantiser ceiling. And a one-minute video was slow to export.

- 🚧 **1: the log from a phone.** Settings ▸ Show the log needed a file
  manager a phone does not have; on a phone it is Share the log, which
  hands the system block and the end of this run's log - about 90 KB,
  headed with the phone's model, Android version and its H.264/HEVC
  encoders - to a messenger or mail. The encoders are also written to
  the log at start.
- 🚧 **2: export quality, again.** Where the phone's first encoder of a
  kind says it takes a constant quality (CQ), the quality setting asks it
  for one - 77 of 0-100 at CRF 20 - instead of a bitrate, and falls back
  to the bitrate where the chip will not open so. Otherwise the chip is
  asked for all the CRF implies (9.2 Mbit/s at 1080p30, not three fifths)
  and its quantiser floor is set low (CRF-8), out of the way of a chip
  whose own floor may sit where it wrote everything. Needs the owner's
  phone, and its log, to confirm.
- 🚧 **2b: what alpha.18 showed** (owner's HONOR ALT-NX1, a Snapdragon
  680 with an Adreno 613, Android 15). The export sheet always asked the
  encoder for an *average* bitrate, which a phone kept as VBR - none of
  stage 1's CBR, alpha.17's ceiling or alpha.18's constant quality ever
  reached the chip; 77 s came out at 0.89 Mbit/s. A phone now takes the
  sheet's bitrate as the quality it comes to (8 Mbit/s at 1080p30 is
  about CRF 21). Its chip has no constant quality in H.264 and keeps it
  for HEVC in a component of its own, `c2.qti.hevc.encoder.cq`, after
  the one Android picks: that one is asked for by name, at 79 of 0-100,
  and the export sheet writes HEVC on such a phone until another codec
  is picked. Share the log also carries the run before this one, where
  an export is when the app was opened again to send it.
- 🚧 **2c: what alpha.19 showed.** The chip took the constant quality
  (`quality 79, bitrate_mode=cq codec_name=c2.qti.hevc.encoder.cq`): a
  clear step up, still short of YouTube and TikTok. Asked for 95 now.
- 🚧 **3a: export speed, the writer on its own thread.** A 3:17 export
  took 9:44 on the Adreno 613, 10 frames a second: reading 17 ms,
  drawing 43 ms and writing 33 ms a frame, one after the other. The
  encoder now runs on a thread of its own a few frames behind the
  drawing, so a frame costs about what reading and drawing do: about
  1.5 times faster. Reading the next frame while this one is drawn is
  next.
- ⏳ **Voice and captions out of step in the first minute; the sound
  dipping or gone for a second now and then** (owner, 3-minute video):
  waits on when and where, and a short recording.
- ⏳ **3: export speed.** Waits on the owner's log: its "export done"
  line says how the time splits between reading, drawing and writing.
- ⏳ **If the chip still keeps nothing:** a software H.264 encoder
  (OpenH264) in the phone's FFmpeg as a "best quality, slower" choice.

## The writer as a content writer (owner, 31 Oct)

The owner found the scripts general - not professional, academic or
educational enough to compete with channels - and every picture in the
same teal and amber. Three stages, built one after another on the
branch and sent up as one pull request and one release.

- 🚧 **1: a professional writer.** A Content style choice in the Long
  video sheet - Documentary, Educational, Academic, Story, News analysis
  - each with its own brief to the writer and its own kind of pictures.
  The writer's standing instructions are a head writer's: a cold open,
  the film's question within twenty seconds, a loop opened every minute
  or so, chapters that end on a turn, a payoff; concrete over abstract,
  exact facts or none; a list of phrases every machine script uses, in
  English and Arabic, never written; vowel marks only where a voice would
  misread. After each chapter an editor's pass tightens the words at the
  same length (a scene that drifts keeps the draft, which fits its
  pictures). The film has one look - its medium chosen for the topic -
  and each chapter its own named palette; teal and amber are one palette
  among many. Shot prompts follow one order (subject and action, setting,
  framing and lens, light, mood), clips one camera move.
- 🚧 **2: the channel and the facts.** A "Your channel" section in the
  sheet - name, audience, voice, opening line, closing line, a script to
  write like - kept and given to the writer with every video. Research
  on the web first (on by default, about ten cents): before the outline
  the writer reads up with Anthropic's web search (up to eight searches)
  and writes notes that cite their sources by number; the outline and
  every chapter build on them and give no figure that is not there or
  certain. The sources are kept in the storyboard and listed under the
  YouTube description. A model without the newer search falls back to
  the first one; research that fails leaves the writing as before.
- 🚧 **3: data cards and a better picture model.** A scene that turns on
  a number, a date or a comparison carries a card - the figure and a
  label - which the cut draws large in yellow on a dark plate, popping
  in a moment into the scene for up to four and a half seconds, on a
  lane of its own under the captions. A Pictures choice beside the image
  model: Economical (flux-schnell) or Professional (flux-1.1-pro, about
  four cents a picture), which sets the model and its price.

## Simple, true and well shown (owner, 7 Nov)

After the content writer (#29) the owner asked for scripts that explain
better - simplify without falsifying - and for better picture prompts,
from what science communicators and the models' own guides recommend.

- 🚧 **Explaining.** An Audience choice - Beginner, General, Informed -
  tells the writer how much the viewer knows. Its standing brief now
  guards against the curse of knowledge: every term in plain words
  first, one everyday analogy per hard idea carried through, the bottom
  line before the detail, at most three steps a chapter, comparisons the
  viewer can picture over bare numbers, sentences under twenty words;
  scenes joined by "but" and "so", not "and then"; a reminder between
  open loops of what is coming.
- 🚧 **Facts.** The research reads the two or three key pages in full
  (web fetch, up to four) besides searching, and brings back how the
  best teachers explain the topic - its clearest analogy and the common
  misconception. The editor's pass checks every number, date, name and
  quote against those notes and corrects or removes what they do not
  support, and listens as a newcomer: unexplained terms, skipped steps,
  breathless sentences.
- ✅ **"The writer returned an outline with no chapters"** (owner, alpha.22;
  fixed in alpha.23, #31).
  A tool answer can give a list as text - in a code fence, or after a
  line of words - and the outline then read as having no chapters. Such
  text is decoded into its list, and a list field given as anything else
  is asked for again instead of being taken as empty.
- 🚧 **Pictures.** Image prompts of 40-70 words of natural prose, the
  lens with its aperture, at most two style words of their own (FLUX's
  guidance); clip motion under twenty words that does not describe the
  picture again - what moves, then one camera move at the end (the
  image-to-video guidance); an abstract idea shown through a physical
  thing that stands for it.

## After alpha.23 (owner's test, 8 Nov)

The owner liked the voice but heard words said wrongly in Arabic, and the
window stopped answering ("LongCut is not responding") the first time a
new video was played, the title saying unsaved changes until it did.

- ✅ **Arabic read with its vowels.** A voice reading unvowelled Arabic
  guesses, and some words come out wrong. After each chapter's editor's
  pass the writer marks every word with its vowels (tashkeel, case
  endings, a sukun at the pause) for the voice, word for word; the
  captions keep the words as written, and stay timed to the voice since
  the words are the same. A marked reading that changed a word is not
  used, nor one made for words edited since. A scene written again gets
  its marks too. The narration itself is written unmarked.
- ✅ **The window stuck after a cut.** Laying a generated video on the
  timeline ran the bookkeeping after every edit - the mix handed to
  playback, the monitor's frame, the art - over all the clips laid so
  far, hundreds of times, on the event loop; the autosave waited behind
  it. The cut is now one batch, its bookkeeping done once at the end.
- ✅ **A log line when the window stops answering.** A watchdog thread
  notes when the event loop has not beaten for two seconds, with what the
  studio was handling, and how long the stall lasted: the next one, if
  any, is found from the shared log.

All three shipped in alpha.24 (#32).

## Animated cards (owner, 8 Nov)

The owner asked whether Claude could make scenes and motion too. It
writes, it does not paint: Replicate keeps making the pictures, and some
content needs none - a figure, a comparison, dates, steps, a definition -
where an animated card says it better, for nothing.

- ✅ **Stage 1: cards among the pictures** (alpha.25, #33). A shot may be an animated card
  (`ShotKind::Motion`, with a `Graphic`): a heading; a figure counting up;
  two to four bars to their values; dates along a line; points one by
  one; a quote and who said it - in the chapter's colour, in the
  narration's language and script, laid out from the right for Arabic.
  The writer chooses one where a picture would only decorate, at most one
  shot in four, never the chapter's first. The job draws the card's
  backdrop, a deep shade of its colour, and the cut lays its words and
  plates as the edit's own text clips with keyframes, on lanes of their
  own; nothing is asked of a model. In the script editor a card shows its
  heading and its lines to edit. A scene with a card has no data card
  over it.
- ✅ **Stage 5: cards only** (alpha.25, #33; now "drawn only", below). A Pictures choice in the Long video sheet:
  pictures and cards, or cards only - every shot a card, a video for the
  cost of its script and voice.
- Later: more cards (a map with a route, a percentage ring, a before and
  after), a card's colour and layout chosen in the script editor, and a
  card turned into a picture or back from there.

## Animated explainers (owner, 8 Nov)

The owner showed an explainer made by Claude Code as a motion designer: a
diagram that builds itself in step with the narration, the way the best
educational channels explain. Claude writes, it does not paint - so it
designs the animation as data and the app draws it, frame by frame, on
the phone as on the desk, for nothing. All four stages are built and
shipped together, each whole.

- 🚧 **Stage 1: the engine** (`explainer/`). An explainer is a canvas in
  pixels (1920x1080 for a wide video) of elements - text, rect, circle,
  line, arrow (bowed, its head arriving last), path, icon, group - and
  steps: fade, pop, grow from an edge, draw an outline on along its
  length, type word by word, move, scale, rotate, recolour, count a
  number up, pulse, shake, and a camera that pans and zooms; six easings.
  Each frame is written as SVG and rasterised by resvg on the CPU, a few
  frames at a time on as many threads, and encoded as an H.264 clip (an
  8 s 1080p clip draws in about 3.5 s on a desk). Arabic is embedded right
  to left, so a line opening on a figure reads right, and thickened for a
  heavy weight, its bundled face having one. The bundled faces only, so a
  frame is the same everywhere. A validator names every fault - ids twice
  or missing, steps on nothing, unknown icons, colours that are not,
  words too small or off the canvas.
- 🚧 **Stage 2: the writer as motion director.** A shot may be an
  explainer (`ShotKind::Explainer`): the chapter call marks it where the
  narration explains a mechanism, a process, a cause and effect, a
  comparison of sizes, a route, a structure or a cycle (up to one shot in
  two for educational and academic videos, one in four for documentary
  and news, one in six for a story), its prompt saying what the diagram
  shows beat by beat. Then a call of its own designs each one under a
  designer's brief and a design system - safe area and the captions'
  fifth kept clear, hierarchy, palette from the chapter's, grid, diagram
  patterns, every element entering on the word that names it (`cue`),
  right-to-left layout for Arabic. Its faults are sent back once to be
  mended; then two frames are drawn and shown to it as pictures, and it
  mends what it sees - overlaps, cut words, crowding, imbalance. One that
  cannot be designed is made as a still of its prompt.
- 🚧 **Stage 3: the icon library.** 191 Lucide icons (ISC), as paths so
  each can be drawn on stroke by stroke, named in the designer's brief.
- 🚧 **Stage 4: in the run.** Explainers are drawn last, each to the slot
  the voice and the shots beside it leave (`plan::timing`, the one sum the
  cut and the job share), its steps on the times their cue words are said
  - from ElevenLabs' word timings, or by the letters for an untimed voice.
  A note beside the clip says what it was drawn to; a new timing or a new
  design draws it again as a take, and Make again draws one. A still of
  its end is the script editor's thumbnail. In the script editor an
  explainer shows what it shows, and Design again designs it anew. The
  Pictures choice is now pictures and drawn shots, or drawn only.
- Later: an explainer in a Short drawn for the tall frame (it is cropped
  from the wide one now); the designer's palette and pace chosen in the
  sheet; pictures from Replicate placed inside an explainer.

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
