// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! One running VST3 plug-in, split the way VST3 wants it: [`Plugin`]
//! (setup, controls, settings, window) lives on the host's main thread,
//! and [`Audio`] (the processing) on its audio thread.

use std::ffi::c_void;
use std::path::Path;
use std::ptr;
use std::sync::{Arc, Mutex};

use vst3::Steinberg::Vst::BusDirections_::{kInput, kOutput};
use vst3::Steinberg::Vst::Event_::EventTypes_::{
    kNoteExpressionValueEvent, kNoteOffEvent, kNoteOnEvent,
};
use vst3::Steinberg::Vst::MediaTypes_::{kAudio, kEvent};
use vst3::Steinberg::Vst::NoteExpressionTypeIDs_::kPanTypeID;
use vst3::Steinberg::Vst::ParameterInfo_::ParameterFlags_::{
    kIsHidden, kIsProgramChange, kIsReadOnly,
};
use vst3::Steinberg::Vst::ProcessContext_::StatesAndFlags_::{
    kBarPositionValid, kPlaying, kProjectTimeMusicValid, kTempoValid, kTimeSigValid,
};
use vst3::Steinberg::Vst::ProcessModes_::kRealtime;
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;
use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, Event, Event__type0, IAudioProcessor,
    IAudioProcessorTrait, IComponent, IComponentHandler, IComponentTrait, IConnectionPoint,
    IConnectionPointTrait, IEditController, IEditControllerTrait, IEventList, IParameterChanges,
    NoteExpressionValueEvent, NoteOffEvent, NoteOnEvent, ParameterInfo, ProcessContext,
    ProcessData, ProcessSetup, SpeakerArr, SpeakerArrangement,
};
use vst3::Steinberg::{
    kResultOk, kResultTrue, FUnknown, IBStream, IPluginBaseTrait, IPluginFactoryTrait, TUID,
};
use vst3::{ComPtr, ComWrapper, Interface};

use crate::com::{from_wstring, Edit, EventList, Handler, HostApp, MemStream, ParamChanges};
use crate::module::{parse_cid, Module};
use crate::protocol::{base64_decode, base64_encode, NoteEvent, ParamInfo, MAX_BLOCK};

/// The tempo and time signature the plug-in is told, from timeline frame
/// `start` (at `start_music` quarter notes) on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tempo {
    pub bpm: f64,
    pub beats_per_bar: u32,
    pub beat_unit: u32,
    pub start: i64,
    pub start_music: f64,
}

impl Default for Tempo {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            beats_per_bar: 4,
            beat_unit: 4,
            start: 0,
            start_music: 0.0,
        }
    }
}

/// A COM pointer moved to another thread. VST3 allows processing on
/// another thread than setup; the plug-in is responsible for the rest.
pub struct SendPtr<T>(pub T);
// SAFETY: see above.
unsafe impl<T> Send for SendPtr<T> {}

pub struct Plugin {
    // Field order is drop order: the plug-in's objects go before its module.
    pub controller: Option<ComPtr<IEditController>>,
    component: ComPtr<IComponent>,
    separate_controller: bool,
    pub edits: Arc<Mutex<Vec<Edit>>>,
    pub latency: u32,
    pub params: Vec<ParamInfo>,
    /// Takes notes and has no audio input: it makes sound rather than
    /// processing it.
    pub instrument: bool,
    /// The main input's and output's speakers, by name.
    pub main_in: Vec<String>,
    pub main_out: Vec<String>,
    _handler: ComWrapper<Handler>,
    _host: ComWrapper<HostApp>,
    _module: Arc<Module>,
}

/// The processing half, owned by the audio thread.
pub struct Audio {
    processor: ComPtr<IAudioProcessor>,
    component: ComPtr<IComponent>,
    rate: f64,
    /// Channels per input and output bus; bus 0 is the main one.
    ins: Vec<usize>,
    in_bufs: Vec<Vec<Vec<f32>>>,
    out_bufs: Vec<Vec<Vec<f32>>>,
    time: i64,
    /// Takes note events.
    events: bool,
    /// Tempo and beats per bar, told to the plug-in with every block.
    tempo: Tempo,
    /// Control changes from the plug-in's window, for the next block.
    pub from_editor: Arc<Mutex<Vec<(u32, f64)>>>,
}

fn stream(data: Vec<u8>) -> ComWrapper<MemStream> {
    ComWrapper::new(MemStream::with(data))
}

fn as_unknown<T: vst3::Class>(w: &ComWrapper<T>) -> *mut FUnknown
where
    T::Interfaces: vst3::com_scrape_types::InterfaceList,
{
    w.as_com_ref::<FUnknown>()
        .map_or(ptr::null_mut(), |r| r.as_ptr())
}

fn channels(arr: SpeakerArrangement) -> usize {
    arr.count_ones() as usize
}

impl Plugin {
    /// Loads `cid` from the bundle at `path`, ready to process at `rate`.
    pub fn load(
        path: &Path,
        cid: &str,
        rate: f64,
        state: Option<&str>,
        main: Option<u64>,
    ) -> Result<(Plugin, Audio), String> {
        crate::stage("loading its file");
        let module = Arc::new(Module::open(path)?);
        Plugin::load_from(module, cid, rate, state, main)
    }

    /// Loads `cid` from a module already open. Several plug-ins of one
    /// bundle share its module: opening it again and closing that copy
    /// would shut the plug-in's code down under the others.
    pub fn load_from(
        module: Arc<Module>,
        cid: &str,
        rate: f64,
        state: Option<&str>,
        main: Option<u64>,
    ) -> Result<(Plugin, Audio), String> {
        let want = parse_cid(cid).ok_or("Bad plug-in id.")?;
        let host = ComWrapper::new(HostApp);
        let edits = Arc::new(Mutex::new(Vec::new()));
        let handler = ComWrapper::new(Handler {
            edits: edits.clone(),
        });
        unsafe {
            crate::stage("creating it");
            let mut obj: *mut c_void = ptr::null_mut();
            let r = module.factory().createInstance(
                want.as_ptr() as *const _,
                IComponent::IID.as_ptr() as *const _,
                &mut obj,
            );
            let component = (r == kResultOk)
                .then(|| ComPtr::<IComponent>::from_raw(obj as *mut IComponent))
                .flatten()
                .ok_or("The plug-in could not be created.")?;
            crate::stage("starting it");
            if component.initialize(as_unknown(&host)) != kResultOk {
                return Err("The plug-in would not start.".into());
            }

            crate::stage("starting its controls");
            // The controller is either the same object or a second class.
            let mut separate = false;
            let controller = match component.cast::<IEditController>() {
                Some(c) => Some(c),
                None => {
                    let mut cid: TUID = [0; 16];
                    let mut c = None;
                    if component.getControllerClassId(&mut cid) == kResultOk {
                        let mut obj: *mut c_void = ptr::null_mut();
                        if module.factory().createInstance(
                            cid.as_ptr(),
                            IEditController::IID.as_ptr() as *const _,
                            &mut obj,
                        ) == kResultOk
                        {
                            c = ComPtr::<IEditController>::from_raw(obj as *mut IEditController);
                            if let Some(ctl) = &c {
                                ctl.initialize(as_unknown(&host));
                                separate = true;
                            }
                        }
                    }
                    c
                }
            };
            if let (true, Some(ctl)) = (separate, &controller) {
                // The two halves talk to each other through connection points.
                if let (Some(a), Some(b)) = (
                    component.cast::<IConnectionPoint>(),
                    ctl.cast::<IConnectionPoint>(),
                ) {
                    a.connect(b.as_ptr());
                    b.connect(a.as_ptr());
                }
            }

            crate::stage("handing over its settings");
            // Saved settings, or the component's own state handed to the controller.
            let (comp_state, ctl_state) = match state.and_then(split_state) {
                Some((c, e)) => (Some(c), Some(e)),
                None => (None, None),
            };
            if let Some(c) = &comp_state {
                let s = stream(c.clone());
                component.setState(stream_ptr(&s));
            }
            if let Some(ctl) = &controller {
                let comp = match comp_state {
                    Some(c) => c,
                    None => {
                        let s = stream(Vec::new());
                        component.getState(stream_ptr(&s));
                        let d = s.data.borrow().clone();
                        d
                    }
                };
                if separate {
                    let s = stream(comp);
                    ctl.setComponentState(stream_ptr(&s));
                    if let Some(e) = ctl_state.filter(|e| !e.is_empty()) {
                        let s = stream(e);
                        ctl.setState(stream_ptr(&s));
                    }
                }
                if let Some(h) = handler.as_com_ref::<IComponentHandler>() {
                    ctl.setComponentHandler(h.as_ptr());
                }
            }

            let processor = component
                .cast::<IAudioProcessor>()
                .ok_or("This plug-in does not process audio.")?;
            crate::stage("setting up its inputs and outputs");
            let (ins, outs, events, main_in, main_out) = setup_buses(&component, &processor, main);
            let instrument = events && ins.is_empty();
            let mut setup = ProcessSetup {
                processMode: kRealtime as i32,
                symbolicSampleSize: kSample32 as i32,
                maxSamplesPerBlock: MAX_BLOCK as i32,
                sampleRate: rate,
            };
            crate::stage("setting up its audio");
            processor.setupProcessing(&mut setup);
            crate::stage("switching it on");
            component.setActive(1);
            processor.setProcessing(1);
            let latency = processor.getLatencySamples();
            crate::stage("reading its controls");

            let params = controller
                .as_ref()
                .map(|c| read_params(c))
                .unwrap_or_default();
            let audio = Audio {
                processor,
                component: component.clone(),
                rate,
                in_bufs: ins.iter().map(|n| vec![vec![0.0; MAX_BLOCK]; *n]).collect(),
                out_bufs: outs
                    .iter()
                    .map(|n| vec![vec![0.0; MAX_BLOCK]; *n])
                    .collect(),
                ins,
                time: 0,
                events,
                tempo: Tempo::default(),
                from_editor: Arc::new(Mutex::new(Vec::new())),
            };
            Ok((
                Plugin {
                    controller,
                    component,
                    separate_controller: separate,
                    edits,
                    latency,
                    params,
                    instrument,
                    main_in,
                    main_out,
                    _handler: handler,
                    _host: host,
                    _module: module,
                },
                audio,
            ))
        }
    }

    /// Shows a control value on the plug-in's controller (its window and
    /// readouts); the audio side gets it with the next block.
    pub fn set_param(&self, id: u32, value: f64) {
        if let Some(c) = &self.controller {
            unsafe {
                c.setParamNormalized(id, value);
            }
        }
    }

    /// The plug-in's settings: component state and controller state.
    pub fn state(&self) -> String {
        unsafe {
            let s = stream(Vec::new());
            self.component.getState(stream_ptr(&s));
            let comp = s.data.borrow().clone();
            let mut ctl = Vec::new();
            if let (true, Some(c)) = (self.separate_controller, &self.controller) {
                let s = stream(Vec::new());
                c.getState(stream_ptr(&s));
                ctl = s.data.borrow().clone();
            }
            let mut out = Vec::with_capacity(comp.len() + ctl.len() + 4);
            out.extend_from_slice(&(comp.len() as u32).to_le_bytes());
            out.extend_from_slice(&comp);
            out.extend_from_slice(&ctl);
            base64_encode(&out)
        }
    }

    /// Replaces the plug-in's settings with saved ones (see [`Plugin::state`]).
    pub fn set_state(&self, data: &str) -> Result<(), String> {
        let (comp, ctl) = split_state(data).ok_or("Those settings could not be read.")?;
        unsafe {
            let s = stream(comp.clone());
            if self.component.setState(stream_ptr(&s)) != kResultOk {
                return Err("The plug-in did not take those settings.".into());
            }
            if let (true, Some(c)) = (self.separate_controller, &self.controller) {
                let s = stream(comp);
                c.setComponentState(stream_ptr(&s));
                if !ctl.is_empty() {
                    let s = stream(ctl);
                    c.setState(stream_ptr(&s));
                }
            }
        }
        Ok(())
    }

    /// The plug-in's look-ahead now (it may change with its settings).
    pub fn latency_now(&self) -> u32 {
        self.component
            .cast::<IAudioProcessor>()
            .map_or(self.latency, |p| unsafe { p.getLatencySamples() })
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        unsafe {
            if let Some(c) = self.controller.take() {
                if self.separate_controller {
                    c.terminate();
                }
            }
            self.component.setActive(0);
            self.component.terminate();
        }
    }
}

fn stream_ptr(s: &ComWrapper<MemStream>) -> *mut IBStream {
    s.as_com_ref::<IBStream>()
        .map_or(ptr::null_mut(), |r| r.as_ptr())
}

/// Splits saved settings back into component and controller state.
fn split_state(data: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let raw = base64_decode(data)?;
    let n = u32::from_le_bytes(raw.get(..4)?.try_into().ok()?) as usize;
    let comp = raw.get(4..4 + n)?.to_vec();
    let ctl = raw.get(4 + n..)?.to_vec();
    Some((comp, ctl))
}

/// Asks for stereo on the main buses (or, with `main`, the plug-in's own
/// layout or the one given), keeps the plug-in's own layout on any others
/// (sidechain inputs and the like), and turns them all on, along with the
/// first note input. Returns the channels of each input and output bus,
/// whether there is a note input, and the main buses' speakers by name.
#[allow(clippy::type_complexity)]
unsafe fn setup_buses(
    component: &ComPtr<IComponent>,
    processor: &ComPtr<IAudioProcessor>,
    main: Option<u64>,
) -> (Vec<usize>, Vec<usize>, bool, Vec<String>, Vec<String>) {
    let events = component.getBusCount(kEvent as i32, kInput as i32) > 0;
    if events {
        component.activateBus(kEvent as i32, kInput as i32, 0, 1);
    }
    let nin = component.getBusCount(kAudio as i32, kInput as i32).max(0);
    let nout = component.getBusCount(kAudio as i32, kOutput as i32).max(0);
    let current = |dir: i32, i: i32| {
        let mut a: SpeakerArrangement = 0;
        processor.getBusArrangement(dir, i, &mut a);
        a
    };
    let first = |dir: i32| match main {
        None => SpeakerArr::kStereo,
        Some(0) => current(dir, 0),
        Some(a) => a,
    };
    let mut ins: Vec<SpeakerArrangement> = (0..nin)
        .map(|i| {
            if i == 0 {
                first(kInput as i32)
            } else {
                current(kInput as i32, i)
            }
        })
        .collect();
    let mut outs: Vec<SpeakerArrangement> = (0..nout)
        .map(|i| {
            if i == 0 {
                first(kOutput as i32)
            } else {
                current(kOutput as i32, i)
            }
        })
        .collect();
    processor.setBusArrangements(ins.as_mut_ptr(), nin, outs.as_mut_ptr(), nout);
    for i in 0..nin {
        component.activateBus(kAudio as i32, kInput as i32, i, 1);
    }
    for i in 0..nout {
        component.activateBus(kAudio as i32, kOutput as i32, i, 1);
    }
    (
        (0..nin)
            .map(|i| channels(current(kInput as i32, i)))
            .collect(),
        (0..nout)
            .map(|i| channels(current(kOutput as i32, i)))
            .collect(),
        events,
        if nin > 0 { speaker_names(current(kInput as i32, 0)) } else { Vec::new() },
        if nout > 0 { speaker_names(current(kOutput as i32, 0)) } else { Vec::new() },
    )
}

/// The speakers of a VST3 arrangement by name, in channel order, named as
/// the editor names them (L, R, C, LFE, Ls, Rs, Lss, Rss, Ltf...). Speakers
/// with no name of their own are numbered.
pub fn speaker_names(arr: SpeakerArrangement) -> Vec<String> {
    // Bit order is channel order.
    const NAMES: [&str; 32] = [
        "L", "R", "C", "LFE", "Ls", "Rs", "Lc", "Rc", "Cs", "Lss", "Rss", "Tc", "Ltf", "Ctf",
        "Rtf", "Ltr", "Ctr", "Rtr", "LFE2", "M", "", "", "", "", "", "", "", "", "", "", "", "",
    ];
    let mono = arr == SpeakerArr::kMono;
    let mut out = Vec::new();
    for (bit, name) in NAMES.iter().enumerate() {
        if arr & (1u64 << bit) != 0 {
            out.push(if mono {
                "M".to_string()
            } else if name.is_empty() {
                format!("{}", out.len() + 1)
            } else {
                (*name).to_string()
            });
        }
    }
    // Rarer speakers (above the named ones) count as numbered channels.
    for _ in out.len()..arr.count_ones() as usize {
        out.push(format!("{}", out.len() + 1));
    }
    out
}

/// A note as VST3 wants it, `offset` frames into the block: the note,
/// and for a note with its own pan, the pan right after it.
fn vst_events(n: &NoteEvent, offset: u32) -> impl Iterator<Item = Event> {
    let note = vst_event(n, offset);
    let pan = (n.on && n.pan != 0.0).then(|| Event {
        busIndex: 0,
        sampleOffset: offset as i32,
        ppqPosition: 0.0,
        flags: 0,
        r#type: kNoteExpressionValueEvent as u16,
        __field0: Event__type0 {
            noteExpressionValue: NoteExpressionValueEvent {
                typeId: kPanTypeID,
                noteId: note_id(n),
                // 0 is left, 0.5 the center, 1 right.
                value: f64::from(n.pan.clamp(-1.0, 1.0)) * 0.5 + 0.5,
            },
        },
    });
    std::iter::once(note).chain(pan)
}

/// One id for each pitch on each channel, so a note's end and its pan
/// find the note.
fn note_id(n: &NoteEvent) -> i32 {
    i32::from(n.channel.min(15)) * 128 + i32::from(n.pitch.min(127))
}

fn vst_event(n: &NoteEvent, offset: u32) -> Event {
    let pitch = i16::from(n.pitch.min(127));
    let channel = i16::from(n.channel.min(15));
    Event {
        busIndex: 0,
        sampleOffset: offset as i32,
        ppqPosition: 0.0,
        flags: 0,
        r#type: (if n.on { kNoteOnEvent } else { kNoteOffEvent }) as u16,
        __field0: if n.on {
            Event__type0 {
                noteOn: NoteOnEvent {
                    channel,
                    pitch,
                    tuning: 0.0,
                    velocity: n.velocity.clamp(0.0, 1.0),
                    length: 0,
                    noteId: note_id(n),
                },
            }
        } else {
            Event__type0 {
                noteOff: NoteOffEvent {
                    channel,
                    pitch,
                    velocity: n.velocity.clamp(0.0, 1.0),
                    noteId: note_id(n),
                    tuning: 0.0,
                },
            }
        },
    }
}

unsafe fn read_params(c: &ComPtr<IEditController>) -> Vec<ParamInfo> {
    let n = c.getParameterCount().max(0);
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let mut info: ParameterInfo = std::mem::zeroed();
        if c.getParameterInfo(i, &mut info) != kResultOk {
            continue;
        }
        let f = info.flags;
        out.push(ParamInfo {
            id: info.id,
            title: from_wstring(&info.title),
            units: from_wstring(&info.units),
            steps: info.stepCount,
            default: info.defaultNormalizedValue,
            hidden: f & (kIsReadOnly | kIsHidden | kIsProgramChange) as i32 != 0,
        });
    }
    out
}

impl Audio {
    /// Processes `channels` interleaved channels in place (the main
    /// buses' channels, in order), applying `changes` first and playing
    /// `notes` at their offsets.
    /// `position` is the timeline frame of the first sample, which the
    /// plug-in is told as the song position; below 0 it keeps counting on.
    pub fn process(
        &mut self,
        position: i64,
        changes: &[(u32, f64)],
        notes: &[NoteEvent],
        io: &mut [f32],
        channels: usize,
    ) {
        if position >= 0 {
            self.time = position;
        }
        let ch = channels.max(1);
        let mut first = true;
        for (i, chunk) in io.chunks_mut(MAX_BLOCK * ch).enumerate() {
            let mut all: Vec<(u32, f64)> = if first { changes.to_vec() } else { Vec::new() };
            if let Ok(mut e) = self.from_editor.lock() {
                all.append(&mut e);
            }
            first = false;
            let from = (i * MAX_BLOCK) as u32;
            let to = from + (chunk.len() / ch) as u32;
            let events: Vec<Event> = notes
                .iter()
                .filter(|n| self.events && n.offset >= from && n.offset < to)
                .flat_map(|n| vst_events(n, n.offset - from))
                .collect();
            self.block(&all, events, chunk, ch);
        }
    }

    /// The tempo the plug-in is told (for synced delays, arpeggios and the like).
    pub fn set_tempo(&mut self, tempo: Tempo) {
        self.tempo = Tempo {
            bpm: tempo.bpm.clamp(1.0, 999.0),
            beats_per_bar: tempo.beats_per_bar.clamp(1, 32),
            beat_unit: tempo.beat_unit.clamp(1, 32),
            ..tempo
        };
    }

    fn block(&mut self, changes: &[(u32, f64)], events: Vec<Event>, io: &mut [f32], width: usize) {
        let n = io.len() / width;
        if n == 0 {
            return;
        }
        // Main input: channel for channel; stereo into a mono input as the average.
        for (b, bus) in self.in_bufs.iter_mut().enumerate() {
            for (c, ch) in bus.iter_mut().enumerate() {
                let ch = &mut ch[..n];
                if b != 0 || c >= width {
                    ch.fill(0.0);
                    continue;
                }
                let mono = self.ins[0] == 1 && width == 2;
                for (i, s) in ch.iter_mut().enumerate() {
                    let f = &io[i * width..i * width + width];
                    *s = if mono { (f[0] + f[1]) * 0.5 } else { f[c] };
                }
            }
        }
        for bus in &mut self.out_bufs {
            for ch in bus.iter_mut() {
                ch[..n].fill(0.0);
            }
        }
        let mut in_ptrs: Vec<Vec<*mut f32>> = self
            .in_bufs
            .iter_mut()
            .map(|b| b.iter_mut().map(|c| c.as_mut_ptr()).collect())
            .collect();
        let mut out_ptrs: Vec<Vec<*mut f32>> = self
            .out_bufs
            .iter_mut()
            .map(|b| b.iter_mut().map(|c| c.as_mut_ptr()).collect())
            .collect();
        let bus = |p: &mut Vec<*mut f32>| AudioBusBuffers {
            numChannels: p.len() as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: if p.is_empty() {
                    ptr::null_mut()
                } else {
                    p.as_mut_ptr()
                },
            },
        };
        let mut ins: Vec<AudioBusBuffers> = in_ptrs.iter_mut().map(bus).collect();
        let mut outs: Vec<AudioBusBuffers> = out_ptrs.iter_mut().map(bus).collect();
        let pc = ComWrapper::new(ParamChanges::from_changes(changes));
        let ev = ComWrapper::new(EventList {
            events: events.into(),
        });
        let mut ctx: ProcessContext = unsafe { std::mem::zeroed() };
        ctx.state =
            (kPlaying | kTempoValid | kTimeSigValid | kProjectTimeMusicValid | kBarPositionValid)
                as u32;
        ctx.sampleRate = self.rate;
        ctx.projectTimeSamples = self.time;
        ctx.continousTimeSamples = self.time;
        let t = self.tempo;
        ctx.tempo = t.bpm;
        ctx.projectTimeMusic =
            t.start_music + (self.time - t.start) as f64 / self.rate * t.bpm / 60.0;
        // Tempo stretches start on a bar, so bars count from there.
        let bar = f64::from(t.beats_per_bar) * 4.0 / f64::from(t.beat_unit);
        ctx.barPositionMusic = t.start_music
            + ((ctx.projectTimeMusic - t.start_music) / bar)
                .floor()
                .max(0.0)
                * bar;
        ctx.timeSigNumerator = t.beats_per_bar as i32;
        ctx.timeSigDenominator = t.beat_unit as i32;
        let mut data = ProcessData {
            processMode: kRealtime as i32,
            symbolicSampleSize: kSample32 as i32,
            numSamples: n as i32,
            numInputs: ins.len() as i32,
            numOutputs: outs.len() as i32,
            inputs: if ins.is_empty() {
                ptr::null_mut()
            } else {
                ins.as_mut_ptr()
            },
            outputs: if outs.is_empty() {
                ptr::null_mut()
            } else {
                outs.as_mut_ptr()
            },
            inputParameterChanges: pc
                .as_com_ref::<IParameterChanges>()
                .map_or(ptr::null_mut(), |r| r.as_ptr()),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: if self.events {
                ev.as_com_ref::<IEventList>()
                    .map_or(ptr::null_mut(), |r| r.as_ptr())
            } else {
                ptr::null_mut()
            },
            outputEvents: ptr::null_mut(),
            processContext: &mut ctx,
        };
        let ok = unsafe { self.processor.process(&mut data) } == kResultOk;
        self.time += n as i64;
        // Some plug-ins report an error yet fill their outputs: keep what
        // they wrote. Only an error with nothing written passes the sound
        // through untouched.
        let wrote = || {
            self.out_bufs
                .first()
                .is_some_and(|b| b.iter().any(|c| c[..n].iter().any(|v| *v != 0.0)))
        };
        let Some(main) = self.out_bufs.first().filter(|_| ok || wrote()) else {
            return;
        };
        // A mono output fills both sides of a stereo stream; otherwise
        // channel for channel, and channels it has no output for keep what came in.
        for i in 0..n {
            let f = &mut io[i * width..i * width + width];
            if main.len() == 1 && width == 2 {
                f[0] = main[0][i];
                f[1] = main[0][i];
                continue;
            }
            for (c, v) in f.iter_mut().enumerate().take(main.len()) {
                *v = main[c][i];
            }
        }
    }

    /// Hands waiting control changes (from the editor and the plug-in's
    /// window) to the processing side without any audio, so they count
    /// even before playback starts, for example in saved settings.
    pub fn flush(&mut self, changes: &[(u32, f64)]) {
        let mut all = changes.to_vec();
        if let Ok(mut e) = self.from_editor.lock() {
            all.append(&mut e);
        }
        if all.is_empty() {
            return;
        }
        let pc = ComWrapper::new(ParamChanges::from_changes(&all));
        let mut data = ProcessData {
            processMode: kRealtime as i32,
            symbolicSampleSize: kSample32 as i32,
            numSamples: 0,
            numInputs: 0,
            numOutputs: 0,
            inputs: ptr::null_mut(),
            outputs: ptr::null_mut(),
            inputParameterChanges: pc
                .as_com_ref::<IParameterChanges>()
                .map_or(ptr::null_mut(), |r| r.as_ptr()),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: ptr::null_mut(),
        };
        unsafe {
            self.processor.process(&mut data);
        }
    }

    /// Clears the plug-in's memory, as after a jump.
    pub fn reset(&mut self) {
        unsafe {
            self.processor.setProcessing(0);
            self.component.setActive(0);
            self.component.setActive(1);
            self.processor.setProcessing(1);
        }
        self.time = 0;
    }

    pub fn latency(&self) -> u32 {
        unsafe { self.processor.getLatencySamples() }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        unsafe {
            self.processor.setProcessing(0);
        }
    }
}

/// True when the result means yes.
pub fn yes(r: i32) -> bool {
    r == kResultOk || r == kResultTrue
}
