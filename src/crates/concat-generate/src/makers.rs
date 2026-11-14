// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Who makes the pictures and the clips: Replicate, which runs many
//! models behind one key, or a model's own maker reached directly with
//! the person's own key - OpenAI and Google Gemini for pictures, Kling and
//! Seedance for clips. The owner would not have the app hold its users to
//! one service.
//!
//! Each writes one file to the path it is given, as every provider does
//! (see `providers`), so the run and the cut do not know which made it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use serde_json::{Value, json};

use crate::providers::{self, agent, refused};

/// The OpenAI picture model used when none is named.
pub const OPENAI_IMAGE_MODEL: &str = "gpt-image-1";
/// The Gemini picture model used when none is named: Gemini 2.5 Flash
/// Image. A model whose name starts with `imagen` is run as Imagen.
pub const GEMINI_IMAGE_MODEL: &str = "gemini-2.5-flash-image";
/// The Kling video model used when none is named.
pub const KLING_MODEL: &str = "kling-v2-1";
/// The Seedance video model used when none is named, as BytePlus names it.
pub const SEEDANCE_MODEL: &str = "seedance-1-0-pro-250528";

/// Where Kling's API is.
const KLING_API: &str = "https://api.klingai.com";
/// Where Seedance's API is: BytePlus ModelArk, outside China.
const SEEDANCE_API: &str =
    "https://ark.ap-southeast.bytepluses.com/api/v3/contents/generations/tasks";

/// The longest one clip may take on a service reached directly.
const CLIP_DEADLINE: Duration = Duration::from_secs(15 * 60);

/// Who makes the pictures.
#[derive(Clone, Debug, PartialEq)]
pub enum Pictures {
    /// A text-to-image model on Replicate.
    Replicate {
        /// The account's API token.
        token: String,
        /// "owner/name"; empty for the default.
        model: String,
    },
    /// OpenAI's image model.
    OpenAi {
        /// The account's API key.
        key: String,
        /// The model; empty for [`OPENAI_IMAGE_MODEL`].
        model: String,
    },
    /// Google's Gemini image model, or Imagen.
    Gemini {
        /// The Gemini API key.
        key: String,
        /// The model; empty for [`GEMINI_IMAGE_MODEL`].
        model: String,
    },
}

impl Pictures {
    /// One picture of `prompt`, framed for `aspect`, saved as
    /// `dir`/`stem` with the extension its bytes call for.
    pub fn make(
        &self,
        prompt: &str,
        aspect: &str,
        dir: &Path,
        stem: &str,
    ) -> Result<PathBuf, String> {
        match self {
            Pictures::Replicate { token, model } => {
                providers::replicate_image(token, model, prompt, aspect, dir, stem)
            }
            Pictures::OpenAi { key, model } => openai_image(key, model, prompt, aspect, dir, stem),
            Pictures::Gemini { key, model } => gemini_image(key, model, prompt, aspect, dir, stem),
        }
    }
}

/// Who makes the clips.
#[derive(Clone, Debug, PartialEq)]
pub enum Clips {
    /// A video model on Replicate, its inputs read from its description.
    Replicate {
        /// The account's API token.
        token: String,
        /// "owner/name".
        model: String,
        /// Its input as JSON, for a model whose description cannot be read.
        input: String,
    },
    /// Kling, by its own API.
    Kling {
        /// The account's access key.
        access_key: String,
        /// The account's secret key, which signs each request.
        secret_key: String,
        /// The model; empty for [`KLING_MODEL`].
        model: String,
    },
    /// Seedance, on BytePlus ModelArk.
    Seedance {
        /// The ModelArk API key.
        key: String,
        /// The model; empty for [`SEEDANCE_MODEL`].
        model: String,
    },
}

impl Clips {
    /// One clip moving `frame` - or, without one, made from the words -
    /// as `prompt` says, about `seconds` long, saved as `dir`/`stem`.
    /// Only for the services reached directly; a Replicate model is run
    /// by the job, which reads what it takes.
    pub fn make_direct(
        &self,
        prompt: &str,
        seconds: f64,
        aspect: &str,
        frame: Option<&Path>,
        dir: &Path,
        stem: &str,
    ) -> Result<PathBuf, String> {
        match self {
            Clips::Replicate { .. } => Err(
                "a Replicate clip is made by the job, which reads the model's inputs".to_owned(),
            ),
            Clips::Kling {
                access_key,
                secret_key,
                model,
            } => kling_clip(
                access_key, secret_key, model, prompt, seconds, aspect, frame, dir, stem,
            ),
            Clips::Seedance { key, model } => {
                seedance_clip(key, model, prompt, seconds, aspect, frame, dir, stem)
            }
        }
    }
}

/// Whether `aspect` ("9:16") is taller than it is wide, or square.
fn shape(aspect: &str) -> std::cmp::Ordering {
    let mut parts = aspect
        .split(':')
        .map(|n| n.trim().parse::<f64>().unwrap_or(1.0));
    let (w, h) = (parts.next().unwrap_or(1.0), parts.next().unwrap_or(1.0));
    h.partial_cmp(&w).unwrap_or(std::cmp::Ordering::Equal)
}

/// The size OpenAI is asked for: the one of the three it makes that is
/// nearest the frame's shape.
fn openai_size(aspect: &str) -> &'static str {
    match shape(aspect) {
        std::cmp::Ordering::Greater => "1024x1536",
        std::cmp::Ordering::Equal => "1024x1024",
        std::cmp::Ordering::Less => "1536x1024",
    }
}

/// A clip's length as a service that takes five or ten seconds has it.
fn five_or_ten(seconds: f64) -> u32 {
    if seconds > 7.5 { 10 } else { 5 }
}

/// `bytes`, written as `dir`/`stem`.png through a partial file and then
/// named for what they hold.
fn write_picture(bytes: &[u8], dir: &Path, stem: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let out = dir.join(format!("{stem}.png"));
    let partial = out.with_extension("partial");
    std::fs::write(&partial, bytes).map_err(|error| format!("{}: {error}", partial.display()))?;
    std::fs::rename(&partial, &out).map_err(|error| format!("{}: {error}", out.display()))?;
    Ok(providers::true_name(&out))
}

fn decoded(data: &str, service: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|error| format!("{service} sent a picture that could not be read: {error}"))
}

/// One picture from OpenAI.
fn openai_image(
    key: &str,
    model: &str,
    prompt: &str,
    aspect: &str,
    dir: &Path,
    stem: &str,
) -> Result<PathBuf, String> {
    if key.trim().is_empty() {
        return Err("OpenAI needs an API key".to_owned());
    }
    let model = if model.trim().is_empty() {
        OPENAI_IMAGE_MODEL
    } else {
        model.trim()
    };
    let agent = agent();
    let answer: Value = agent
        .post("https://api.openai.com/v1/images/generations")
        .set("authorization", &format!("Bearer {}", key.trim()))
        .send_json(json!({
            "model": model,
            "prompt": prompt,
            "size": openai_size(aspect),
            "quality": "high",
            "n": 1,
        }))
        .map_err(|error| refused("OpenAI", error))?
        .into_json()
        .map_err(|error| error.to_string())?;
    let picture = &answer["data"][0];
    if let Some(data) = picture["b64_json"].as_str() {
        return write_picture(&decoded(data, "OpenAI")?, dir, stem);
    }
    if let Some(url) = picture["url"].as_str() {
        let out = dir.join(format!("{stem}.png"));
        providers::fetch(&agent, url, &out)?;
        return Ok(providers::true_name(&out));
    }
    Err("OpenAI answered without a picture".to_owned())
}

/// One picture from Gemini, or from Imagen when the model is one.
fn gemini_image(
    key: &str,
    model: &str,
    prompt: &str,
    aspect: &str,
    dir: &Path,
    stem: &str,
) -> Result<PathBuf, String> {
    if key.trim().is_empty() {
        return Err("Gemini needs an API key".to_owned());
    }
    let model = if model.trim().is_empty() {
        GEMINI_IMAGE_MODEL
    } else {
        model.trim()
    };
    let base = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}");
    let ask = |method: &str, body: Value| -> Result<Value, String> {
        agent()
            .post(&format!("{base}:{method}"))
            .set("x-goog-api-key", key.trim())
            .send_json(body)
            .map_err(|error| refused("Gemini", error))?
            .into_json()
            .map_err(|error| error.to_string())
    };
    if model.starts_with("imagen") {
        let answer = ask(
            "predict",
            json!({
                "instances": [{"prompt": prompt}],
                "parameters": {"sampleCount": 1, "aspectRatio": aspect},
            }),
        )?;
        let data = answer["predictions"][0]["bytesBase64Encoded"]
            .as_str()
            .ok_or("Imagen made no picture: its safety filter may have refused it")?;
        return write_picture(&decoded(data, "Imagen")?, dir, stem);
    }
    let answer = ask(
        "generateContent",
        json!({
            "contents": [{"parts": [{"text": prompt}]}],
            "generationConfig": {
                "responseModalities": ["IMAGE"],
                "imageConfig": {"aspectRatio": aspect},
            },
        }),
    )?;
    match gemini_picture(&answer) {
        Ok(data) => write_picture(&decoded(data, "Gemini")?, dir, stem),
        Err(why) => Err(why),
    }
}

/// The picture in a Gemini answer, or why there is none - a refusal says
/// "safety", which the run knows to ask again gently.
fn gemini_picture(answer: &Value) -> Result<&str, String> {
    let parts = answer["candidates"][0]["content"]["parts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let found = answer["candidates"][0]["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|part| {
            part.get("inlineData")
                .or_else(|| part.get("inline_data"))
                .and_then(|data| data["data"].as_str())
        });
    if let Some(data) = found {
        return Ok(data);
    }
    let reason = answer["promptFeedback"]["blockReason"]
        .as_str()
        .or_else(|| answer["candidates"][0]["finishReason"].as_str())
        .unwrap_or("no reason given");
    let said: String = parts
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let reason = match reason {
        "SAFETY" | "PROHIBITED_CONTENT" | "IMAGE_SAFETY" | "BLOCKLIST" => {
            format!("{reason} (safety)")
        }
        other => other.to_owned(),
    };
    Err(format!("Gemini made no picture: {reason} {}", said.trim())
        .trim()
        .to_owned())
}

/// The token Kling's API takes: a JWT signed with the secret key, good
/// for half an hour.
fn kling_token(access_key: &str, secret_key: &str, now: u64) -> String {
    use hmac::{Hmac, Mac};
    let url = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header = url.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let claims = url.encode(
        json!({"iss": access_key.trim(), "exp": now + 1800, "nbf": now.saturating_sub(5)})
            .to_string(),
    );
    let signed = format!("{header}.{claims}");
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret_key.trim().as_bytes())
        .expect("HMAC takes a key of any length");
    mac.update(signed.as_bytes());
    let signature = url.encode(mac.finalize().into_bytes());
    format!("{signed}.{signature}")
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// One clip from Kling: image-to-video from `frame` when there is one,
/// text-to-video otherwise.
#[allow(clippy::too_many_arguments)]
fn kling_clip(
    access_key: &str,
    secret_key: &str,
    model: &str,
    prompt: &str,
    seconds: f64,
    aspect: &str,
    frame: Option<&Path>,
    dir: &Path,
    stem: &str,
) -> Result<PathBuf, String> {
    if access_key.trim().is_empty() || secret_key.trim().is_empty() {
        return Err("Kling needs an API access key and secret key".to_owned());
    }
    let model = if model.trim().is_empty() {
        KLING_MODEL
    } else {
        model.trim()
    };
    let auth = || format!("Bearer {}", kling_token(access_key, secret_key, now()));
    let (kind, mut body) = match frame {
        Some(frame) => {
            let bytes =
                std::fs::read(frame).map_err(|error| format!("{}: {error}", frame.display()))?;
            (
                "image2video",
                json!({"image": base64::engine::general_purpose::STANDARD.encode(bytes)}),
            )
        }
        None => ("text2video", json!({"aspect_ratio": aspect})),
    };
    body["model_name"] = json!(model);
    body["prompt"] = json!(prompt);
    body["duration"] = json!(five_or_ten(seconds).to_string());
    body["mode"] = json!("std");
    let agent = agent();
    let start = || -> Result<String, String> {
        let answer: Value = agent
            .post(&format!("{KLING_API}/v1/videos/{kind}"))
            .set("authorization", &auth())
            .send_json(body.clone())
            .map_err(|error| refused("Kling", error))?
            .into_json()
            .map_err(|error| error.to_string())?;
        if answer["code"].as_i64().unwrap_or(0) != 0 {
            return Err(format!(
                "Kling answered 400: {}",
                answer["message"].as_str().unwrap_or("no reason given")
            ));
        }
        let task = answer["data"]["task_id"]
            .as_str()
            .ok_or("Kling gave no task to follow the clip by")?;
        Ok(format!("{KLING_API}/v1/videos/{kind}/{task}"))
    };
    follow(&agent, dir, stem, "Kling", &auth, start, |task| {
        let data = &task["data"];
        match data["task_status"].as_str().unwrap_or_default() {
            "succeed" => data["task_result"]["videos"][0]["url"]
                .as_str()
                .map(|url| Ok(url.to_owned()))
                .or(Some(Err("Kling made the clip but gave no file".to_owned()))),
            "failed" => Some(Err(format!(
                "Kling could not make {stem}: {}",
                data["task_status_msg"]
                    .as_str()
                    .unwrap_or("no reason given")
            ))),
            _ => None,
        }
    })
}

/// One clip from Seedance on BytePlus ModelArk: from `frame` when there
/// is one, from the words otherwise.
#[allow(clippy::too_many_arguments)]
fn seedance_clip(
    key: &str,
    model: &str,
    prompt: &str,
    seconds: f64,
    aspect: &str,
    frame: Option<&Path>,
    dir: &Path,
    stem: &str,
) -> Result<PathBuf, String> {
    if key.trim().is_empty() {
        return Err("Seedance needs an API key".to_owned());
    }
    let model = if model.trim().is_empty() {
        SEEDANCE_MODEL
    } else {
        model.trim()
    };
    let duration = five_or_ten(seconds);
    // A clip from a picture keeps the picture's shape; one from words is
    // told the frame's.
    let mut content = vec![json!({
        "type": "text",
        "text": match frame {
            Some(_) => format!("{prompt} --duration {duration}"),
            None => format!("{prompt} --ratio {aspect} --duration {duration}"),
        },
    })];
    if let Some(frame) = frame {
        let bytes =
            std::fs::read(frame).map_err(|error| format!("{}: {error}", frame.display()))?;
        let mime = match providers::sniffed_extension(frame) {
            Some("jpg") => "image/jpeg",
            Some("webp") => "image/webp",
            _ => "image/png",
        };
        content.push(json!({
            "type": "image_url",
            "image_url": {
                "url": format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )
            },
        }));
    }
    let agent = agent();
    let auth = || format!("Bearer {}", key.trim());
    let body = json!({"model": model, "content": content});
    let start = || -> Result<String, String> {
        let answer: Value = agent
            .post(SEEDANCE_API)
            .set("authorization", &auth())
            .send_json(body.clone())
            .map_err(|error| refused("Seedance", error))?
            .into_json()
            .map_err(|error| error.to_string())?;
        let task = answer["id"]
            .as_str()
            .ok_or("Seedance gave no task to follow the clip by")?;
        Ok(format!("{SEEDANCE_API}/{task}"))
    };
    follow(
        &agent,
        dir,
        stem,
        "Seedance",
        &auth,
        start,
        |task| match task["status"].as_str().unwrap_or_default() {
            "succeeded" => task["content"]["video_url"]
                .as_str()
                .map(|url| Ok(url.to_owned()))
                .or(Some(Err(
                    "Seedance made the clip but gave no file".to_owned()
                ))),
            "failed" | "cancelled" | "expired" => Some(Err(format!(
                "Seedance could not make {stem}: {}",
                task["error"]["message"]
                    .as_str()
                    .unwrap_or("no reason given")
            ))),
            _ => None,
        },
    )
}

/// Starts a task with `start` - or follows the one a run that ended
/// before it came back had started, already paid for - asks after it
/// every five seconds until `done` says it is made or failed, and saves
/// the clip as `dir`/`stem`.mp4.
#[allow(clippy::too_many_arguments)]
fn follow(
    agent: &ureq::Agent,
    dir: &Path,
    stem: &str,
    service: &str,
    auth: &dyn Fn() -> String,
    start: impl FnOnce() -> Result<String, String>,
    done: impl Fn(&Value) -> Option<Result<String, String>>,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let pending = providers::pending(dir, stem);
    let kept = std::fs::read_to_string(&pending)
        .ok()
        .map(|url| url.trim().to_owned())
        .filter(|url| url.starts_with("https://"));
    let url = match kept {
        Some(url) => url,
        None => {
            let url = start()?;
            let _ = std::fs::write(&pending, &url);
            url
        }
    };
    let started = std::time::Instant::now();
    let made = loop {
        let task: Value = agent
            .get(&url)
            .set("authorization", &auth())
            .call()
            .map_err(|error| refused(service, error))?
            .into_json()
            .map_err(|error| error.to_string())?;
        match done(&task) {
            Some(Ok(file)) => break file,
            Some(Err(error)) => {
                let _ = std::fs::remove_file(&pending);
                return Err(error);
            }
            None if started.elapsed() > CLIP_DEADLINE => {
                return Err(format!("{service} took too long to make {stem}"));
            }
            None => std::thread::sleep(Duration::from_secs(5)),
        }
    };
    let out = dir.join(format!("{stem}.mp4"));
    providers::fetch(agent, &made, &out).inspect_err(|_| {
        let _ = std::fs::remove_file(&pending);
    })?;
    let _ = std::fs::remove_file(&pending);
    Ok(providers::true_name(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kling's token is a JWT of three parts, signed so a change to the
    /// claims does not pass for the same token.
    #[test]
    fn a_kling_token_is_a_signed_jwt() {
        let token = kling_token("ak", "sk", 1_700_000_000);
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let url = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let claims: Value = serde_json::from_slice(&url.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["iss"], "ak");
        assert_eq!(claims["exp"], 1_700_001_800u64);
        assert_eq!(claims["nbf"], 1_699_999_995u64);
        assert_ne!(token, kling_token("ak", "other", 1_700_000_000));
        assert_eq!(token, kling_token("ak", "sk", 1_700_000_000));
    }

    /// A frame is asked of OpenAI in the size nearest its shape, and a
    /// clip's length is five seconds or ten.
    #[test]
    fn sizes_and_lengths_are_what_the_services_take() {
        assert_eq!(openai_size("16:9"), "1536x1024");
        assert_eq!(openai_size("9:16"), "1024x1536");
        assert_eq!(openai_size("4:5"), "1024x1536");
        assert_eq!(openai_size("1:1"), "1024x1024");
        assert_eq!(five_or_ten(4.0), 5);
        assert_eq!(five_or_ten(9.0), 10);
    }

    /// Gemini's picture is found in its answer, under either spelling;
    /// a refusal says "safety", so the run asks once more gently.
    #[test]
    fn a_gemini_answer_gives_its_picture_or_why_not() {
        let made = json!({"candidates": [{"content": {"parts": [
            {"text": "Here it is"}, {"inlineData": {"mimeType": "image/png", "data": "iVBO"}}
        ]}}]});
        assert_eq!(gemini_picture(&made), Ok("iVBO"));
        let snake =
            json!({"candidates": [{"content": {"parts": [{"inline_data": {"data": "AAAA"}}]}}]});
        assert_eq!(gemini_picture(&snake), Ok("AAAA"));
        let refused = json!({"promptFeedback": {"blockReason": "SAFETY"}});
        let why = gemini_picture(&refused).unwrap_err();
        assert!(providers::refused_for_content(&why), "{why}");
    }
}
