// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform.

//! Measuring the separation models on this computer, against a test set
//! whose parts are known, so each model gets a score of Audiform's own
//! beside the one its author published.
//!
//! Music models are scored on the MUSDB18-HQ test songs (each song comes
//! as the mix and its vocals, drums, bass and other parts). Clean-up
//! models are scored on inputs made here from those songs' vocals, where
//! the answer is known exactly: the voice with noise added, with a room
//! reverb added, or with some of the backing leaking in. Models whose
//! part the test set does not hold (lead against backing vocals, single
//! instruments, the center and wide of a stereo mix) are listed with the
//! reason they were not scored.
//!
//! The score is the signal-to-distortion ratio of each song taken whole
//! (the reference's energy over the error's, in dB), averaged over the
//! songs: the same measure the public separation leaderboards use.

use std::path::{Path, PathBuf};

use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;
use serde::Serialize;

use super::ModelEntry;

/// Every test set file is read at this rate, in stereo.
pub const RATE: u32 = 44_100;

/// The parts of a MUSDB18-HQ song folder.
pub const PARTS: [&str; 5] = ["mixture", "vocals", "drums", "bass", "other"];

/// How the noise test mixes: noise this many dB under the voice.
pub const NOISE_SNR_DB: f32 = 5.0;
/// The reverb test's room: decay to -60 dB in this many seconds.
pub const REVERB_RT60: f32 = 1.2;
/// The reverb test: the reverb this many dB under the dry voice.
pub const REVERB_LEVEL_DB: f32 = -3.0;
/// The leakage tests: the leaking part this many dB under its own level.
pub const VOCAL_BLEED_DB: f32 = -15.0;
pub const INSTRUMENTAL_BLEED_DB: f32 = -20.0;

/// What a model is fed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Test {
    /// The song's mix.
    Music,
    /// The vocals with noise added.
    Noise,
    /// The vocals in a room.
    Reverb,
    /// The vocals with some of the instrumental leaking in.
    VocalBleed,
    /// The instrumental with some of the vocals leaking in.
    InstrumentalBleed,
}

impl Test {
    pub fn describe(self) -> String {
        match self {
            Test::Music => "MUSDB18-HQ test songs, the full mix".into(),
            Test::Noise => format!(
                "the songs' vocals with pink noise {NOISE_SNR_DB} dB under them"
            ),
            Test::Reverb => format!(
                "the songs' vocals in a made-up room ({REVERB_RT60} s decay, reverb {REVERB_LEVEL_DB} dB)"
            ),
            Test::VocalBleed => format!(
                "the songs' vocals with their instrumental leaking in at {VOCAL_BLEED_DB} dB"
            ),
            Test::InstrumentalBleed => format!(
                "the songs' instrumentals with their vocals leaking in at {INSTRUMENTAL_BLEED_DB} dB"
            ),
        }
    }
}

/// The right answer a result is compared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reference {
    Vocals,
    Drums,
    Bass,
    Other,
    /// The mix without the vocals.
    Instrumental,
    /// The mix without the drums.
    NoDrums,
    /// The mix without the bass.
    NoBass,
    /// For a made-up input: the part that was there before anything was added.
    Clean,
    /// For a made-up input: what was added (the noise, the reverb, the leak).
    Added,
}

impl Reference {
    pub fn name(self) -> &'static str {
        match self {
            Reference::Vocals => "vocals",
            Reference::Drums => "drums",
            Reference::Bass => "bass",
            Reference::Other => "other",
            Reference::Instrumental => "instrumental (mix minus vocals)",
            Reference::NoDrums => "mix minus drums",
            Reference::NoBass => "mix minus bass",
            Reference::Clean => "the clean part",
            Reference::Added => "what was added",
        }
    }
}

/// One comparison: the sum of these outputs against a reference.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub outputs: Vec<usize>,
    pub label: String,
    pub reference: Reference,
}

/// How a model is measured.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub test: Test,
    pub checks: Vec<Check>,
    /// Anything that makes the score less comparable.
    pub caveat: Option<String>,
}

fn check(outputs: &[usize], names: &[&str], reference: Reference) -> Check {
    Check {
        outputs: outputs.to_vec(),
        label: outputs
            .iter()
            .map(|&k| names.get(k).copied().unwrap_or("?"))
            .collect::<Vec<_>>()
            .join(" + "),
        reference,
    }
}

/// How `m` is measured, or why it cannot be.
pub fn plan(m: &ModelEntry) -> Result<Plan, String> {
    let names = m.output_names();
    let find = |n: &str| names.iter().position(|x| x.eq_ignore_ascii_case(n));
    let two = names.len() == 2;
    let simple = |test: Test, target: Reference, rest: Reference| Plan {
        test,
        checks: vec![check(&[0], &names, target), check(&[1], &names, rest)],
        caveat: None,
    };
    if m.id == "mega-53" {
        return Err("53 results whose vocals, bass and drums are split finer than the test set's (backing vocals, kick, snare and so on are results of their own); its single-instrument splits are scored instead where the test set has the part".into());
    }
    match m.id {
        "mel-denoise" | "mel-denoise-aggressive" => {
            return Ok(simple(Test::Noise, Reference::Clean, Reference::Added))
        }
        "dereverb-anvuew" => return Ok(simple(Test::Reverb, Reference::Clean, Reference::Added)),
        "deeffect-gilliaan" => {
            let mut p = simple(Test::Reverb, Reference::Clean, Reference::Added);
            p.caveat = Some("scored on reverb only; it also takes off delays and other effects, which this test does not add".into());
            return Ok(p);
        }
        "cleaninvert-gilliaan" => {
            return Ok(simple(Test::VocalBleed, Reference::Clean, Reference::Added))
        }
        "bleed-suppressor" => {
            return Ok(simple(
                Test::InstrumentalBleed,
                Reference::Clean,
                Reference::Added,
            ))
        }
        _ => {}
    }
    if m.id == "delimit-becruily" {
        return Err("no public test set pairs loud masters with their unlimited mixes".into());
    }
    if names.iter().any(|n| n.eq_ignore_ascii_case("Lead vocal")) {
        return Err("the test set has the vocals as one part, so lead and backing vocals cannot be told apart".into());
    }
    if m.id.starts_with("duet") {
        return Err("no public test set has two singers recorded apart".into());
    }
    if m.category == "Stereo" {
        return Err("no test set has the center and the wide of a mix as parts".into());
    }
    // Several stems: each one the test set has, and the rest summed into "other".
    if !m.stems.is_empty() && !two {
        let (Some(v), Some(d), Some(b)) = (find("Vocals"), find("Drums"), find("Bass")) else {
            return Err(
                "its results do not line up with the test set's vocals, drums, bass and other"
                    .into(),
            );
        };
        let rest: Vec<usize> = (0..names.len())
            .filter(|k| ![v, d, b].contains(k))
            .collect();
        return Ok(Plan {
            test: Test::Music,
            checks: vec![
                check(&[v], &names, Reference::Vocals),
                check(&[d], &names, Reference::Drums),
                check(&[b], &names, Reference::Bass),
                check(&rest, &names, Reference::Other),
            ],
            caveat: (rest.len() > 1).then(|| {
                "its other result is split further; those results are summed to compare with the test set's other".into()
            }),
        });
    }
    let target = names
        .first()
        .copied()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mega = m.group.starts_with("Mega 53");
    let caveat = mega.then(|| {
        "one instrument from the Mega 53 model; its idea of this part can differ from the test set's".to_string()
    });
    let plan = match target.as_str() {
        "vocals" => simple(Test::Music, Reference::Vocals, Reference::Instrumental),
        "drums" | "drum kit" => simple(Test::Music, Reference::Drums, Reference::NoDrums),
        "bass" => simple(Test::Music, Reference::Bass, Reference::NoBass),
        _ => {
            return Err(format!(
                "the test set has no {} part of its own (only vocals, drums, bass and other)",
                names
                    .first()
                    .copied()
                    .unwrap_or("such")
                    .to_ascii_lowercase()
            ))
        }
    };
    Ok(Plan { caveat, ..plan })
}

/// The signal-to-distortion ratio of `estimate` against `reference`, both
/// interleaved alike, in dB. None when the reference is silent (a song
/// with no such part), which would make any score meaningless.
pub fn sdr(reference: &[f32], estimate: &[f32]) -> Option<f64> {
    let n = reference.len().min(estimate.len());
    if n == 0 {
        return None;
    }
    let mut signal = 0.0f64;
    let mut error = 0.0f64;
    for (r, e) in reference[..n].iter().zip(&estimate[..n]) {
        let r = f64::from(*r);
        signal += r * r;
        let d = r - f64::from(*e);
        error += d * d;
    }
    // Quieter than about -80 dBFS on average: nothing to compare with.
    if signal / n as f64 <= 1e-8 {
        return None;
    }
    const EPS: f64 = 1e-10;
    Some(10.0 * ((signal + EPS) / (error + EPS)).log10())
}

/// Mean, median, lowest and highest, for the report.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Stats {
    pub mean: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub count: usize,
}

pub fn stats(values: &[f64]) -> Option<Stats> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    let median = if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    };
    Some(Stats {
        mean: v.iter().sum::<f64>() / n as f64,
        median,
        min: v[0],
        max: v[n - 1],
        count: n,
    })
}

/// The test songs under `dir`: its `test` folder when it has one (the
/// whole MUSDB18-HQ folder was picked), else `dir` itself. Each song is a
/// folder with the five parts as WAV files. Sorted by name.
pub fn songs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let base = if dir.join("test").is_dir() {
        dir.join("test")
    } else {
        dir.to_path_buf()
    };
    let mut out: Vec<PathBuf> = std::fs::read_dir(&base)
        .map_err(|e| format!("Could not read {}: {e}", base.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            PARTS
                .iter()
                .all(|part| p.join(format!("{part}.wav")).is_file())
        })
        .collect();
    out.sort();
    if out.is_empty() {
        return Err(format!(
            "No test songs in {}. Pick the MUSDB18-HQ folder (the one holding \"test\"), or its \"test\" folder: each song folder holds mixture.wav, vocals.wav, drums.wav, bass.wav and other.wav.",
            base.display()
        ));
    }
    Ok(out)
}

/// One song's parts, stereo at `RATE`, all the same length.
pub struct Song {
    pub name: String,
    pub mixture: Vec<f32>,
    pub vocals: Vec<f32>,
    pub drums: Vec<f32>,
    pub bass: Vec<f32>,
    pub other: Vec<f32>,
}

impl Song {
    /// Builds a song from its parts (each interleaved stereo), cut to the
    /// shortest and to `limit` samples when given.
    pub fn new(name: String, mut parts: [Vec<f32>; 5], limit: Option<usize>) -> Self {
        let mut n = parts.iter().map(Vec::len).min().unwrap_or(0) & !1;
        if let Some(l) = limit {
            n = n.min(l & !1);
        }
        for p in &mut parts {
            p.truncate(n);
        }
        let [mixture, vocals, drums, bass, other] = parts;
        Song {
            name,
            mixture,
            vocals,
            drums,
            bass,
            other,
        }
    }

    fn minus(&self, part: &[f32]) -> Vec<f32> {
        self.mixture.iter().zip(part).map(|(m, p)| m - p).collect()
    }

    /// What `test` feeds the model, and the clean and added parts it was
    /// made of (the mix has none).
    pub fn input(&self, test: Test, seed: u32) -> Made {
        match test {
            Test::Music => Made {
                input: self.mixture.clone(),
                clean: Vec::new(),
                added: Vec::new(),
            },
            Test::Noise => {
                let noise = pink_noise(self.vocals.len(), seed);
                let added = scaled_under(&noise, &self.vocals, NOISE_SNR_DB);
                Made::sum(self.vocals.clone(), added)
            }
            Test::Reverb => {
                let wet = convolve_stereo(&self.vocals, &room(seed));
                let added = scaled_under(&wet, &self.vocals, -REVERB_LEVEL_DB);
                Made::sum(self.vocals.clone(), added)
            }
            Test::VocalBleed => {
                let inst = self.minus(&self.vocals);
                let g = db(VOCAL_BLEED_DB);
                Made::sum(self.vocals.clone(), inst.iter().map(|x| x * g).collect())
            }
            Test::InstrumentalBleed => {
                let g = db(INSTRUMENTAL_BLEED_DB);
                Made::sum(
                    self.minus(&self.vocals),
                    self.vocals.iter().map(|x| x * g).collect(),
                )
            }
        }
    }

    /// The right answer for `r`, given the made-up input when there is one.
    pub fn reference(&self, r: Reference, made: &Made) -> Vec<f32> {
        match r {
            Reference::Vocals => self.vocals.clone(),
            Reference::Drums => self.drums.clone(),
            Reference::Bass => self.bass.clone(),
            Reference::Other => self.other.clone(),
            Reference::Instrumental => self.minus(&self.vocals),
            Reference::NoDrums => self.minus(&self.drums),
            Reference::NoBass => self.minus(&self.bass),
            Reference::Clean => made.clean.clone(),
            Reference::Added => made.added.clone(),
        }
    }
}

/// A test input and what it is made of.
pub struct Made {
    pub input: Vec<f32>,
    pub clean: Vec<f32>,
    pub added: Vec<f32>,
}

impl Made {
    fn sum(clean: Vec<f32>, added: Vec<f32>) -> Self {
        let input = clean.iter().zip(&added).map(|(a, b)| a + b).collect();
        Made {
            input,
            clean,
            added,
        }
    }
}

fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| f64::from(*v) * f64::from(*v)).sum()
}

/// `x` scaled to sit `under_db` below `level`'s energy.
fn scaled_under(x: &[f32], level: &[f32], under_db: f32) -> Vec<f32> {
    let ex = energy(x);
    if ex <= 0.0 {
        return vec![0.0; x.len()];
    }
    let g = ((energy(level) / ex).sqrt() * f64::from(db(-under_db))) as f32;
    x.iter().map(|v| v * g).collect()
}

/// A small fixed random sequence, so every run makes the same inputs.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Interleaved stereo pink noise (each side its own), `len` samples.
fn pink_noise(len: usize, seed: u32) -> Vec<f32> {
    let mut out = Vec::with_capacity(len);
    let mut rng = Rng(seed.max(1));
    // Paul Kellett's economy filter, one state per side.
    let mut s = [[0.0f32; 3]; 2];
    for k in 0..len {
        let c = k % 2;
        let w = rng.next();
        let b = &mut s[c];
        b[0] = 0.99765 * b[0] + w * 0.099_046;
        b[1] = 0.96300 * b[1] + w * 0.296_516_4;
        b[2] = 0.57000 * b[2] + w * 1.052_691_3;
        out.push(b[0] + b[1] + b[2] + w * 0.1848);
    }
    out
}

/// A made-up room: decaying noise, each side its own, after a short gap,
/// with no direct sound (that is the dry voice).
fn room(seed: u32) -> [Vec<f32>; 2] {
    let len = (REVERB_RT60 * 1.25 * RATE as f32) as usize;
    let gap = (0.012 * RATE as f32) as usize;
    let decay = -6.907_755 / (REVERB_RT60 * RATE as f32); // ln(10^-3) per sample
    let mut rng = Rng(seed.wrapping_mul(2_654_435_761).max(1));
    let side = |rng: &mut Rng| -> Vec<f32> {
        (0..len)
            .map(|k| {
                if k < gap {
                    0.0
                } else {
                    rng.next() * (decay * (k - gap) as f32).exp()
                }
            })
            .collect()
    };
    [side(&mut rng), side(&mut rng)]
}

/// Convolves each side of interleaved stereo `x` with its own response,
/// keeping `x`'s length (overlap-add through the FFT).
fn convolve_stereo(x: &[f32], ir: &[Vec<f32>; 2]) -> Vec<f32> {
    let frames = x.len() / 2;
    let mut out = vec![0.0f32; x.len()];
    let taps = ir[0].len().max(ir[1].len());
    let size = (2 * taps).next_power_of_two();
    let block = size - taps + 1;
    let mut planner = RealFftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(size);
    let inv = planner.plan_fft_inverse(size);
    for (c, h) in ir.iter().enumerate() {
        let mut hbuf = vec![0.0f32; size];
        hbuf[..h.len()].copy_from_slice(h);
        let mut hspec = fwd.make_output_vec();
        let _ = fwd.process(&mut hbuf, &mut hspec);
        let mut buf = vec![0.0f32; size];
        let mut spec = fwd.make_output_vec();
        let mut start = 0;
        while start < frames {
            let n = block.min(frames - start);
            buf.iter_mut().for_each(|v| *v = 0.0);
            for k in 0..n {
                buf[k] = x[(start + k) * 2 + c];
            }
            let _ = fwd.process(&mut buf, &mut spec);
            for (s, h) in spec.iter_mut().zip(&hspec) {
                *s *= *h;
            }
            // The first and last bins must be real for the inverse.
            if let Some(first) = spec.first_mut() {
                *first = Complex32::new(first.re, 0.0);
            }
            if let Some(last) = spec.last_mut() {
                *last = Complex32::new(last.re, 0.0);
            }
            let _ = inv.process(&mut spec, &mut buf);
            let scale = 1.0 / size as f32;
            for (k, v) in buf.iter().enumerate() {
                let f = start + k;
                if f >= frames {
                    break;
                }
                out[f * 2 + c] += v * scale;
            }
            start += block;
        }
    }
    out
}

/// Adds the given outputs together, each cut or padded to `len`.
pub fn sum_outputs(outputs: &[Vec<f32>], which: &[usize], len: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; len];
    for &k in which {
        if let Some(o) = outputs.get(k) {
            for (d, s) in out.iter_mut().zip(o) {
                *d += s;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{catalog, model};
    use super::*;

    #[test]
    fn sdr_is_energy_over_error() {
        let r = vec![1.0f32, -1.0, 1.0, -1.0];
        // A perfect estimate scores very high; half the signal scores 6 dB.
        assert!(sdr(&r, &r).unwrap() > 90.0);
        let half: Vec<f32> = r.iter().map(|x| x * 0.5).collect();
        assert!((sdr(&r, &half).unwrap() - 6.0206).abs() < 1e-3);
        // Silence is the error itself: 0 dB.
        assert!(sdr(&r, &[0.0; 4]).unwrap().abs() < 1e-6);
        // A silent reference gives no score.
        assert_eq!(sdr(&[0.0; 4], &r), None);
    }

    #[test]
    fn stats_mean_and_median() {
        let s = stats(&[1.0, 5.0, 3.0, 10.0]).unwrap();
        assert_eq!(
            (s.mean, s.median, s.min, s.max, s.count),
            (4.75, 4.0, 1.0, 10.0, 4)
        );
        assert!(stats(&[]).is_none());
    }

    #[test]
    fn plans_match_the_test_set() {
        let p = plan(&model("bs-polarformer").unwrap()).unwrap();
        assert_eq!(p.test, Test::Music);
        assert_eq!(p.checks[0].reference, Reference::Vocals);
        assert_eq!(p.checks[1].reference, Reference::Instrumental);
        // Six stems: guitar and piano are summed into other.
        let p = plan(&model("bs-roformer-sw").unwrap()).unwrap();
        assert_eq!(p.checks.len(), 4);
        assert_eq!(p.checks[3].reference, Reference::Other);
        assert_eq!(p.checks[3].label, "Other + Guitar + Piano");
        assert_eq!(
            plan(&model("xlance-drums").unwrap()).unwrap().checks[1].reference,
            Reference::NoDrums
        );
        assert_eq!(
            plan(&model("mel-denoise").unwrap()).unwrap().test,
            Test::Noise
        );
        assert_eq!(
            plan(&model("dereverb-anvuew").unwrap()).unwrap().test,
            Test::Reverb
        );
        assert_eq!(
            plan(&model("bleed-suppressor").unwrap()).unwrap().test,
            Test::InstrumentalBleed
        );
        assert!(plan(&model("karaoke-anvuew").unwrap()).is_err());
        assert!(plan(&model("xlance-guitar").unwrap()).is_err());
        assert!(plan(&model("centerwide-bs-gilliaan").unwrap()).is_err());
        assert!(plan(&model("mega-53").unwrap()).is_err());
        // Every model gets a plan or a reason.
        for m in catalog() {
            match plan(&m) {
                Ok(p) => assert!(
                    p.checks
                        .iter()
                        .all(|c| c.outputs.iter().all(|&k| k < m.output_names().len())),
                    "{}",
                    m.id
                ),
                Err(why) => assert!(!why.is_empty(), "{}", m.id),
            }
        }
    }

    #[test]
    fn made_inputs_add_up_and_sit_at_their_levels() {
        let n = 2 * RATE as usize;
        let tone: Vec<f32> = (0..n)
            .map(|k| ((k / 2) as f32 * 0.03).sin() * 0.5)
            .collect();
        let song = Song::new(
            "t".into(),
            [
                tone.clone(),
                tone.clone(),
                tone.clone(),
                vec![0.0; n],
                vec![0.0; n],
            ],
            None,
        );
        for test in [Test::Noise, Test::Reverb] {
            let m = song.input(test, 7);
            assert_eq!(m.input.len(), n);
            // The input is exactly clean plus added.
            let back: Vec<f32> = m.clean.iter().zip(&m.added).map(|(a, b)| a + b).collect();
            assert!(sdr(&m.input, &back).unwrap() > 90.0);
            // Doing nothing scores about the level the addition sits under.
            let want = if test == Test::Noise {
                NOISE_SNR_DB
            } else {
                -REVERB_LEVEL_DB
            };
            assert!(
                (sdr(&m.clean, &m.input).unwrap() - f64::from(want)).abs() < 0.2,
                "{test:?}"
            );
        }
        // The same seed makes the same input.
        assert_eq!(
            song.input(Test::Noise, 3).input,
            song.input(Test::Noise, 3).input
        );
    }

    #[test]
    fn convolution_matches_direct() {
        let x: Vec<f32> = (0..64).map(|k| (k as f32 * 0.7).sin()).collect();
        let ir = [vec![0.5, 0.25, 0.0, -0.1], vec![1.0, 0.0, 0.0, 0.0]];
        let y = convolve_stereo(&x, &ir);
        for f in 0..32 {
            for c in 0..2 {
                let want: f32 = (0..4)
                    .filter(|&t| t <= f)
                    .map(|t| ir[c][t] * x[(f - t) * 2 + c])
                    .sum();
                assert!((y[f * 2 + c] - want).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn songs_are_found_in_the_test_folder() {
        let dir = std::env::temp_dir().join(format!("af-musdb-{}", std::process::id()));
        let song = dir.join("test").join("A Song");
        std::fs::create_dir_all(&song).unwrap();
        for p in PARTS {
            std::fs::write(song.join(format!("{p}.wav")), b"").unwrap();
        }
        std::fs::create_dir_all(dir.join("test").join("Not a song")).unwrap();
        assert_eq!(songs(&dir).unwrap(), vec![song.clone()]);
        assert_eq!(songs(&dir.join("test")).unwrap(), vec![song]);
        assert!(songs(&dir.join("test").join("Not a song")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
