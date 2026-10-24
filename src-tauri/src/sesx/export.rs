// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Export of a session as a `.sesx` multitrack session file, so a user
//! can take an edit back to an editor that reads that format. It is a
//! copy for interoperability, never our save format, and it carries less
//! than our own file: tracks (name, mono or stereo, height, volume, pan,
//! mute, solo), clips (position, source range, gain, mute, lock, fades,
//! time stretch and pitch), VST3 plug-ins with their settings (on clips,
//! tracks and the master) and the media files they use. Built-in effects,
//! envelopes, loops, buses and markers are left out, and the report
//! counts them so the user is told.
//!
//! The XML layout follows the same documented-by-example structure the
//! importer reads. Component ids are fresh for every file.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use uuid::Uuid;

use crate::session::model::{Clip, EffectChain, FadeShape, MediaRef, Session};

/// What was left out, for the notice shown after export.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SesxExportReport {
    pub tracks: usize,
    pub clips: usize,
    /// Track, clip and master Voice Chains with at least one built-in
    /// effect (VST3 plug-ins are written).
    pub chains_left_out: usize,
    /// Clips whose volume, pan or effect envelopes were left out.
    pub envelopes_left_out: usize,
    /// Looped clips, written as plain clips.
    pub clips_simplified: usize,
    pub markers_left_out: usize,
    /// Buses, plus tracks routed to a bus, with sends, or with a
    /// sidechain. Every track is written straight to the mix.
    pub routing_left_out: usize,
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn gain(db: f64) -> f64 {
    if db <= -96.0 {
        0.0
    } else {
        10f64.powf(db / 20.0)
    }
}

fn guid() -> String {
    Uuid::new_v4().to_string()
}

/// The fader, mute and pan components of a track or clip.
fn components(
    out: &mut String,
    indent: &str,
    fader_id: &str,
    volume_db: f64,
    muted: bool,
    pan: Option<f64>,
) {
    let mute_id = if fader_id == "clipGain" {
        "clipMute"
    } else {
        "trackMute"
    };
    let _ = writeln!(
        out,
        r#"{indent}<component componentGuid="{}" componentID="Audition.Fader" id="{fader_id}" name="volume" powered="true">"#,
        guid()
    );
    let _ = writeln!(
        out,
        r#"{indent}  <parameter index="0" name="volume" parameterValue="{}"/>"#,
        gain(volume_db)
    );
    let _ = writeln!(
        out,
        r#"{indent}  <parameter index="1" name="static gain" parameterValue="1"/>"#
    );
    let _ = writeln!(out, "{indent}</component>");
    let _ = writeln!(
        out,
        r#"{indent}<component componentGuid="{}" componentID="Audition.Mute" id="{mute_id}" name="Mute" powered="true">"#,
        guid()
    );
    let m = if muted { 1 } else { 0 };
    let _ = writeln!(
        out,
        r#"{indent}  <parameter index="0" parameterValue="{m}"/>"#
    );
    let _ = writeln!(
        out,
        r#"{indent}  <parameter index="1" name="mute" parameterValue="{m}"/>"#
    );
    let _ = writeln!(out, "{indent}</component>");
    if let Some(p) = pan {
        let _ = writeln!(
            out,
            r#"{indent}<component componentGuid="{}" componentID="Audition.StereoPanner" id="trackPan" name="StereoPanner" powered="true">"#,
            guid()
        );
        let _ = writeln!(
            out,
            r#"{indent}  <parameter index="0" name="Pan" parameterValue="{}"/>"#,
            p.clamp(-100.0, 100.0)
        );
        let _ = writeln!(out, "{indent}</component>");
    }
}

fn fade_type(shape: FadeShape) -> &'static str {
    match shape {
        FadeShape::Logarithmic => "log",
        FadeShape::Cosine => "cosine",
        _ => "linear",
    }
}

/// Media path as stored: relative to the exported file when it can be.
fn relative(media: &MediaRef, base: &Path) -> Option<String> {
    pathdiff::diff_paths(&media.path, base).map(|p| p.to_string_lossy().replace('\\', "/"))
}

/// A VST3 effect id as a `.sesx` component id ("3:" and four groups of
/// eight hex digits). The class id's bytes are in this platform's layout.
fn vst3_component(effect: &str) -> Option<String> {
    let hex = effect.strip_prefix(crate::plugins::PREFIX)?;
    let b = altered_plughost::module::parse_cid(hex)?;
    let be = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let words = if cfg!(windows) {
        let a = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let hi = u32::from(u16::from_le_bytes([b[4], b[5]]));
        let lo = u32::from(u16::from_le_bytes([b[6], b[7]]));
        [a, (hi << 16) | lo, be(8), be(12)]
    } else {
        [be(0), be(4), be(8), be(12)]
    };
    Some(format!(
        "3:{:08x} {:08x} {:08x} {:08x}",
        words[0], words[1], words[2], words[3]
    ))
}

/// Our saved plug-in settings (32-bit component length, then both parts)
/// in the `.sesx` form (64-bit component and controller lengths first).
fn state_to_chunk(state: &str) -> Option<String> {
    use altered_plughost::protocol::{base64_decode, base64_encode};
    let raw = base64_decode(state)?;
    let comp = u32::from_le_bytes(raw.get(..4)?.try_into().ok()?) as usize;
    let body = raw.get(4..)?;
    let ctl = body.len().checked_sub(comp)?;
    let mut out = (comp as u64).to_le_bytes().to_vec();
    out.extend_from_slice(&(ctl as u64).to_le_bytes());
    out.extend_from_slice(body);
    Some(base64_encode(&out))
}

/// Writes the VST3 plug-ins of `chain` as an effect rack. Counts the
/// chain as left out when it holds a built-in effect that is switched on.
fn rack_xml(out: &mut String, indent: &str, chain: &EffectChain, report: &mut SesxExportReport) {
    if chain
        .slots
        .iter()
        .any(|s| s.enabled && !crate::plugins::is_plugin(&s.effect))
    {
        report.chains_left_out += 1;
    }
    let plugins: Vec<_> = chain
        .slots
        .iter()
        .filter_map(|s| Some((s, vst3_component(&s.effect)?)))
        .collect();
    if plugins.is_empty() {
        return;
    }
    let _ = writeln!(out, r#"{indent}<effectsRack preFader="true">"#);
    for (i, (s, id)) in plugins.iter().enumerate() {
        let name =
            crate::plugins::find(&s.effect).map_or_else(|| "Plug-in".to_string(), |f| f.info.name);
        let _ = writeln!(
            out,
            r#"{indent}  <slot powered="{}" slotIndex="{i}">"#,
            s.enabled && chain.enabled
        );
        let _ = writeln!(
            out,
            r#"{indent}    <component componentGuid="{}" componentID="{id}" name="{}" presetName="(Custom)">"#,
            guid(),
            esc(&name)
        );
        if let Some(chunk) = s.plugin_state.as_deref().and_then(state_to_chunk) {
            let _ = writeln!(
                out,
                "{indent}      <presetChunkData><![CDATA[{chunk}]]></presetChunkData>"
            );
        }
        let _ = writeln!(out, "{indent}    </component>");
        let _ = writeln!(out, "{indent}  </slot>");
    }
    let _ = writeln!(out, "{indent}</effectsRack>");
}

fn clip_xml(
    out: &mut String,
    c: &Clip,
    index: usize,
    file_index: usize,
    len: u64,
    report: &mut SesxExportReport,
) {
    if c.looped.is_some() {
        report.clips_simplified += 1;
    }
    if c.envelopes
        .iter()
        .any(|e| e.enabled && !e.points.is_empty())
    {
        report.envelopes_left_out += 1;
    }
    // A looped clip keeps its place on the timeline and plays its
    // source region once.
    let src_len = c.source_out.saturating_sub(c.source_in);
    let stretch = c.stretch.filter(|_| c.looped.is_none());
    // A stretched clip's source points are written in stretched time.
    let ratio = stretch.map_or(1.0, |s| s.ratio);
    let scaled = |v: u64| (v as f64 * ratio).round() as u64;
    let len = if c.looped.is_some() {
        src_len.min(len)
    } else {
        len
    };
    let (src_in, src_span) = if stretch.is_some() {
        (scaled(c.source_in), len)
    } else {
        (c.source_in, src_len.min(len.max(1)))
    };
    let _ = writeln!(
        out,
        r#"        <audioClip clipAutoCrossfade="true" crossFadeHeadClipID="-1" crossFadeTailClipID="-1" endPoint="{}" fileID="{file_index}" hue="-1" id="{index}" lockedInTime="{}" looped="false" name="{}" offline="false" select="false" sourceInPoint="{}" sourceOutPoint="{}" startPoint="{}" zOrder="{index}">"#,
        c.start + len,
        c.locked,
        esc(&c.name),
        src_in,
        src_in + src_span,
        c.start,
    );
    components(out, "          ", "clipGain", c.gain_db, c.muted, None);
    if let Some(s) = stretch {
        let adv = s
            .advanced
            .unwrap_or(crate::session::model::StretchAdvanced {
                window_ms: 37.0,
                transient_sensitivity: 40.0,
                preserve_formants: true,
                high_precision: true,
            });
        let kind = if !s.keep_pitch {
            "varispeed"
        } else if s.polyphonic {
            "normal"
        } else {
            "solo"
        };
        let _ = writeln!(
            out,
            r#"          <clipStretch adaptiveWindowSize="{}" pitchAdjustment="{}" preserveFormants="{}" stretchMode="{}" stretchQuality="{}" stretchRatio="{}" stretchType="{kind}" transientSensitivity="{}"/>"#,
            adv.window_ms,
            if s.keep_pitch { s.pitch } else { 0.0 },
            adv.preserve_formants,
            if s.rendered { "rendered" } else { "realtime" },
            if adv.high_precision { "high" } else { "medium" },
            s.ratio,
            adv.transient_sensitivity,
        );
    }
    let fin = c.fade_in.length.min(len);
    let fout = c.fade_out.length.min(len);
    let _ = writeln!(
        out,
        r#"          <fadeIn crossFadeLinkType="linkedAsymmetric" endPoint="{fin}" startPoint="0" type="{}"/>"#,
        fade_type(c.fade_in.shape)
    );
    let _ = writeln!(
        out,
        r#"          <fadeOut crossFadeLinkType="linkedAsymmetric" endPoint="{len}" startPoint="{}" type="{}"/>"#,
        len - fout,
        fade_type(c.fade_out.shape)
    );
    rack_xml(out, "          ", &c.effects, report);
    let _ = writeln!(out, "        </audioClip>");
}

/// Builds the `.sesx` text for `session`, with media paths made relative
/// to `base` (the folder the file goes in) where possible.
pub fn to_sesx(session: &Session, base: &Path) -> (String, SesxExportReport) {
    let mut report = SesxExportReport {
        markers_left_out: session.markers.len(),
        ..Default::default()
    };
    let rate = session.sample_rate.max(1);
    let media_index = |id| session.media.iter().position(|m| m.id == id);
    let mut end = 0u64;
    let mut tracks = String::new();
    let mut clip_no = 0usize;
    const MASTER_ID: u32 = 10_000;
    for (ti, t) in session.tracks.iter().enumerate() {
        let stereo = matches!(t.channels, crate::session::model::ChannelLayout::Stereo);
        let channel = if stereo { "stereo" } else { "mono" };
        let _ = writeln!(
            tracks,
            r#"      <audioTrack automationLaneOpenState="false" id="{}" index="{}" select="false" visible="true">"#,
            MASTER_ID as usize + 1 + ti,
            ti + 1
        );
        let _ = writeln!(
            tracks,
            r#"        <trackParameters trackHeight="{}" trackHue="-1" trackMinimized="false">"#,
            t.height
        );
        let _ = writeln!(tracks, "          <name>{}</name>", esc(&t.name));
        let _ = writeln!(tracks, "        </trackParameters>");
        let _ = writeln!(
            tracks,
            r#"        <trackAudioParameters audioChannelType="{channel}" automationMode="1" monitoring="false" recordArmed="{}" solo="{}" soloSafe="false">"#,
            t.record_armed, t.solo
        );
        let _ = writeln!(
            tracks,
            r#"          <trackOutput outputID="{MASTER_ID}" type="trackID"/>"#
        );
        let _ = writeln!(tracks, r#"          <trackInput inputID="-1"/>"#);
        components(
            &mut tracks,
            "          ",
            "trackFader",
            t.volume_db,
            t.muted,
            Some(t.pan),
        );
        let _ = writeln!(tracks, "        </trackAudioParameters>");
        rack_xml(&mut tracks, "        ", &t.effects, &mut report);
        for c in &t.clips {
            let Some(fi) = media_index(c.media_id) else {
                continue;
            };
            let src_rate = session.media[fi].sample_rate;
            let len = c.timeline_length(src_rate, rate);
            if len == 0 {
                continue;
            }
            end = end.max(c.start + len);
            clip_xml(&mut tracks, c, clip_no, fi, len, &mut report);
            clip_no += 1;
            report.clips += 1;
        }
        let _ = writeln!(tracks, "      </audioTrack>");
        report.tracks += 1;
    }
    report.routing_left_out = session.buses.len()
        + session
            .tracks
            .iter()
            .filter(|t| {
                t.output != crate::session::model::Output::Master
                    || !t.sends.is_empty()
                    || crate::session::routing::key_sources(&t.effects)
                        .next()
                        .is_some()
            })
            .count();
    let master_channel = match session.master.channels {
        crate::session::model::ChannelLayout::Mono => "mono",
        _ => "stereo",
    };
    let _ = writeln!(
        tracks,
        r#"      <masterTrack automationLaneOpenState="false" id="{MASTER_ID}" index="{}" select="false" visible="true">"#,
        session.tracks.len() + 1
    );
    let _ = writeln!(
        tracks,
        r#"        <trackParameters trackHeight="{}" trackHue="-1" trackMinimized="false">"#,
        session.master.height
    );
    let _ = writeln!(tracks, "          <name>Mix</name>");
    let _ = writeln!(tracks, "        </trackParameters>");
    let _ = writeln!(
        tracks,
        r#"        <trackAudioParameters audioChannelType="{master_channel}" automationMode="1" monitoring="false" recordArmed="false" solo="false" soloSafe="true">"#
    );
    let _ = writeln!(
        tracks,
        r#"          <trackOutput outputID="1" type="hardwareOutput"/>"#
    );
    let _ = writeln!(tracks, r#"          <trackInput inputID="-1"/>"#);
    components(
        &mut tracks,
        "          ",
        "trackFader",
        session.master.volume_db,
        false,
        Some(0.0),
    );
    let _ = writeln!(tracks, "        </trackAudioParameters>");
    rack_xml(
        &mut tracks,
        "        ",
        &session.master.effects,
        &mut report,
    );
    let _ = writeln!(tracks, "      </masterTrack>");

    let mut files = String::new();
    for (i, m) in session.media.iter().enumerate() {
        let rel = relative(m, base)
            .map(|r| format!(r#" relativePath="{}""#, esc(&r)))
            .unwrap_or_default();
        let _ = writeln!(
            files,
            r#"    <file absolutePath="{}" id="{i}"{rel}/>"#,
            esc(&m.path.to_string_lossy())
        );
    }

    let bit_depth = match session.bit_depth {
        16 | 24 | 32 => session.bit_depth,
        _ => 32,
    };
    let playhead = session.view.playhead;
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n<!DOCTYPE sesx>\n<sesx version=\"1.9\">\n");
    let _ = writeln!(
        xml,
        r#"  <session audioChannelType="{master_channel}" bitDepth="{bit_depth}" duration="{}" sampleRate="{rate}">"#,
        end.max(1)
    );
    xml.push_str("    <tracks>\n");
    xml.push_str(&tracks);
    xml.push_str("    </tracks>\n");
    let _ = writeln!(
        xml,
        r#"    <sessionState ctiPosition="{playhead}" smpteStart="0">"#
    );
    let _ = writeln!(
        xml,
        r#"      <selectionState selectionDuration="0" selectionStart="{playhead}"/>"#
    );
    xml.push_str("    </sessionState>\n  </session>\n  <files>\n");
    xml.push_str(&files);
    xml.push_str("  </files>\n</sesx>\n");
    (xml, report)
}

/// Writes `session` as a `.sesx` file at `path`.
pub fn export(session: &Session, path: &Path) -> std::io::Result<SesxExportReport> {
    let base: PathBuf = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let (xml, report) = to_sesx(session, &base);
    let tmp = path.with_extension("sesx.tmp");
    std::fs::write(&tmp, xml.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{new_id, ChannelLayout, EffectChain, Fade, Track};

    fn session(dir: &Path) -> Session {
        let mut s = Session::new("Ep 1", 48_000);
        let m = MediaRef {
            id: new_id(),
            path: dir.join("Audio").join("host & guest.wav"),
            relative_path: None,
            sample_rate: 48_000,
            channels: 2,
            length: 480_000,
        };
        let mut t = Track::new("Host <1>", ChannelLayout::Mono);
        t.volume_db = -6.0;
        t.pan = -25.0;
        t.clips.push(Clip {
            id: new_id(),
            name: "Take".into(),
            media_id: m.id,
            start: 96_000,
            source_in: 48_000,
            source_out: 240_000,
            gain_db: 3.0,
            muted: false,
            locked: true,
            color: None,
            fade_in: Fade {
                length: 4_800,
                shape: FadeShape::Cosine,
                curve: 0.0,
            },
            fade_out: Fade::default(),
            envelopes: Vec::new(),
            effects: EffectChain::default(),
            group: None,
            looped: None,
            stretch: None,
            audio_type: None,
        });
        s.media.push(m);
        s.tracks.push(t);
        s
    }

    /// What we write, our own importer reads back the same.
    #[test]
    fn round_trips_through_the_importer() {
        let dir = tempfile::tempdir().unwrap();
        let s = session(dir.path());
        let path = dir.path().join("Ep 1.sesx");
        let report = export(&s, &path).unwrap();
        assert_eq!(
            (report.tracks, report.clips, report.chains_left_out),
            (1, 1, 0)
        );
        let back = crate::sesx::import(&path).unwrap();
        let t = &back.session.tracks[0];
        assert_eq!(t.name, "Host <1>");
        assert!(matches!(t.channels, ChannelLayout::Mono));
        assert!((t.volume_db + 6.0).abs() < 0.11, "{}", t.volume_db);
        assert_eq!(t.pan, -25.0);
        let c = &t.clips[0];
        assert_eq!(
            (c.start, c.source_in, c.source_out, c.locked),
            (96_000, 48_000, 240_000, true)
        );
        assert_eq!(
            (c.fade_in.length, c.fade_in.shape),
            (4_800, FadeShape::Cosine)
        );
        assert_eq!(back.session.sample_rate, 48_000);
        let xml = std::fs::read_to_string(&path).unwrap();
        assert!(
            xml.contains(r#"relativePath="Audio/host &amp; guest.wav""#),
            "{xml}"
        );
    }

    #[test]
    fn round_trips_stretch_pitch_and_plug_ins() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        let state = altered_plughost::protocol::base64_encode(
            &[&3u32.to_le_bytes()[..], b"abcde"].concat(),
        );
        s.tracks[0]
            .effects
            .slots
            .push(crate::session::model::EffectSlot {
                id: new_id(),
                effect: "vst3:565354415438314175746F2D54756E65".into(),
                enabled: false,
                params: Default::default(),
                preset: None,
                sidechain: None,
                plugin_state: Some(state.clone()),
            });
        s.tracks[0].clips[0].stretch = Some(crate::session::model::ClipStretch {
            ratio: 1.25,
            pitch: -3.0,
            polyphonic: true,
            ..Default::default()
        });
        let path = dir.path().join("x.sesx");
        let report = export(&s, &path).unwrap();
        assert_eq!((report.chains_left_out, report.clips_simplified), (0, 0));
        let back = crate::sesx::import(&path).unwrap();
        let t = &back.session.tracks[0];
        let slot = &t.effects.slots[0];
        assert_eq!(slot.effect, "vst3:565354415438314175746F2D54756E65");
        assert!(!slot.enabled);
        assert_eq!(slot.plugin_state.as_deref(), Some(state.as_str()));
        let (a, b) = (&s.tracks[0].clips[0], &t.clips[0]);
        let st = b.stretch.unwrap();
        assert_eq!(
            (st.ratio, st.pitch, st.polyphonic, st.keep_pitch),
            (1.25, -3.0, true, true)
        );
        assert_eq!(
            (b.start, b.source_in, b.source_out),
            (a.start, a.source_in, a.source_out)
        );
    }

    #[test]
    fn counts_what_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        s.tracks[0]
            .effects
            .slots
            .push(crate::session::model::EffectSlot {
                id: new_id(),
                effect: "altaudio.gain".into(),
                enabled: true,
                params: Default::default(),
                preset: None,
                sidechain: None,
                plugin_state: None,
            });
        s.markers.push(crate::session::model::Marker {
            id: new_id(),
            name: "Ad".into(),
            kind: Default::default(),
            position: 0,
            duration: 0,
            description: String::new(),
            chapter: None,
            contracted_duration: None,
            color: None,
        });
        let (_, r) = to_sesx(&s, dir.path());
        assert_eq!((r.chains_left_out, r.markers_left_out), (1, 1));
    }
}
