// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Where the shots and the narration come from.
//!
//! Every provider writes one file to a path it is given and nothing else,
//! so any of them can be swapped for another, or for a file made by hand,
//! without the cut noticing. A file already there is never made twice:
//! generation is resumable, and a shot replaced by hand stays.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use crate::writer::message_of;

/// What a shot file may be.
pub const SHOT_EXTENSIONS: [&str; 8] = ["mp4", "mov", "webm", "mkv", "png", "jpg", "jpeg", "webp"];
/// What a narration file may be.
pub const VOICE_EXTENSIONS: [&str; 5] = ["wav", "mp3", "m4a", "flac", "ogg"];

/// Where shots come from.
#[derive(Clone, Debug, PartialEq)]
pub enum Shots {
    /// Colour cards: a draft cut, to judge the pacing before paying for a
    /// single shot. The cut moves over them slowly, as it does any still.
    Draft,
    /// Made by hand from the prompt sheet and saved in the job's `shots/`.
    Folder,
    /// Made on Replicate, as the script says: a still from a text-to-image
    /// model, which the cut moves over, or a clip from a video model. A
    /// model that takes a picture to start from is sent a still made first
    /// from the shot's own prompt, so a clip keeps the film's look rather
    /// than the video model's; one that takes words alone is sent those.
    Replicate {
        /// The account's API token.
        token: String,
        /// The text-to-image model, "owner/name".
        image_model: String,
        /// The video model, "owner/name"; empty for stills only.
        video_model: String,
        /// The video model's input as JSON, for a model whose own
        /// description cannot be read; strings may hold `{prompt}`,
        /// `{seconds}` and `{aspect}`.
        video_input: String,
    },
}

/// The text-to-image model a Replicate job uses when none is named: fast,
/// cheap, and it takes an aspect ratio.
pub const DEFAULT_IMAGE_MODEL: &str = "black-forest-labs/flux-schnell";

/// The video model offered when none is named: image-to-video, fast and at
/// the cheap end, so a clip moves a still the image model made.
pub const DEFAULT_VIDEO_MODEL: &str = "wan-video/wan-2.2-i2v-fast";

/// How many times a call to a service is made before it is given up on:
/// a long video is hundreds of calls, and one busy minute on the far side
/// must not end an unattended run.
pub const ATTEMPTS: u32 = 3;

/// The longest one shot may take to make before it is given up on, so an
/// unattended run never waits forever on a prediction that is stuck.
const SHOT_DEADLINE: Duration = Duration::from_secs(15 * 60);

/// `work` until it succeeds, at most [`ATTEMPTS`] times, waiting longer
/// between each: the last failure if none does.
pub fn retry<T>(mut work: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    let mut last = String::new();
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(Duration::from_secs(5 * u64::from(attempt * attempt)));
        }
        match work() {
            Ok(value) => return Ok(value),
            // A request the service refused as it stands - a bad key, no
            // credit, an input it does not take - fails the same way
            // again: only a busy or broken moment is worth waiting out.
            Err(error) if refused_for_good(&error) => return Err(error),
            Err(error) => {
                log::warn!("attempt {} of {ATTEMPTS} failed: {error}", attempt + 1);
                last = error;
            }
        }
    }
    Err(last)
}

/// Where the narration comes from, besides the window's own voices.
#[derive(Clone, Debug, PartialEq)]
pub enum Voice {
    /// No narration: each scene lasts its written seconds.
    None,
    /// Read by the window's own text to speech; the caller speaks.
    BuiltIn,
    /// ElevenLabs, which reads most languages, Arabic among them.
    ElevenLabs {
        /// The account's API key.
        key: String,
        /// The voice to read with.
        voice_id: String,
    },
}

/// The file `stem` already has in `dir` under one of `extensions`.
pub fn existing(dir: &Path, stem: &str, extensions: &[&str]) -> Option<PathBuf> {
    extensions
        .iter()
        .map(|extension| dir.join(format!("{stem}.{extension}")))
        .find(|path| path.metadata().is_ok_and(|meta| meta.len() > 0))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(300))
        .build()
}

fn refused(service: &str, error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body = response.into_string().unwrap_or_default();
            format!("{service} answered {code}: {}", message_of(&body))
        }
        error => format!("{service} did not answer: {error}"),
    }
}

/// A colour card for scene `n`: a diagonal wash between two hues, written
/// as a PNG of the frame's size.
pub fn draft(out: &Path, width: u32, height: u32, n: usize) -> Result<(), String> {
    let hue = (n as f32 * 137.5) % 360.0;
    let (a, b) = (rgb(hue, 0.55, 0.35), rgb((hue + 40.0) % 360.0, 0.6, 0.6));
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    let span = (width + height) as f32;
    for y in 0..height {
        for x in 0..width {
            let t = (x + y) as f32 / span;
            for channel in 0..3 {
                pixels.push((a[channel] + (b[channel] - a[channel]) * t).round() as u8);
            }
        }
    }
    let file = std::fs::File::create(out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&pixels))
        .map_err(|error| format!("{}: {error}", out.display()))
}

/// HSL to 0..=255 channels.
fn rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h = hue / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c / 2.0;
    [(r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0]
}

/// Whether a service's answer says the request itself is wrong: a 4xx
/// other than 408 (timeout) and 429 (too many requests).
pub fn refused_for_good(error: &str) -> bool {
    error
        .split("answered ")
        .nth(1)
        .and_then(|rest| rest.get(..3))
        .and_then(|code| code.parse::<u16>().ok())
        .is_some_and(|code| (400..500).contains(&code) && code != 408 && code != 429)
}

/// What a Replicate model takes, read from the model's own description,
/// so one input fits any of them: the field the starting picture goes in
/// is called `image` on one model and `start_image` on the next, and a
/// clip's length is seconds on one and frames on another.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelInputs {
    /// The inputs, by name.
    properties: serde_json::Map<String, Value>,
    /// The schemas they refer to for their choices.
    schemas: Value,
}

/// The names a model's starting picture goes by, likeliest first.
const IMAGE_FIELDS: [&str; 7] = [
    "image",
    "start_image",
    "first_frame_image",
    "input_image",
    "first_frame",
    "image_url",
    "start_frame",
];

/// Words in a picture input's name that say it is not the starting one.
const NOT_THE_START: [&str; 7] = [
    "end",
    "last",
    "tail",
    "mask",
    "reference",
    "ref_",
    "negative",
];

impl ModelInputs {
    /// The inputs an OpenAPI description of a model names.
    pub fn from_schema(schema: &Value) -> ModelInputs {
        let schemas = schema["components"]["schemas"].clone();
        ModelInputs {
            properties: schemas["Input"]["properties"]
                .as_object()
                .cloned()
                .unwrap_or_default(),
            schemas,
        }
    }

    /// The input the starting picture goes in; None for a model that takes
    /// words alone.
    pub fn image_field(&self) -> Option<String> {
        IMAGE_FIELDS
            .iter()
            .find(|name| self.properties.contains_key(**name))
            .map(|name| (*name).to_owned())
            .or_else(|| {
                self.properties
                    .keys()
                    .find(|name| {
                        let name = name.to_lowercase();
                        name.contains("image")
                            && !NOT_THE_START.iter().any(|word| name.contains(word))
                    })
                    .cloned()
            })
    }

    /// The choices an input allows, its own or the schema's it refers to.
    fn choices(&self, name: &str) -> Vec<Value> {
        let Some(property) = self.properties.get(name) else {
            return Vec::new();
        };
        if let Some(choices) = property["enum"].as_array() {
            return choices.clone();
        }
        property["allOf"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(std::iter::once(property))
            .filter_map(|part| part["$ref"].as_str())
            .filter_map(|reference| reference.rsplit('/').next())
            .find_map(|schema| self.schemas[schema]["enum"].as_array().cloned())
            .unwrap_or_default()
    }

    /// The input for one clip: the prompt, the starting picture when the
    /// model takes one, as near `seconds` as the model allows, and the
    /// frame when it is one the model offers. Everything else is left to
    /// the model's defaults.
    pub fn clip(&self, prompt: &str, seconds: f64, aspect: &str, image: Option<&str>) -> Value {
        let mut input = serde_json::Map::new();
        input.insert("prompt".to_owned(), json!(prompt));
        if let (Some(url), Some(field)) = (image, self.image_field()) {
            input.insert(field, json!(url));
        }
        if self.properties.contains_key("duration") {
            if let Some(duration) = self.nearest("duration", seconds) {
                input.insert("duration".to_owned(), duration);
            }
        } else if self.properties.contains_key("num_frames") {
            let fps = ["frames_per_second", "fps"]
                .iter()
                .find_map(|name| self.properties.get(*name)?["default"].as_f64())
                .unwrap_or(16.0);
            if let Some(frames) = self.nearest("num_frames", seconds * fps) {
                input.insert("num_frames".to_owned(), frames);
            }
        }
        if self
            .choices("aspect_ratio")
            .iter()
            .any(|choice| choice.as_str() == Some(aspect))
        {
            input.insert("aspect_ratio".to_owned(), json!(aspect));
        }
        Value::Object(input)
    }

    /// The value of `name` nearest `wanted`: the nearest of its choices,
    /// or `wanted` within its bounds; None when it says neither, so the
    /// model's default stands.
    fn nearest(&self, name: &str, wanted: f64) -> Option<Value> {
        let number = |value: &Value| {
            value.as_f64().or_else(|| {
                value
                    .as_str()?
                    .trim_end_matches(|c: char| !c.is_ascii_digit())
                    .parse()
                    .ok()
            })
        };
        let choices = self.choices(name);
        if !choices.is_empty() {
            return choices
                .into_iter()
                .filter_map(|choice| Some((number(&choice)?, choice)))
                .min_by(|(a, _), (b, _)| (a - wanted).abs().total_cmp(&(b - wanted).abs()))
                .map(|(_, choice)| choice);
        }
        let property = self.properties.get(name)?;
        let (low, high) = (property["minimum"].as_f64()?, property["maximum"].as_f64()?);
        let value = wanted.round().clamp(low, high);
        Some(if property["type"] == "integer" {
            json!(value as i64)
        } else {
            json!(value)
        })
    }
}

/// What `model` takes, from Replicate's description of it.
pub fn replicate_inputs(token: &str, model: &str) -> Result<ModelInputs, String> {
    let model_info: Value = agent()
        .get(&format!(
            "https://api.replicate.com/v1/models/{}",
            model.trim()
        ))
        .set("authorization", &format!("Bearer {}", token.trim()))
        .call()
        .map_err(|error| refused("Replicate", error))?
        .into_json()
        .map_err(|error| error.to_string())?;
    let schema = &model_info["latest_version"]["openapi_schema"];
    if schema.is_null() {
        return Err(format!("Replicate does not describe {model}'s inputs"));
    }
    Ok(ModelInputs::from_schema(schema))
}

/// One clip from a Replicate video model, sent `input`, saved as
/// `dir`/`stem`.<its extension>.
pub fn replicate_clip(
    token: &str,
    model: &str,
    input: Value,
    dir: &Path,
    stem: &str,
) -> Result<PathBuf, String> {
    replicate(token, model, input, dir, stem)
}

/// The input `template` makes for one clip, for a model whose own
/// description could not be read.
pub fn templated_clip(
    template: &str,
    prompt: &str,
    seconds: f64,
    aspect: &str,
) -> Result<Value, String> {
    let template: Value = serde_json::from_str(template)
        .map_err(|error| format!("the model's input is not JSON: {error}"))?;
    Ok(fill(&template, prompt, seconds, aspect))
}

/// Puts the picture at `path` where a Replicate model can read it, and
/// says where: uploaded to the account's files, or, failing that, the
/// picture itself written into the address.
pub fn replicate_upload(token: &str, path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    };
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("frame.png");
    let boundary = format!("longcut-{:x}", bytes.len() as u64 ^ 0x5eed_cafe_f00d);
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"content\"; filename=\"{name}\"\r\n\
         Content-Type: {mime}\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let uploaded = agent()
        .post("https://api.replicate.com/v1/files")
        .set("authorization", &format!("Bearer {}", token.trim()))
        .set(
            "content-type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send_bytes(&body)
        .map_err(|error| refused("Replicate", error))
        .and_then(|response| {
            response
                .into_json::<Value>()
                .map_err(|error| error.to_string())
        });
    match uploaded {
        Ok(file) => match file["urls"]["get"].as_str() {
            Some(url) => Ok(url.to_owned()),
            None => Err("Replicate kept the picture but gave no address for it".to_owned()),
        },
        Err(error) if refused_for_good(&error) && error.contains("401") => Err(error),
        Err(error) => {
            log::warn!(
                "uploading {} failed, sending it inline: {error}",
                path.display()
            );
            Ok(format!("data:{mime};base64,{}", base64(&bytes)))
        }
    }
}

/// Standard base64, padded.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// One still from a Replicate text-to-image model of the FLUX family's
/// inputs, framed for `aspect` and saved as `dir`/`stem`.png.
pub fn replicate_image(
    token: &str,
    model: &str,
    prompt: &str,
    aspect: &str,
    dir: &Path,
    stem: &str,
) -> Result<PathBuf, String> {
    let model = if model.trim().is_empty() {
        DEFAULT_IMAGE_MODEL
    } else {
        model
    };
    let input = json!({"prompt": prompt, "aspect_ratio": aspect, "output_format": "png"});
    replicate(token, model, input, dir, stem)
}

/// Runs one prediction on `model` with `input` and saves what it made as
/// `dir`/`stem`.<the file's extension>.
fn replicate(
    token: &str,
    model: &str,
    input: Value,
    dir: &Path,
    stem: &str,
) -> Result<PathBuf, String> {
    if token.trim().is_empty() || model.trim().is_empty() {
        return Err("Replicate needs an API token and a model".to_owned());
    }
    let started = std::time::Instant::now();
    let agent = agent();
    let auth = format!("Bearer {}", token.trim());
    // A shot asked for by a run that ended before it came back - the app
    // closed, the project changed - is paid for already: follow that one
    // rather than asking, and paying, again.
    let pending = pending(dir, stem);
    let resumed = std::fs::read_to_string(&pending)
        .ok()
        .map(|url| url.trim().to_owned())
        .filter(|url| url.starts_with("https://"))
        .and_then(|url| {
            agent
                .get(&url)
                .set("authorization", &auth)
                .call()
                .ok()?
                .into_json::<Value>()
                .ok()
        })
        .filter(|prediction| {
            !matches!(
                prediction["status"].as_str().unwrap_or_default(),
                "failed" | "canceled"
            )
        });
    let mut prediction: Value = match resumed {
        Some(prediction) => prediction,
        None => {
            let prediction: Value = agent
                .post(&format!(
                    "https://api.replicate.com/v1/models/{}/predictions",
                    model.trim()
                ))
                .set("authorization", &auth)
                .send_json(json!({ "input": input }))
                .map_err(|error| refused("Replicate", error))?
                .into_json()
                .map_err(|error| error.to_string())?;
            if let Some(url) = prediction["urls"]["get"].as_str() {
                let _ = std::fs::write(&pending, url);
            }
            prediction
        }
    };
    // A video takes minutes; ask every five seconds until it says.
    loop {
        match prediction["status"].as_str().unwrap_or_default() {
            "succeeded" => break,
            "failed" | "canceled" => {
                let _ = std::fs::remove_file(&pending);
                return Err(format!(
                    "Replicate could not make {stem}: {}",
                    prediction["error"].as_str().unwrap_or("no reason given")
                ));
            }
            _ if started.elapsed() > SHOT_DEADLINE => {
                return Err(format!("Replicate took too long to make {stem}"));
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_secs(5));
        let url = prediction["urls"]["get"]
            .as_str()
            .ok_or("Replicate gave no address to follow the shot at")?
            .to_owned();
        prediction = agent
            .get(&url)
            .set("authorization", &auth)
            .call()
            .map_err(|error| refused("Replicate", error))?
            .into_json()
            .map_err(|error| error.to_string())?;
    }
    let output = &prediction["output"];
    let url = output
        .as_str()
        .or_else(|| output.get(0).and_then(Value::as_str))
        .ok_or("Replicate made the shot but gave no file")?;
    let extension = Path::new(url.split('?').next().unwrap_or(url))
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("mp4");
    let out = dir.join(format!("{stem}.{extension}"));
    // Replicate keeps what it made for an hour: one gone by now is asked
    // for again on the next try.
    fetch(&agent, url, &out).inspect_err(|_| {
        let _ = std::fs::remove_file(&pending);
    })?;
    let _ = std::fs::remove_file(&pending);
    Ok(out)
}

/// Where the address of a shot asked for but not yet saved is kept, so a
/// run that ends before it comes back follows it instead of paying again.
pub fn pending(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.pending"))
}

/// The template with `{prompt}`, `{seconds}` and `{aspect}` filled in. A
/// string that is `{seconds}` alone becomes the number, which is what
/// most models take.
fn fill(value: &Value, prompt: &str, seconds: f64, aspect: &str) -> Value {
    match value {
        Value::String(text) if text == "{seconds}" => json!(seconds.round() as i64),
        Value::String(text) => Value::String(
            text.replace("{prompt}", prompt)
                .replace("{seconds}", &format!("{}", seconds.round() as i64))
                .replace("{aspect}", aspect),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| fill(v, prompt, seconds, aspect))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), fill(v, prompt, seconds, aspect)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The narration for one scene from ElevenLabs, as an MP3.
pub fn elevenlabs(key: &str, voice_id: &str, text: &str, out: &Path) -> Result<(), String> {
    if key.trim().is_empty() || voice_id.trim().is_empty() {
        return Err("ElevenLabs needs an API key and a voice id".to_owned());
    }
    let response = agent()
        .post(&format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}",
            voice_id.trim()
        ))
        .set("xi-api-key", key.trim())
        .set("accept", "audio/mpeg")
        .send_json(json!({"text": text, "model_id": "eleven_multilingual_v2"}))
        .map_err(|error| refused("ElevenLabs", error))?;
    save(response, out)
}

fn fetch(agent: &ureq::Agent, url: &str, out: &Path) -> Result<(), String> {
    let response = agent
        .get(url)
        .call()
        .map_err(|error| refused("the download", error))?;
    save(response, out)
}

/// Writes a response's body, through a partial file so an interrupted
/// download is never taken for a finished one.
fn save(response: ureq::Response, out: &Path) -> Result<(), String> {
    let partial = out.with_extension("partial");
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    std::fs::write(&partial, bytes).map_err(|error| format!("{}: {error}", partial.display()))?;
    std::fs::rename(&partial, out).map_err(|error| format!("{}: {error}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_passing_failure_is_tried_again() {
        let mut calls = 0;
        let refused: Result<(), String> = retry(|| {
            calls += 1;
            Err("Replicate answered 401: invalid token".to_owned())
        });
        assert!(refused.is_err());
        assert_eq!(calls, 1);
        assert!(!refused_for_good("Replicate answered 429: slow down"));
        assert!(!refused_for_good("Replicate answered 503: busy"));
        assert!(!refused_for_good("Replicate did not answer: timed out"));
        let mut tries = 0;
        let healed = retry(|| {
            tries += 1;
            if tries < 2 {
                Err("Replicate answered 502: bad gateway".to_owned())
            } else {
                Ok(tries)
            }
        });
        assert_eq!(healed, Ok(2));
    }

    #[test]
    fn the_template_is_filled_with_types_kept() {
        let template = json!({"prompt": "{prompt}", "duration": "{seconds}", "aspect_ratio": "{aspect}",
                              "note": "{seconds}s", "fps": 24});
        let filled = fill(&template, "a dune", 5.4, "9:16");
        assert_eq!(
            filled,
            json!({"prompt": "a dune", "duration": 5, "aspect_ratio": "9:16",
                                  "note": "5s", "fps": 24})
        );
    }

    #[test]
    fn a_clip_is_fitted_to_what_the_model_takes() {
        // Kling's shape: the picture in start_image, the length a choice of
        // seconds kept in a schema of its own.
        let kling = ModelInputs::from_schema(&json!({"components": {"schemas": {
            "Input": {"properties": {
                "prompt": {"type": "string"},
                "start_image": {"type": "string", "format": "uri"},
                "end_image": {"type": "string", "format": "uri"},
                "duration": {"allOf": [{"$ref": "#/components/schemas/duration"}]},
                "aspect_ratio": {"allOf": [{"$ref": "#/components/schemas/aspect_ratio"}]}
            }},
            "duration": {"type": "integer", "enum": [5, 10]},
            "aspect_ratio": {"type": "string", "enum": ["16:9", "9:16", "1:1"]}
        }}}));
        assert_eq!(kling.image_field().as_deref(), Some("start_image"));
        assert_eq!(
            kling.clip("waves", 6.2, "9:16", Some("https://x/f.png")),
            json!({"prompt": "waves", "start_image": "https://x/f.png", "duration": 5,
                   "aspect_ratio": "9:16"})
        );
        // Wan's shape: the length in frames, at the model's own rate.
        let wan =
            ModelInputs::from_schema(&json!({"components": {"schemas": {"Input": {"properties": {
                "prompt": {"type": "string"},
                "image": {"type": "string", "format": "uri"},
                "num_frames": {"type": "integer", "minimum": 81, "maximum": 121},
                "frames_per_second": {"type": "integer", "default": 16}
            }}}}}));
        assert_eq!(
            wan.clip("a dune", 6.0, "16:9", Some("data:x")),
            json!({"prompt": "a dune", "image": "data:x", "num_frames": 96})
        );
        // Words alone: no picture to send, and a length it does not bound
        // is left to its default.
        let words =
            ModelInputs::from_schema(&json!({"components": {"schemas": {"Input": {"properties": {
                "prompt": {"type": "string"},
                "reference_image": {"type": "string"},
                "duration": {"type": "string", "enum": ["4s", "8s"]}
            }}}}}));
        assert_eq!(words.image_field(), None);
        assert_eq!(
            words.clip("rain", 7.0, "1:1", None),
            json!({"prompt": "rain", "duration": "8s"})
        );
    }

    #[test]
    fn a_picture_is_written_as_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn a_draft_card_is_a_png_of_the_frame() {
        let dir =
            std::env::temp_dir().join(format!("concat-generate-draft-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("c01s01_1.png");
        draft(&out, 64, 36, 3).unwrap();
        let decoder = png::Decoder::new(std::fs::File::open(&out).unwrap());
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (64, 36));
        assert_eq!(existing(&dir, "c01s01_1", &SHOT_EXTENSIONS), Some(out));
        assert_eq!(existing(&dir, "c01s01_2", &SHOT_EXTENSIONS), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
