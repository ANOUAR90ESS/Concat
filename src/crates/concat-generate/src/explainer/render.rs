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

/// How much smaller the soft layers are drawn: they are blurred or
/// smooth, and enlarged they look the same at a sixteenth of the cost.
const SOFT: u32 = 4;

/// One frame at `t`, `size` pixels, as straight RGBA: the soft layer drawn
/// small and enlarged, the glows drawn small and added over it as light,
/// and the sharp layer over both at full size.
fn pixels(
    explainer: &Explainer,
    canvas: (f64, f64),
    size: (u32, u32),
    t: f64,
    fonts: &Fonts,
) -> Result<Vec<u8>, String> {
    use super::svg::{Layer, frame};
    let small = (size.0.div_ceil(SOFT).max(1), size.1.div_ceil(SOFT).max(1));
    let soft = draw(
        &frame(explainer, canvas, t, Layer::Soft),
        canvas,
        small,
        fonts,
        t,
    )?;
    let mut pixmap = tiny_skia::Pixmap::new(size.0, size.1)
        .ok_or_else(|| format!("a frame of {}x{} cannot be drawn", size.0, size.1))?;
    let up = |from: (u32, u32)| {
        tiny_skia::Transform::from_scale(
            size.0 as f32 / from.0 as f32,
            size.1 as f32 / from.1 as f32,
        )
    };
    pixmap.draw_pixmap(
        0,
        0,
        soft.as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            blend_mode: tiny_skia::BlendMode::Source,
            ..tiny_skia::PixmapPaint::default()
        },
        up(small),
        None,
    );
    if explainer.all().iter().any(|element| element.glow > 0.0) {
        let glow = draw(
            &frame(explainer, canvas, t, Layer::Glow),
            canvas,
            small,
            fonts,
            t,
        )?;
        // Added only where something glows: the rest of the frame is
        // left as it is, and enlarging nothing costs nothing.
        if let Some((x0, y0, x1, y1)) = lit(&glow) {
            let (kx, ky) = (
                size.0 as f32 / small.0 as f32,
                size.1 as f32 / small.1 as f32,
            );
            let area = tiny_skia::Rect::from_ltrb(
                (x0 as f32 - 1.0) * kx,
                (y0 as f32 - 1.0) * ky,
                (x1 as f32 + 2.0) * kx,
                (y1 as f32 + 2.0) * ky,
            );
            if let Some(area) = area {
                let paint = tiny_skia::Paint {
                    shader: tiny_skia::Pattern::new(
                        glow.as_ref(),
                        tiny_skia::SpreadMode::Pad,
                        tiny_skia::FilterQuality::Bilinear,
                        1.0,
                        up(small),
                    ),
                    blend_mode: tiny_skia::BlendMode::Plus,
                    ..tiny_skia::Paint::default()
                };
                pixmap.fill_rect(area, &paint, tiny_skia::Transform::identity(), None);
            }
        }
    }
    // The elements over their glows: lit round, not washed out.
    let sharp = tree(&frame(explainer, canvas, t, Layer::Sharp), fonts, t)?;
    resvg::render(&sharp, scale(canvas, size), &mut pixmap.as_mut());
    // The backdrop covers the frame, so every pixel is opaque and the
    // premultiplied channels are the straight ones.
    Ok(pixmap.take())
}

/// The pixels of `pixmap` that are not clear, as the corners of the box
/// round them; None when it is clear throughout.
fn lit(pixmap: &tiny_skia::Pixmap) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut corners: Option<(u32, u32, u32, u32)> = None;
    for (n, pixel) in pixmap.pixels().iter().enumerate() {
        if pixel.alpha() > 2 {
            let (x, y) = (n as u32 % w, n as u32 / w);
            corners = Some(match corners {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        }
    }
    corners.map(|(x0, y0, x1, y1)| (x0, y0, x1.min(w - 1), y1.min(h - 1)))
}

/// One layer's SVG parsed, its words shaped in the bundled faces.
fn tree(svg: &str, fonts: &Fonts, t: f64) -> Result<usvg::Tree, String> {
    let options = usvg::Options {
        fontdb: Arc::clone(&fonts.0),
        font_family: "Inter".to_owned(),
        ..usvg::Options::default()
    };
    usvg::Tree::from_str(svg, &options)
        .map_err(|error| format!("the explainer's frame at {t:.2} s: {error}"))
}

/// The canvas fitted to `size` pixels.
fn scale(canvas: (f64, f64), size: (u32, u32)) -> tiny_skia::Transform {
    let scale = (size.0 as f64 / canvas.0).min(size.1 as f64 / canvas.1) as f32;
    tiny_skia::Transform::from_scale(scale, scale)
}

/// One layer drawn on its own, `size` pixels, over a clear ground.
fn draw(
    svg: &str,
    canvas: (f64, f64),
    size: (u32, u32),
    fonts: &Fonts,
    t: f64,
) -> Result<tiny_skia::Pixmap, String> {
    let tree = tree(svg, fonts, t)?;
    let mut pixmap = tiny_skia::Pixmap::new(size.0, size.1)
        .ok_or_else(|| format!("a frame of {}x{} cannot be drawn", size.0, size.1))?;
    resvg::render(&tree, scale(canvas, size), &mut pixmap.as_mut());
    Ok(pixmap)
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
            drop(encoder);
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
    // Closed before it is moved: Windows will not rename a file still open.
    drop(encoder);
    std::fs::rename(&partial, out).map_err(|error| format!("{}: {error}", out.display()))
}
