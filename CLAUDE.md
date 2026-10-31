# Audiform: project instructions

Copyright (c) The GeekCast Radio Network, LLC (Altered Audio Editor) and the Audiform contributors. Licensed under GPLv3.

Audiform is a free, open-source audio editor, a fork of Altered Audio Editor by Steve Phillips and The GeekCast Radio Network, LLC (The GCRN, a podcast network producing independent audio content since 2009). Altered Audio Editor was built for podcasters and voiceover artists: multitrack editing, per-host voice processing, transcription, chapter export, and one-click publishing. Audiform keeps all of that and adds music and immersive work: 3D Space sessions, MIDI, automation, tempo.

**Fork rules (added for Audiform):**
- The project lead is Broman (GitHub `nwh212`). Wherever this file says "project lead" or "maintainer", that is who decides for Audiform.
- Give Altered Audio Editor and its authors full, generous credit (README, About, Help > Based on Altered Audio Editor, NOTICE.md). Keep every existing file's GCRN copyright line; new files name the Audiform contributors.
- Never use the Altered Audio Editor name or logo as Audiform's own (see TRADEMARKS.md); only to say what Audiform is based on. Problem reports go to Audiform, not to the original project.
- Audiform is its own project. File formats, ids and folder layout follow what Audiform needs. One feature per commit, decisions logged in `docs/decisions.md`.
- Audiform keeps opening Altered Audio Editor's sessions (`.altaudio`, and its `.sesx` exports) and carrying over its settings for as long as that is feasible. When its format changes, add reading support for the new version where practical; when it no longer is, say so in `docs/decisions.md` and tell the project lead.
- Builds: test builds (test-build.yml) are Windows only; full releases (release.yml) build Windows, Linux and macOS (decided by the project lead 2026-10-27).
- Contributions are released under GPL v3.0 or later. Outside contributors also agree to CLA.md, which gives the maintainer (Broman) the right to use their contributions under other terms too. Do not merge an outside pull request without the agreement box checked.

This is not a clone of any commercial product. It is a purpose-built tool shaped by long podcast production experience. The editing model follows common professional audio workflows that any experienced editor will recognize. The design, naming, code, and podcast-specific features are original.

Audiform ships from its own GitHub releases (github.com/nwh212/audiform).

Source of truth for scope and phases: this file, with the reasons behind each choice in `docs/decisions.md`. There is no separate spec doc. Ask before deviating.

Also read CLAUDE.local.md if present: it holds private notes and the naming map. It is never committed (see `.gitignore`). Never copy its contents into tracked files.

## Current phase: P5 Effects started (P1 and P2 done, P3 editing nearly done)

- P1 Foundation: all 12 tasks built; exit gate passed on a real `.sesx` session (2026-10-14).
- P2 Playback: engine, Mixer, Levels and Time Info built. Bus tracks and sends moved to P5 with effects.
- P3 Editing, in progress: clip editing, envelopes, ripple delete, clipboard, markers, mixdown, stems, Check Loudness, Merge Clips, Make Unique Copy, Loop Clip, Waveform editor. Tool strip (mode switch, Move, Razor, Slip, Time Selection), shared transport bar, six starting tracks, Loop Playback and Skip Selection, track header controls (dials, monitor, polarity, input picker), Spectrum and Pitch views with Box, Lasso and Brush select, Reduce, Remove, Repair and Spot Repair, double-click a clip to edit its file. Recording v1 (pulled forward from P4): armed tracks record their picked inputs, input meters when armed, live waveform while recording. Track collapse and resize, colored track scrollbar, Zoom Out Full, box select of clips. Since then: punch-in and a recording offset, Insert Silence, Generate Tone and Noise, time stretch, audio types on clips, Add Tracks, ad slot and social clip markers, marker colors and notes, the Playlist panel, chapter export (files and embedded), AAC export on Windows. Then markers in the Waveform editor (WAV cue points), input monitoring, and Batch Convert tags. ASIO built for 0.1.5 (release builds; needs hardware testing). `.aup3` import is parked as a possible later feature.
- P5 Effects, first batch (2026-10-18): Voice Chain on clips and tracks with eight built-in effects (Parametric EQ, Hum Cut, Dynamics, Limiter, Breath Gate, De-Esser, Speech Leveler, Gain), presets, Effects menu entries. Second batch: Noise Reducer, Hiss Reducer, Room Reducer, Click Repair, Clip Repair, DC Offset Fix. Then the Batch Convert rework (processing presets, Output and Sample Settings dialogs) and Music Fit (Clip > Fit Music to Length), then QuickPass repair scans (Click Repair, Clip Repair, Mark Audio), then Band EQ, Curve EQ, Notch Filter, Precision Filter, Center Remover and Pitch Shift. Then (2026-10-18, fourth batch): live chain edits without a playback restart, chain meters, Voice Chain preview and apply in the Waveform editor, Noise Print Reducer, Sound Eraser, Phase Align, Elliptic filters, response graphs (Curve EQ points draggable), Batch Convert normalize and fades, M4A tags, Music Fit plan choice. Then Edit > Settings (all pages; working settings live, the rest greyed out with the reason). 0.1.3 (2026-10-19): Speaker Balance, host profiles and Guest track mode, session backups, record count-in, marker name, timecode rate. 0.1.4 (2026-10-19): Waveform editor Save writes the Voice Chain into the file (bypass it to leave it out), support wording, Settings trimmed to what this app has or plans, experimental Linux and macOS builds, `docs/design-principles.md`, grouped input picker, the session follows files that changed length in the Waveform editor, effect settings windows, faster Opus decoding (patched `opus-decoder` in `src-tauri/vendor`), switching Waveform files keeps unsaved edits, plain wheel zoom over the lanes, Loop Playback without a selection loops the session, the session row in the Files panel, a recording input for the Waveform editor. 0.1.5 (2026-10-19): Phase Rotate, clip effect automation (EffectParam envelopes now play), File > Export > Multitrack Session (.sesx), ASIO (Cargo feature `asio`, on in release builds), Waveform editor refits while its panel settles. 0.1.6 (2026-10-20): noise reducers ignore digital silence (later clips kept their effects), playback stops at the end, loop over a fresh selection, QuickPass and Stretch in the Waveform editor, Files right-click, envelope snapping, custom loudness target, meter response, finer waveform levels for short files, Move to Track Above/Below. Every effect in the behavior references gets a renamed equivalent; the list and the agreed QuickPass and Batch Convert layouts are in `docs/backlog.md`.

`docs/decisions.md` records what was built and why. Later-phase features get data model fields (scaffolding) before their code. If a task pulls toward a later phase, say so before building it.

P1 exit gate (kept as a regression check): open a real `.sesx`, see its tracks and clips in the multitrack view, save it as `.audfrm`, close, reopen, and everything (layout, libraries, clip positions) is intact.

### P1 task order

1. Repo scaffold: Tauri 2 + Svelte 5 + TypeScript + Vite. GPLv3 LICENSE, copyright headers, README, CI that runs `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `npm run check`, `npm test` on Windows. A Linux CI job and experimental Linux and macOS release builds are in the workflows.
2. Data model in Rust with serde: Session, Track, Bus, Clip, Envelope, Marker, EffectChain, Library, LibraryItem, Template, HostProfile, Workspace. Every file type carries `schema_version`. Publish JSON Schemas to `/schemas`. Round-trip tests for each type.
3. Command and undo stack in the frontend: every state change is a command with `do` and `undo`. History panel lists the stack.
4. Docking shell with dockview: Default workspace (see Workspace section below). Panels are individually toggled on/off; there are no named workspace presets beyond Default and Podcast mode.
5. Menu bar (File, Edit, Multitrack, Clip, Effects, View, Window, Help). Item contents are our own, defined in `src/app/menus/model.ts`. Panel toggles and Reset to Default Layout live in the Window menu. Help links to thegcrn.com and the GCRN plugins page. Unbuilt items show disabled with the reason as a tooltip, never fake.
6. Keymap as data (`src/keymap/default.json`), user changes saved to `keymap.alkeys` in the app config folder, edited in Edit > Keyboard Shortcuts. See Keyboard shortcuts section below.
7. Files panel: import WAV, AIFF, MP3, FLAC, M4A/AAC, OGG via symphonia. Columns: name, status, duration, sample rate, channels. Drag from OS into the panel.
8. Drive Manager panel: browse drives and folders, preview audio.
9. Libraries panel: folder plus `library.json`, collections, copy-in by default, item metadata, search, drag item to track.
10. Multitrack view (display only in P1): tracks, clips with real waveforms from a peak cache, ruler, playhead, zoom, scroll, drag-to-reorder tracks. No playback yet.
11. `.sesx` import: tracks, clips, positions, file paths. Effects are skipped with a visible notice.
12. Session save, load, autosave every 2 minutes, crash recovery on next launch.

## Stack

- Shell: Tauri 2
- Frontend: Svelte 5, TypeScript strict, Vite, dockview for docking (the public package that wraps dockview-core)
- Core: Rust stable
- Crates: symphonia (decode), hound (WAV), rubato (resample), cpal (audio I/O, P2), rtrb (lock-free command queue, P2), ebur128 (loudness, P5), nnnoiseless (noise reduction, P5), chapters and id3 (metadata, P7), roxmltree (.sesx), serde and serde_json
- Platforms, in priority order: Windows first, Linux (Zorin OS) second, macOS third. Windows is the main and only tested target. Linux and macOS release builds are experimental until testers confirm them. Code stays portable from day one:
  - No Windows-only APIs outside clearly marked platform modules (`src/lib/platform.ts` in the interface, `#[cfg(target_os)]` blocks in the core).
  - Paths go through `PathBuf` in Rust and are never built by string concatenation. Path case matters on Linux, not on Windows.
  - UI copy names the right system tool per platform (Explorer, File Manager, Finder).
  - Every Rust test and live check also runs on Linux, since Claude's workspace is Linux.
  - Linux builds need the ALSA headers (`libasound2-dev` on Zorin and Ubuntu) for audio output.
  - Known later work: Linux and macOS installers, Cmd instead of Ctrl on macOS, macOS signing, and per-platform audio I/O and GPU checks in P2 and P7.

## Repo layout

```
/src-tauri/src
  commands/     Tauri command handlers, thin
  session/      data model, load/save, migrations
  library/      libraries and templates
  sesx/         .sesx session import
  codecs/       decode and encode
  peaks/        peak cache (.alpk) build and read
  engine/       P2: transport, mixer graph, real-time thread
  dsp/          P5: effects
  export/       P3+: mixdown, metadata, chapters
/src
  app/          app shell, menus, docking
  panels/       one folder per panel
  keymap/       default.json and keymap loader
  commands/     undo stack and command definitions
  stores/       Svelte stores mirroring core state
  styles/       design tokens
/schemas        JSON Schemas for every file format
/docs           architecture notes, decisions log
```

## Workspace layout

Window > Workspace lists the built-in workspaces (Default, Focused Edit, Mixing, Voiceover, Review and Export) plus any the user saves. They let the app cover a full editing job out of the box; names and layouts are our own, built in `src/app/docking/registry.ts`. Each workspace remembers its arrangement. Built-in ones can be reset, not deleted; user ones can be saved, reset and deleted. Panels are toggled from the Window menu or a tab's right-click menu. Tabs have no close button, so a stray click never closes a panel.

Default layout. This is the only Default layout. It puts the common podcast workflow in reach: files and voice processing on the left, the timeline in the center, effects and libraries on the right.

Left column (~270px, full window height):
- Top: Files panel with import/folder/search toolbar, Name/Status/Duration columns
- Bottom: Tabbed panel group — Voice Chain (front tab), Batch Convert, with >> overflow (QuickPass was removed by the project lead on 2026-10-30)
  - Voice Chain shows: Clip Effects / Track Effects tabs, Presets dropdown, Track label, 16 numbered slots with colored effect badges
  - Below slots: Input level meter, Output level meter, Mix (Dry/Wet slider)

Center (fills remaining width):
- Top: zoom/overview strip showing full session waveform
- Editor tab and Mixer tab
- Main area: track lanes with headers (expand arrow, name, M/S/R, volume knob+dB, pan knob, send routing, Read/automation dropdown)
- Timecode display (large, e.g. 1:15:05.399) above transport
- Transport bar: stop, play, pause, back/forward, record, loop — centered; zoom controls right-aligned
- Bottom: Levels panel under the center and right columns only (the left column runs the full window height), with Time Info (~270px) at its right end, lined up under the right column. Time Info has Start/End/Duration fields and sample rate/bit depth/size readout.

Right column (~270px, same width as Time Info below it):
- Libraries (the Audio FX Panel placeholder that sat above it was removed on 2026-10-27; audio types are set from the Clip menu)

## Host profiles and preset inventory

### Host profiles (P5)

A host profile is a per-person starting point: gate settings plus the Voice Chain preset to load on that person's track. Built-in profiles are generic. The project's private host profiles and personal presets live in `CLAUDE.local.md`.

| Profile | Gate threshold | For | Notes |
| --- | --- | --- | --- |
| Close Mic | −14 dB | A strong voice right on the mic | Attack 2 ms, hold 50 ms, release 200 ms |
| Standard Mic | −20 dB | A typical speaker at normal mic distance | Same timing as Close Mic |
| Remote Guest | −30 dB (softer) | Someone calling in over the internet or phone | Telephone EQ, heavier noise reduction |

### Voice Chain presets to build (P5)

**Voice chains:**
- Podcast Voice, Podcast Voice 2, Podcast Voice 3, Podcast Voice 3 -3 dB, Podcast Voice 4 -3 dB
- Voice Leveler and Noise Gate
- Clean Up and Level Voice Over
- Radio Announcer Voice
- Voice Over

**Breath gates:**
- Breath Gate (generic)
- Breath Gate - Low: gate at −14 dB, attack 2 ms, hold 50 ms, release 200 ms
- Breath Gate - Medium: gate at −20 dB, same timing

**Voiceover presets:**
- DeEss and Limit Male Voice Over
- DeEss and Limit Female Voice Over

**QuickPass presets:**
- Clean up Podcast Interview
- Just Keep the Fun Parts
- Trim Long Silent Pauses
- Trim Short Digital Dropouts
- Reduce Microphone Bleed
- Speed up the Lecture
- Mark Non-Silent Areas
- Podcast - Top 100 Clips

**EQ presets:**
- Hum Cut 60 Hz (US)
- Hum Cut 50 Hz (Europe)
- Rumble Cut
- Subsonic Cut
- Telephone - Receiver
- Telephone - Voice Mail

Build the Hum Cut, Rumble Cut and Subsonic Cut presets from scratch with our own settings.

### Panels outside the Default layout

Not in Default: Markers, History. (A saved-commands panel was dropped.) History and Markers stay available from the Window menu.

**Excluded from Default workspace (not shown, available via the Window menu):**
- Video panel (not planned)
- Playlist (built 2026-10-18, Window menu); CD layout (not planned)
- Surround Panner, Basic Adjustments, Phase Analysis, Phase Scope (available in the Window menu, not in Default)
- Podcast mode panels: Chapters, Transcript, Podcast Publish (View > Podcast Mode toggle)
- Voiceover mode panels: Script, Take Lanes, Delivery Check (View > Voiceover Mode toggle)

## Planned additions (community requests, October 2026)

Full detail, status table, risks and order of work are in `docs/backlog.md` (see "Roadmap audit"). Phase placement as of 2026-10-20:

- **Built:** non-destructive time stretch (Clip > Time Stretch, Alt+drag on a clip edge, exact-length stretch in the Waveform editor), non-destructive silence and breath handling in QuickPass, one-click loudness targets, chapter export, marker colors and notes, Playlist panel, ASIO and per-channel input picking (hardware untested), Phase Align and Phase Rotate, JSON Schemas in `/schemas`.
- **Decided 2026-10-20:** the app opens to an empty window (setting for last or new session); master and bus chains run on the render thread, track faders lag half a second, the master fader stays instant; the shortcut table above is approved.
- **Proposed 2026-10-24 (contribution, see decisions.md):** bus tracks, aux sends (pre or post fader), track and bus outputs, the master chain and sidechain keys for Dynamics and Breath Gate, all through one mix graph on the render thread. Plain sessions keep instant faders. VST3 plug-ins in Voice Chains, each in its own host process (`src-tauri/plughost`), with the Plug-in Manager and plug-in windows on Windows. MIDI tracks: MIDI clips with notes in beats, a piano roll, the session tempo and a VST3 instrument per track, whose sound goes through the track's Voice Chain, buses and sends like any track's.
- **Next (small, no hardware):** Help and About links (Discord, GitHub, Report a Problem), Invert and Reverse, QuickPass layout rework, View menu fill-in, clean start window, Voice Chain slot menu by effect family, ASIO and WASAPI buffer, latency and sample rate controls.
- **P5:** delay, echo, modulation and reverb effects, VST3 plug-in windows on Linux and macOS (hosting itself is proposed above; no VST2), auto-ducking, multi-microphone alignment and bleed reduction, Realtime Export, analysis panels and the Frequency Band Splitter.
- **Later:** sandboxed scripting (QuickJS), a local-only MCP endpoint (off by default, read-only first), MIDI controller mapping then MIDI tracks and instruments, video reference track and ADR cues.
- **Rules that apply to all of these:** no network access from scripting or the MCP endpoint; any assistant or script edit is one labelled, undoable step; plugins and scripts can never crash or freeze the editor; every new third-party dependency has its licence checked and credited in `ATTRIBUTION.md` first; no OpenAPI spec (there is no HTTP API); new keyboard shortcuts only with the project lead's approval (candidates are listed in the backlog).

## Podcast-specific features (phase assignments in spec)

These are confirmed podcast-specific additions. Data model fields are scaffolded in P1 even when the feature builds later.

- **Text-based editing (P7):** delete words in the transcript to cut the audio, one undoable step with a short crossfade and a preview. Needs local transcription with word timestamps (off until a model is loaded).
- **Soundboard (with MIDI controllers):** trigger pads place or play sound effects while recording.
- **Filler word removal (P7):** post-transcription detection of "um," "uh," "you know," "like" across all host tracks; a reviewable list with timecodes where every item is kept or cut by the editor. Never automatic: a filler can carry emotion, and closing the gap can make speech unnatural. Cuts keep a minimum natural pause (adjustable), use a short crossfade, and can shorten a long filler instead of removing it. Each item can be auditioned in context. Cut All exists only after the list has been reviewed.
- **Publish Package (P7):** one button exports mastered MP3, chapters JSON, VTT/SRT/text transcripts, show notes draft, and social clips from one session.
- **Podcast Manager sync (P6 read-only, P7 write-back):** session creation picks a show via WordPress REST API; auto-loads template, episode number, hosts, profiles, library. Write-back pushes chapters, transcript, credits.
- **Social clip extractor (P7):** Social Clip markers export audio + transcript snippet in a format Social Publisher picks up directly.
- **Network library (P7):** a GCRN-scope library layer above per-show libraries; shared bumpers, promos, legal reads across all 38 shows; linked items propagate updates. `scope` field in data model from P1.
- **Guest track mode (P5):** track type for remote guests; softer gate, heavier noise reduction, telephone-bandwidth EQ preset.
- **Ad read timer (P5):** ad slot markers carry contracted duration; export warns if actual read is off by more than 5 seconds.
- **Pre-export loudness scan (P5):** pass/fail card before any export showing integrated LUFS, true peak, noise floor, duration. Export requires acknowledgment.
- **Music Remover via Demucs (P7):** local GPU source separation; vocals-only or full stems as new tracks.

Podcast mode is a toggle (View > Podcast Mode), not a separate workspace. It shows these additional panels: Chapters, Transcript, Podcast Publish. Turning it off hides them. State is saved per session.

Voiceover mode is a toggle (View > Voiceover Mode). It shows: Script panel, Take Lanes, Delivery Check. Same save behavior.

## Keyboard shortcuts

Standard Windows shortcuts only. No application-specific keymap beyond these:

| Action | Shortcut |
| --- | --- |
| Undo | Ctrl+Z |
| Redo | Ctrl+Y |
| Save | Ctrl+S |
| Save As | Ctrl+Shift+S (approved by the Audiform project lead, 2026-10-30) |
| Open | Ctrl+O |
| New session | Ctrl+N |
| Select all | Ctrl+A |
| Cut | Ctrl+X |
| Copy | Ctrl+C |
| Paste | Ctrl+V |
| Delete selected | Delete or Backspace |
| Play / Stop (plays; stops when playing or recording) | Spacebar |
| Record (starts recording; stops it when recording) | Shift+Space (approved by the Audiform project lead, 2026-10-26) |
| Split at playhead | Ctrl+K |
| Zoom in | Ctrl+= |
| Zoom out | Ctrl+- |
| Find | Ctrl+F |
| Razor tool | R |
| Move tool | V |
| Time Selection tool | T |
| Slip tool | Y |
| Time stretch a clip (drag its edge) | Alt+drag |

Approved 2026-10-20 and assigned in 0.1.9 (disabled menu items hold the chord for features not built yet; these are the common defaults most editors share):

| Action | Shortcut |
| --- | --- |
| Toggle Waveform and Multitrack editor | 1 (also 0 and 9 pick Multitrack and Waveform) |
| Go to start, go to end | Home, End |
| Previous or next marker, clip or selection edge | Ctrl+Left, Ctrl+Right |
| Previous or next marker only | Ctrl+Alt+Left, Ctrl+Alt+Right |
| Add marker | M or * |
| Return playhead to start on stop (toggle) | Shift+X |
| Zoom in or out horizontally | =, - |
| Zoom in or out vertically | Alt+=, Alt+- |
| Zoom reset, zoom in full, zoom out full (selected track, all axes) | Backslash, Shift+/, Shift+Backslash, Ctrl+Backslash |
| Show spectral display, show HUD | Shift+D, Shift+U |
| Hide all effect windows | Ctrl+Shift+H |
| Meter input signal | Alt+I |
| Repeat last command (with its dialog, then the dialog only) | Shift+R, Ctrl+R |
| Convert sample type | Shift+T |
| Capture a noise print | Shift+P |
| Activate left or right channel (Waveform editor) | Up, Down |
| Nudge clip left, right; copy clip to the track above, below (a new track below the last) | Alt+comma, Alt+period, Alt+Up, Alt+Down |
| Mute, Solo, Record or Monitor on all tracks | Ctrl+Shift+click |
| Dial fine and coarse steps | Ctrl+drag, Shift+drag |

Approved 2026-10-23 (the right-click menu rework; disabled items hold their chords):

| Action | Shortcut |
| --- | --- |
| Add stereo audio track, add bus track (P5) | Alt+A, Alt+B |
| Add Tracks, Delete Empty Tracks | Shift+A, Shift+E |
| Delete Selected Track | Shift+Delete (was Ctrl+Alt+Backspace, which Windows did not pass on; changed by the project lead 2026-10-27) |
| Group Clips, Suspend Groups (not built) | Ctrl+G, Ctrl+Shift+G |
| Ripple delete: selected clips, time selection in selected clips (not built), time selection in all tracks | Shift+Backspace, Alt+Backspace, Ctrl+Shift+Backspace |
| Deselect All, next clip in selected track, clips to end of track, clear time selection | Ctrl+Shift+A, Period, Ctrl+Alt+T, G |
| Add CD Track marker, add Cart Timer marker | Shift+M, C |

Defaults are stored in `src/keymap/default.json`. Users change them in Edit > Keyboard Shortcuts; only their changes are saved, to `keymap.alkeys` in the app config folder. Every menu item shows its shortcut. No other shortcuts are added without the project lead's explicit approval. We never ship another product's complete shortcut set as a preset.

## Naming conventions

All feature, panel and workspace names are our own. Never use other products' panel names, workspace names or feature brand names in UI copy, code comments, docs or error messages; see the naming map in CLAUDE.local.md. When a name is in doubt, pick a plain descriptive one (for example Audio FX (called Voice Chain, then briefly FX Chain, before 2026-10-27), Batch Convert, QuickPass, Time Info, Drive Manager, Noise Reducer, Speech Leveler, Dynamics, Limiter, Click Repair, Breath Gate, Loudness Match).

## Legal guardrails (non-negotiable)

These rules exist because reproducing the exact visual expression of a commercial product is legally risky even without copying code. Apply them at every task.

**Workflow principles, not visual copies.** The app should feel familiar to anyone who knows audio editing. It must not look like any commercial audio editor. Panel arrangements, visual hierarchy, and UI styling are designed fresh.

**What is common practice and fine to use:** two-editor model (Waveform and Multitrack), non-destructive multitrack editing, an effects chain per track/clip, session-based file management, drag-and-drop from a file panel to tracks, standard keyboard shortcuts listed above.

**What must be original:** all panel visual design, all icons and artwork (never another product's icons), menu structures (similar is fine, identical trees are not), feature names (see Naming conventions above), and any visual motif that would make a user of a commercial audio editor think they are looking at that product.

**AI-generated code:** write specs in terms of behavior, not implementation. "The waveform displays min/max peaks" is a behavior. "Copy another product's waveform rendering" is not acceptable framing. Claude Code cannot prove it has never seen other products' code, so prompts must describe what to build, not what to copy.

**Before adding any feature:** if the description says "like X in another product" or "match X from another product," stop and reframe it as a functional description. Log the reframe in `/docs/decisions.md`.

**Interoperability:** the app imports `.sesx` multitrack session files and can export a copy of a session as `.sesx` (File > Export). Our own save format is `.audfrm` (decided by the project lead 2026-10-27; the same JSON as the older `.altaudio`, which still opens and saves again as `.audfrm`). Format identifiers inside those files are read as data only and never shown as our own names.

## Privacy (non-negotiable)

Zero logins, zero telemetry, zero tracking. The app is fully usable offline without anyone doing anything.
- No network requests of any kind from the app or the core: no update checks, analytics, crash reporting or license checks.
- No accounts or sign-in, ever.
- The only web access is the Help menu opening pages in the user's browser when they pick a link, limited to the allow list in `src-tauri/src/commands/links.rs`.
- A support link for Steve Phillips (the original author) and his book, Altered Audio: Podcasting Demystified, live in the Help menu only, under Based on Altered Audio Editor. Never in the user's face, never a nag, never forced.
- Updates (decided 2026-10-14, build after the first GitHub release): Help > Check for Updates, plus an optional weekly check that is OFF by default, set in Preferences and on the first-run screen as an unticked box. The check fetches one signed `latest.json` from GitHub Releases through the Tauri updater; no login, no API calls. A found update shows one line in the status bar, never a pop-up, never during playback or recording, never installs without a click. The privacy guard allows only the updater plugin and that one host.
- Any future feature that needs the network (for example Podcast Manager sync) must be off by default, clearly labeled, and never required. See `PRIVACY.md`.
- Audio Separation downloads (decided 2026-10-28) are the one such exception so far: the engine and a model, only when the user clicks Download, from `src-tauri/src/separation/download.rs` only (system curl, HTTPS, github.com and huggingface.co), each checked against a pinned SHA-256. `scripts/check-privacy.mjs` enforces it. Wording: it only pulls the necessary model for demixing; audio separation is handled locally on user hardware and speeds / results depend on that hardware.

## Loudness defaults

- Delivery targets (from published podcast loudness guidance): podcast stereo -16 LUFS, mono -19 LUFS, Spotify -14 LUFS; true peak at or below -1 dBTP.
- Broadcast follows the region set in Check Loudness and Loudness Match. Default is the US (ATSC A/85: -24 LUFS, true peak -2 dBTP). Others: Canada (same as US), Europe and the UK (EBU R128: -23, -1), Australia and New Zealand (OP-59: -24, -2), Japan (ARIB TR-B32: -24, -1).
- Voice Chain Limiter (P5): default ceiling -6 dB, with a -3 dB preset. Every parameter stays user-configurable: ceiling, input boost, look-ahead, release, and linked channels. Users can save their own presets.

## Architecture rules

- The interface never touches audio. It sends commands to the Rust core and renders state it receives.
- All edits go through the command stack. No direct state mutation from components.
- The real-time audio thread (P2+) never allocates, locks, or does I/O. Commands arrive over a lock-free queue.
- Multitrack is non-destructive. Source audio is never modified outside the Waveform editor.
- All file formats are versioned JSON with migrations. Never break an old file.
- Long work (decode, peak building, import) runs off the UI thread with progress shown.
- No browser localStorage for project data. Settings live in the app config directory.

## Usability charter

The bar is an editor that feels right to someone who has edited podcasts for years.

1. Every action exists in a menu and shows its shortcut.
2. Every panel can dock, tab, float, resize, and maximize (floating and maximizing are in the panel menu on the active tab or a right-click, not buttons).
3. Drag and drop works everywhere: OS to Files, Files to tracks, Libraries to tracks, clips between tracks.
4. Right-click context menus on every clip, track, marker, file, and panel tab.
5. Interaction feedback within one frame (16 ms). Anything slower shows progress and never freezes the window.
6. Every edit is undoable, including track reorders.
7. Work is never lost: autosave every 2 minutes plus crash recovery.
8. Dark UI tuned for long sessions: dense, quiet chrome, clips and waveforms carry the color.
9. Podcast-specific tools are never in the way during voiceover work, and vice versa.

## Podcast editor workflow

This is the canonical step order for podcast editing. Every podcast-specific feature is mapped to its step. Claude Code must implement each feature so it fires at the correct moment, not as a standalone tool the user has to hunt for.

| Step | Action | Podcast features that fire |
| --- | --- | --- |
| 1 New session | File > New Session, pick show | Podcast Manager sync (P6): loads template, episode number, host profiles, library |
| 2 Import and arrange | Drag files to tracks, library assets to timeline | Guest track mode (P5) auto-applies on Guest tracks; Ad Slot markers placed with contracted duration; Social Clip regions marked |
| 3a Speaker Balance | Multitrack > Speaker Balance (one click) | Levelator concept (P5): analyzes integrated LUFS per host track, aligns all tracks to the quietest as reference; runs before Voice Chain so gate thresholds behave consistently |
| 3b Voice Chain | Host profiles load per track, user tweaks | Breath gate fires at the host profile's threshold (Low −14 dB, Medium −20 dB); full chain: Noise Reducer > Speech Leveler > Dynamics > Parametric EQ > Limiter > Breath Gate |
| 4 Transcribe | Multitrack > Transcribe Session (whisper.cpp, local GPU) | Filler word removal (P7) unlocks; chapter and show notes seeds generated |
| 5 Edit | Timeline cuts, text-based editing, chapter markers | Ad read timer (P5) shows live duration on ad slot markers |
| 6 Music Remover | Right-click clip > Remove Music | Music Remover via Demucs (P7), local GPU, vocals-only or full stems |
| 7 QC | File > Check Loudness | Pre-export loudness scan (P5): pass/fail card blocks export until acknowledged; ad slot duration warnings |
| 8 Export | File > Publish Package | Publish Package (P7): MP3 + chapters + transcripts + show notes + social clips; Podcast Manager write-back; RSS.com upload |

Rules for implementing these:
- Each feature appears in the menu at its step, not buried in a settings dialog.
- Features that are not yet built show as disabled menu items, not hidden ones.
- The workflow order is enforced by availability: Transcribe is disabled until tracks have audio; Remove Filler Words is disabled until transcription is complete; Publish Package is disabled until Check Loudness passes.

## Reference sources (algorithms only)

The editing model follows common professional practice (two editors, non-destructive multitrack, effects chains). No commercial product's visual design or naming is a reference.

Two open-source projects may be studied for specific technical problems:

- **Audacity** (GPL): effects and processing, especially noise reduction, silence trimming, click removal, loudness normalization.
- **SoundCraft / ArtCraft** (MIT or Apache-2.0): Rust audio engine patterns, playback, file handling.

Rules:
- Never copy their interface, workflow, or data model.
- Credit any ported algorithm in `ATTRIBUTION.md`.
- Ask the project lead before porting anything larger than a single algorithm.
- Ported effects must be A/B tested against the original and documented.

## Conventions

- Rust: `cargo fmt`, `cargo clippy -- -D warnings`, unit tests beside code, integration tests in `/src-tauri/tests`.
- TypeScript: strict mode, `svelte-check` clean, Vitest for logic.
- Every source file starts with the GPLv3 header. Files from Altered Audio Editor keep `Copyright (c) The GeekCast Radio Network, LLC.`; new files say `Copyright (c) The Audiform contributors.` A new file that takes code moved out of a file with the GeekCast Radio Network line keeps that line too, above the Audiform one.
- Small commits, one task per branch, conventional commit messages.
- Log every architecture decision in `/docs/decisions.md` with date and reason.
- UI copy, docs, and comments: plain, short sentences, no em dashes, no other products' feature names.
- Optional heavy features are add-ons: nothing in the installer, downloaded only when the user first uses them, run in their own process, so the core stays light for people who never use them. Audio Separation is the model: the panel and downloads are in `src-tauri/src/separation/`; the engine itself is a closed program kept outside this repository, talked to only through its job file and output lines.
- Audio Separation: at every major update, check for a better best publicly available model per type (vocals and instrumental for now) and update the list; always show each model's SDR in the list (docs/RELEASING.md step 2).
- Theme pointers are drawn at 16 by 16 pixels (about two thirds of the 24 pixel first drafts), hot spot in the middle except the arrow's tip. The project lead set this size on 2026-10-28; keep new pointer sets to it.

## Definition of done for any task

- Works on Windows.
- Tests pass, lint clean.
- Undoable if it changes state.
- Has a menu entry and keyboard shortcut per the shortcut table.
- Visual design is original, not a copy of any other product.
- Feature name follows the naming conventions.
- The project lead has tried it and it feels right for editing a podcast.
