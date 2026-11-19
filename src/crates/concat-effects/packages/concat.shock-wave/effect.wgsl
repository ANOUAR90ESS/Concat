struct Params { amount: f32, x: f32, y: f32, duration: f32, width: f32 }

// A ring that starts at the centre as the clip begins and spreads to the
// far corners over the duration, bending what it passes over - pushed out
// on its leading edge, pulled in on its trailing one - and weakening as it
// goes. Before and after it the picture is untouched. Timed to the clip's
// own start, so it plays the same wherever the clip sits. After
// pmndrs/postprocessing's ShockWaveEffect (zlib), measured in heights of
// the frame so the ring is round on any frame.

const PI: f32 = 3.14159265;

/// How far the ring travels, in heights of the frame: past the corner of
/// a frame twice as wide as it is high, from its middle.
const TRAVEL: f32 = 1.2;

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let t = frame.clip_time / max(params.duration, 0.05);
    if (t >= 1.0) {
        return sample(uv);
    }
    let offset = (uv - vec2<f32>(params.x, params.y) / 100.0) * frame.size;
    let d = length(offset) / frame.size.y;
    let half_width = params.width / 100.0 * 0.15;
    // Where in the ring the point lies: -1 at its inner edge, 1 at its
    // outer one.
    let across = (d - t * TRAVEL) / half_width;
    if (abs(across) >= 1.0) {
        return sample(uv);
    }
    let push = sin(across * PI) * params.amount / 100.0 * 0.04 * (1.0 - t);
    let away = offset / max(length(offset), 1e-6);
    return sample(uv - away * push * frame.size.y / frame.size);
}
