# Backlog

Planned work, roughly in the order it will be picked up. Entries say what is built and what is not; the table under "Roadmap audit" is the current status. Controls for unbuilt features show as disabled with the reason.

## Multitrack: right-click menu on an empty track (built)

Right-clicking empty space in a lane opens the session menu, with submenus: Ripple Delete, Select, Groups, Track, Insert, Marker, Mixdown Session to New File, Export Mixdown, Bounce to New Track, plus Paste and Session Properties. Every entry runs the same action as the menu bar. Entries for features that don't exist yet show disabled, with the reason as a tooltip.

Still disabled, waiting on their features:

- Add 5.1 Audio Track and Add 5.1 Bus Track (surround is not planned).
- Add Mono, Stereo and 5.1 Bus Track (bus tracks arrive with effects, P5).
- Add Video Track (video is not planned).
- Add Tracks (a dialog for adding several tracks at once).
- Insert > Silence (needs the Generate tools).
- Marker > Add Chapter Marker, Add Ad Slot, Add Social Clip (podcast mode).
- Select > Clips of Audio Type (clips have no audio type yet).

Shortcuts seen in the reference for these actions are not adopted. Every new shortcut needs approval before it is added.

## Transport bar (check what already exists first)

Loop playback and skip selection already exist as play modes. Next to Record:

- Loop playback
- Skip selection (play through the session but skip the selected range)

## Zoom buttons (below the transport)

- Zoom in amplitude
- Zoom out amplitude
- Zoom in time
- Zoom out time
- Zoom out full (all axes)
- Zoom in at in point
- Zoom in at out point
- Zoom to selection
- Zoom to time
- Zoom to selected track

## On-device AI (everything runs locally)

Audio never leaves the computer. Models download once, then work offline. No account, no cloud service, no telemetry.

Suggested order, easiest and safest first:

1. **Auto-captions.** Speech to text on the user's own machine, with word timestamps. Output as a caption track and as SRT, VTT and plain-text transcript files. Works on audio now, and on video once video exists. Candidate engine: a Whisper-family model through a Rust or C++ runtime (MIT-licensed code, several model sizes so slow machines can pick a small one).
2. **Background noise and music removal.** Two tiers. Noise suppression (hiss, hum, room tone) uses small models that can ship inside the app. Music and background removal (voice separation) uses a larger model that downloads on first use. Always non-destructive: runs as a new clip or a render, with a before and after preview.
3. **Text to speech.** Type text, get a voice track on the timeline. Candidate: a small open voice engine with a short list of bundled-license voices.
4. **Voice cloning from a few seconds of audio.** Last, because of the license and misuse questions below.

### Model downloads and the privacy rule

The app has zero network calls today, and the CI privacy guard enforces that. Downloading a model breaks that rule once, so it needs an explicit exception:

- Off by default. The only network use is a button the user clicks ("Download model, 460 MB").
- One fixed host, nothing else, and the file's checksum is verified after download.
- A manual route for fully offline machines: download the model file elsewhere and point the app at it.
- The privacy guard is updated to allow exactly that one code path and nothing else. This is a decision to approve before any of it is built.

### Open questions

- **Model licenses.** Code licensed GPL-compatible is not enough: each model's weights carry their own license, and several well-known voice-cloning models are non-commercial only. Every model gets its license checked before it is offered, and listed in `ATTRIBUTION.md`.
- **Voice cloning safety.** Consent prompt before cloning ("I have the right to use this voice"), a visible label on generated audio, and no hidden watermark removal. This is a policy decision and goes in `docs/decisions.md`.
- **Speed and size.** CPU first, GPU later. Large models make a big download and slow renders on older laptops, so the model picker shows size and expected speed.
- **Video.** Captions on a video waveform need a video track, which isn't built yet. Captions and transcripts for audio come first.

## Carried over

- Batch Convert: tags for M4A output.
- AAC encoder priming trim; HE-AAC.
- `.aup3` import (parked; see below).
- Automatic latency measurement.
- Automatic release signing.
- Effects, VST3, automation.

## QuickPass follow-ups

- Scan types beyond silence: clicks and pops, clipping.
- Presets (Trim Long Silent Pauses, Mark Non-Silent Areas) with the P5 presets.
- Keep a minimum pause (shorten long silences instead of removing them whole).

## Community requests (October 2026)

Collected from the project's Discord group. Each entry says what we will build, where it fits, and the catch to settle before building. Nothing here is started. Order of work is at the end.

### 1. Scripting, schemas and an AI-assistant endpoint (schemas built; scripting and endpoint not started)

- **Scripting.** User macros and batch transforms in a sandboxed script engine. Plan: JavaScript run by an engine embedded in the Rust core (QuickJS through the `rquickjs` crate), with no file, network or process access and only a small, versioned API that calls the same actions the menus use. Every script edit goes through the undo stack as one step. Lua is the alternative; JavaScript wins because the interface and users already know it.
- **Schemas and types.** Schemas for every file format stay in `/schemas`, generated from the Rust types so they cannot drift. Add generated TypeScript definitions for the script API and the file formats. We will not publish an OpenAPI spec: there is no HTTP API, and adding one would break the no-network rule. Our real file names are `.altaudio` (session), `.altemplate`, `.alworkspace`, `.alhost`, `.alkeys`, `.alprefs` and `library.json`; the request's `.altaudio.session` style names are not used.
- **MCP server.** A small companion program speaks the Model Context Protocol over stdio and talks to the running editor through a local pipe, never the network. Off by default, switched on in Preferences with a plain explanation. Starts read-only (inspect the session); editing and export need a second, explicit switch. Every change an assistant makes is labelled in History and can be undone.
- **Phase:** after the effects engine settles the data model (P5). Schemas and generated types can start sooner.
- **Catch:** a script or assistant that can export or delete is a safety surface. Default read-only, per-action permissions, no write access to files outside the session folder.

### 2. Time stretch (shrink and stretch): built 2026-10-17 as Clip > Time Stretch; drag-to-stretch waits on the shortcut decision

- Stretch a clip by dragging its edge to fit an exact length (promo and ad slots), non-destructive, with pitch kept. Range about 25 percent either way. Also a tape-style speed change that moves pitch with speed.
- **How:** clips gain a `stretch` field (a session schema step with a migration, so old files still open). Preview uses a fast stretch; export and a cached render use the high-quality one. Candidate engines: Rubber Band (GPL, compatible with ours, but a C++ dependency and licence note in `ATTRIBUTION.md`) or a Rust phase-vocoder and WSOLA of our own. Decide with listening tests on speech.
- **Shortcut:** the request says Alt+Drag. New shortcuts need project lead approval, so this is listed, not assigned. Until approved, stretch is a Clip menu command with a length field and a tool-strip mode.
- **Phase:** P3 or P4 editing. High value for ad work. Early candidate.

### 3. Plugin hosting (built 2026-10-24, see decisions.md)

- **VST3** hosting on tracks and the master, then optionally **CLAP** (an open, simpler format). The VST3 SDK is MIT licensed, so hosting inside a GPL app is clean.
- **VST2 is not planned.** Steinberg stopped licensing the VST2 SDK, so shipping a VST2 host is a legal risk for a public project. We can revisit if the terms change. This is a deliberate decision, logged in `decisions.md`.
- **Crash isolation:** plugins run in a separate helper process and exchange audio through shared memory, so a crashing plugin shows an error on its slot and the session carries on. This is the largest piece of the work and sets the schedule.
- **Plugin windows:** open each plugin's own window beside the editor. Embedding it inside the main window is fragile on every platform and is not promised.
- **Not the `nih-plug` crate:** that is for writing plugins. Hosting needs the VST3 interfaces directly (for example the `vst3` crate) or our own bindings.
- **Phase:** P5, with Realtime Export (some live plugins misbehave when a mixdown renders faster than playback).

### 4. Low-latency audio on Windows (ASIO) and discrete channels: ASIO built for 0.1.5 (needs hardware testing); routing matrix, buffer size and latency controls still to do

- Add ASIO next to the current Windows audio paths, through the audio library we already use. Check the ASIO SDK licence terms before shipping a build that includes it; do not assume they fit a GPL release.
- Per-channel input and output routing so each track can take its own channel from a multi-microphone interface, plus loopback channels for system audio.
- **Phase:** P4 recording. Needs real hardware to test. Latency compensation and punch-in already sit in that phase.

### 5. MIDI and virtual instruments (tracks, piano roll and VST3 instruments proposed 2026-10-24; controllers not started)

- A MIDI track type with clips, playback and a piano roll, virtual instruments on those tracks (depends on plugin hosting), and external controller mapping to transport, faders, pan and trigger pads.
- **Phase:** its own phase after Podcast mode, as already planned. The soundboard-trigger use (pads that fire clips) is small and could ship earlier without the piano roll.
- **Catch:** the piano roll and instrument hosting are a second product. Controller mapping for transport and faders is cheap and could come first.

### 6. Markers, playlist and video (markers, Playlist and chapter export built; video and ADR not started)

- **Markers:** point, range, chapter and cue markers with labels, colors and notes in both editors. Most exist already. Add colors, notes and the Waveform editor.
- **Playlist panel:** sequence marked ranges, preview the arrangement without changing the clips, and export each segment as its own file.
- **Chapter export:** a timestamp list, CSV and podcast chapters JSON are built (2026-10-17). Still to do: chapters embedded in the file itself (ID3v2 in MP3, Vorbis comments in FLAC and Ogg).
- **Video track and ADR:** import video as a reference track in sync with the audio, a resizable preview, count-in beeps and a visual pre-roll for re-recording, loop regions, audio extraction on import, and writing the finished mix back into the original video file without re-encoding the picture.
- **Catch:** a web view's video element cannot step exact frames. Frame-accurate preview needs our own decoder path, probably FFmpeg (LGPL build, dynamically linked, licence noted). Writing audio back without touching the video is a stream copy, which is the easy half. Check which containers to support.
- **Phase:** markers, playlist and chapter export in P3 to P7 as listed. Video and ADR as their own phase, tied to Voiceover mode.

### 7. Podcast tools (loudness and silence handling built; ducking, alignment and bleed not started)

- **Auto-ducking:** one setting that lowers music tracks while chosen speech tracks are active. Fits the effects engine (P5). A simple version needs only volume envelopes and QuickPass's speech detection, so a first release can come before the full compressor-style version.
- **One-click loudness:** built 2026-10-17 as Effects > Loudness Match, with broadcast targets by region (US -24 by default).
- **Multi-mic alignment and bleed:** estimate each microphone's delay against the others and shift it to remove comb filtering. Delay estimation is well understood and feasible. Removing bleed itself (one voice leaking into another microphone) is harder and quality varies; ship alignment first, a gate-based bleed reduction second.
- **Silence and breath management without cutting:** built 2026-10-17 as QuickPass Turn Down (volume envelope dips, clips stay whole). Still to do: a breath detector that tells breaths from quiet speech, which needs spectral analysis.
- **Phase:** loudness and non-destructive silence now; ducking and alignment in P5.

### Order of work

1. Fix what is broken (waveforms, Levels panel) and ship v0.1.1.
2. Non-destructive silence and breath handling in QuickPass; one-click loudness targets (both built 2026-10-17).
3. Time stretch (built 2026-10-17).
4. Chapter export, marker colors and notes, Playlist panel.
5. Effects engine (P5) with plugin hosting, ducking and mic alignment.
6. Recording hardware work: ASIO and per-channel routing.
7. Scripting, then the assistant endpoint.
8. Video and ADR, MIDI.

## Built 2026-10-17 (second batch)

AAC (M4A) export on Windows, Alt+drag stretch, Generate Tone and Noise, Insert Silence, punch-in recording with pre-roll, recording latency offset, marker colors and notes.

Still open from editing and recording: Paste to New File and Mix Paste checks, Clips of Audio Type, the Playlist panel, input monitoring through the speakers, automatic latency measurement, ASIO and per-channel routing, AAC on macOS (Audio Toolbox) and Linux (bundled encoder).

## Built 2026-10-18

Silence over a time selection, audio types and Select Clips of Audio Type, Add Tracks, ad slot and social clip markers, chapters embedded in MP3 and FLAC, the Playlist panel.

Next in editing and recording: markers in the Waveform editor (saved as WAV cue points), input monitoring through the speakers, automatic latency measurement, ASIO and per-channel routing. Formats: Batch Convert metadata (copy tags across, set title and artist), AAC encoder-delay trim, `.aup3` import, AAC on macOS and Linux, HE-AAC (needs a bundled encoder).

## Built 2026-10-18 (second batch)

Markers in the Waveform editor (saved in the WAV as cue points, moved by cuts, pastes and crops, shown and edited in the Markers panel), input monitoring through the speakers, Batch Convert tags (copy each file's tags and WAV markers, set title, artist, album, year, genre and comment with name tokens).

Next: the effects phase (P5). Still open: ASIO and per-channel routing (needs the project lead's interface to test and an SDK licence check), automatic latency measurement, M4A tags, AAC on macOS and Linux, HE-AAC.

## Parked: `.aup3` import

Parked by the project lead on 2026-10-18 as a possible later feature. What it would take: an `.aup3` file is an SQLite database holding the project layout and the audio in compressed blocks. Import would read the track and clip layout and rebuild each clip's audio into WAV files beside the session, then open it like a `.sesx` import. Needs real sample projects to test against, an SQLite reader (a bundled build of the SQLite library, public domain), and our own parser written from the file's observable structure. Effects in the project would be skipped with a notice, as with `.sesx`.

## Effects (P5), first batch built 2026-10-18

Built: Voice Chain on clips and tracks, eight effects (Parametric EQ, Hum Cut, Dynamics, Limiter, Breath Gate, De-Esser, Speech Leveler, Gain), built-in and user presets, Effects menu entries, look-ahead compensation.

Next batches, in order:
1. Noise Reducer (learn a noise print from a selection, or an automatic speech model; licence check first for any model), Click Repair, hum detection.
2. Chain input and output meters; live slider changes without a restart.
3. Waveform editor: apply an effect or a chain to the selection or file (destructive, undoable), with preview.
4. Pitch and Time: pitch shift with formant keeping, speed change.
5. Master chain and buses (moves the master mix to the render thread), sends.
6. Speaker Balance, host profiles, Guest track mode, Audio FX Panel.
7. VST3 hosting in a helper process.

## Effects to build, renamed (planned 2026-10-18)

- Restoration, done: Noise Reducer, Noise Print Reducer, Sound Eraser, Hiss Reducer, Room Reducer, Click Repair, Clip Repair, DC Offset Fix, Phase Align (Clip menu).
- Filter and EQ, done: Band EQ (10, 20, 30), Curve EQ with a draggable graph, Notch Filter, Precision Filter (Butterworth, Chebyshev, Bessel, Elliptic), Center Remover, response graphs for every EQ and filter.
- Time and pitch: Pitch Shift done. Speed and Pitch is covered by Time Stretch plus Pitch Shift. Left: Pitch Curve (pitch over time), Pitch Tune (automatic correction to a scale), Pitch Draw (manual correction), formant keeping.
- Others asked for in batch presets: Center Remover (vocal removal) done, Fade In and Fade Out and Normalize done (Batch Convert level row), Telephone Voice.
- Music Fit: built 2026-10-18 (Clip > Fit Music to Length), with a choice of up to three plans and where each joins. Later: fit to the end of a time selection by dragging.

## QuickPass and Batch Convert layout (agreed 2026-10-18)

- Batch Convert (built 2026-10-18): file list with Name, Status and Stage; a processing preset picked per run (a Voice Chain preset or one effect) applied before export; an Export checkbox; Export Settings dialog with file naming (prefix and suffix, or a template), location (next to the source, or a folder), replace existing files, format list, sample type (rate, channels with left and right mix, bit depth with dither options) and format settings; remove finished files from the list; keep markers and metadata.
- QuickPass (built 2026-10-18): scan type list (Click Repair, Clip Repair, Silence, Mark Audio), presets with save and delete, Scan and Settings views, results table (fixed, start, duration, clip) with Fix, Fix All and Clear Fixed, and a footer count of problems found and fixed.


## Built 2026-10-18 (fourth batch)

Live chain edits (sliders apply while dragging, no playback restart unless an effect's look-ahead changes), chain meters in and out, Voice Chain preview and apply in the Waveform editor, Noise Print Reducer, Sound Eraser, Phase Align, Elliptic filters, response graphs with a draggable Curve EQ, Batch Convert normalize and fades, M4A tags, Music Fit plan choice.

Still open, and why:
- Pitch Tune, Pitch Curve, Pitch Draw and formant keeping. Pitch Tune needs a frame-by-frame pitch tracker driving a time-varying shift, and formant keeping needs a spectral envelope pass on top of the shifter. Each is a batch of its own, best done together since they share the envelope work.
- Master chain and buses. Today faders and the master sum run on the audio thread, so they react within one buffer. A master chain or a bus chain must run on the render thread, half a second ahead, so the mix has to move there too, and faders would then lag by that half second unless the fader stays on the audio thread after the chain. Needs a design call from the project lead before building.
- Clip chain meters (track chains are metered).
- AAC encoder-delay trim. Windows' encoder adds silence at the start; how much it adds and whether players skip it needs checking on Windows with a test file before the edit list is written.
- HE-AAC, AAC on macOS and Linux, release signing: see the notes in the session reply of 2026-10-18 (need a bundled encoder, Mac hardware, and the project lead's signing keys).

## Settings (built 2026-10-18)

Edit > Settings with every page; see decisions.md. Greyed-out items waiting on other work: channel routing and driver type (ASIO), buffer size, numbered session backups, Waveform edge crossfades, pan law (buses), spectrum detail and range, timecode frame rate, scrubbing, record countdown, weekly update check, a light theme, moving the cache folder.

## Built for 0.1.3 (2026-10-19)

Speaker Balance, host profiles with Guest track mode, session backups, record count-in, new-marker name, timecode frame rate. Next: the master chain and bus decision (see decisions.md), then Pitch Tune, Pitch Curve, Pitch Draw and formant keeping, then Waveform editor edge crossfades and scrubbing.

## Voice Chain effect windows (built for 0.1.4, first step)

Built: adding an effect, clicking its name or its gear button opens its controls in a floating, movable window inside the app (one at a time), with the response graph and Learn button, an On/Off button, and the track and slot number. Esc or the close button hides it. Still to do: several windows open at once, remembering where each window was, and separate system windows for a second monitor.

## Optional AI connections (idea, not approved)

Spitballed by the project lead: off by default, never required. Local first (a local language model, local text-to-speech, local speech-to-speech), and cloud services only with the user's own key. Needs a decision entry, a privacy review and a privacy-check change before any code. See docs/design-principles.md.

## From the 0.1.6 tester reports (not built yet)

- Master track in the multitrack view, and bus lanes in the Editor. Buses, sends, outputs, the master chain and sidechain keys are built (2026-10-24); buses live in the Mixer for now.
- A per-track Mono button (hear and meter a track summed to mono).
- Crossfade options for overlapping clips (how the fade weighs each clip) and more fade shapes with adjustable curve in the clip's fade handles.
- Clip Properties dialog.
- Wheel over the track headers: today it scrolls the tracks; a tester wanted it to zoom.
- A sound that turned strange after about five hours in a session (fixed by re-adding the file). Needs a way to reproduce.
- Default shortcuts Alt+Up and Alt+Down for Move to Track Above and Below, if approved.
- Pitch Shift needs a short how-to (its controls and what Semitones and Cents do).

## Next round, in order (set 2026-10-20)

1. **Crash when opening Settings > Devices (top priority, confirmed by two testers and the project lead, with and without an audio interface).** The app closes at once, no error. One tester's interface (a Focusrite Vocaster Two) made it worse: with it unplugged the crash stopped; plugged back in it crashed again on the second visit. Reproduce on the same page: Edit > Settings > Devices.
   - **What the page does when it opens:** three device lookups at the same time (outputs, inputs, and the list of installed ASIO drivers). The ASIO lookup asks every installed ASIO driver to load itself so it can report its channels. A buggy or busy driver (interface drivers are the usual suspects) can take the whole process down with a native crash that no error handler can catch, which matches "closes instantly". Release builds include ASIO since 0.1.5. The crash also happened on 0.1.6 for the project lead. The first tester is probably not on 0.1.3 (Settings does not open there) and may be on 0.1.4, which has no ASIO, so there are two suspects: the ASIO driver list (0.1.5 and later) and the plain system device lookup that every version runs when the page opens. If both versions crash, the plain lookup (or the page itself) is the common cause.
   - **Fix plan:** (a) list ASIO driver names from the Windows registry without loading any driver; load one only when the user picks it. (b) Run the three lookups one after another, not in parallel. (c) Crash guard: write a marker file before touching ASIO or a device list; if it is still there at the next start, skip that lookup, show a one-line note and offer Try Again. (d) Show devices as "loading" first so the page never waits on hardware. (e) Longer term, load ASIO in the same helper-process style planned for plugins.
   - **To confirm the cause we need:** the app version, and the Windows crash entry (Event Viewer > Windows Logs > Application, the "Application Error" line with the faulting module name), or the output of `npm run tauri dev` from a repro. A quick split test: `npm run tauri dev` builds without ASIO and prints errors in the terminal. If the page crashes there too, the cause is the plain lookup or the page; if only the release build crashes, it is ASIO.
   - **Interim option:** ship the next release with ASIO driver listing off until the fix lands.
1b. **macOS: recording gives a flat waveform (reported by a tester on the 0.1.6 Apple silicon build; fix prepared, untested).** macOS denies microphone access silently because the app has no microphone usage description in its Info.plist and its signature carries no audio-input entitlement, so the app never appears in System Settings > Privacy & Security > Microphone and every input records silence.
   - **Fix (config only, no code):** add `src-tauri/Info.plist` with `NSMicrophoneUsageDescription` (Tauri merges that file automatically), add `src-tauri/Entitlements.plist` with `com.apple.security.device.audio-input`, and point `bundle.macOS.entitlements` at it in `tauri.conf.json` (hardened runtime on). These three files are prepared in the repo copy. The first recording after install then triggers the standard permission prompt.
   - **Not from the suggested steps:** the config key is `infoPlist`, not `infoPlistPath`, and a stray key fails the config check, so a copied snippet would break the build. We also have no Apple Developer ID, so there is no Developer ID signing or notarization; the release build keeps its ad hoc signature (`APPLE_SIGNING_IDENTITY: "-"`), and the entitlement and usage string work with that. A manual `codesign` step is not needed because Tauri signs with the entitlements file when it is configured.
   - **Cons to know:** ad hoc signed apps may be asked for microphone permission again after an update, because macOS ties the grant to the signature. Users still open the app with right-click, Open the first time.
   - **Also add:** when a recording is silent from the first second, show a one-line note naming the likely cause ("No sound is arriving. On Mac, check System Settings > Privacy & Security > Microphone, and on Windows the microphone privacy setting"), instead of a flat line with no explanation.
   - **Verify:** the Apple silicon CI build must go green, then the tester retests. Check with `codesign -d --entitlements - Altered.app` on a Mac that the entitlement is present.
2. **Cache and Files: a default folder in Documents, easy to change (requested and revised 2026-10-20).** Today the waveform cache, effect results and unsaved recordings go to the app's own hidden folder, and Settings > Cache and Files has a disabled Move Cache button. Change: everything defaults to one visible folder, `Documents\Altered Audio Editor\`, created on first use, with subfolders `Cache`, `Recordings`, `Sessions` and `Templates`. Every location can be changed in Settings > Cache and Files with a Browse button, and a first-run note says where things go and how to change it. Saved sessions keep their Recordings folder beside the session file, as now.
   - **Settings page layout (from a tester example, our own wording):** Working files: primary folder and an optional second folder (used when the first is full). Waveform drawings: a checkbox to keep them next to the original audio files when possible, a folder for the rest, and a Clean button. Templates folder with Browse. A "Save waveform drawings" checkbox (on by default). Free space shown beside each folder. Moving a folder copies first, then switches over, with progress.
   - **Pros:** visible and easy to find and back up; one place to clean; people with a small system drive can point it at a bigger disk in two clicks; no setup screen to get through.
   - **Cons and mitigations:** (a) Windows often redirects Documents into OneDrive, and then gigabytes of cache and recordings would sync and files can lock mid-recording. Detect a Documents path inside OneDrive (or another sync folder) and warn on first run, offering a local folder instead. (b) Cache is disposable but Recordings are not: only the cache and working files get a Clean button, never Recordings. (c) A chosen folder that later goes missing (unplugged drive): fall back to the default with a one-line notice, never fail silently. (d) Network or removable drives get a dropout warning.
   - **Needs:** a folders section in the preferences file, the existing cache commands reading it instead of the app cache folder, a writable-folder check, a move command with progress, and the sync-folder detection.
3. **Live recording waveform: draw exactly what the finished file will look like (reported 2026-10-20, applies to the Waveform editor and the multitrack).** Two problems. (a) The Waveform editor draws nothing until Stop: it records under the key "wave", but `WaveformView.svelte` never reads the live-take store that the multitrack view uses. (b) Even in the multitrack, the live picture is a different drawing from the final one: live blocks are 1024 frames each and go through a separate drawing path, while the finished file uses the peak cache (32 or 64 frames per value at the finest level) and the normal drawing code. So the live waveform looks blockier and then visibly changes when the take is replaced after Stop. The rule from now on: **what you see while recording is the same data and the same drawing as the finished waveform, so nothing is swapped or redrawn at Stop.**
   - **Fix:** build the peak cache while recording instead of a separate live block list. Each time frames are written, append their min and max at the finest level (32 frames) and update the coarser levels, using the same rounding and scaling as the peak builder. Both editors then draw the growing take through the existing peak drawing path, so there is one renderer and one data source. Coarser levels are exact min and max combinations of finer ones, so the 64-frame level used for long files is derived from the 32-frame data with no loss. At Stop the cache is finalized and saved as the file's normal cache; no rebuild.
   - **Waveform editor:** draw the take from the end of the file (or the punch point), grow and scroll the view with it, then keep the same view at Stop.
   - **Things that could make it differ, to check:** device rate against file rate (record at the device rate and do not resample), recording offset and latency compensation (apply the same shift to the live drawing as to the saved clip, or the picture jumps when it is applied), input gain or monitoring effects (draw the signal that is written to the file, not the monitored one), and the meters and time readout during a Waveform recording.
   - **Tests:** a Rust test that the incrementally built levels are byte-identical to levels built from the finished file; a check that a recorded take draws the same pixels just before and just after Stop.
   - Small to medium. The data path exists; the work is replacing the live block format with incremental peak levels.
3b. **Two reports on long jobs (2026-10-20).**
   - **Apply to Whole File in the Waveform editor breaks if you switch files while it runs, with an error in the status bar.** The result comes back for the file the job started on, but the editor applies it to whichever file is open by then. **Fix:** tag every long job with the file it started on and apply the result to that file's own document (open or parked), never to "the current file"; or lock file switching with a clear "Applying, please wait" until it ends (cheap stopgap). Better: run Apply as a background task with progress in the status bar and a Cancel, so several files can be processed one after another. Applying the same Voice Chain to many files at once already exists in Batch Convert (processing presets); say so in the error text for now.
   - **Exporting a multitrack session locks the whole app until it finishes, and a second copy of the app offers to recover "the unsaved session" that the first copy is exporting.** Two causes. (a) The export already runs on a worker thread over a copy of the session, but the interface shows a full-window progress overlay (the `exporting` state in `App.svelte`) that blocks everything. **Fix:** show export progress in the status bar with a Cancel and let the user keep working; edits made during an export do not change the export because it uses its own snapshot. (b) Crash recovery lists every autosave in the shared folder, so a live second copy looks like a crashed one. **Fix:** each running copy writes a small lock file with its process id into the autosave folder, and recovery skips autosaves whose lock belongs to a running process. Alternative: allow only one running copy and focus the open window when a second is started. **Pros of the lock file:** two windows can work on two projects. **Cons:** more code, and stale locks need a process check. Recommended: the lock file, since the single-window route would block the "second copy" workflow the tester used as a workaround.
3c. **Two more reports (2026-10-21).**
   - **Wheel over the track headers.** The mouse wheel over the track header column should scroll the tracks up and down, and Ctrl+wheel over a header should make that track taller or shorter (all selected tracks if several are selected). A taller header shows its full controls: the whole input name instead of a cut-off one ("Mono: Microphone (2- Yet..." today), the output selector (see "Multitrack headers"), and the meter. Today the headers stay cramped with no way to open them up by wheel. Open question to confirm: whether Ctrl+wheel should resize one track or all tracks. Proposed: one track, and all tracks with Ctrl+Shift+wheel. The Ctrl+wheel and Ctrl+Shift+wheel combinations need shortcut approval before they ship.
   - **An "Untitled" session still shows up.** To pin down before fixing: it is either (a) a leftover autosave offered by crash recovery after the empty-launch change, (b) an unsaved blank session still listed in the Files panel or the window title, or (c) a session created on launch even though the setting is empty. Ask for where it appears. Likely fix: no session is created until the user starts one; recovery lists only real unsaved work, and a blank autosave is deleted on close.
3d. **Clip selection is hard to see and hard to do in bulk (2026-10-21).** Selecting clips in the multitrack view does not show a clear selection outline, so it is not obvious what a cut, delete or move will act on. Wanted: (a) a visible outline and tint on every selected clip, (b) mass select: drag a box across lanes (box select of clips is listed as built, so first check why it is not showing or working), Ctrl+click to add or remove one clip, Shift+click for a range on one track, and Select All Clips on a Track from the track menu, (c) a count of selected clips in the status bar, (d) cut, delete and ripple delete acting on the whole selection as one undoable step. To confirm before fixing: whether the outline is missing for clicked clips, for box-selected clips, or both. Fix order: outline first (small), then check box select, then the bulk actions.
3e. **Work without a multitrack session, and close it (2026-10-21).** The app should not need a multitrack session open. A person who only edits single files should be able to work in the Waveform editor alone, and File > Close should close the session rather than start a fresh blank one. Proposed: with nothing open the Editor shows a start screen (Open an audio file, New Session, Record, recent files); Close Session returns to it; the multitrack view appears only when a session exists. The session already can be missing in the interface (many actions say "Open or create a session first"), but the core always holds one, so a core command to close it and a start screen are needed. To confirm with the project lead: whether this means a start screen, or being able to hide the Multitrack tab while keeping a session open.
3f. **Bug round 5 (2026-10-22).**
   - **Done:** after a recording on an ASIO driver, playback said "No audio output device found" and Settings showed no devices. ASIO drivers open for one program at a time, so a second lookup found nothing. Playback and previews now fall back to the system output when the chosen driver has none, and Settings and the track input menu keep the last known device lists (with a note) instead of going empty. Unverified on hardware; to test: pick ASIO, record, then play, then open Settings > Audio Devices.
   - **Done:** mouse wheel. Over the lanes it scrolls the tracks; over the ruler it zooms the timeline; over a track header it makes that track taller or shorter (Shift: all tracks). Ctrl + wheel zooms the timeline, Alt + wheel scales waveforms, Shift + wheel scrolls sideways. The old "wheel over the timeline zooms" setting was removed.
   - **Done:** start screen. A session with nothing in it shows a quiet "No session is open in the Multitrack editor." screen, the same as the Waveform editor's empty screen (changed 2026-10-27 from one with New Session, Open, Import Files and Use an Empty Session buttons), and hides the six empty tracks and the Files panel row, instead of an "Untitled Session". Remaining for backlog 3e: File > Close to close a session instead of starting a blank one.
   - **Open: real channel names for ASIO inputs and outputs** (for example "Host Microphone L", "Loopback 1 L" from the interface driver). The system driver already shows friendly names; ASIO shows "Mono, channel 1". Needs the driver's channel info call, which the audio library does not expose, so it needs a small extension and a real interface to test. Names should appear in Settings and in each track's input menu, falling back to numbers when the driver gives none.
   - **Open: Channel Routing and Audio Hardware settings pages** with the controls a tester sketched: a table mapping file channels to device channels for inputs and outputs, driver class, default input and output, clock, latency, sample rate and a driver control panel button. The page is greyed out today; it needs ASIO channel info (above) and hardware to test.
   - **Unclear, to confirm: "Apply to Whole File across files should also be tracks".** Likely wanting Apply to work on selected tracks or clips in the multitrack, not only on the open file in the Waveform editor. Confirm what is wanted before building.
## Built in 0.1.9 (cloud-checked, not yet tried on hardware)

3g shortcuts and Space rule (still open: zoom keys Shift+/, Shift+Backslash, Ctrl+Backslash, vertical zoom, and every disabled-menu item), 3h ruler drag pans, 3i middle-button pan, 3l fade handles, 3m waveform contrast, 3n ruler steps and 4-hour zoom cap, 3o clip-drag auto-scroll, 3p Alt+wheel track height (Alt+Shift+wheel is amplitude), 3u panel tabs and menu (Stacked and Solo disabled), 3v QuickPass Fix by and gaps stay open (still open: sortable Start column, counts line, collapse handle, preset icons), 3w Editor first, 3x panels fit. 3j Track Visibility, 3k clip envelope menu, 3q Voice Chain rows (built 2026-10-22, see notes on each). Not started: 3r, 3s, 3t, 3y, 3z.

3g. **Keyboard shortcuts, all of them, and the Space bar (requested 2026-10-22, for the next round).**
   - **Assign every approved shortcut** from the table in `CLAUDE.md`, including the ones whose features are not built. Built features get working actions. Unbuilt ones get a disabled menu item with its reason and the shortcut shown, so the chord is reserved and visible. The keymap test requires every bound action to exist in the menus, so each one needs a menu entry.
   - **Can be wired to working actions now:** Home and End (go to start and end), Ctrl+Left and Right (previous or next marker, clip edge or selection edge), Ctrl+Alt+Left and Right (markers only), M and * (add marker), `=` and `-` (zoom in and out horizontally, alongside the existing Ctrl+= and Ctrl+-), 1, 0 and 9 (switch between the Multitrack and Waveform editors), Alt+= and Alt+- (zoom in and out vertically, which the Alt+wheel waveform scaling already does), the zoom reset and zoom out full keys (Backslash, Shift+Backslash), Up and Down (switch channel in the Waveform editor), and nudge (Alt+comma, Alt+period, Alt+Up, Alt+Down) once a clip nudge action exists.
   - **Unbuilt, so disabled with a reason:** Shift+X (return to start on stop), Shift+D and Shift+U (spectral display and HUD), Ctrl+Shift+H (hide effect windows), Alt+I (meter input), Shift+R and Ctrl+R (repeat last command), Shift+T (convert sample type), Shift+P (capture noise print), Ctrl+Shift+click (mute, solo, record or monitor on all tracks), and Ctrl or Shift drag on dials for fine and coarse steps.
   - **Space bar rules (project lead):** Space plays the multitrack, or the open file in the Waveform editor when that is what is showing. While recording or playing, Space stops it (recording stops and places the take; playing stops). With nothing playing, Space plays the multitrack. This replaces the Pause or Stop choice in Settings > General: the setting goes away. Today Space pauses or stops playback per that setting and does not stop a recording.
   - **Checks:** the keymap tests (every bound action exists, no duplicate chords), plain-key shortcuts must not fire while typing in a text field (the dispatcher already skips inputs), and every menu item shows its shortcut.
3h. **Dragging on the ruler should slide the timeline, not select (2026-10-22).** Today a click and drag on the time ruler (the numbers above the tracks) starts a time selection and highlights the tracks (`onRulerPointer` calls `startRange`). Wanted: dragging left or right on the ruler pans the timeline smoothly, with no selection and no track highlight; a plain click with no movement still places the playhead; dragging a marker flag on the ruler still moves that marker. Same rule in the Waveform editor ruler. Selecting a time range stays on dragging in the lanes (Time Selection tool). Small change: replace the range start with a pan drag that adjusts the view scroll as the pointer moves, with a 3 pixel movement threshold before it counts as a drag.
3i. **Middle-button drag pans the view (2026-10-22).** Pressing and holding the middle mouse button (the wheel click) and dragging anywhere in the Multitrack view slides the view left and right with the mouse, for as long as the button is held; the pointer shows a hand (grabbing) while it does. Vertical movement scrolls the tracks up and down at the same time. Same in the Waveform editor (horizontal only). It must not select, move clips or place the playhead, and it works over lanes, ruler and empty space; middle click over a track header does nothing. Also stop the browser's autoscroll circle from opening on a middle press. Small: a pointer handler on the editor scroller for `button === 1` that updates the view scroll from pointer movement, with `cursor: grabbing` while active. Pairs with 3h (ruler drag pans).
3j. Multitrack tab and panel menu, plus Track Visibility. Right-click on an editor tab or panel header opens a menu: Close Panel, Undock Panel, Close Other Panels in Group, a Panel Group submenu (close group, undock group, maximize group, stacked group, solo panels in stack, small tabs), then Close, Close All and the list of open files. A Track Visibility submenu: apply visibility to the mixer (toggle); Load and Save presets 1 to 9 (Shift+1 to Shift+5 for the first five); Hide Selected Track (Alt+H); Show All Tracks (Alt+Shift+A); Show Only Audio Tracks; Show Only Bus Tracks (disabled until bus tracks exist); Show Only Tracks with Clips; with clips within the time selection; with clips at the playhead. Notes: tabs have no close button by design and panel toggles live in the Window menu; hiding tracks needs a `hidden` field on tracks (schema plus migration) and the mixer must honor it. Shortcuts need approval before assignment. Built 2026-10-22: `hidden` track field (optional, left out when off, schema updated, no migration needed), Multitrack > Track Visibility and the lanes' right-click submenu (Hide Selected Track, Show All, Only Tracks with Clips, within Time Selection, at Playhead, as one undoable step). Hidden tracks leave the Editor and Mixer lists but still play. Still open: Show Only Audio Tracks does nothing until bus tracks exist, Show Only Bus Tracks and presets 1 to 9 are disabled, Alt+H, Alt+Shift+A and Shift+1 to 5 await approval, and the multitrack tab's Close, Close All and open-files list.

3k. Clip envelope menu. The small dropdown arrow beside the Pan label in a clip's header (the envelope selector) opens a menu: Add/Remove Effects, a divider, then Rack Input, Rack Output, Rack Mix and Rack Power. Name and wire them to our own Voice Chain: Add/Remove Effects opens the rack picker, and the four rack items choose which rack parameter the clip envelope shows (input gain, output gain, wet/dry mix, bypass/power). Check what the clip envelope selector offers today and extend it. Needs rack parameters exposed as automatable lanes; items without a backing feature show disabled with a reason. Partly built 2026-10-22: the menu now has Add/Remove Effects (opens Voice Chain) and Chain Input Gain, Chain Output Gain, Chain Mix and Chain Power shown disabled with a reason. Still open: chain-level envelope targets in the Rust engine.

3l. Fade handles: move them out of the clip's title bar to just below it, inside the waveform body (top-left for fade in, top-right for fade out), and make them larger and higher contrast (bigger square, light fill with dark outline, hover highlight, easy to grab). Today they sit in the title bar (see `LaneCanvas.svelte` fade drawing and its hit test, which must move with them). Keep dragging behavior as is. Same treatment in the Waveform editor if it has fade handles. Not built.

3m. Waveform contrast in clips: draw the waveform much brighter than the clip background so it stands out (lighter, more saturated waveform color against a darker clip body; check selected, muted and locked clips too, and both themes). Keep per-clip colors working by deriving the waveform color from the clip color (lighten) and the body from a darkened version. Same in the Waveform editor. Candidate for a Settings toggle only if people disagree. Not built.

3n. Finer ruler and a 4 hour timeline. The ruler picks labeled steps from a fixed list (`STEPS` in `src/panels/editor/timeline.ts`, 1 ms to 1 hour, at least 90 px apart), so zoomed out it lands on coarse 10 minute or hourly labels. Make it more granular at every zoom: labels in seconds, then minutes, then hours as the zoom changes, with finer minor ticks between labels (denser minor ticks, a smaller minimum label gap, extra steps such as 3, 4, 6, 20, 40 s and 3, 4, 6, 10, 20 min). Timeline extent: default maximum of 4 hours; it grows past that only when content genuinely runs longer (longest clip end or recording, plus padding). Check `MAX_SPP`, `fit` and the scroll range so zoom-out and scrolling honor the 4 hour cap. Same ruler in the Waveform editor (shared `rulerTicks`). Add unit tests for ticks at several zoom levels. Not built.

3o. Auto-scroll while dragging a clip. When a clip is dragged near the left or right edge of the lanes, the timeline scrolls in that direction with the drag (speed rises as the pointer nears or passes the edge, stops at time 0 on the left, extends on the right), and the clip keeps following the pointer so it lands where it is dropped. Applies to moving multiple selected clips too, and ideally to trimming and fade-handle drags and marquee selection. Needs the drag to track pointer position on a timer, not only on pointer move events, so scrolling continues while the mouse is held still at the edge. Snap and the vertical track change must keep working during the scroll. Not built.

3p. Alt plus wheel over the lanes resizes track height. With the pointer over the tracks themselves, Alt+wheel makes the tracks taller or shorter (all tracks, or the track under the pointer; decide at the desk), and the lanes get a vertical scrollbar/scroll view when they no longer fit. It must not change the audio level or make waveforms look clipped (drawn peaking): taller tracks scale the waveform drawing only, never gain, and the drawing clamps to the lane so loud audio is not drawn past the track edge. Conflict to resolve: today Alt+wheel zooms the waveform amplitude (see the wheel rules in decisions), which is likely the "peaking" look. Options: (a) Alt+wheel over lanes = track height and amplitude zoom moves to Alt+Shift+wheel or the Waveform editor only; (b) keep amplitude zoom and add another modifier for height. Recommendation: (a), since taller tracks give the same visual benefit without distorting the picture. Ctrl+wheel stays time zoom. Update the wheel table in docs and CLAUDE.md. Not built.

3q. Voice Chain list layout. Each slot row reads: slot number, power toggle (a clear on/off icon, lit when on, dim when off) to the LEFT of the effect name, the name, then the settings (gear) button and the remove (X) button at the far right. Remove the up/down arrows and the color-coded squares. Reordering moves to drag and drop on the row (grip on hover), with a keyboard alternative (Alt+Up/Down) so nothing is lost. Empty slots show as dim numbered rows with a power icon, with a final "Add effect" row. Power state must be readable at a glance, including in the disabled state, without relying on color alone (icon fill plus text color). Clicking the name selects the slot; a slot menu (arrow at the row end or right-click) offers Edit Effect, Remove Effect, then the effect families (Amplitude and Compression, Delay and Echo, Filter and EQ, Modulation, Noise Reduction and Restoration, Reverb, Special, Stereo Imaging, Time and Pitch) as submenus; plugin-host entries (VST, VST3) show disabled with a reason until supported. Merges with the existing "Voice Chain slot menu by family" item. Applies to the Clip Effects and Track Effects tabs. Built 2026-10-22: power button left of the name, gear and remove at the right, no arrows or color badges, drag to reorder, Alt+Up/Down, right-click slot menu with Replace With families. Still open: dim numbered empty rows and the final Add effect row.

3r. Effect editor windows. (a) Dynamics: a transfer-curve editor: input dB on the horizontal axis, output dB on the vertical, a diagonal unity line, draggable points on a curve with a Spline Curves checkbox, add/remove point and reset buttons, a segment table (segment, ratio, type Compressor or Expander, threshold above/below dB), Make-Up Gain, and a live level meter beside the graph. Tabs: Dynamics and Settings (attack, release, detection, look-ahead as the engine allows; controls the engine cannot do show disabled with a reason). Footer shows the input/output channels, track name and slot number, and a power toggle. Needs a dynamics processor in the Rust core that takes a point list (check what the current Speech Leveler or compressor supports; may need a new effect). (b) Every effect editor gets a Presets dropdown at the top (built-in presets plus Save As and Delete of user presets, stored with app settings, never uploaded), with the same footer and power toggle. (d) Limiter editor as the pattern for simple effects: Presets dropdown (with a few built-ins such as Limit to -6 dB) with Save As and Delete, Peak or True Peak choice, sliders with tick scales and a blue value readout with units (Maximum Amplitude, Input Boost, Look-Ahead Time, Release Time), Link Channels checkbox, footer with In/Out channels, power toggle, track and slot. Built-in presets for every effect that has settings (Noise Reducer, Speech Leveler, Parametric EQ, De-Esser, Limiter, reverb and the rest), plus user presets; effects with nothing to preset skip the dropdown. Check which Limiter options the core supports (True Peak, look-ahead range) and show the rest disabled with a reason. Preset storage format: one JSON file per effect type in the app config folder. (c) All effect settings pop-ups open centered in the app window, and re-center on open, with a sensible minimum size. Describe the reference only as "the reference" in public files. Not built.

3s. New effects to build, found by studying the user's own rack chains and scenario presets (behavior only; no files, values or names copied). The character voices and scenario looks (distant, large room, outside, radio, television, intercom, walla) are built from a few effects we do not have yet. Build, in order of use: (1) Reverb (room size, decay, damping, pre-delay, dry/wet) with built-in presets such as small room, large room, hall, plate, outside; (2) Delay and Echo (time, feedback, tone, wet/dry; slapback and long echo presets); (3) Modulation (chorus, flanger, phaser); (4) Stereo Imaging (width, mid/side balance). Then add presets that use them: Godlike, Demonic, Evil Voice, Ethereal and Angelic gain a reverb or delay stage; add scenario presets named in our own words (Radio Speaker, TV Speaker, Intercom, Distant, Close Up, Large Room, Small Room, Outside, Crowd Murmur, Balanced High Tone, Balanced Low Tone). Each effect gets the editor layout from 3r. Original DSP only (GPL-3.0 code written here). Not built.

3t. Free fade bending. A fade today is one of three fixed shapes (linear, logarithmic, S curve), so a fade cannot be bent by hand. Add a drag handle on the fade curve itself (midpoint of the curve): drag up or down to bend the fade continuously, from fast-then-slow through straight to slow-then-fast, with the live curve drawn as you drag and the shaded silent side following it. Applies to fade in and fade out, on clips in the multitrack and in the Waveform editor if it has fades. Double-click the handle to reset to straight. The session already has a `curve` field on each fade (-1.0 to 1.0, 0.0 neutral, in the schema and model) that nothing reads yet: wire it through `fadeGain` in `LaneCanvas.svelte` and the matching Rust gain used by the engine, offline render and export, so what you see is what plays and bounces (one shared formula, with a test that the JS and Rust values match). The three shapes become presets that set `curve`; the shape in Settings stays the default for new fades. Keep undo as one step per drag. Not built.

3u. Simpler panel tab strips. Remove the float and maximize buttons from the right end of every panel group header; they do not suit panels like Files, Voice Chain, Batch Convert or QuickPass. The strip becomes just the tabs, plus (a) a small menu icon (three lines) on the currently active tab only, which opens the panel menu from 3j (Undock Panel, Maximize Panel Group, Close Panel and the rest, so floating and maximizing stay reachable without taking space), and (b) an overflow chevron (>>) at the far right when tabs do not fit, listing the hidden tabs to switch to. Active tab is marked clearly (brighter label, underline in the accent color). Check `src/app/docking` for the dockview header action components to remove or replace. The Usability charter in CLAUDE.md says every panel can float and maximize; that stays true through the menu, so update the wording to "from the panel menu". The reference screenshot shows the menu icon on the active tab; confirm at the desk that this is the "dots" meant. Confirmed from the reference: the three-line icon on the active tab and a right-click on any tab open the same menu. Items: Close Panel, Undock Panel, Close Other Panels in Group, then a Panel Group Settings submenu (Close Panel Group, Undock Panel Group, Maximize Panel Group, a divider, Stacked Panel Group, Solo Panels in Stack shown disabled until stacked groups exist, Small Tabs as a checkmark that shrinks tab height and padding). This is the same menu as 3j, so build it once and reuse it for the multitrack's tab. Stacked and Solo need a stacked-group layout the docking shell does not have yet; show them disabled with a reason. Not built.

3v. QuickPass fix choice, and no gap closing. Replace the Remove and Remove All verb buttons with a "Fix by" dropdown that appears for every scan type and lists that scan's fixes (silence: Strip Silence, Split Silence; clicks and clipping: Repair; noise: Turn Down; and so on, plus Mark where it applies), then Fix Selected and Fix All buttons that apply the chosen fix (confirm this reading at the desk). Strip Silence removes the silent audio but leaves the clips where they are, so every other track and the conversation stay aligned. Split Silence cuts the clip at the silences and leaves all pieces in place. Neither tightens the gaps by default: the "Close the gap on" option (this track only / all tracks) moves into an advanced setting named "Close gaps after fixing", off by default, with a warning that it can misalign tracks. Today the Remove action defaults to closing the gap on this track (`ripple` in `QuickPassPanel.svelte`, `removeOp`, `shiftFindings`); change the default to no ripple and update the tests. Kept pause setting ("Keep a pause of ms") stays. Further detail from the reference of the results list: the buttons are named after the chosen fix (Split, Split All, Clear Split; with Strip Silence they read Strip, Strip All, Clear Stripped), so the verb buttons stay and follow the Fix by choice (this replaces the Fix Selected and Fix All reading above). The first column carries the fix name and shows a check mark once an item is done; columns are Start (click the header to sort, arrow shows direction), Duration and Clip, with long clip names cut with an ellipsis. Time values show millisecond precision (0:12.604) with duration in seconds. Footer line: "519 problems found, 519 fixed" (our wording, counts update live), and a status line "Scan finished in 36.6 seconds" with the duration rounded to one decimal (never a raw float). A small handle between the scan bar and the list collapses the settings area to give the list more room. The preset row keeps Save and Delete icons beside the preset menu. How the two silence fixes look in the reference. Split Silence: the clip is cut at every silence and ALL pieces stay exactly where they were, speech pieces and silent pieces alike, as separate clips with outlines; nothing moves and nothing is deleted, so the edit is fully reversible by dragging pieces or deleting the silent ones yourself. Strip Silence: the silent pieces are removed and the gaps stay open; speech clips keep their original positions on every track, and the other tracks are untouched. Neither changes the length of the session. Each fix is one undo step, however many pieces it makes (hundreds on a long interview). A selected track in the reference showed its stripped gaps against a grey lane, which is just selection color. Not built.

3w. Tab order at the top of the center area: Editor first, then Mixer, in the Default layout, every built-in workspace and saved layouts (migrate old layouts that have Mixer first). Check `DEFAULT_LAYOUT` and the workspace definitions in `src/app/docking/registry.ts`. Not built.

3x. Left panels fit their space. Voice Chain, Batch Convert, QuickPass and the other tabbed panels must fit the column width with no horizontal scrollbar or overflow to the right; long text ellipsizes and columns shrink. Done in the cloud copy for QuickPass (columns now share the width), Voice Chain and Batch Convert (horizontal overflow hidden); check every panel at the 270 px default width and a narrower width at the desk, including Files, Libraries and Time Info.

3y. Clip right-click menu audit. The reference clip menu, in groups, in our own words, to compare with the current clip menu and fill gaps (anything we cannot do yet shows disabled with a reason): (1) Edit Source File; (2) Cut, Copy, Paste, Delete, Ripple Delete submenu, Trim submenu, Bring Clip to Front, Send Clip to Back (clip stacking order, needs a z-order idea in the data model, disabled until then); (3) Split, Split All Clips at Playhead (new shortcut candidate Ctrl+Alt+Shift+K, needs approval), Merge Clips, Make Unique Copy, Silence Selection in Selected Clips, Match Clip Loudness, Align Speech (ties to the multi-microphone alignment item); (4) Rename, Clip Gain (candidate Shift+G, needs approval), Clip Color, Lock in Time, Loop, Mute; (5) Reveal in Files Panel, Relink Media; (6) Select submenu, Groups submenu (clip groups: new idea, disabled), Stretch submenu, Remix submenu (disabled until built); (7) Track submenu; (8) Mixdown Session to New File and Export Mixdown submenus, Bounce to New Track submenu; (9) Clip Properties, Session Properties. Same menu on the clip's header arrow. Shortcut chords come only from the approved table. Not built.

3z. Libraries: your own groupings, and a proper empty state. Today a new show library always starts with seven podcast bins (Intro / Outro, Ads, Promos, Clips, Music Beds, Sound Effects, Legal Reads, from `STARTER_BINS` in `src/panels/libraries/bins.ts`), though users can already add, rename and fill bins with New Bin. Changes: (a) the New Library dialog offers a start choice: Podcast bins, Empty, or Copy bins from another library; Empty is a first-class option and the choice is remembered per user; (b) any bin, starter or custom, can be renamed, recolored, reordered by drag, and deleted (delete removes the group, never the audio files); (c) a library or bin with no items shows a centered empty state: a small icon, "This library is empty" (or "This bin is empty"), and the hint "Drag audio files here, or use + to add", with the + button doing the same as New Bin or Add Files as fits; (d) a way to save the current set of bins as a reusable set ("Save Bins as Starter Set") that appears in the New Library dialog, so a gaming or music show can have its own groupings; (e) keep Add Starter Bins in the library menu. Built-in starter set stays named for podcasts, since that is who the app is for. Check the library.json schema needs no change (collections already hold name, color, items). Not built.

4. Small and certain items from the table below: Help and About links, Invert and Reverse, QuickPass layout, Files panel New File, View menu fill-in, empty window on launch, Voice Chain slot menu by family, audio hardware settings that need no hardware.
5. **Shortcuts.** Assign the approved set in `CLAUDE.md` as each feature lands: Home and End, marker navigation, M for a marker, nudge clip, zoom keys and the rest. Wire the ones whose features already exist first (Home, End, M, zoom, Ctrl+Left and Right, nudge, Alt+Up and Down), then the others with their features.
6. Then the build order at the end of this audit (time-based effects, master and bus tracks, and so on).

## Roadmap audit (2026-10-20)

Checked against the code. "Built" means it is in the repo and tested in the sandbox; none of the audio work has been tried on real hardware yet.

| Area | Status | What is left |
| --- | --- | --- |
| Scripting, schemas, assistant endpoint | Schemas built (`/schemas`, with a test that keeps them current). Scripting and endpoint not started. | Generated TypeScript definitions. QuickJS script engine. Local stdio and pipe endpoint, off by default, read-only first. No OpenAPI (no HTTP API, no network). |
| Time stretch and varispeed | Built: Clip > Time Stretch, Alt+drag on a clip edge, pitch kept (WSOLA) or tape-style, about 25 percent either way, exact-length stretch in the Waveform editor (0.1.6). | Formant keeping, a higher quality engine (Rubber Band is GPL, needs a note in `ATTRIBUTION.md`, decide by listening tests on speech). |
| Plugin hosting (VST3) | Built (proposed 2026-10-24, see decisions.md). | Separate host process per plug-in (pipes, not shared memory), Plug-in Manager with user folders, plug-in windows on Windows, settings saved in the session. Still open: windows on Linux and macOS, macOS bundles, live input monitoring through plug-ins, plug-in sidechain inputs. VST2 stays out. |
| ASIO and channel routing | ASIO built (Cargo feature, on in release builds, falls back to the normal driver). Per-channel input picking built. | Hardware testing. Buffer size, sample rate and latency controls. Output routing and a routing matrix. Loopback channels. |
| MIDI and instruments | MIDI tracks, MIDI clips, a piano roll, session tempo and VST3 instruments built (proposed 2026-10-24, see decisions.md). | Free (non-snapping) stretch of MIDI clips when clip stretching reaches them (tester request: MIDI clips should get a free stretch, not only one that snaps), hearing notes while drawing them, recording from a MIDI keyboard, `.mid` import and export, tempo changes along the timeline, moving MIDI clips between tracks with Alt+Up and Alt+Down, copy and paste of MIDI clips, quantize, controllers and sustain pedal, and the controller mapping in the MIDI plan below. |
| Markers, Playlist, chapters | Built: point, range, chapter, cue, ad slot and social clip markers with colors and notes, Playlist panel, chapter export (files and embedded), Waveform editor cue points. | Clip markers that move with their clip (check), marker keyboard navigation (needs shortcut approval). |
| Video and ADR | Not started. | See the video plan below. |
| Podcast tools | One-click loudness built (Loudness Match, targets by region and custom). Silence and breath handling built non-destructively (QuickPass Turn Down, Breath Gate). Phase Align and Phase Rotate built. | Auto-ducking. Multi-mic alignment and bleed reduction. A smarter breath detector. |

### Plan and catches for the unbuilt parts

- **Scripting (QuickJS).** Sandboxed: no file, network or process access. The script API is a small versioned set of the same actions the menus run. A script run is one labelled, undoable step. Ships with generated `.d.ts` files so editors can complete the API. **Pros:** JavaScript is what the interface already speaks. **Cons:** a second language surface to keep stable. We version the API from day one.
- **Local assistant endpoint.** A companion program speaks the Model Context Protocol over stdio and reaches the editor through a local pipe. Never the network. Off by default; a second, explicit switch allows edits and exports. Every assistant edit is labelled in History and undoable. No write access outside the session folder.
- **VST3 hosting.** The SDK is MIT licensed, so hosting is clean. Plugins run in a separate helper process so a crashing plugin shows an error on its slot and the session carries on. Plugin windows open beside the editor rather than inside it. The `nih-plug` crate is for writing plugins, not hosting them. CLAP is a later option. VST2 is excluded on purpose: Steinberg no longer licenses the SDK, so a public project cannot ship it. This is the largest single piece of work.
- **MIDI.** First step is small and useful: controller mapping (transport, faders, pan, trigger pads that fire clips) with a MIDI-learn dialog. Piano roll and instrument hosting are a second product and wait on plugin hosting. Control surface protocols (the common mixing-console ones) are fiddly and cannot be tested without the hardware, so they come after generic MIDI-learn.
- **Video reference track and ADR.** Import video as a reference track, resizable preview, frame-accurate stepping, pre-roll beeps and visual cue, loop regions, audio extraction on import, and writing the finished mix back into the original container without re-encoding the picture. **Catch:** a web view cannot step exact frames, so we need our own decoder path (an LGPL FFmpeg build, dynamically linked, credited in `ATTRIBUTION.md`). Writing audio back is a stream copy and is the easy half.
- **Auto-ducking.** One setting: music tracks lower while chosen speech tracks are active, with attack, release and depth. A first version can use the speech detection QuickPass already has and write volume envelopes (visible, editable, undoable). The live sidechain is built (2026-10-24: Dynamics or Breath Gate on the music track, Listen to the voice track); the one-setting dialog is still to do.
- **Multi-mic alignment and bleed.** Estimate each microphone's delay against the others and shift it (Phase Align already shifts by a fixed amount). Bleed removal quality varies; ship alignment first, gate-based bleed reduction second.

## Menus, settings and behavior build-out (2026-10-20)

Feature checks and requests from testers, grouped by area, with status. Behavior only: names, layout and visuals stay our own.

### Help menu and About
- Keep our Help menu. Add Join our Discord, the project on GitHub, and Report a Problem opening the GitHub issues page. Remove Show Log Files. About gets links to the website, GitHub and Discord, and a credits list (libraries and contributors from `ATTRIBUTION.md`). Liven it up a little, never at the editor's expense.
- The new links go on the allow list in `src-tauri/src/commands/links.rs`. Not built.

### Opening the app
- A clean empty window on launch, not a multitrack session with six tracks. Approved 2026-10-20: the default is an empty window. A setting can pick last session or new session instead. Not built.

### Voice Chain: slot menu and effect families
- Clicking an empty slot opens a menu: Edit Effect, Remove Effect, then a submenu per family, then Plug-ins (VST3 when it exists). Our families today are Amplitude, Dynamics, Filter and EQ, Restoration, Time and Pitch. Target families: Amplitude and Compression, Delay and Echo, Filter and EQ, Modulation, Noise Reduction and Restoration, Reverb, Special, Stereo Imagery, Time and Pitch. Not built; the effects already exist and only need regrouping.
- Mapping to what we have. Amplitude and Compression: Gain, Dynamics, Limiter, Speech Leveler, De-Esser built; Channel Mixer, multiband compressor to do. Filter and EQ: Parametric EQ, Band EQ (10, 20, 30), Curve EQ, Precision Filter, Notch Filter, Hum Cut built. Noise Reduction and Restoration: Noise Reducer, Noise Print Reducer, Hiss Reducer, Room Reducer, Click Repair, Clip Repair, DC Offset Fix, Sound Eraser built. Stereo Imagery: Center Remover and Phase Rotate built; a stereo expander and a frequency-dependent phase shifter to do. Time and Pitch: Pitch Shift built; automatic pitch correction to do (Pitch Tune).
- Missing families, all to be written fresh with our own names: Delay and Echo (delay, echo, analog-style delay), Modulation (chorus, flanger, phaser), Reverb (algorithmic plus convolution with a loaded impulse response file), Special (distortion, a vocal enhancer, a mastering chain, a loudness meter effect).
- Starter presets: a list of about forty named chains was shown. Podcast and voice-over ones exist in part; creative ones (telephone, radio, hospital radio, underwater, spooky echo, stereo widening and so on) are to be rebuilt with our own settings and names. Not built.
- Effect windows: a Preview button, draggable graphs (straight or spline), and Hide All Effect Windows. Partly built (graphs for Curve EQ, effect windows built 0.1.4).

### Batch Convert output settings
- The panel needs the file name template or prefix and postfix, location with Same as source and Overwrite, format, sample type with Change, format settings with Change, and the three checkboxes (remove files from the panel on completion, close files, include markers and metadata). Mostly built in 0.1.2 to 0.1.3; check each one against this list.
- Formats that should open and export: WAV (incl. BWF and RF64), AIFF, FLAC, Ogg, MP3, MP2, AAC, plus Opus. We read and write most of these today; the gaps are RF64 and BWF handling, MP2 export, and the long tail of libsndfile-style formats. Monkey's Audio (APE) and Windows Media formats are not planned: APE has no maintained Rust reader or writer, and the Windows Media codecs are proprietary and Windows only. Say so in the format menu rather than hide it.

### QuickPass layout
- The panel is cramped in the narrow left column and scrolls sideways. Rework it: no horizontal scroll, controls wrap, Find Levels sits right under the silence and sound definitions, and the Fix by block follows. Fix by options: Strip Silence (remove and close the gap) and Split Silence (cut into clips, keep the gap), plus our non-destructive Turn Down. Not built; this is a layout bug.
- Parameter parity for the repair scans: declip (gain with auto, tolerance, minimum clip size, cubic or FFT fill, FFT size) and declick (threshold, complexity). Our Clip Repair and Click Repair cover the job; their settings pages need the same controls. Mark Audio is built.

### Multitrack headers
- **Output selector** (the MIX box): None, Mix, Bus (add a mono or stereo bus), hardware outputs by channel, Audio Hardware. Missing. It depends on the master track and bus decision below.
- **Input default** should read None, not a device, until the user picks one (check).
- **Automation mode** (Off, Read, Write, Latch, Touch): the box shows Read and the others are greyed out. Read plays automation today; the other modes need track-level automation. Make the Read item behave as a visible state, not a dead control, until then.
- **Master track and buses:** the open design call stays open: the chain lags the faders by half a second (recommended), or chains run before their own fader. Needed by the output selector and by pre-fader and post-fader effect placement.

### Audio hardware settings
- Device class: ASIO, MME, WASAPI (ours: ASIO and the default driver). Per class: default input and output, clock source, latency in ms (30 to 500) for MME, I/O buffer size and sample rate for ASIO, exclusive mode and latency for WASAPI, Attempt to force hardware to document sample rate, Use machine-specific device defaults. A Settings button opens the driver's own control panel (ASIO) or the operating system's sound settings (WASAPI). Built: driver pick and device lists. Not built: everything else here. Needs hardware testing; the ASIO control panel call needs checking in `asio-sys`.

### Menus to fill in
- **File:** New Audio File also in the Files panel's new menu (it is in the File menu already). Import and export of application settings as one file (local only).
- **View:** Show HUD, Show Editor Panel Controls, Show Clip Gain Controls, Hide All Effect Windows, a fuller Zoom submenu (Zoom Reset, Zoom to Selected Clips, zoom in and out full for the selected track, Zoom Out Full for all axes), Time Display submenu (decimal, SMPTE at 59.94, 30, 29.97, 29.97 drop, 25, 24 and 23.976, samples, bars and beats, a custom rate, edit tempo), Video Display (disabled until the video track), Waveform Channels (layered or uniquely colored), Status Bar items to switch on and off, Metering (reset indicators, show valleys, color gradient, LED style, range from 24 to 120 dB, dynamic or static peaks). Today we have three time formats, the envelope toggles and basic zoom.
- **Effects:** Invert and Reverse (small, no new design needed; first to build), the empty Delay and Reverb and QuickPass presets submenus, Plug-in Manager (with VST3).
- **Analysis (new):** Frequency Analysis, Amplitude Statistics, Histogram, Frequency Band Splitter (split a clip into bands on new tracks, with crossover points). Not built.

### Navigation and playback behavior
- Auto-scroll: pagewise or centered, one choice per editor, plus a toggle that switches off when you move the playhead by hand. Return the playhead to the start position on stop (first or second stop). Stop at the end of a selection when play starts inside it (built 0.1.6). Play while scrubbing. Shuttle speeds on J, K and L (slow, half, normal, double, fast). Meter color crossover levels. Zoom-to-time presets (five slots you can overwrite). Right-click drag in the ruler zooms to a range. Not built unless noted.
- Keyboard shortcuts, **approved 2026-10-20 (use the common defaults listed here; assign each when its feature ships):** Home and End (start and end), Ctrl+Left and Ctrl+Right (previous and next marker, clip or selection edge), M (add marker), Alt+comma and Alt+period (nudge clip), Alt+Up and Alt+Down (move clip to track above and below), Shift+R and Ctrl+R (repeat last command), Shift+P (capture a noise print), Up and Down (switch the active channel), Shift+X (return to start on stop), Shift+click on M, S, R for all tracks, Shift-drag and Ctrl-drag on dials for coarse and fine steps.
- Visual keyboard shortcut editor: a picture of the keyboard with assigned keys highlighted, click a key to see its commands, drag a command onto a key, conflict warning with Undo and Clear, saved sets and Copy to Clipboard. Our editor is a list and works. A visual one is a nice-to-have; decide later. We will not ship another product's shortcut set as a preset.

### Recording and editing behavior
- Punch-in with pre-roll and post-roll lengths, overwrite and insert modes while recording, take files named after the track. Punch-in and recording offset are built; the pre-roll length setting and insert mode are not.
- Audio FX Panel by clip type (dialogue, music, sound effects, ambience), each with its own options; saved presets; limits on each control that a preset author can set; auto-ducking with Monitor Clip Changes and Reanalyze. Types and a basic panel are built; the rest is not.
- Paste with a crossfade at the seams, adjust a selection to zero crossings, more fade shapes. Not built.
- Sample rate and bit depth conversion with dither and noise shaping choices, and saved conversion presets. Dither exists on export; the choices and presets do not.
- Control surfaces: see MIDI above.

### Suggested order when credits return
1. Small and certain: Help and About links, Invert and Reverse, QuickPass layout, Files panel New File, View menu fill-in, clean start, Voice Chain slot menu regrouping, Audio hardware settings that need no hardware (latency list, sample rate display).
2. Effects families that are plain DSP: delay, echo, chorus, flanger, phaser, distortion, reverb, then presets.
3. Master track, buses and the output selector, then auto-ducking and track automation modes.
4. Analysis panels and the Band Splitter.
5. Plugin hosting, then scripting and the assistant endpoint, then MIDI controllers, then video and ADR.

## Effects, multitrack and export build-out (2026-10-20)

Every effect family with the controls testers expect, then multitrack, clips, automation, stretch, video, export and metadata. Behavior only. Status by area:

### Effects, family by family (each written fresh, our own names)
- **Amplitude and Compression.** Built: Gain, Dynamics, Limiter, Speech Leveler, De-Esser, DC Offset Fix. To do: Channel Mixer (swap, mix or invert channels), a multiband compressor (crossover markers, per-band bypass, solo, input spectrum), a hard-limit preset set, a tube-style compressor (a saturation stage in front of Dynamics), Normalize as an effect with DC offset and per-channel equalizing, and an expander and downward gate mode in Dynamics (graph editing with invert, spline curves, a separate level detector section).
- **Delay and Echo.** None built. Delay (time in ms, beats or samples, feedback, mix), Echo (decay, repeats, left and right delay) and an analog-style delay (filtered, wobbling repeats).
- **Filter and EQ.** Built. Check each against the manual: the Notch Filter should take up to six bands, Band EQ needs a range setting and a spline option, the Parametric EQ needs Q or bandwidth entry, and the Precision Filter should offer the common filter families (Butterworth, Bessel, Chebyshev, elliptic) with order, cutoff and ripple. Elliptic is done; the rest of the families are not.
- **Modulation.** None built. Chorus (voices, delay, rate, depth, spread, feedback, quality), flanger (initial and final delay, stereo phase, sweep mode) and phaser (stages, depth, resonance, phase difference).
- **Noise Reduction and Restoration.** Built in some form: Noise Reducer, Noise Print Reducer, Hiss Reducer, Room Reducer, Click Repair, Clip Repair, Hum Cut, Sound Eraser. To do: a noise reduction graph you can draw (noise floor per frequency), real-time adaptive noise reduction with a spectral decay rate, hum removal with harmonic slope and a processing focus choice (low, mid, high, full band, voice), a click eliminator with threshold, complexity, FFT size, run size and light-crackle smoothing, and an automatic phase correction (detect and fix phase between channels over time).
- **Reverb.** None built. Convolution (load an impulse response file, damping, mix), a full algorithmic reverb (decay, perception, room size, damping low and high, diffusion, early reflections, dry and wet), a studio reverb and a surround reverb (not planned: surround is out of scope).
- **Special.** None built. Distortion (with a curve editor), a Doppler shifter (moving source, probably low value), a mastering chain (EQ, multiband, limiter, widener, in one dialog), a loudness meter and a loudness radar as effects (our Levels panel and Check Loudness cover the job; a Voice Chain meter slot is optional). The guitar amp suite is out of scope for a voice editor.
- **Stereo Imagery.** Built: Center Remover, Phase Rotate. To do: Center Channel Extractor controls (crossover bleed, advanced settings, extract or remove), a graphic phase shifter (phase set per frequency on a curve, with a 180 or 360 degree range) and a stereo expander (center pan, width).
- **Time and Pitch.** Built: Pitch Shift, Time Stretch, exact-length stretch. To do: automatic pitch correction (key, scale, attack, sensitivity, formant), a manual pitch editor on the pitch view (draw and reshape the pitch curve), a pitch bender (sweeps drawn over time), formant keeping, and a quality choice. Stretch and Pitch should also offer solo-instrument-or-voice and splice-frequency options.
- **Generate.** Noise (white, pink, brown, with stereo modes) and tones (shapes, frequency sweeps, phase difference) are built; check them against the option lists.
- **Remix.** Built as Fit Music to Length; the manual's target-duration, minimum-length and maximum-slack options map onto our plan choice.

### Multitrack and tracks
- Track types: audio, bus, and a video track (video not planned yet). Mono, stereo and 5.1 on tracks (5.1 not planned).
- Delete Empty Tracks (a Multitrack menu command). Not built.
- Track visibility: a Tracks panel that lists tracks, hides and shows them, and saves visibility presets. Not built.
- Wheel over the track headers zooms the track heights; Shift+wheel resizes widths. We currently scroll there. Approved to change (matches the tester ask).
- Shift-click (with Ctrl) on an input, output, mute, solo, record or monitor button applies it to all tracks. Not built.
- Routing: buses, sends with pre- or post-fader switch, the Mix track, track EQ on every track, effects pre- or post-fader, outputs to hardware ports. Buses, sends (pre or post) and outputs to the master or a bus are built (2026-10-24); outputs to hardware ports and effects after the fader are not. Bus tracks cannot take inputs directly and output only to hardware or other buses.
- Time display options for sessions: start offset and display format per session. Partly built (time formats).

### Clips
- Move, Slip and Razor are built. Overlapping clips without a crossfade play only the top clip; crossfades appear where clips overlap (check our behavior matches).
- Collapse a gap (ripple delete) is built. Loop clips are built. Snap to zero crossings while editing is not built.
- Match clip volume (peak, loudness, or average RMS) across clips: partly built (Loudness Match). Add a peak and RMS choice.
- Fades: more shapes and symmetrical or independent crossfade curves. Not built.
- Create one clip from many: Merge Clips built. Edit a bus or Mix track as a clip: Bounce to New Track built.
- Clip stretch options: rendered or real-time, monophonic or polyphonic material, preserve formants. We have the stretch; the choices and formant keeping are not built.
- Edit Original: from a mixdown file, reopen the session that made it (needs a session reference in the file's metadata). Not built; small.

### Automation
- Track envelopes for volume, pan and effect settings, and clip envelopes. Clip envelopes built; track-level automation not built.
- Automation modes: Read, Write, Latch, Touch (record fader moves while playing, with keyframe thinning settings to avoid too many points), plus Off. Not built.
- Spline (curved) segments, per keyframe or for a whole envelope, and numeric keyframe entry. Not built.

### Video and surround (not started, one later phase)
- Insert video, spot-frame preview while adjusting a clip, video preferences (see the video plan above). Surround output and 5.1 panning are not planned.

### Export
- Save all open files as one batch, export a range (whole session, selection, or between markers), include markers and other metadata (built for files), export a session with copies of its media in a folder (collect and save: built 2026-10-25 as File > Collect Session Media and Settings > Session Media). Not built, except as noted. OMF and Final Cut interchange export is not planned for now: they are large specifications that are hard to verify without the target programs.
- Format settings: compression (speed against size), constant or variable bitrate, and 16.8 and 24.0 floating point options. Bitrate choices are built for MP3 and AAC; variable bitrate and the float options are not.
- Close Unused Media: built as Remove Unused Files.

### Metadata
- A metadata panel with tag sets (switch, save, delete), search, typed fields, and album art for MP3. Tags are built for Batch Convert and files; a full panel with saved sets and cover art is not.

### Order suggestion (docs only; nothing started)
1. Delay, echo, chorus, flanger, phaser and the Dynamics expander and gate mode.
2. Reverb (algorithmic first, convolution second), distortion, Channel Mixer, stereo expander.
3. Master and bus tracks with sends and the output selector, then track automation modes.
4. Multiband compressor, graphic phase shifter, pitch correction and the manual pitch editor.
5. Delete Empty Tracks, Tracks panel, session collect and save, metadata panel.

## Podcast AI and transcript features (scoped 2026-10-20: a small set planned, the rest shelved)

**Decision 2026-10-20:** keep the high-feasibility core, shelve the rest.

**Planned (in this order):**
1. Local transcription with word timestamps (P7). Off until the user loads or downloads a model; see the model-download rule above.
2. Transcript panel with SRT, VTT and plain-text export (already planned).
3. Text-based editing: delete words in the transcript to cut the audio, as one undoable step with a short crossfade and a preview before cutting.
4. Filler word removal: reviewable list, select all, ripple cut (already planned).
5. A Clean Voice preset (noise reduction, tone, leveler, de-ess, limiter) from effects that already exist.
6. Soundboard: trigger pads that place or play sound effects while recording, with the MIDI controller work.
Already built and unchanged: silence removal with a kept pause, chapter markers and export.

**Shelved, not planned for this project** (kept in the table below for reference only): retake finder, edit for clarity, clip finder, chapter suggestions, show notes and other language-model writing, text to speech, voice cloning and Regenerate, translation and dubbing, an enhancement model for studio sound, remote recording, publishing and hosting, video layouts and animated captions, built-in music and effect libraries.

Original note: requested by testers as ideas for later updates. Everything stays local and optional: no account, no cloud, off until the user turns it on (see "On-device AI" above and `docs/design-principles.md`). Nothing here is started.

**The one piece that unlocks most of it is a local transcript with word timestamps.** Once a transcript exists, text-based editing, filler removal, retake detection, chapters, captions and clip finding are mostly ordinary editing operations driven by words instead of waveforms. Build that first, test it on real episodes, and everything below gets cheaper.

| Feature | Feasibility | Notes |
| --- | --- | --- |
| Local transcription (word level, many languages) | **High** | Whisper-family model through a Rust or C++ runtime (MIT code). CPU is fine, GPU faster. Models are 75 MB to 3 GB; the user picks the size. Needs the model-download exception, or the user points the app at a file they downloaded. Transcribe each track separately in a multitrack session so crosstalk does not confuse it, then merge by time. |
| Text-based editing (delete words to cut audio) | **High, medium effort** | A transcript layer links every word to a media time range. Deleting words becomes a split and ripple delete with a short crossfade at zero crossings, as one undoable step. Word timing from Whisper is approximate, so the editor snaps to the nearest quiet point and shows a preview before it cuts. Cons: timing errors on fast speech and overlapping voices; every cut needs an audition. |
| Remove filler words | **High** (after transcription) | A reviewable list with timecodes, select all, ripple cut. Already planned for P7. The word list is user-editable. |
| Remove retakes | **Medium** | Find consecutive passages with nearly the same words and keep the last (or best-sounding) take. This is plain text similarity, no AI needed. Always show the list for review. Cons: false matches on repeated phrases such as show catchphrases. |
| Edit for clarity (cut chatter) | **Low to medium** | A rule-based version (fillers, long pauses, repeated words, false starts) is feasible. Judging what is "non-essential" needs a language model, and that part is unreliable. Ship the rules first. |
| One-click noise removal | **High, mostly built** | Noise Reducer, Hiss Reducer, Room Reducer and Sound Eraser exist. A single "Clean Voice" preset is quick. |
| One-click studio sound | **Medium to high** | A chain preset (noise, tone EQ, leveler, de-ess, limiter) works with what exists. A speech enhancement model (such as DeepFilterNet, MIT or Apache licensed, runs on CPU in real time) would sound better than the current noise code; check its weights license first. Room reverb removal and bandwidth extension are harder and vary in quality. |
| Text to speech | **Medium** | Small open voices run on CPU (Piper, Kokoro). Check each voice's license. Good for scratch narration and fixes; the quality gap to the best commercial voices is real. |
| Voice cloning and Regenerate (fix a flubbed word with a synthetic version) | **Medium to low** | Open models exist, but many weights are non-commercial only, they need a GPU for good results, and matching room tone, pacing and emotion at a splice is hard. Hard requirement before any code: a consent prompt (only your own voice or one you have the right to use), a visible label on generated audio, and no watermark removal. Often a re-record is faster; offer this late. |
| Translation and AI dubbing | **Low, not planned** | Needs translation, synthesis in another language, timing fit and lip timing. Too large for this project right now. |
| Create clips (find the best moments) | **Medium** | Transcript plus simple scoring (speech energy, laughter, question and answer shape, quotable length) can propose Social Clip markers for review. A language model improves it but is optional. Markers and clip export already exist. |
| Chapter generator | **Medium** | Topic changes found from the transcript, titles from a local language model or typed by the user. Chapter markers and export are built. |
| Show notes, titles, blog post, video description, script generator | **Medium, optional** | Needs a language model. Two ways: a small local model through a sidecar runtime, or leave it to the user's own assistant. The second is cheaper and fits our rules: export the transcript and chapters (text, SRT, VTT), and let the optional local assistant endpoint expose them. We do not bundle a language model in the first release. |
| Captions and subtitles export | **High** (after transcription) | SRT, VTT and text. Already planned. Burned-in animated captions need video rendering (see below). |
| Soundboard (fire sound effects while recording) | **High, small** | Trigger pads (keys or MIDI pads) place clips on a track at the playhead while recording, or play live into the monitor mix. Fits the MIDI controller plan. Needs shortcut assignment, which is the project lead's call. |
| Speaker colors and labels | **High, mostly built** | Tracks have colors. Speaker labels on a single mixed file need speaker separation (diarization); skip it when each person has their own track. |
| Remote recording with guests, separate tracks, 4K | **Not compatible with our rules** | It needs servers, accounts and the network, which this app will not have. Alternative: keep Guest tracks, import each person's separate recording from whichever service the user likes, and align them (alignment is planned). Local recording is built. |
| Direct publishing, hosting, newsletters, social scheduling, live streaming, webinars | **Out of scope** | Different products. The Publish Package export (P7) and an optional, off-by-default upload to the user's own podcast host are the planned limit. |
| Video layouts, overlays, backgrounds, logos, animated captions, audiograms | **Low** | A video renderer is a separate product. A simple audiogram or captioned clip export through FFmpeg is possible after the video track work. |
| Built-in music and sound effect libraries | **Not planned** | Licensing and size. Libraries panel works with the user's own files; Help can link to free sources. |
| Silence removal with the option to keep or restore pauses | **Built** | QuickPass: remove, split, or turn down, with a pause kept. Undo restores. |

## Chapters in the file and the feed, commercial markers, and filler words (creator feedback, 2026-10-21)

### Chapter and ad markers into the MP3 and the feed
- **Already built:** chapter markers export as a timestamp list, CSV and podcast chapters JSON, and are written into exported MP3 and FLAC files. Chapters are edited as markers while editing, so there is no separate manual step in the editor.
- **How hosts use them (checked 2026-10-21):** Apple Podcasts accepts chapters from the episode description, from the feed's `podcast:chapters` tag (through the host), or from metadata embedded in the file (ID3 in MP3 or AAC). Apple's published guidance for titles: 45 characters or fewer, under five words, no repeated words, title case. Some hosts read embedded chapters when the file is uploaded (Transistor says it does, and adjusts chapter times when dynamic ad insertion changes the length). Whether a given host also publishes them in the feed is the host's side; a host that does not can be given the chapters JSON. Check the project lead's own host before promising "import into the feed".
- **To add:** per-chapter link and image (ID3 chapter frames can carry a URL and a picture; the JSON supports both); a soft warning in the Chapters panel when a title breaks the 45 character and five word guidance; a one-click "Copy chapters as JSON" and "Save chapters JSON beside the export" so a host without file import can take the file; an "Export" option that always writes chapters when any exist (on by default).
- **Commercial (ad) markers:** there is no standard place for ad breaks in an MP3 or an RSS feed. Hosts with dynamic ad insertion have their own marker tools and do not read a common format. Plan: export Ad Slot markers as a separate small CSV or JSON of timestamps and contracted lengths (for pasting into a host's tools), and keep them out of the chapter list so listeners never see "Ad break" as a chapter. If a host is known to read a specific format, add it as an export option then.
- **Cons:** chapter data baked into the MP3 can go stale if the host inserts ads and does not adjust it; the JSON route avoids that. Embedded chapters do not carry through a re-encode unless the encoder writes them, so the export step must always be the last step.

### Filler word removal: keep the editor in charge
- Feedback from a long-time editor: a filler can carry emotion in an answer, and cutting them all can bring words too close together and sound unnatural. Agreed, and it matches how the feature was already framed (a reviewable list).
- **Spec change:** never an automatic cut. Every item is a keep or cut decision in the list, with audition in context (a few seconds around it), and filters by word, by speaker and by length. Cuts keep a minimum natural pause (adjustable, about 80 to 150 ms to start) instead of closing the gap, and use a short crossfade at zero crossings. Add "shorten" (trim a long filler to a brief one) and "turn down" as non-destructive alternatives to cutting. "Cut All" appears only after the list has been reviewed. Spacing between words after a cut is judged on cadence, not just removal: if the gap after a cut is shorter than the speaker's usual gap, the edit is flagged.

## Added 2026-10-23: broadcast and disc features, and shortcut follow-ups

Decision (project lead): broadcast and CD features stay in the plan for people who want them.
- **Built as scaffolding:** CD Track and Cart Timer marker kinds (Edit > Markers, the lanes' right-click menu, Markers panel), Shift+M and C. Subclip markers can now be added from a time selection.
- **Still to build:** CD layout and disc export (track list from CD Track markers, gaps, CD-Text, cue sheet); Cart Timer behavior (show the cart's set length, warn when a read is over or under, like the ad read timer); Edit > Markers export of these kinds.
- **Shortcut chords held by disabled items:** Alt+B (Bus Track, P5), Ctrl+Shift+G (Suspend Groups), Alt+Backspace (Time Selection in Selected Clips). Not built: Suspend Groups, Remove Focus Clip from Group, Time Selection in Selected Clips, Bounce Selected Track, Bounce Selected Clips in Time Selection.
- **Not in our plans (note for later):** Add Video Track, 5.1 tracks and the video shortcuts from the reference menu.

## Requested 2026-10-23, for later

- **Final Cut Pro interchange (FCPXML export), and better interchange in general.** Export a session's tracks, clips, positions, fades and markers as FCPXML so editors can finish in a video editor; later look at other interchange formats. Own design, built from the published format, no other product's code.
- **MCP control** (local-only endpoint, off by default, read-only first, every edit one labelled undoable step). Already under "Later" in CLAUDE.md; this request raises its priority.
