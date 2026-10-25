// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! One running VST3 plug-in, split the way VST3 wants it: [`Plugin`]
//! (setup, controls, settings, window) lives on the host's main thread,
//! and [`Audio`] (the processing) on its audio thread.

use std::ffi::c_void;
use std::path::Path;
use std::ptr;
use std::sync::{Arc, Mutex};

use vst3::Steinberg::Vst::BusDirections_::{kInput, kOutput};
use vst3::Steinberg::Vst::Event_::EventTypes_::{kNoteOffEvent, kNoteOnEvent};
use vst3::Steinberg::Vst::MediaTypes_::{kAudio, kEvent};
use vst3::Steinberg::Vst::ParameterInfo_::ParameterFlags_::{
    kIsHidden, kIsProgramChange, kIsReadOnly,
};
use vst3::Steinberg::Vst::ProcessContext_::StatesAndFlags_::{
    kPlaying, kTempoValid, kTimeSigValid,
};
use vst3::Steinberg::Vst::ProcessModes_::kRealtime;
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;
use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, Event, Event__type0, IAudioProcessor,
    IAudioProcessorTrait, IComponent, IComponentHandler, IComponentTrait, IConnectionPoint,
    IConnectionPointTrait, IEditController, IEditControllerTrait, IEventList, IParameterChanges,
    NoteOffEvent, NoteOnEvent, ParameterInfo, ProcessContext, ProcessData, ProcessSetup,
    SpeakerArr, SpeakerArrangement,
};
use vst3::Steinberg::{
    kResultOk, kResultTrue, FUnknown, IBStream, IPluginBaseTrait, IPluginFactoryTrait, TUID,
};
use vst3::{ComPtr, ComWrapper, Interface};

use crate::com::{from_wstring, Edit, EventList, Handler, HostApp, MemStream, ParamChanges};
use crate::module::{parse_cid, Module};
use crate::protocol::{base64_decode, base64_encode, NoteEvent, ParamInfo, MAX_BLOCK};

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
    tempo: (f64, i32),
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
    ) -> Result<(Plugin, Audio), String> {
        crate::stage("loading its file");
        let module = Arc::new(Module::open(path)?);
        Plugin::load_from(module, cid, rate, state)
    }

    /// Loads `cid` from a module already open. Several plug-ins of one
    /// bundle share its module: opening it again and closing that copy
    /// would shut the plug-in's code down under the others.
    pub fn load_from(
        module: Arc<Module>,
        cid: &str,
        rate: f64,
        state: Option<&str>,
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
            let (ins, outs, events) = setup_buses(&component, &processor);
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
                tempo: (120.0, 4),
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

/// Asks for stereo on the main buses, keeps the plug-in's own layout on
/// any others (sidechain inputs and the like), and turns them all on,
/// along with the first note input. Returns the channels of each input
/// and output bus, and whether there is a note input.
unsafe fn setup_buses(
    component: &ComPtr<IComponent>,
    processor: &ComPtr<IAudioProcessor>,
) -> (Vec<usize>, Vec<usize>, bool) {
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
    let mut ins: Vec<SpeakerArrangement> = (0..nin)
        .map(|i| {
            if i == 0 {
                SpeakerArr::kStereo
            } else {
                current(kInput as i32, i)
            }
        })
        .collect();
    let mut outs: Vec<SpeakerArrangement> = (0..nout)
        .map(|i| {
            if i == 0 {
                SpeakerArr::kStereo
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
    )
}

/// A note as VST3 wants it, `offset` frames into the block.
fn vst_event(n: &NoteEvent, offset: u32) -> Event {
    let pitch = i16::from(n.pitch.min(127));
    Event {
        busIndex: 0,
        sampleOffset: offset as i32,
        ppqPosition: 0.0,
        flags: 0,
        r#type: (if n.on { kNoteOnEvent } else { kNoteOffEvent }) as u16,
        __field0: if n.on {
            Event__type0 {
                noteOn: NoteOnEvent {
                    channel: 0,
                    pitch,
                    tuning: 0.0,
                    velocity: n.velocity.clamp(0.0, 1.0),
                    length: 0,
                    noteId: i32::from(pitch),
                },
            }
        } else {
            Event__type0 {
                noteOff: NoteOffEvent {
                    channel: 0,
                    pitch,
                    velocity: n.velocity.clamp(0.0, 1.0),
                    noteId: i32::from(pitch),
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
    /// Processes interleaved stereo in place, applying `changes` first
    /// and playing `notes` at their offsets.
    /// `position` is the timeline frame of the first sample, which the
    /// plug-in is told as the song position; below 0 it keeps counting on.
    pub fn process(
        &mut self,
        position: i64,
        changes: &[(u32, f64)],
        notes: &[NoteEvent],
        io: &mut [f32],
    ) {
        if position >= 0 {
            self.time = position;
        }
        let mut first = true;
        for (i, chunk) in io.chunks_mut(MAX_BLOCK * 2).enumerate() {
            let mut all: Vec<(u32, f64)> = if first { changes.to_vec() } else { Vec::new() };
            if let Ok(mut e) = self.from_editor.lock() {
                all.append(&mut e);
            }
            first = false;
            let from = (i * MAX_BLOCK) as u32;
            let to = from + (chunk.len() / 2) as u32;
            let events: Vec<Event> = notes
                .iter()
                .filter(|n| self.events && n.offset >= from && n.offset < to)
                .map(|n| vst_event(n, n.offset - from))
                .collect();
            self.block(&all, events, chunk);
        }
    }

    /// The tempo the plug-in is told (for synced delays, arpeggios and the like).
    pub fn set_tempo(&mut self, bpm: f64, beats_per_bar: i32) {
        self.tempo = (bpm.clamp(1.0, 999.0), beats_per_bar.clamp(1, 32));
    }

    fn block(&mut self, changes: &[(u32, f64)], events: Vec<Event>, io: &mut [f32]) {
        let n = io.len() / 2;
        if n == 0 {
            return;
        }
        // Main input: stereo as it is, mono as the average.
        for (b, bus) in self.in_bufs.iter_mut().enumerate() {
            for (c, ch) in bus.iter_mut().enumerate() {
                let ch = &mut ch[..n];
                if b != 0 || c >= 2 {
                    ch.fill(0.0);
                    continue;
                }
                let mono = self.ins[0] == 1;
                for (i, s) in ch.iter_mut().enumerate() {
                    let (l, r) = (io[i * 2], io[i * 2 + 1]);
                    *s = if mono {
                        (l + r) * 0.5
                    } else if c == 0 {
                        l
                    } else {
                        r
                    };
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
        ctx.state = (kPlaying | kTempoValid | kTimeSigValid) as u32;
        ctx.sampleRate = self.rate;
        ctx.projectTimeSamples = self.time;
        ctx.continousTimeSamples = self.time;
        ctx.tempo = self.tempo.0;
        ctx.projectTimeMusic = self.time as f64 / self.rate * self.tempo.0 / 60.0;
        ctx.timeSigNumerator = self.tempo.1;
        ctx.timeSigDenominator = 4;
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
        for i in 0..n {
            let (l, r) = match main.len() {
                0 => (io[i * 2], io[i * 2 + 1]),
                1 => (main[0][i], main[0][i]),
                _ => (main[0][i], main[1][i]),
            };
            io[i * 2] = l;
            io[i * 2 + 1] = r;
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
