// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Neural amp captures: plays `.nam` model files, the open format made by
//! the Neural Amp Modeler project (MIT licence), whose reference code
//! was read to get the file layout and the sums right. Two kinds of
//! capture are played here: WaveNet (dilated convolutions, by far the
//! most common) and LSTM (a recurrent network). Newer kinds report that
//! they are not supported yet instead of playing wrongly.
//!
//! A model is mono: one sample in, one sample out, at the rate it was
//! trained at (nearly always 48 kHz).

use serde_json::Value;

/// Frames worked on at once; longer blocks are split.
const BLOCK: usize = 256;

/// What a model file says about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    /// "WaveNet" or "LSTM".
    pub kind: String,
    /// The rate it was trained at, when the file says.
    pub rate: Option<f64>,
    /// Its output loudness in dB, when the file says, for leveling.
    pub loudness: Option<f64>,
    pub name: Option<String>,
}

/// A loaded model, ready to play.
#[derive(Debug, Clone)]
pub enum Model {
    WaveNet(WaveNet),
    Lstm(Lstm),
}

/// Reads a `.nam` file's text.
pub fn parse(text: &str) -> Result<(Model, ModelInfo), String> {
    let v: Value =
        serde_json::from_str(text).map_err(|_| "This is not a model file.".to_string())?;
    parse_value(&v)
}

pub fn parse_value(v: &Value) -> Result<(Model, ModelInfo), String> {
    let kind = v["architecture"].as_str().unwrap_or("").to_string();
    let meta = &v["metadata"];
    let info = ModelInfo {
        kind: kind.clone(),
        rate: v["sample_rate"].as_f64(),
        loudness: meta["loudness"].as_f64(),
        name: meta["name"]
            .as_str()
            .map(str::to_string)
            .filter(|s| !s.is_empty()),
    };
    let weights: Vec<f32> = v["weights"]
        .as_array()
        .ok_or("The model has no weights.")?
        .iter()
        .map(|w| w.as_f64().unwrap_or(0.0) as f32)
        .collect();
    let config = &v["config"];
    let model = match kind.as_str() {
        "WaveNet" => Model::WaveNet(WaveNet::new(config, &weights)?),
        "LSTM" => Model::Lstm(Lstm::new(config, &weights)?),
        "" => return Err("This is not a model file.".into()),
        other => {
            return Err(format!(
                "{other} models are not supported yet. WaveNet and LSTM captures play."
            ))
        }
    };
    Ok((model, info))
}

impl Model {
    /// Runs `buf` through the model in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        match self {
            Model::WaveNet(m) => {
                for chunk in buf.chunks_mut(BLOCK) {
                    m.process(chunk);
                }
            }
            Model::Lstm(m) => m.process(buf),
        }
    }

    /// Back to silence: clears its memory, then runs silence through it
    /// so it settles, as the reference does.
    pub fn reset(&mut self) {
        match self {
            Model::WaveNet(m) => m.clear(),
            Model::Lstm(m) => m.clear(),
        }
        let mut left = self.prewarm();
        let mut zeros = [0f32; BLOCK];
        while left > 0 {
            let n = left.min(BLOCK);
            zeros[..n].fill(0.0);
            self.process(&mut zeros[..n]);
            left -= n;
        }
    }

    fn prewarm(&self) -> usize {
        match self {
            Model::WaveNet(m) => m.prewarm,
            Model::Lstm(m) => m.prewarm,
        }
    }
}

/// Reads weights in file order.
struct Reader<'a> {
    w: &'a [f32],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<Vec<f32>, String> {
        let end = self.at + n;
        let s = self
            .w
            .get(self.at..end)
            .ok_or("The model's weights are cut short.")?;
        self.at = end;
        Ok(s.to_vec())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Act {
    Tanh,
    Relu,
    Sigmoid,
    Hardtanh,
    LeakyRelu,
    Silu,
}

impl Act {
    fn parse(v: &Value) -> Result<Act, String> {
        let name = v.as_str().or_else(|| v["type"].as_str()).unwrap_or("Tanh");
        Ok(match name {
            "Tanh" | "Fasttanh" => Act::Tanh,
            "ReLU" => Act::Relu,
            "Sigmoid" => Act::Sigmoid,
            "Hardtanh" => Act::Hardtanh,
            "LeakyReLU" => Act::LeakyRelu,
            "SiLU" | "Swish" => Act::Silu,
            other => {
                return Err(format!(
                    "The model uses a {other} step, which is not supported yet."
                ))
            }
        })
    }

    #[inline]
    fn apply(self, x: f32) -> f32 {
        match self {
            Act::Tanh => fast_tanh(x),
            Act::Relu => x.max(0.0),
            Act::Sigmoid => sigmoid(x),
            Act::Hardtanh => x.clamp(-1.0, 1.0),
            Act::LeakyRelu => {
                if x > 0.0 {
                    x
                } else {
                    0.01 * x
                }
            }
            Act::Silu => x * sigmoid(x),
        }
    }
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// A quick tanh: a rational fit within about 1e-4, the one the Neural
/// Amp Modeler plug-in plays captures with (MIT licence, see
/// ATTRIBUTION.md), so they sound as they do there, and much cheaper
/// than the exact one.
#[inline]
fn fast_tanh(x: f32) -> f32 {
    let ax = x.abs();
    let x2 = x * x;
    (x * (2.455_507_5 + 2.455_507_5 * ax + (0.893_229_85 + 0.821_226_7 * ax) * x2))
        / (2.445_066_3 + (2.445_066_3 + x2) * (x + 0.814_642_7 * x * ax).abs())
}

#[inline]
fn fast_sigmoid(x: f32) -> f32 {
    0.5 * (fast_tanh(x * 0.5) + 1.0)
}

/// One dilated layer: conv over its input's past, plus the input signal
/// mixed in, an activation, a 1x1 back to its width added to its input.
#[derive(Debug, Clone)]
struct Layer {
    dilation: usize,
    /// Conv weights by tap, then output, then input: [k][o][i].
    conv: Vec<f32>,
    conv_b: Vec<f32>,
    /// Input signal mixed in: [o][c].
    mixin: Vec<f32>,
    /// 1x1 after the activation: [o][i], and its bias.
    out: Vec<f32>,
    out_b: Vec<f32>,
    gated: bool,
    /// The layer's input, one row per channel: `look` frames of past,
    /// then the block. Rows are `look + BLOCK` long.
    hist: Vec<f32>,
    look: usize,
}

impl Layer {
    fn stride(&self) -> usize {
        self.look + BLOCK
    }
}

/// `y += a * x`, element by element (the compiler turns this into vector code).
#[inline]
fn axpy(y: &mut [f32], a: f32, x: &[f32]) {
    for (y, x) in y.iter_mut().zip(x) {
        *y += a * x;
    }
}

#[derive(Debug, Clone)]
struct LayerArray {
    input: usize,
    cond: usize,
    channels: usize,
    head: usize,
    kernel: usize,
    act: Act,
    /// Input to channels: [o][i].
    rechannel: Vec<f32>,
    layers: Vec<Layer>,
    /// Channels to head: [o][i], and its bias when it has one.
    head_w: Vec<f32>,
    head_b: Option<Vec<f32>>,
}

/// A WaveNet capture: layer arrays one after another, each adding to a
/// head that becomes the output.
#[derive(Debug, Clone)]
pub struct WaveNet {
    arrays: Vec<LayerArray>,
    head_scale: f32,
    prewarm: usize,
    // Scratch, sized once; one row of BLOCK per channel.
    cond: Vec<f32>,
    z: Vec<f32>,
    head_in: Vec<f32>,
    head_out: Vec<f32>,
    array_out: Vec<f32>,
}

fn usize_of(v: &Value, key: &str) -> Result<usize, String> {
    v[key]
        .as_u64()
        .map(|n| n as usize)
        .ok_or_else(|| format!("The model's settings lack {key}."))
}

impl WaveNet {
    fn new(config: &Value, weights: &[f32]) -> Result<WaveNet, String> {
        if !config["head"].is_null() {
            return Err(
                "This WaveNet model has a separate head, which is not supported yet.".into(),
            );
        }
        let specs = config["layers"]
            .as_array()
            .ok_or("The model's settings lack layers.")?;
        let mut r = Reader { w: weights, at: 0 };
        let mut arrays = Vec::new();
        let mut prewarm = 1;
        for s in specs {
            // Newer files describe layers differently; those are not read here.
            if s.get("kernel_sizes").is_some() || s["head"].is_object() {
                return Err("This is a newer WaveNet model, which is not supported yet.".into());
            }
            let input = usize_of(s, "input_size")?;
            let cond = usize_of(s, "condition_size")?;
            let channels = usize_of(s, "channels")?;
            let head = usize_of(s, "head_size")?;
            let kernel = usize_of(s, "kernel_size")?;
            let gated = s["gated"].as_bool().unwrap_or(false);
            let head_bias = s["head_bias"].as_bool().unwrap_or(false);
            let act = Act::parse(&s["activation"])?;
            let dilations: Vec<usize> = s["dilations"]
                .as_array()
                .ok_or("The model's settings lack dilations.")?
                .iter()
                .map(|d| d.as_u64().unwrap_or(1) as usize)
                .collect();
            let zc = if gated { 2 * channels } else { channels };
            let rechannel = r.take(channels * input)?;
            let mut layers = Vec::new();
            for &d in &dilations {
                // File order is [o][i][k]; kept here as [k][o][i] so each
                // tap's rows are contiguous.
                let raw = r.take(zc * channels * kernel)?;
                let mut conv = vec![0f32; kernel * zc * channels];
                for o in 0..zc {
                    for i in 0..channels {
                        for k in 0..kernel {
                            conv[(k * zc + o) * channels + i] =
                                raw[(o * channels + i) * kernel + k];
                        }
                    }
                }
                let conv_b = r.take(zc)?;
                let mixin = r.take(zc * cond)?;
                let out = r.take(channels * channels)?;
                let out_b = r.take(channels)?;
                let look = (kernel - 1) * d;
                prewarm += look;
                layers.push(Layer {
                    dilation: d,
                    conv,
                    conv_b,
                    mixin,
                    out,
                    out_b,
                    gated,
                    hist: vec![0.0; (look + BLOCK) * channels],
                    look,
                });
            }
            let head_w = r.take(head * channels)?;
            let head_b = if head_bias { Some(r.take(head)?) } else { None };
            arrays.push(LayerArray {
                input,
                cond,
                channels,
                head,
                kernel,
                act,
                rechannel,
                layers,
                head_w,
                head_b,
            });
        }
        let head_scale = r.take(1)?[0];
        if r.at != weights.len() {
            return Err("The model's weights do not match its settings.".into());
        }
        // Each array's head feeds the next one's head; check the sizes line up.
        for w in arrays.windows(2) {
            if w[0].head != w[1].channels {
                return Err("The model's layer sizes do not line up.".into());
            }
        }
        let first = arrays.first().ok_or("The model has no layers.")?;
        if first.input != 1 || arrays.iter().any(|a| a.cond != 1) {
            return Err("The model does not take one input channel.".into());
        }
        if arrays.last().is_some_and(|a| a.head != 1) {
            return Err("The model does not give one output channel.".into());
        }
        let widest = arrays
            .iter()
            .map(|a| a.channels.max(a.head) * 2)
            .max()
            .unwrap_or(1);
        Ok(WaveNet {
            arrays,
            head_scale,
            prewarm,
            cond: vec![0.0; BLOCK],
            z: vec![0.0; BLOCK * widest],
            head_in: vec![0.0; BLOCK * widest],
            head_out: vec![0.0; BLOCK * widest],
            array_out: vec![0.0; BLOCK * widest],
        })
    }

    fn clear(&mut self) {
        for a in &mut self.arrays {
            for l in &mut a.layers {
                l.hist.fill(0.0);
            }
        }
    }

    /// Up to BLOCK frames, in place. Works a row (one channel over the
    /// block) at a time, so the inner loops run along time.
    fn process(&mut self, buf: &mut [f32]) {
        let n = buf.len();
        self.cond[..n].copy_from_slice(buf);
        let cond = &self.cond[..n];
        let row = |c: usize| c * BLOCK..c * BLOCK + n;
        for ai in 0..self.arrays.len() {
            let a = &mut self.arrays[ai];
            let ch = a.channels;
            // This array's input (the signal, or the last array's output),
            // rechanneled into the first layer's history.
            {
                let l0 = &mut a.layers[0];
                let (stride, look) = (l0.stride(), l0.look);
                for o in 0..ch {
                    let dst = &mut l0.hist[o * stride + look..o * stride + look + n];
                    dst.fill(0.0);
                    for i in 0..a.input {
                        let w = a.rechannel[o * a.input + i];
                        let src = if ai == 0 {
                            cond
                        } else {
                            &self.array_out[row(i)]
                        };
                        axpy(dst, w, src);
                    }
                }
            }
            // Head input: zero for the first array, else the last array's head.
            for o in 0..ch {
                if ai == 0 {
                    self.head_in[row(o)].fill(0.0);
                } else {
                    let r = row(o);
                    self.head_in[r.clone()].copy_from_slice(&self.head_out[r]);
                }
            }
            let nl = a.layers.len();
            for li in 0..nl {
                let (cur, rest) = a.layers[li..].split_first_mut().expect("in range");
                let zc = if cur.gated { 2 * ch } else { ch };
                let (stride, look) = (cur.stride(), cur.look);
                // Dilated conv over the input's past, plus the signal mixed in.
                for o in 0..zc {
                    let z = &mut self.z[o * BLOCK..o * BLOCK + n];
                    z.fill(cur.conv_b[o]);
                    for k in 0..a.kernel {
                        // Tap k reads (kernel - 1 - k) dilations back.
                        let from = look - (a.kernel - 1 - k) * cur.dilation;
                        let taps = &cur.conv[(k * zc + o) * ch..(k * zc + o + 1) * ch];
                        for (i, w) in taps.iter().enumerate() {
                            let x = &cur.hist[i * stride + from..i * stride + from + n];
                            axpy(z, *w, x);
                        }
                    }
                    axpy(z, cur.mixin[o], cond);
                }
                if cur.gated {
                    let (top, bottom) = self.z.split_at_mut(ch * BLOCK);
                    for o in 0..ch {
                        for (t, b) in top[row(o)].iter_mut().zip(&bottom[row(o)]) {
                            *t = a.act.apply(*t) * sigmoid(*b);
                        }
                    }
                } else {
                    for o in 0..ch {
                        for v in &mut self.z[row(o)] {
                            *v = a.act.apply(*v);
                        }
                    }
                }
                for o in 0..ch {
                    let r = row(o);
                    axpy(&mut self.head_in[r.clone()], 1.0, &self.z[r]);
                }
                // Next layer's input (or this array's output): input + 1x1(z).
                for o in 0..ch {
                    let input = &cur.hist[o * stride + look..o * stride + look + n];
                    let dest: &mut [f32] = match rest.first_mut() {
                        Some(next) => {
                            let (ns, nl) = (next.stride(), next.look);
                            &mut next.hist[o * ns + nl..o * ns + nl + n]
                        }
                        None => &mut self.array_out[o * BLOCK..o * BLOCK + n],
                    };
                    let b = cur.out_b[o];
                    for (d, x) in dest.iter_mut().zip(input) {
                        *d = x + b;
                    }
                    for i in 0..ch {
                        axpy(dest, cur.out[o * ch + i], &self.z[row(i)]);
                    }
                }
            }
            // Head: channels to head size.
            for o in 0..a.head {
                let b = a.head_b.as_ref().map_or(0.0, |b| b[o]);
                let (out, inp) = (&mut self.head_out, &self.head_in);
                let dst = &mut out[o * BLOCK..o * BLOCK + n];
                dst.fill(b);
                for i in 0..ch {
                    axpy(dst, a.head_w[o * ch + i], &inp[i * BLOCK..i * BLOCK + n]);
                }
            }
            // Keep each layer's last `look` frames for the next block.
            for l in &mut a.layers {
                let (stride, look) = (l.stride(), l.look);
                if look > 0 {
                    for c in 0..ch {
                        let r = c * stride;
                        l.hist.copy_within(r + n..r + n + look, r);
                    }
                }
            }
        }
        for (t, s) in buf.iter_mut().enumerate() {
            *s = self.head_scale * self.head_out[t];
        }
    }
}

#[derive(Debug, Clone)]
struct Cell {
    input: usize,
    hidden: usize,
    /// [4H][input + H], gates in, forget, cell, out.
    w: Vec<f32>,
    b: Vec<f32>,
    h0: Vec<f32>,
    c0: Vec<f32>,
    xh: Vec<f32>,
    c: Vec<f32>,
    gates: Vec<f32>,
}

/// An LSTM capture: recurrent layers, then a weighted sum.
#[derive(Debug, Clone)]
pub struct Lstm {
    cells: Vec<Cell>,
    head_w: Vec<f32>,
    head_b: f32,
    prewarm: usize,
}

impl Lstm {
    fn new(config: &Value, weights: &[f32]) -> Result<Lstm, String> {
        let layers = usize_of(config, "num_layers")?;
        let input = usize_of(config, "input_size")?;
        let hidden = usize_of(config, "hidden_size")?;
        if input != 1 {
            return Err("The model does not take one input channel.".into());
        }
        let mut r = Reader { w: weights, at: 0 };
        let mut cells = Vec::new();
        for l in 0..layers {
            let inp = if l == 0 { input } else { hidden };
            let w = r.take(4 * hidden * (inp + hidden))?;
            let b = r.take(4 * hidden)?;
            let h0 = r.take(hidden)?;
            let c0 = r.take(hidden)?;
            let mut xh = vec![0.0; inp + hidden];
            xh[inp..].copy_from_slice(&h0);
            cells.push(Cell {
                input: inp,
                hidden,
                w,
                b,
                c: c0.clone(),
                h0,
                c0,
                xh,
                gates: vec![0.0; 4 * hidden],
            });
        }
        let head_w = r.take(hidden)?;
        let head_b = r.take(1)?[0];
        if r.at != weights.len() {
            return Err("The model's weights do not match its settings.".into());
        }
        Ok(Lstm {
            cells,
            head_w,
            head_b,
            // Half a second at 48 kHz, as the reference does.
            prewarm: 24_000,
        })
    }

    fn clear(&mut self) {
        for c in &mut self.cells {
            c.xh.fill(0.0);
            let inp = c.input;
            c.xh[inp..].copy_from_slice(&c.h0);
            c.c.copy_from_slice(&c.c0);
        }
    }

    fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let mut x = *s;
            let mut first = true;
            let mut prev_h: &[f32] = &[];
            for c in &mut self.cells {
                let h = c.hidden;
                if first {
                    c.xh[0] = x;
                } else {
                    c.xh[..c.input].copy_from_slice(prev_h);
                }
                let width = c.input + h;
                for (r, g) in c.gates.iter_mut().enumerate() {
                    let row = &c.w[r * width..(r + 1) * width];
                    let mut acc = c.b[r];
                    for (w, v) in row.iter().zip(&c.xh) {
                        acc += w * v;
                    }
                    *g = acc;
                }
                for i in 0..h {
                    let ig = fast_sigmoid(c.gates[i]);
                    let fg = fast_sigmoid(c.gates[h + i]);
                    let gg = fast_tanh(c.gates[2 * h + i]);
                    let og = fast_sigmoid(c.gates[3 * h + i]);
                    c.c[i] = fg * c.c[i] + ig * gg;
                    c.xh[c.input + i] = og * fast_tanh(c.c[i]);
                }
                prev_h = &c.xh[c.input..];
                first = false;
            }
            let mut y = self.head_b;
            for (w, v) in self.head_w.iter().zip(prev_h) {
                y += w * v;
            }
            x = y;
            *s = x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WaveNet with two arrays and known weights, small enough to work
    /// out by hand: every weight 0.1, head scale 0.5.
    #[test]
    fn reads_and_runs_a_tiny_wavenet() {
        let config = serde_json::json!({
            "layers": [
                {"input_size": 1, "condition_size": 1, "head_size": 1, "channels": 1, "kernel_size": 2,
                 "dilations": [1], "activation": "Tanh", "gated": false, "head_bias": false}
            ],
            "head": null, "head_scale": 0.5
        });
        // rechannel 1, conv 2 + bias 1, mixin 1, 1x1 1 + bias 1, head 1, scale 1.
        let w = vec![1.0, 0.5, 0.25, 0.0, 1.0, 0.0, 0.0, 1.0, 0.5];
        let mut m = WaveNet::new(&config, &w).unwrap();
        let mut buf = [1.0, 0.0];
        m.process(&mut buf);
        // t0: conv = 0.5*0 + 0.25*1 = 0.25, + mixin 1*1 = 1.25, tanh; out = scale * head.
        assert!((buf[0] - 0.5 * 1.25f32.tanh()).abs() < 1e-3);
        // t1: conv = 0.5*1 + 0.25*0 = 0.5, mixin 0.
        assert!((buf[1] - 0.5 * 0.5f32.tanh()).abs() < 1e-3);
        // Too few weights is an error, not a crash.
        assert!(WaveNet::new(&config, &w[..5]).is_err());
    }

    #[test]
    fn says_which_models_it_cannot_play() {
        let e =
            parse(r#"{"architecture":"SlimmableContainer","config":{},"weights":[]}"#).unwrap_err();
        assert!(e.contains("not supported"), "{e}");
        assert!(parse("not json").is_err());
    }

    /// Checked against the reference implementation's output for the
    /// same files and input, when they are on hand (see
    /// docs/decisions.md for how the reference output was made).
    #[test]
    #[ignore]
    fn matches_the_reference() {
        let dir = std::env::var("NAM_REF").expect("NAM_REF");
        for name in ["wavenet_a1_standard", "wavenet", "lstm"] {
            let text =
                std::fs::read_to_string(format!("{dir}/core/example_models/{name}.nam")).unwrap();
            let want: Vec<f32> = std::fs::read_to_string(format!("{dir}/ref_{name}.txt"))
                .unwrap()
                .lines()
                .map(|l| l.parse().unwrap())
                .collect();
            let (mut m, _) = parse(&text).unwrap();
            m.reset();
            let mut s: u32 = 12345;
            let mut input: Vec<f32> = (0..want.len())
                .map(|i| {
                    s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let noise = f64::from(s >> 8) / 16_777_216.0 * 2.0 - 1.0;
                    (0.5 * (2.0 * std::f64::consts::PI * 110.0 * i as f64 / 48_000.0).sin()
                        * (i as f64 / 2_000.0).min(1.0)
                        + 0.05 * noise) as f32
                })
                .collect();
            for c in input.chunks_mut(64) {
                m.process(c);
            }
            let worst = input
                .iter()
                .zip(&want)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            println!("{name}: largest difference {worst:e}");
            assert!(worst < 1e-4, "{name}: {worst}");
        }
    }

    /// How long a standard capture takes for ten seconds of audio.
    #[test]
    #[ignore]
    fn speed() {
        let dir = std::env::var("NAM_REF").expect("NAM_REF");
        let text =
            std::fs::read_to_string(format!("{dir}/core/example_models/wavenet_a1_standard.nam"))
                .unwrap();
        let (mut m, _) = parse(&text).unwrap();
        let mut buf = vec![0.1f32; 480_000];
        let t = std::time::Instant::now();
        for c in buf.chunks_mut(2_048) {
            m.process(c);
        }
        println!("ten seconds in {:?}", t.elapsed());
    }
}
