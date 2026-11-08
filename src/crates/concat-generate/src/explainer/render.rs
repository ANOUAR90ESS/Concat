// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! An explainer drawn: each frame's SVG rasterised by resvg on the CPU -
//! the same on a phone as on a desk - a few frames at a time on as many
//! threads, and written as an H.264 clip; or one frame as a PNG, for a
//! thumbnail and for the writer to look at its own work.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use resvg::{tiny_skia, usvg};

use super::Explainer;

/// Frames a second an explainer is drawn at: the cut's own rate.
pub const RATE: u32 = 30;

/// The faces explainers are set in: the bundled ones every title can use
/// and the script faces for Arabic, Hebrew and the rest - never the
/// machine's, so an explainer is drawn the same on every phone and desk.
#[derive(Clone)]
pub struct Fonts(Arc<usvg::fontdb::Database>);

impl Fonts {
    /// The faces, read once for the life of the app.
    pub fn shared() -> Fonts {
        static FONTS: OnceLock<Fonts> = OnceLock::new();
        FONTS
            .get_or_init(|| {
                let mut db = usvg::fontdb::Database::new();
                for face in concat_text::BASE_FONTS
                    .iter()
                    .chain(concat_text::SCRIPT_FALLBACKS.iter())
                {
                    db.load_font_data(face.to_vec());
                }
                db.set_sans_serif_family("Inter");
                db.set_serif_family("DM Serif Display");
                db.set_monospace_family("Google Sans Code");
                Fonts(Arc::new(db))
            })
            .clone()
    }
}

/// One frame at `t`, `size` pixels, as straight RGBA.
fn pixels(
    explainer: &Explainer,
    canvas: (f64, f64),
    size: (u32, u32),
    t: f64,
    fonts: &Fonts,
) -> Result<Vec<u8>, String> {
    let svg = super::svg::frame(explainer, canvas, t);
    let options = usvg::Options {
        fontdb: Arc::clone(&fonts.0),
        font_family: "Inter".to_owned(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(&svg, &options)
        .map_err(|error| format!("the explainer's frame at {t:.2} s: {error}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(size.0, size.1)
        .ok_or_else(|| format!("a frame of {}x{} cannot be drawn", size.0, size.1))?;
    let scale = (size.0 as f64 / canvas.0).min(size.1 as f64 / canvas.1) as f32;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // The backdrop covers the frame, so every pixel is opaque and the
    // premultiplied channels are the straight ones.
    Ok(pixmap.take())
}

/// The frame at `t` as a PNG at `out`, `size` pixels.
pub fn render_still(
    explainer: &Explainer,
    canvas: (f64, f64),
    size: (u32, u32),
    t: f64,
    out: &Path,
) -> Result<(), String> {
    let rgba = pixels(explainer, canvas, size, t, &Fonts::shared())?;
    write_png(out, size, &rgba)
}

/// The frame at `t` as PNG bytes, for the writer to look at.
pub fn still_png(
    explainer: &Explainer,
    canvas: (f64, f64),
    size: (u32, u32),
    t: f64,
) -> Result<Vec<u8>, String> {
    let rgba = pixels(explainer, canvas, size, t, &Fonts::shared())?;
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, size.0, size.1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&rgba))
            .map_err(|error| format!("the still: {error}"))?;
    }
    Ok(bytes)
}

fn write_png(out: &Path, size: (u32, u32), rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size.0, size.1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(rgba))
        .map_err(|error| format!("{}: {error}", out.display()))
}

/// The explainer, cues placed, as a clip `seconds` long at `out`: written
/// beside it first and moved into place, so a clip cut short is never
/// taken for a finished one. `stop` is asked between frames.
pub fn render_clip(
    explainer: &Explainer,
    canvas: (f64, f64),
    size: (u32, u32),
    seconds: f64,
    out: &Path,
    stop: &dyn Fn() -> bool,
) -> Result<(), String> {
    let fonts = Fonts::shared();
    let frames = ((seconds.max(0.1)) * RATE as f64).round().max(1.0) as usize;
    // An even frame: H.264's 4:2:0 wants both sides even.
    let size = (size.0 & !1, size.1 & !1);
    let partial = out.with_extension("partial.mp4");
    let mut encoder = concat_media::Encoder::create(
        &partial,
        size.0,
        size.1,
        concat_core::time::FrameRate::THIRTY,
        &concat_media::EncodeOptions {
            // Flat colour and sharp edges: a low CRF keeps them clean, and
            // the file stays small all the same.
            crf: 16,
            ..concat_media::EncodeOptions::default()
        },
    )
    .map_err(|error| format!("{}: {error}", partial.display()))?;
    let threads = std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .clamp(1, 6);
    let mut next = 0;
    while next < frames {
        if stop() {
            let _ = std::fs::remove_file(&partial);
            return Err("cancelled".to_owned());
        }
        let batch: Vec<usize> = (next..frames.min(next + threads)).collect();
        let drawn: Vec<Result<Vec<u8>, String>> = std::thread::scope(|scope| {
            let workers: Vec<_> = batch
                .iter()
                .map(|&n| {
                    let fonts = &fonts;
                    scope.spawn(move || {
                        pixels(explainer, canvas, size, n as f64 / RATE as f64, fonts)
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .unwrap_or_else(|_| Err("a frame's drawing stopped".to_owned()))
                })
                .collect()
        });
        for rgba in drawn {
            let frame = concat_core::frame::Frame::from_rgba(size.0, size.1, rgba?)
                .ok_or("a frame came out the wrong size")?;
            concat_media::FrameSink::write_frame(&mut encoder, &frame)
                .map_err(|error| format!("{}: {error}", partial.display()))?;
        }
        next += batch.len();
    }
    concat_media::FrameSink::finish(&mut encoder)
        .map_err(|error| format!("{}: {error}", partial.display()))?;
    std::fs::rename(&partial, out).map_err(|error| format!("{}: {error}", out.display()))
}
