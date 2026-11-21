// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! What the window remembers between runs, as one small JSON file in the
//! app's config directory. None of it is project state: the theme, which
//! models are chosen, which languages. A missing or unreadable file is the
//! defaults, never an error.

use concat_host::AppDirs;
use serde::{Deserialize, Serialize};

const FILE: &str = "settings.json";

/// The interface scales Settings > Appearance offers, smallest first, as
/// percents of the display's own scale. Nothing on the list - and no value
/// at all - leaves the window at the scale the platform hands it. 110 is
/// for a 1x desk where 100 reads small and 125 is a size too far.
/// https://github.com/jub0t/Concat/issues/320
pub const UI_SCALES: [u32; 6] = [100, 110, 125, 150, 175, 200];

/// Remembered preferences.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    /// The dark theme. `None` is the app's default, which is dark.
    pub dark: Option<bool>,
    /// The accent, by the lower-cased name the theme lists it under —
    /// "lime", "sky", ... — or as "#rrggbb" for a colour picked by hand.
    /// `None`, or a name no longer on the list, is the first one, which is
    /// Lime.
    pub accent: Option<String>,
    /// The playhead's colour as "#rrggbb", when one was picked by hand in
    /// Settings > Appearance. `None` is the palette's own, which is lime.
    pub playhead: Option<String>,
    /// The interface scale, as a percent from [`UI_SCALES`]: the window and
    /// everything in it are drawn this much larger than the display's own
    /// scale says. `None` - or a value the list no longer has - follows the
    /// display. Settings > Appearance picks it, and it is remembered here
    /// for the window a launch makes; the window that is open is rescaled in
    /// place the moment the choice is made.
    /// https://github.com/jub0t/Concat/issues/320
    pub ui_scale: Option<u32>,
    /// The chosen transcriber model id, e.g. "base.en".
    pub transcriber_model: Option<String>,
    /// The chosen speech model id.
    pub tts_model: Option<String>,
    /// The chosen Kokoro speaker id.
    pub tts_voice: Option<i32>,
    /// The interface's locale code ("de", "pt-BR", ...); absent is English.
    pub locale: Option<String>,
    /// Package ids starred in the effect libraries, in no order. One list
    /// across all three shelves: a star is a fact about a package, and which
    /// library it happens to be filed in is not part of it.
    #[serde(default)]
    pub favourites: Vec<String>,
    /// The playhead stops at the end of the content instead of going where
    /// it is put. Off by default: a click past the last clip lands there, so
    /// a clip can be dropped at the playhead beyond everything else.
    pub playhead_stops_at_end: bool,
    /// Proxies: a file larger than HD gets a smaller copy written in the
    /// background, and playback at Half and Quarter reads that copy. Off
    /// by default, as in CapCut: a proxy is a full re-encode of every large
    /// file, on the CPU of a phone or through a hardware encoder a laptop
    /// may not take (an HDR one opened NVENC first), and the person who
    /// wants one for smoother playback turns it on in Settings.
    pub proxies: bool,
    /// The magnetic timeline: a delete and a trim close the gap they would
    /// leave, on the lane they happen on. Off by default, because a gap is
    /// sometimes the point; ⇧⌫ ripples either way.
    /// https://github.com/jub0t/Concat/issues/106
    pub magnetic: bool,
    /// Trim follow: while a clip's edge is dragged, the playhead rides the
    /// edge, so the monitor shows the exact frame the cut lands on. Off by
    /// default; the tray's button beside the hand turns it on.
    pub trim_follow: bool,
    /// The preview axis: the monitor shows the frame under the pointer as
    /// it crosses the lanes, the playhead staying where it is. Off by
    /// default: a monitor that jumps with the pointer startles whoever has
    /// not asked for it, and the tray's button is where they ask.
    pub preview_axis: bool,
    /// The preview axis plays the sound under the pointer too. Off by
    /// default: a burst of sound on every pass of the pointer is a lot to
    /// ask of a room.
    pub preview_axis_audio: bool,
    /// Where model downloads look first: a `SourcePreference` by name.
    /// Absent is automatic.
    pub download_source: Option<String>,
    /// The base URL a custom download source appends a model's file to.
    pub download_base: Option<String>,
    /// The Concat API on a socket while the window is open.
    #[serde(default)]
    pub server: ServerPrefs,
    /// The look new titles start from and generated captions are set in,
    /// kept from a title by the Text panel's "Use for new titles"; its
    /// words are empty. None starts titles in the bundled face.
    pub title_style: Option<concat_project::model::TextStyle>,
    /// What the log records, by the name concat_host::logs reads: `info`
    /// unless the Settings sheet was asked for more or less. CONCAT_LOG in
    /// the environment overrides it for one run.
    pub log_level: Option<String>,
    /// The Long video sheet's keys and last choices.
    pub generate: GeneratePrefs,
}

/// A series' settings, saved under its name: everything the Long video
/// sheet remembers but the services' keys and the other series.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Template {
    /// What the person called it.
    pub name: String,
    /// The sheet's choices.
    pub prefs: Box<GeneratePrefs>,
}

impl GeneratePrefs {
    /// These choices as a series to save: without the keys, which stay
    /// where they are, and without the series saved before.
    pub fn as_template(&self, name: &str) -> Template {
        let mut prefs = self.clone();
        prefs.anthropic_key.clear();
        prefs.replicate_token.clear();
        prefs.elevenlabs_key.clear();
        prefs.openai_key.clear();
        prefs.gemini_key.clear();
        prefs.kling_access_key.clear();
        prefs.kling_secret_key.clear();
        prefs.seedance_key.clear();
        prefs.templates.clear();
        prefs.memory.clear();
        Template {
            name: name.trim().to_owned(),
            prefs: Box::new(prefs),
        }
    }

    /// Keeps `template` among the series, in place of one of its name.
    pub fn keep_template(&mut self, template: Template) {
        match self
            .templates
            .iter_mut()
            .find(|kept| kept.name == template.name)
        {
            Some(kept) => *kept = template,
            None => self.templates.push(template),
        }
    }

    /// Brings back the series at `index`: its choices, under the keys and
    /// the series these already hold. False when there is no such series.
    pub fn apply_template(&mut self, index: usize) -> bool {
        let Some(template) = self.templates.get(index).cloned() else {
            return false;
        };
        let mut prefs = *template.prefs;
        // Every key stays the one in use, and so do the series and what
        // each channel has made: none of them is a series' to change.
        prefs.anthropic_key = std::mem::take(&mut self.anthropic_key);
        prefs.replicate_token = std::mem::take(&mut self.replicate_token);
        prefs.elevenlabs_key = std::mem::take(&mut self.elevenlabs_key);
        prefs.openai_key = std::mem::take(&mut self.openai_key);
        prefs.gemini_key = std::mem::take(&mut self.gemini_key);
        prefs.kling_access_key = std::mem::take(&mut self.kling_access_key);
        prefs.kling_secret_key = std::mem::take(&mut self.kling_secret_key);
        prefs.seedance_key = std::mem::take(&mut self.seedance_key);
        // Who writes goes with the keys: it is the person's, not a series'.
        prefs.writer = self.writer;
        prefs.writer_model = std::mem::take(&mut self.writer_model);
        prefs.templates = std::mem::take(&mut self.templates);
        prefs.memory = std::mem::take(&mut self.memory);
        *self = prefs;
        true
    }

    /// Notes that the channel in use made a video called `title`, so the
    /// next one is told and does not make it again. Nothing is noted for a
    /// channel without a name; a title noted before moves to the end.
    pub fn remember(&mut self, title: &str) {
        let (channel, title) = (memory_key(&self.channel_name), title.trim());
        if channel.is_empty() || title.is_empty() {
            return;
        }
        let made = self.memory.entry(channel).or_default();
        made.retain(|kept| kept != title);
        made.push(title.to_owned());
        if made.len() > MEMORY {
            made.drain(..made.len() - MEMORY);
        }
    }

    /// The videos the channel in use has made, oldest first.
    pub fn made_before(&self) -> &[String] {
        self.memory
            .get(&memory_key(&self.channel_name))
            .map_or(&[], Vec::as_slice)
    }

    /// Who writes: the key, and the model, Claude's or ChatGPT's as the
    /// sheet's choice says.
    pub fn writer(&self) -> (String, String) {
        match self.writer {
            // Marked as OpenAI's, so any model typed goes to OpenAI.
            1 => (
                self.openai_key.clone(),
                format!(
                    "{}{}",
                    concat_generate::chatgpt::MARK,
                    match self.writer_model.trim() {
                        "" => concat_generate::chatgpt::DEFAULT_MODEL,
                        model => model,
                    }
                ),
            ),
            _ => (
                self.anthropic_key.clone(),
                concat_generate::writer::DEFAULT_MODEL.to_owned(),
            ),
        }
    }

    /// How many Shorts are cut from a video: three unless chosen.
    pub fn shorts_count(&self) -> usize {
        self.shorts_count.unwrap_or(3).clamp(1, 5)
    }

    /// la lógica, the channel the app comes with ready to use: Spanish,
    /// a cinematic documentary in 16:9, analytical and independent, on
    /// Spanish politics, philosophy, psychology, society and the economy.
    /// The keys, the voice and who makes the pictures stay as they are.
    pub fn la_logica(&self) -> Template {
        let mut ready = self.as_template(LA_LOGICA);
        let prefs = &mut ready.prefs;
        prefs.channel_name = LA_LOGICA.to_owned();
        prefs.channel_audience = "Adultos en España que quieren entender la política, la \
            sociedad y la mente: buscan análisis, no consignas."
            .to_owned();
        prefs.channel_voice = "Documental cinematográfico realista. Voz masculina grave y \
            pausada. Analítico, independiente y no partidista; sereno, preciso, con una \
            ironía fina."
            .to_owned();
        prefs.channel_topics =
            "Política española; filosofía; psicología; sociedad y economía.".to_owned();
        prefs.channel_rules = "Presenta cada postura en sus mejores términos antes de \
            analizarla. Separa los hechos, con su fuente y su cifra, de las opiniones, y di \
            cuál es cuál. Nunca insultes a personas ni a partidos. Nada de sensacionalismo. \
            Termina con una pregunta que deje pensando."
            .to_owned();
        prefs.channel_opening.clear();
        prefs.channel_closing.clear();
        prefs.format = 0;
        prefs.language = 3;
        prefs.aspect = 0;
        prefs.minutes = Some(3);
        prefs.content_style = 0;
        prefs.level = Some(1);
        prefs.pictures = 1;
        prefs.visuals = 0;
        prefs.hook = Some(true);
        prefs.research = Some(true);
        prefs.brand_color = "#C9A227".to_owned();
        prefs.shorts_count = Some(4);
        ready
    }
}

/// The name the ready channel goes by.
pub const LA_LOGICA: &str = "la lógica";

/// How many videos a channel's memory keeps: enough not to repeat a
/// season, few enough to tell the writer in every call.
const MEMORY: usize = 60;

/// The name a channel's memory is kept under: its name as typed, without
/// the spaces around it or the case.
fn memory_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// What the Long video sheet remembers: the services' keys, kept in this
/// file on this machine and sent nowhere but to the service each belongs
/// to, and the choices made last time.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GeneratePrefs {
    /// Claude, which writes the script.
    pub anthropic_key: String,
    /// Who writes the script and the picture prompts: 0 Claude, with the
    /// Anthropic key; 1 ChatGPT, with the OpenAI key.
    pub writer: usize,
    /// The OpenAI model that writes when ChatGPT does; empty for the
    /// default.
    pub writer_model: String,
    /// Replicate, which makes the shots.
    pub replicate_token: String,
    /// The Replicate text-to-video model, "owner/name"; empty for stills
    /// only.
    pub replicate_model: String,
    /// The Replicate text-to-image model; empty for the default.
    pub replicate_image_model: String,
    /// Who makes the pictures: 0 Replicate, 1 OpenAI, 2 Google Gemini.
    pub picture_maker: usize,
    /// Who makes the clips: 0 Replicate, 1 Kling, 2 Seedance.
    pub clip_maker: usize,
    /// OpenAI, when it makes the pictures: the key, and the model, empty
    /// for the default.
    pub openai_key: String,
    /// The OpenAI picture model; empty for the default.
    pub openai_image_model: String,
    /// Google Gemini, when it makes the pictures.
    pub gemini_key: String,
    /// The Gemini picture model; empty for the default.
    pub gemini_image_model: String,
    /// Kling, when it makes the clips: the access key and the secret key
    /// its requests are signed with, and the model.
    pub kling_access_key: String,
    /// Kling's secret key.
    pub kling_secret_key: String,
    /// The Kling model; empty for the default.
    pub kling_model: String,
    /// Seedance on BytePlus ModelArk, when it makes the clips.
    pub seedance_key: String,
    /// The Seedance model; empty for the default.
    pub seedance_model: String,
    /// Export the finished cut without being asked; None is yes.
    pub auto_export: Option<bool>,
    /// The cut's own sounds - a whoosh into a scene, a pop, a chime - and
    /// the music lifted where nobody speaks; on unless turned off.
    pub sounds: Option<bool>,
    /// Open every video on a hook written last, from the whole script,
    /// with a picture of its own; on unless turned off.
    pub hook: Option<bool>,
    /// Deeper analysis: research chosen for the subject with each claim's
    /// standing, an editorial analysis before writing and an editor's
    /// review after; off unless turned on.
    pub deep: Option<bool>,
    /// Check every picture a model drew against its narration and draw
    /// again those that fail; off unless turned on.
    pub picture_check: Option<bool>,
    /// The score out of 10 a picture needs; none for the default.
    pub picture_threshold: Option<u8>,
    /// How many times a failed picture is drawn again; none for the
    /// default.
    pub picture_attempts: Option<u8>,
    /// The channel's colour, `#rrggbb` as typed: its opening and closing
    /// are lit in it, and captions' key words and stickers take it.
    pub brand_color: String,
    /// The channel's logo, a picture's path, kept in a corner of every
    /// video; empty for none.
    pub brand_logo: String,
    /// The channel's opening and closing drawn onto every video.
    pub brand_bumpers: bool,
    /// The series saved: each a name and the sheet's choices as they
    /// were, to bring back with one pick.
    pub templates: Vec<Template>,
    /// Row of the format list: none, one idea in a minute, a story, a
    /// lesson.
    pub format: usize,
    /// The shape of the person's videos in their own words, told to the
    /// writer: a format of their own, saved with a series.
    pub structure: String,
    /// ElevenLabs, which reads the narration.
    pub elevenlabs_key: String,
    /// The ElevenLabs voice.
    pub elevenlabs_voice: String,
    /// Which ElevenLabs model reads: 0 multilingual v2, 1 v3, the more
    /// expressive.
    pub elevenlabs_model: usize,
    /// Words the voice says wrong and how to write them so they are said
    /// right, as typed: `word = reading`, separated by semicolons.
    pub pronunciation: String,
    /// Row of the language list.
    pub language: usize,
    /// Row of the minutes list.
    pub minutes: Option<usize>,
    /// Row of the frames.
    pub aspect: usize,
    /// Who reads: none, the window's voices, ElevenLabs.
    pub voice: Option<usize>,
    /// Where shots come from: draft cards, Replicate, by hand.
    pub shots: usize,
    /// Row of the clips list: the share of shots made as clips.
    pub clips: Option<usize>,
    /// The most one run may spend, in dollars, as typed; empty for no
    /// limit.
    pub spend_limit: String,
    /// What one clip from the video model costs, as typed; empty for the
    /// default model's.
    pub clip_price: String,
    /// What one still from the image model costs, as typed; empty for the
    /// default model's.
    pub image_price: String,
    /// How the captions look: 0 classic, 1 animated.
    pub caption_look: usize,
    /// Row of the content style list: documentary, educational, academic,
    /// story, news.
    pub content_style: usize,
    /// Row of the audience level list: beginner, general, informed;
    /// general unless chosen.
    pub level: Option<usize>,
    /// Read up on the web before writing; on unless turned off.
    pub research: Option<bool>,
    /// What the pictures are: 0 pictures and animated cards, 1 animated
    /// cards only, none made by a model.
    pub visuals: usize,
    /// Row of the picture style list: auto, photo, flat illustration, 3D
    /// cartoon.
    pub pictures: usize,
    /// The channel the videos are for, told once and given to the writer
    /// every time: its name, who watches it, how it sounds, the line it
    /// opens with and the one it closes with, and a script it likes.
    pub channel_name: String,
    /// Who watches it.
    pub channel_audience: String,
    /// How it sounds.
    pub channel_voice: String,
    /// The line every video opens with.
    pub channel_opening: String,
    /// The line every video closes with.
    pub channel_closing: String,
    /// A few lines of a script to write like.
    pub channel_example: String,
    /// What the channel covers, as typed: the subjects its ideas come
    /// from.
    pub channel_topics: String,
    /// The channel's editorial rules, as typed: what every script keeps
    /// to.
    pub channel_rules: String,
    /// How many Shorts are cut from a video, one to five; three unless
    /// chosen.
    pub shorts_count: Option<usize>,
    /// What each channel has made, by its name in lower case: the titles,
    /// oldest first, told to the writer so a video is not made twice.
    /// Kept apart from the series, which bring back choices, not history.
    pub memory: std::collections::BTreeMap<String, Vec<String>>,
}

impl Preferences {
    /// Which of `names` — the theme's accents, in its order — the remembered
    /// accent is. The first when nothing is remembered, and the first when
    /// the name is one the list no longer has: it is the default, and a
    /// choice that has gone away should fall back to the default rather
    /// than to whatever happens to sit at its old position.
    pub fn accent_index(&self, names: impl IntoIterator<Item = impl AsRef<str>>) -> usize {
        let Some(wanted) = self.accent.as_deref().map(str::trim) else {
            return 0;
        };
        names
            .into_iter()
            .position(|name| Self::accent_id(name.as_ref()) == wanted.to_lowercase())
            .unwrap_or(0)
    }

    /// The id an accent is remembered under: its name, lower-cased, so the
    /// file reads `"accent": "sky"` and a hand-typed capital still matches.
    pub fn accent_id(name: &str) -> String {
        name.trim().to_lowercase()
    }

    /// The remembered accent when it is a colour rather than a name: the
    /// hex, for the caller to parse. A hash is what tells the two apart,
    /// and no name on the list starts with one.
    pub fn custom_accent(&self) -> Option<&str> {
        self.accent
            .as_deref()
            .map(str::trim)
            .filter(|text| text.starts_with('#'))
    }

    /// The scale index the Settings row shows: 0 follows the display, then
    /// one per [`UI_SCALES`]. A remembered percent the list no longer has
    /// reads as 0, the way a name the accent list no longer has falls back
    /// to the default rather than to whatever sits at its old position.
    pub fn scale_index(&self) -> usize {
        self.ui_scale
            .and_then(|percent| UI_SCALES.iter().position(|&scale| scale == percent))
            .map_or(0, |row| row + 1)
    }

    /// The percent a remembered row names: `None` at 0, which is the
    /// display's own, and `None` for a row past the list.
    pub fn scale_at(index: usize) -> Option<u32> {
        index
            .checked_sub(1)
            .and_then(|row| UI_SCALES.get(row).copied())
    }

    /// The multiplier the window backend takes for a percent: the percent
    /// over a hundred, which is what `SLINT_SCALE_FACTOR` parses.
    pub fn scale_factor(percent: u32) -> f64 {
        f64::from(percent) / 100.0
    }
}

/// The Settings sheet's Automation page: whether the API is served while the
/// window is open, where, and behind which token.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ServerPrefs {
    /// Serve while the window is open.
    pub enabled: bool,
    /// The TCP address JSON-RPC lines are served on.
    pub listen: String,
    /// What a connection presents first. Empty means none, which the
    /// server only allows on loopback.
    pub token: String,
}

impl Default for ServerPrefs {
    fn default() -> Self {
        ServerPrefs {
            enabled: false,
            listen: DEFAULT_LISTEN.to_owned(),
            token: String::new(),
        }
    }
}

/// Where the window's server listens unless told otherwise: loopback, on
/// the port `concat-cli serve` uses too.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:7420";

impl Preferences {
    /// Reads the file, or the defaults when there is none.
    pub fn load(dirs: &AppDirs) -> Self {
        std::fs::read(dirs.config.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes the file. Best effort: a preference that did not stick is
    /// not worth interrupting anyone over.
    pub fn save(&self, dirs: &AppDirs) {
        let _ = std::fs::create_dir_all(&dirs.config);
        if let Ok(encoded) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(dirs.config.join(FILE), encoded);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bringing back a series keeps every key in use and what each channel
    /// has made.
    #[test]
    fn a_series_keeps_every_key_and_the_memory() {
        let mut prefs = GeneratePrefs {
            channel_name: "Tarikh".into(),
            ..GeneratePrefs::default()
        };
        prefs.keep_template(prefs.as_template("Tarikh"));
        prefs.openai_key = "sk-open".into();
        prefs.kling_secret_key = "kl".into();
        prefs.remember("The fall of Granada");
        assert!(prefs.apply_template(0));
        assert_eq!(prefs.openai_key, "sk-open");
        assert_eq!(prefs.kling_secret_key, "kl");
        assert_eq!(prefs.made_before(), ["The fall of Granada"]);
    }

    /// Claude writes with the Anthropic key unless ChatGPT is picked, which
    /// writes with the OpenAI key and its default model unless one is
    /// named; a series brought back keeps who writes.
    #[test]
    fn who_writes_picks_the_key_and_the_model() {
        let mut prefs = GeneratePrefs {
            anthropic_key: "sk-ant".into(),
            openai_key: "sk-open".into(),
            ..GeneratePrefs::default()
        };
        assert_eq!(
            prefs.writer(),
            (
                "sk-ant".to_owned(),
                concat_generate::writer::DEFAULT_MODEL.to_owned()
            )
        );
        prefs.writer = 1;
        assert_eq!(
            prefs.writer(),
            (
                "sk-open".to_owned(),
                format!("openai:{}", concat_generate::chatgpt::DEFAULT_MODEL)
            )
        );
        // A model in no OpenAI pattern still goes to OpenAI.
        prefs.writer_model = " chat-latest ".into();
        assert_eq!(prefs.writer().1, "openai:chat-latest");
        assert!(concat_generate::chatgpt::is_openai(&prefs.writer().1));
        prefs.keep_template(GeneratePrefs::default().as_template("Plain"));
        assert!(prefs.apply_template(0));
        assert_eq!(prefs.writer, 1);
        assert_eq!(prefs.writer().1, "openai:chat-latest");
    }

    /// A channel remembers its titles once each, the latest last, and no
    /// more than it keeps; another channel's are its own.
    #[test]
    fn a_channel_remembers_what_it_made() {
        let mut prefs = GeneratePrefs {
            channel_name: " La Lógica ".into(),
            ..GeneratePrefs::default()
        };
        prefs.remember("Uno");
        prefs.remember("Dos");
        prefs.remember("Uno");
        assert_eq!(prefs.made_before(), ["Dos", "Uno"]);
        for n in 0..MEMORY {
            prefs.remember(&n.to_string());
        }
        assert_eq!(prefs.made_before().len(), MEMORY);
        prefs.channel_name = "Otra".into();
        assert!(prefs.made_before().is_empty());
        prefs.channel_name.clear();
        prefs.remember("Nadie");
        assert!(prefs.made_before().is_empty());
    }

    /// The ready channel is la lógica in Spanish and 16:9, and keeps the
    /// keys out of it.
    #[test]
    fn la_logica_is_ready() {
        let prefs = GeneratePrefs {
            anthropic_key: "sk".into(),
            ..GeneratePrefs::default()
        };
        let ready = prefs.la_logica();
        assert_eq!(ready.name, LA_LOGICA);
        assert_eq!(ready.prefs.language, 3);
        assert_eq!(ready.prefs.aspect, 0);
        assert_eq!(ready.prefs.shorts_count(), 4);
        assert!(ready.prefs.anthropic_key.is_empty());
        assert!(!ready.prefs.channel_rules.is_empty());
    }

    /// A series is saved without the keys and brought back under the keys
    /// in use; saving under a name again takes the first one's place.
    #[test]
    fn a_series_is_saved_and_brought_back_without_touching_the_keys() {
        let mut prefs = GeneratePrefs {
            anthropic_key: "sk-secret".into(),
            channel_name: "Tarikh".into(),
            content_style: 2,
            ..GeneratePrefs::default()
        };
        let saved = prefs.as_template(" Tarikh ");
        assert_eq!(saved.name, "Tarikh");
        assert!(saved.prefs.anthropic_key.is_empty());
        prefs.keep_template(saved);
        prefs.content_style = 4;
        prefs.keep_template(prefs.as_template("Tarikh"));
        assert_eq!(prefs.templates.len(), 1);
        prefs.content_style = 0;
        prefs.anthropic_key = "sk-newer".into();
        assert!(prefs.apply_template(0));
        assert_eq!(prefs.content_style, 4);
        assert_eq!(prefs.anthropic_key, "sk-newer");
        assert_eq!(prefs.templates.len(), 1);
        assert!(!prefs.apply_template(3));
    }

    const NAMES: [&str; 3] = ["Lime", "Sky", "Violet"];

    /// Nothing remembered is the first accent, which is the default.
    #[test]
    fn no_accent_is_the_first() {
        assert_eq!(Preferences::default().accent_index(NAMES), 0);
    }

    /// A remembered name finds its row whatever its case or spacing, since
    /// the file is plain JSON somebody may have edited.
    #[test]
    fn accent_matches_by_name_not_by_case() {
        for spelled in ["violet", "Violet", "VIOLET", "  violet\n"] {
            let prefs = Preferences {
                accent: Some(spelled.to_owned()),
                ..Preferences::default()
            };
            assert_eq!(prefs.accent_index(NAMES), 2, "{spelled:?}");
        }
    }

    /// A name the list no longer has falls back to the default rather than
    /// to a position, and an empty list cannot be indexed into at all.
    #[test]
    fn unknown_or_empty_accent_falls_back() {
        let prefs = Preferences {
            accent: Some("chartreuse".to_owned()),
            ..Preferences::default()
        };
        assert_eq!(prefs.accent_index(NAMES), 0);
        assert_eq!(prefs.accent_index(Vec::<String>::new()), 0);
        let blank = Preferences {
            accent: Some(String::new()),
            ..Preferences::default()
        };
        assert_eq!(blank.accent_index(NAMES), 0);
    }

    /// The id is what gets written and what gets matched, so the two have
    /// to agree on one spelling.
    #[test]
    fn accent_id_round_trips_through_the_file() {
        let prefs = Preferences {
            accent: Some(Preferences::accent_id("Sky")),
            ..Preferences::default()
        };
        let text = serde_json::to_string(&prefs).unwrap();
        assert!(text.contains("\"accent\":\"sky\""), "{text}");
        let back: Preferences = serde_json::from_str(&text).unwrap();
        assert_eq!(back.accent_index(NAMES), 1);
    }

    /// A hex is a picked colour and a word is not, whatever else is around
    /// it; and a picked colour is no name, so the index falls back to the
    /// default until the caller has parsed it.
    #[test]
    fn a_hash_means_a_picked_colour() {
        let picked = Preferences {
            accent: Some("  #FF00AA ".to_owned()),
            ..Preferences::default()
        };
        assert_eq!(picked.custom_accent(), Some("#FF00AA"));
        assert_eq!(picked.accent_index(NAMES), 0);
        let named = Preferences {
            accent: Some("sky".to_owned()),
            ..Preferences::default()
        };
        assert_eq!(named.custom_accent(), None);
        assert_eq!(Preferences::default().custom_accent(), None);
        let blank = Preferences {
            accent: Some(String::new()),
            ..Preferences::default()
        };
        assert_eq!(blank.custom_accent(), None);
    }

    /// A file from before the accent existed still loads, and reads as the
    /// default.
    #[test]
    fn older_file_without_an_accent_is_the_default() {
        let back: Preferences = serde_json::from_str(r#"{"dark": false}"#).unwrap();
        assert_eq!(back.dark, Some(false));
        assert_eq!(back.accent, None);
        assert_eq!(back.accent_index(NAMES), 0);
    }

    /// A picked playhead colour is written as its hex and read back as
    /// one, and a file from before it existed reads as the palette's own.
    #[test]
    fn a_picked_playhead_round_trips_and_an_older_file_has_none() {
        let prefs = Preferences {
            playhead: Some("#ff453a".to_owned()),
            ..Preferences::default()
        };
        let text = serde_json::to_string(&prefs).unwrap();
        assert!(text.contains("\"playhead\":\"#ff453a\""), "{text}");
        let back: Preferences = serde_json::from_str(&text).unwrap();
        assert_eq!(back.playhead.as_deref(), Some("#ff453a"));
        let older: Preferences = serde_json::from_str(r#"{"dark": true}"#).unwrap();
        assert_eq!(older.playhead, None);
    }

    /// The scale reaches its row by the percent the file holds, and a
    /// percent nobody offers falls back to the display's own - row 0 - as
    /// an unknown accent falls back to the default.
    #[test]
    fn a_remembered_scale_finds_its_row() {
        let at = |percent: Option<u32>| {
            Preferences {
                ui_scale: percent,
                ..Preferences::default()
            }
            .scale_index()
        };
        assert_eq!(at(None), 0);
        assert_eq!(at(Some(100)), 1);
        assert_eq!(at(Some(110)), 2);
        assert_eq!(at(Some(200)), 6);
        assert_eq!(at(Some(133)), 0);
    }

    /// The row finds the percent back, row 0 is the display's own, and a
    /// row past the list is none of the app's.
    #[test]
    fn a_row_finds_its_scale() {
        assert_eq!(Preferences::scale_at(0), None);
        assert_eq!(Preferences::scale_at(1), Some(100));
        assert_eq!(Preferences::scale_at(2), Some(110));
        assert_eq!(Preferences::scale_at(6), Some(200));
        assert_eq!(Preferences::scale_at(7), None);
    }

    /// A scale reads as the multiplier the backend's `SLINT_SCALE_FACTOR`
    /// wants: the percent over a hundred.
    #[test]
    fn a_scale_reads_as_a_multiplier() {
        assert_eq!(Preferences::scale_factor(100), 1.0);
        assert_eq!(Preferences::scale_factor(110), 1.1);
        assert_eq!(Preferences::scale_factor(150), 1.5);
        assert_eq!(Preferences::scale_factor(200), 2.0);
    }

    /// A scale is written as its percent and read back, and a file from
    /// before the scale existed follows the display.
    #[test]
    fn a_scale_round_trips_and_an_older_file_follows_the_display() {
        let prefs = Preferences {
            ui_scale: Some(150),
            ..Preferences::default()
        };
        let text = serde_json::to_string(&prefs).unwrap();
        assert!(text.contains("\"uiScale\":150"), "{text}");
        let back: Preferences = serde_json::from_str(&text).unwrap();
        assert_eq!(back.scale_index(), 4);
        let older: Preferences = serde_json::from_str(r#"{"dark": true}"#).unwrap();
        assert_eq!(older.ui_scale, None);
        assert_eq!(older.scale_index(), 0);
    }
}
