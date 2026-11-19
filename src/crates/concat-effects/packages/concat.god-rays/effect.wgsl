struct Params { amount: f32, x: f32, y: f32, length: f32, threshold: f32 }

// Light scattered towards the eye along the line from each pixel to a
// bright point - the shafts through a window, or a break in the clouds.
// Each pixel gathers the bright parts of the picture between it and the
// light, nearer ones weighing more, and the gathered light is added to the
// picture by the amount. Only what is brighter than the threshold casts a
// ray, so a dark picture gets none. After the volumetric light scattering
// of GPU Gems 3, chapter 13, as pmndrs/postprocessing's GodRaysEffect draws
// it (zlib), here without a depth buffer: the picture's own highlights are
// the light.

const TAPS: i32 = 32;

/// How fast a ray fades along its length: the far end weighs e^-2 of the
/// near one.
const DECAY: f32 = 2.0;

fn light() -> vec2<f32> {
    return vec2<f32>(params.x, params.y) / 100.0;
}

/// How much of its distance to the light a pixel gathers from.
fn reach() -> f32 {
    return clamp(params.length / 100.0, 0.01, 0.95);
}

/// `uv` moved `t` of the way to the light.
fn toward(uv: vec2<f32>, t: f32) -> vec2<f32> {
    return mix(uv, light(), t);
}

/// The light a point gives off, premultiplied: its colour where it is
/// brighter than the threshold, fading in over a fifth of white below it.
fn bright(uv: vec2<f32>) -> vec4<f32> {
    let c = sample(uv);
    let t = params.threshold / 100.0;
    let k = smoothstep(t, t + 0.2, luma(c.rgb));
    return vec4<f32>(c.rgb * k * c.a, c.a);
}

/// The light along the first thirty-second of the ray from `uv`.
fn near(uv: vec2<f32>) -> vec4<f32> {
    let step = reach() / f32(TAPS * TAPS);
    var sum = vec4<f32>(0.0);
    for (var i = 0; i < TAPS; i++) {
        sum += bright(toward(uv, step * f32(i)));
    }
    return sum / f32(TAPS);
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let step = reach() / f32(TAPS);
    var sum = vec3<f32>(0.0);
    var total = 0.0;
    for (var i = 0; i < TAPS; i++) {
        let w = exp(-DECAY * f32(i) / f32(TAPS));
        sum += near_at(toward(uv, step * f32(i))).rgb * w;
        total += w;
    }
    let c = sample(uv);
    return vec4<f32>(c.rgb + sum / total * params.amount / 100.0, c.a);
}
