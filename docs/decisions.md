# Decisions log

Newest first. Each entry: date, decision, reason.

## 2026-10-24: first outside contributor, and how his agreement is recorded

Decision: nwh212 (Discord: Broman) is the project's first outside contributor, listed in `CONTRIBUTORS.md`. On Discord on 2026-10-24 he wrote that he has read `CLA.md` and agrees to it for his pull request and his future contributions, licensed under GPL v3.0 or later as `CLA.md` describes. His work (MIDI tracks and the piano roll, plug-in hosting in a separate process, buses, sends and sidechain, the Phase Wheel and Properties panels, session import fixes) is merged on the `next` branch.
Reason: the agreement bot is the normal way to agree, but he agreed in writing before the merge. A screenshot of the message is kept in the private `private/cla-records/` folder (never committed). For the public record, `CONTRIBUTORS.md` names the agreement. Ask him to also confirm in a comment on the pull request so the agreement lives on GitHub too.

## 2026-10-14: Formats in and out, Batch Convert, Export Mixdown dialog, per-speaker tracks, system audio, bulk trim, track meters

Decision:
- Opening: Opus (`.opus`, Ogg Opus) now decodes through the pure-Rust `opus-decoder` crate (MIT or Apache-2.0), wrapped as a decoder in our codec registry. Checked against every format a common call recorder exports: FLAC, WAV (16-bit, 8-bit, MS and IMA ADPCM), AAC (M4A), ALAC, Ogg Vorbis, Ogg FLAC and Opus open. HE-AAC does not: the decoder handles AAC-LC only. Audacity 3 projects (`.aup3`) are a possible later import.
- Writing: one encoder module writes WAV (16, 24, 32-bit float), FLAC (16, 24; pure-Rust `flacenc`, Apache-2.0, streamed frame by frame so long episodes never sit in memory) and MP3 (LAME 3.100 through `mp3lame-encoder`, LGPL, built from source; constant 32 to 320 kbps or variable quality 0 to 9). It converts channels and sample rate on the way, keeping the exact length. Mixdowns, Batch Convert and Waveform saves all use it.
- Waveform editor: FLAC and MP3 files save in place in their own format (no conversion needed). Saving an MP3 in place, or Save As to MP3, warns that MP3 re-encoding loses quality. Formats we can read but not write (M4A, Ogg, Opus) go through Save As. Save As offers WAV, FLAC and MP3.
- File > Export > Session Mixdown opens our Export Mixdown dialog: what to mix (whole session, time selection, selected clips), name and folder, format, sample rate, channels, bit depth or MP3 bit rate, a summary of what the file will be with its rough size, and "Add the file to the Files panel". It remembers the last format and folder and asks before replacing a file. Include markers and metadata is shown disabled until metadata export exists.
- Batch Convert panel (the left column tab): a list of files (Add Files, Add Folder, add the Files panel's selection, or drop files, folders or zips), naming with text before and after the name, output next to each original or into one folder, Replace existing files, the same format controls, and Remove finished files from the list. Converts one file at a time on a worker; each row shows progress, Done, or why it failed, and the rest carry on. Stop discards only the file in progress. Every file is written under a temporary name and moved into place when complete, so a stopped or failed file never leaves a partial file and converting a file onto itself is safe. Rows that would write the same file, or replace their own source without Replace on, are flagged and skipped.
- Multitrack > Insert Files as Tracks: each file goes on its own new track at the session start, named after the file without a leading speaker number ("1-name.flac" becomes "name"). Zips (a call recorder's per-speaker download) are extracted into a folder beside the zip first; only audio is extracted, every file lands directly in that folder, entries with paths that climb out are skipped. Importing or dropping a zip anywhere does the same. Insert Selected Files as Tracks does it for the Files panel's selection.
- Recording a call: there is no direct link to any chat service (that would need an account and the network, which the privacy rules rule out). Instead, on Windows each output device is offered as an input named "System audio: device", recording what the computer plays (the call) while mics record on their own tracks. Windows sends nothing while the output is silent, so the recorder pads silence by the clock to keep the take in sync.
- Edge trims on a selected clip trim every selected clip by the same amount (the grabbed clip snaps; each clip stops at its own limits), as one undo step. Trim cursors are a bracket with an arrow.
- Track meters: left and right bars, green to -12 dB, yellow to -3 dB, red above, with a held peak line and a clip light per channel. Clip lights (track and Levels panel) stay lit after stopping until clicked.
- CI uses actions/checkout and actions/setup-node v5 (Node 24).

Reason: the project lead's shows record calls as per-speaker FLAC, edit in WAV and publish MP3; this removes the conversion steps around that and makes multi-speaker imports one action. The dialogs and panels follow the common behaviors of these tools; their layout, wording and names are our own.

## 2026-10-14: Track layout pass: collapse, resize, track scrollbar, meters, box select, live recording

Decision:
- Each track header has a collapse arrow (folds the track to its name row; saved as `collapsed`, left out of files when off) and a resize edge at the bottom (drag; one undoable step).
- A track scrollbar on the right of the lanes shows each track's color. Drag the thumb to scroll; drag either end of the thumb to make every track taller or shorter.
- A Zoom Out Full button at the end of the overview bar fits the whole session across and every track into the lanes' height. The overview window has a clearer border and grips on both ends.
- With the Move tool, dragging empty space draws a dashed box that selects the clips it touches (Shift adds); a click still moves the playhead. The Time Selection tool keeps the full-height time selection.
- A slim vertical meter sits at the right edge of each header: the input level while the track is armed (inputs open for metering as soon as a track is armed, without recording), playback level otherwise. While recording, each take draws its waveform on its track as it arrives.
- Razor tool: a razor blade cursor, and the cut line also marked in the ruler. Clips get a subtle raised edge; alternate lane shading is stronger.
- Volume and pan dials are larger with small symbols. The automation menu lists Off, Read, Write, Latch and Touch; only Read can be chosen until automation arrives (P5).
- The Editor tab names what it shows: "Editor - session name" (with a star when unsaved), or the file open in the Waveform editor.

Reason: long sessions with many tracks need folding, sizing and a way to see and reach every track; recording needs a level before and while you record. The look follows our own design; only the behaviors are common practice.

## 2026-10-14: Recording v1, spectral tools, pitch view, double-click to edit

Decision:
- Recording (moved up from P4 at the project lead's request): Record in the transport bar or the Multitrack menu. Every armed track records the input picked on its header into its own 32-bit float WAV, starting at the playhead, while the rest of the session plays. With no track armed, the selected track is armed. Takes go in a Recordings folder next to the saved session, or the app's cache folder until the session is saved. On stop, each take becomes a clip on its track and shows in the Files panel, as one undoable step. In the Waveform editor, Record inserts the take into the file at the playhead.
- Recording engine: one thread per input device. The device callback only copies samples into a lock-free ring buffer (two seconds deep); the same thread writes each track's channels to disk every 10 ms. Input strings from the picker ("Mono: device 1", "Stereo: device 1-2") route channels; a missing device gives a plain error.
- Not yet: input monitoring through the app, latency compensation, punch-in, metronome. The Monitor button saves its setting now.
- Audio Hardware... in the input picker opens the system's own sound settings (Windows Sound settings; pavucontrol or the desktop's sound panel on Linux; System Settings on macOS). It starts a local settings app only.
- Tool strip: Spectrum and Pitch view toggles, then Move, Razor, Slip, Time Selection, then four spectral tools: Box Select, Lasso Select, Brush Select (with a size slider) and Spot Repair. Spectral tools work in the Waveform editor; choosing one turns the spectrum on. Shift adds to a selection. The Slip icon now shows a clip with arrows so it reads clearly.
- Spectral selections are shapes in time and frequency. For an edit, the interface turns them into a grid of weights and the core applies Reduce, Remove or Repair inside it with softened edges. Repair fills each frequency band from the level just before and after the area, keeping the original phase; Spot Repair runs it as soon as the stroke ends.
- Pitch view draws the fundamental frequency (60 Hz to 1 kHz, difference-function method on audio low-passed at 1.1 kHz and thinned to about 11 kHz) over the spectrum; with the spectrum off, the spectrum stays as a dim backdrop. The line breaks at octave jumps.
- The waveform and spectrum share the Waveform editor with a draggable divider.
- Double-clicking a clip in the Multitrack (Move or Slip tool) opens its file in the Waveform editor.

Reason: recording a single host or a voiceover directly is basic to the job, and the spectral tools cover the repairs editors reach for most (clicks, coughs, whistles, phone buzz) without a separate app. Names are our own and describe what each tool does.

## 2026-10-14: Spectrum view v1, host profile names

Decision:
- Waveform editor: a Spectrum button (and View > Show Spectrum) shows frequencies over time under the waveform. Log frequency axis from 30 Hz to Nyquist, 2048-point frames, colors from near black through indigo, teal and amber to warm white over 100 dB. It renders the edit list, so it shows exactly what plays. The view covers up to 20 minutes at once; zoom in for longer files.
- Drag a rectangle on the spectrum to select an area of time and frequency. Effects > Spectrum (and a small bar over the selection) offers Reduce Selected Area 12 dB, Reduce Selected Area 24 dB and Remove Selected Area. The core cuts the stretch into overlapping frames, scales the bins inside the rectangle with soft edges, and rebuilds it; outside the rectangle the audio comes back unchanged. The changed stretch is written to a new file in the app's cache folder and swapped into the edit list as one undoable step. Saving renders it into the file as usual.
- Next for spectral editing: lasso and brush selections, and healing that fills a selection from the audio around it.
- Built-in host profiles are named for what they are: Close Mic (-14 dB), Standard Mic (-20 dB), Remote Guest (-30 dB, telephone EQ, heavier noise reduction).

Reason: removing a single sound (a whistle, a phone buzz, a chair squeak) without touching the voice under it is a common repair. A first testable version lets people try it early. Profile names should say what each one is for.

## 2026-10-14: Track header controls, timeline toggles, interface font, public repo cleanup

Decision:
- Track headers are their own raised panel, a step lighter than the lanes, with a dark gutter between the header column and the lanes and under each track. Headers are 214 px wide.
- Each header: drag grip, name, Mute, Solo, Arm and Monitor Input; round Volume and Pan dials (drag up or right, Shift for fine, double-click to reset); Invert Polarity, which flips the track's gain sign in playback and mixdown (stored as `polarity_inverted`, left out of files when off, so old files are unchanged); an input picker listing Default, None, every mono channel and stereo pair of the computer's audio inputs (names only, nothing is opened); and an automation mode menu shown disabled until track automation (P5). Rows hide on short tracks.
- Timeline toggles beside the header view strip: Metronome (disabled until P4), Follow Playhead (pages the lanes during playback, on by default, saved), Snapping (the existing setting) and Add Cue Marker. The sample rate moved to the editor toolbar.
- Interface font: Source Sans 3 (SIL Open Font License), bundled through `@fontsource/source-sans-3`, Latin regular and semibold only. Base size 13 px. Nothing loads from the network. Credited in ATTRIBUTION.md.
- Public repo cleanup before the first public release: personal names, private presets, notes about reference material and other products' feature names were removed from tracked files. Private notes live in `CLAUDE.local.md` and the gitignored `private/` folder. The repo history restarts from a clean first commit.

Reason: dense headers need to read as separate from the lanes and from each other, and the controls an editor reaches for most (input, monitor, polarity, dials) belong on the track. An open humanist font reads well at small sizes. The public repo should carry only original, shareable material.

## 2026-10-14: Track separation, Loop Playback, Skip Selection, Files preview, app icon, README previews

Decision:
- Multitrack lanes: a 3 px dark break under every track, alternating row shades, and a clearly lighter shade across the whole selected track (header and lane). Each header has a 5 px color strip in the track's color; tracks with no color take the clip palette by row, and their clips do too.
- Loop Playback (transport bar) repeats the time selection; Skip Selection plays around it, so a cut can be heard before it is made. They are one setting with three states (normal, loop, skip), saved between runs. The renderer jumps at the boundary, sample-accurate, with no fade at the seam. Starting inside a skipped range starts after it; starting after a loop plays straight on. Changing the mode while playing restarts from the same spot.
- Zoom row: Zoom In/Out Amplitude now in the Multitrack too (display only, up to 16x; Alt + wheel as in the Waveform editor), plus Taller and Shorter Tracks, which change every track's height in one undoable step.
- Above the track headers: a header view strip. Mix (the current controls) is the only view built; Inputs/Outputs, Effects, Sends and EQ show disabled with the phase that builds them (P4, P5).
- Files panel: Play/Stop Preview, Loop Preview and Auto-Play (preview a file when selected) under the list. Loop and Auto-Play are saved. No Space shortcut was added to the Files list, because new shortcuts need the project lead's approval; Libraries already has one.
- Voice Chain items (Clip vs Track Effects, the presets row, input and output trim) are logged for P5, not built now.
- App icon: `npx tauri icon brand/icon/icon-1024.png` for every size, then `brand/icon/icon.ico` copied over `src-tauri/icons/icon.ico` to keep the simpler glyph at small sizes. The About dialog shows the logo.
- README: the logo (light and dark), two preview images of the app from a demo session made of generated audio (no real show audio), feature and privacy highlights, and the support link.

Reason: in a busy session tracks blurred together and the selected track did not stand out. Loop, skip and the full zoom row are part of a common podcast editing workflow. Placeholders follow the existing rule: unbuilt items are visible but disabled with the reason, never fake.

## 2026-10-14: New Session dialog, session templates, clip envelope chip

Decision:
- File > New Session asks for a name, a folder (default Documents/Altered Audio/Sessions), an optional template, sample rate, bit depth and mix (stereo or mono). With a folder, the session gets its own folder and is saved at once, so merged clips and copies have a home from the start; leave it blank to save later. A clash with an existing session reopens the dialog with the reason. The last-used folder, template, rate, depth and mix are remembered; New Audio File remembers its rate, channels and depth too.
- Templates: File > Save as Session Template saves the open session (tracks, levels, clips such as a show's intro and outro, markers, libraries) as a `.altemplate` in the app's config folder. New sessions made from a template take its rate, depth and mix, get their own id, and record which template they came from. Templates can be deleted from the dialog.
- Mix: Mono sets the master to mono, and mixdowns of the session are written mono unless an export asks otherwise.
- Each clip's title bar shows a small envelope chip ("Volume", "Pan", "Vol + Pan" or "Envelopes"); clicking it switches the volume and pan lines.
- Ctrl+A under a dialog no longer selects the page's text.

Reason: these are the dialog choices and clip header details an experienced editor expects. A show template is what makes a weekly episode a two-click start.

## 2026-10-14: Tool strip, tools, transport bar, six starting tracks

Decision:
- A strip under the menu bar switches between the Waveform and Multitrack editors and holds the tools: Move (default), Razor, Slip, Time Selection. Edit > Tool lists the same tools. Shortcuts R (Razor), V (Move), T (Time Selection) and Y (Slip), all approved by the project lead.
- Razor: click a clip to split it there, Shift splits every track at that time, Alt ignores snapping; a dashed guide shows the cut. Slip: drag inside a clip to move its audio while the clip stays put (loops change phase). Time Selection: drag anywhere, even over clips. In the Waveform editor only Time Selection applies.
- Both editors share one bar under the editor: position clock (click to change format), stop, play or pause, go to start, rewind and fast forward (5 s, Shift 1 s), go to end, record and loop (disabled until recording, P4), and zoom buttons (amplitude zoom in the Waveform editor).
- New sessions start with six empty stereo tracks, Track 1 to Track 6. Opening audio into an empty session puts each file on the next empty track.

Reason: a common podcast workflow needs quick tool switching and one transport for both editors. Layout and icons are our own; behavior matches what an experienced editor expects.

## 2026-10-14: Waveform editor design

Decision: the Waveform editor edits one audio file at a time, shown in the Editor panel in place of the multitrack lanes (View > Waveform Editor, or Clip > Open in Waveform Editor, or double-click a file in the Files panel). Internally the file is a second, private session in the core with one track whose clips are the edit list: the first clip covers the whole file, and every edit (delete, cut, paste, silence, gain, fades, crop) splits and rearranges those clips with the same ops, undo and renderer the multitrack uses. Nothing touches the file until Save, which renders the edit list (clip settings only, original channel count) to a temporary file beside the original and then replaces it. Save As writes a new file and leaves the original alone. Files that are not WAV always go through Save As. The Waveform editor has its own undo history; Edit menu commands go to whichever editor is showing.

Built (2026-10-14): double-click a file in Files (or Clip > Open in Waveform Editor, or View > Waveform Editor). Edits: Delete, Cut, Copy, Paste (replaces a selection, otherwise inserts at the playhead), Crop, Silence, Fade In, Fade Out (equal-power curve written into piece envelopes so fades cross earlier edits), Amplify, Normalize (peak target, measured by the core), and a quick level control: drag to raise or lower the selection or whole file. Level scale in dB on the right, amplitude zoom (Alt + wheel or the bar buttons), and a shared transport and zoom bar under the editor. The action registry gained overlays: while the Waveform editor shows, Edit, Save, Close, Zoom and play commands act on it. File menu labels change to Save Audio, Save Audio As and Close Audio File so it is always clear which document they act on.

Reason: one editing engine for both views means one set of rules and tests; the edit list keeps every step undoable without copying audio on each edit, and the file on disk only changes when the user saves. Overwriting a file changes every clip that plays it, which is what Make Unique Copy is for, and the save confirmation says so.

## 2026-10-14: Loop Clip

Decision: Clip > Loop Clip (a checked toggle) makes a clip repeat its source region. The model gains `looped: { length, phase }` on Clip: `length` is the clip's timeline length in session samples, `phase` is where the clip's left edge falls inside the region, in source samples. Looped clips trim right with no limit and trim left back to the session start; left trims, splits and ripple deletes move the phase so every piece keeps its place in the loop. Turning looping off plays the region once from the phase, so nothing jumps. The renderer seeks back to the region start at the end of each pass. The lane shows a dashed line at each repeat and a "Looped" tag. Optional field with a serde default, so older sessions load unchanged and schema_version stays 1.

Reason: music beds and room tone under long segments, without making copies of the file.

Seams: a 2.5 ms dip on each side of every seam inside the clip (not at the clip's own edges, which have their fades), so a region whose ends don't line up never clicks. Found by Check Loudness: a looped pure tone measured +1 dB true peak before the dip.

## 2026-10-14: Make Unique Copy

Decision: Clip > Make Unique Copy gives each selected clip (not locked) its own file: just the part of the source it plays, copied untouched at the source's rate and channel count as 32-bit float, into a "Clip Copies" folder beside the session. The clip keeps its id, position, gain, fades and envelopes; only its file changes. One undo step. Unsaved sessions are asked to save first.

Reason: gives a clip a file of its own before destructive work in the Waveform editor, so other clips sharing the source are never affected. Copying only the used region keeps copies small.

## 2026-10-14: Clip pan envelopes

Decision: View > Envelopes > Clip Pan shows a cyan line per clip, full left at the top, center in the middle, full right at the bottom. Same gestures as volume: press the line to add a point, drag to move, Shift locks time, right-click to delete a point or reset. A tip beside the pointer shows the value while dragging ("Pan L 40", "Volume -6.0 dB"). Pan is a balance, the same rule as track pan: centered plays both sides at full level. The pan envelope's id is the clip id with its lowest bit flipped, so add and remove round-trip exactly. Split, trim and ripple delete carry pan points with the audio, like volume.

Reason: P3 item. One shared envelope path for volume and pan keeps a single set of rules, tests and gestures.

## 2026-10-14: Merge Clips

Decision: Clip > Merge Clips takes two or more selected clips on one track (none locked), renders them with only their own settings (gain, fades, volume and pan envelopes; not the track fader, track pan, mute or Master) to a 32-bit float WAV in a "Merged Clips" folder beside the session, and swaps them for one clip at the first clip's start. One undo step; original files are never touched. Names never overwrite ("l Merged 2.wav"). An unsaved session asks to be saved first, since the merged file needs a home.

Reason: track settings still apply to the merged clip, so baking them would apply them twice. Float keeps any level above 0 dBFS intact.

## 2026-10-14: Public repo prep

Decision:
- Support link: Help > Support Altered Audio Editor opens https://paypal.me/scp21. `paypal.me` joins the link allow list. Nowhere else in the app.
- License stays `GPL-3.0-or-later`.
- The project's private host profiles and personal presets moved to `CLAUDE.local.md`, which is gitignored. Public built-ins are generic: Host, Co-host, Guest profiles; Breath Gate - Low (-14 dB, 2/50/200 ms) and Breath Gate - Medium (-20 dB, same timing). The host profile test fixture is `host.alhost`.
- Test data uses generic names (Host, Show) instead of real people and shows.
- GitHub: under the GeekCast Radio Network organization, repo for Altered Audio Editor.
- There is no separate spec doc. CLAUDE.md plus this file are the source of truth.
- CLAUDE.md intro now says it is a purpose-built tool, not a clone of any commercial product.

Reason: the repo goes public; personal voice settings stay private, while the settings that work still ship as neutral presets.

## 2026-10-14: Update checks, off by default

Decision: Help > Check for Updates always works when clicked. An automatic weekly check exists but is off by default (Preferences, and an unticked box on the first-run screen). Updates come from GitHub Releases via the Tauri updater, signed with the project's release key, fetched as a release file (not the GitHub API, which rate-limits at 60 an hour per IP). A found update is one status bar line; never a pop-up, never mid-playback or recording, never auto-installed. No launcher: one app doesn't need one.

Reason: bug fixes need a way to reach people, but the zero-network promise is the product. Off by default keeps "nothing leaves your machine unless you ask" true. Rejected: on by default (breaks the promise), auto-install (swapping a production tool mid-episode), a launcher with login (wrong for a single open-source app).

## 2026-10-14: Check Loudness (File menu)

Decision: File > Check Loudness measures the mix (or the time selection) with EBU R128 (`ebur128` crate): integrated, loudness range, true peak, sample peak, plus a mono fold-down. Targets, from published podcast loudness guidance: podcast stereo -16 LUFS, podcast mono -19 LUFS, Spotify -14 LUFS, broadcast -23 LUFS, all with true peak at or below -1 dBTP. Pass within 1 LU. When master gain alone reaches the target without breaking the peak ceiling, a one-click "Raise/Lower Master" button applies it as an undoable edit and measures again. When it can't, the advice points to limiting vocals (a limiter at -6 dB, or -3 dB, depending on the episode).

Reason: loudness is the most common podcast delivery mistake, and measuring the real mix (same renderer as export) means what you check is what you ship. Gain only, no hidden limiting: the Voice Chain Limiter (P5) owns peak control.

## 2026-10-14: Privacy, Help links, security hardening

Decision: zero logins, telemetry, tracking or network calls, written into CLAUDE.md, `PRIVACY.md` and the About dialog. Help menu gains "Book: Altered Audio, Podcasting Demystified" (Barnes & Noble) and "Support Altered Audio Editor..." (disabled until `LINKS.support` is set). `scripts/check-privacy.mjs` runs in `npm run check` and fails the build on network APIs, tracking libraries, HTTP or telemetry crates, sockets in Rust, or a CSP looser than self. CI gains a security job: the privacy guard, `npm audit`, `cargo audit`. Library Clean Up now dry-runs and confirms, refuses folders that aren't the library's own, skips symlinks and non-audio files. Added `SECURITY.md`, `CONTRIBUTING.md`, issue and PR templates.

Reason: trust is the product for a free tool. A guard in the build keeps the promise true as contributors arrive, instead of relying on review.

## 2026-10-14: Clip volume envelopes

Decision: View > Envelopes > Clip Volume shows a yellow volume line on every clip, at 0 dB until it has points. Press on the line to add a point there; drag a point to move it in time (it stays between its neighbors) and level; hold Shift to change only the level. Right-click a point to delete it, or the line to reset the envelope. Each change is one undo step. The setting is saved with the editor settings.
- Levels run from +12 dB at the top of the clip to -60 dB at the bottom, and the bottom is silence. Between points the level moves in a straight line in dB; before the first point and after the last it holds. Playback and every mixdown follow it exactly (same plan and renderer).
- Points are stored on the clip in session samples from the clip start (the `envelopes` field that was already in the model). The volume envelope's id is the clip's id, so undo and redo give exactly the same session.
- Moving a clip moves its envelope with it. Trimming the left edge, splitting, and ripple deletes keep the points lined up with the audio, adding a point at each cut so the level there does not jump.
- Clip Pan envelopes and track automation come later.

Reason: the everyday use is ducking a music bed under a voice and riding a loud or quiet moment, without a separate effect.

## 2026-10-14: Libraries panel redesign: show libraries and bins

Decision (a livelier Libraries panel, organized per show with bins for Ads, Promos, Clips, Intro/Outro and so on):
- One library per show is the model (it already was). The panel header now shows the show: a colored badge with the show's initials, the name, and item and bin counts. Click the name to switch libraries or make a new one; click the badge to set the show color (Automatic picks a stable color from the name).
- Collections are called bins in the interface. The data stays `collections` in `library.json`, so nothing breaks. Bins gain an optional color, and libraries an optional show color (serde defaults; schema regenerated; no version bump).
- New show libraries start with podcast bins: Intro / Outro, Ads, Promos, Clips, Music Beds, Sound Effects, Legal Reads, each with its own color. Older libraries get them from Add Starter Bins (only the missing ones are added).
- Bins show as a strip of chips under the header, each with a color dot and a count, plus All. Click a chip to show that bin. Drag items from the list onto a chip to add them (the chip lights up). Right-click a chip to rename, recolor, move it left or right, or delete it. The + chip makes a new bin.
- Rows are taller and show a colored stripe (the item's color, else its first bin's), a play button on hover for a quick preview, the name with the item's bin dots and first tags under it, a small waveform of the whole file, and the duration.
- The details area under the list adds bin toggles: one chip per bin, on when the item is in it. The details area is capped so the list always keeps room.
- An item can still be in several bins, which suits audio better than one folder per file (a promo can also be an ad).

Reason: matches how a show's assets are actually used, and makes the panel scannable at a glance. Names and look are our own.

## 2026-10-14: Track stems, clip groups, timecode display

Decision, Track Stems (File > Export > Track Stems): pick a folder, and every track that has clips and is not muted is written as its own stereo WAV named "Session - 01 Track.wav". Each stem runs the full length of the session, so stems line up when imported anywhere else. A stem plays its track even if another track is soloed, with the track's volume, pan and the master volume applied. Progress shows per stem, and Cancel stops the rest.

Decision, clip groups (Clip > Group Clips, Ungroup Clips, and right-click): a new optional `group` field on clips (serde default, so no version bump and old files open unchanged; schema regenerated). Clicking any clip in a group selects the whole group, so moves, deletes, copies and exports act on all of it. Ctrl+click adds or removes a whole group. Split parts stay in their group. Pasted copies get a group of their own, apart from the originals. Grouped clips show "Grouped" in their title bar.

Decision, timecode display (View > Time Format > Timecode): hh:mm:ss:ff at a fixed 30 frames per second, non-drop. Used by the ruler, the clock, Time Info and the Markers panel. Clicking the clock cycles time, samples, timecode. A frame rate setting can come with video support if ever needed.

## 2026-10-14: Workspaces, panel tab menu, saved editor settings, Default left column

Decision, Default layout: the left column (Files over Voice Chain, Batch Convert, QuickPass) runs the full window height. Levels and Time Info sit only under the center and right columns. The right column and Time Info are both 270 px so their edges line up. Default layout version 5, so older saved Default arrangements are rebuilt once.

Decision, panel tabs: tabs have no close button. Each group's active tab shows a menu button with three stacked dots (chosen over a hamburger icon so it reads as "more options", not navigation) that opens the same menu as right-clicking the tab. The active tab's title is underlined, in the accent color when its group has focus and in a dim color otherwise; the old accent line along the top of the tab is gone. The left column is now 270 px, like the right column, so the three tabs in the left group fit with the button (Default layout version 6). Right-click a tab for Float Panel (or Dock Group when floating), Maximize Group or Restore Group, Close Panel, Close Other Panels in Group, and Close Group. Panels still turn on and off from the Window menu. This makes it hard to break the layout by accident. Panel-specific items (for example in the Voice Chain) will join this menu when those panels are built.

Decision, workspaces (options beyond Default, so the app covers a full editing job out of the box). This replaces the earlier "Default only" rule in CLAUDE.md.
- Built in: Default; Focused Edit (timeline, files, levels only); Mixing (Mixer up front, tall meters, files and markers); Voiceover (files and markers, libraries); Review and Export (markers and history, wide meters). Names and layouts are our own and describe the job.
- Each workspace remembers its arrangement on its own, saved as you change it, in `workspaces/<id>.alworkspace` in the app config folder. The single saved layout from before is moved there as Default on first run.
- Window > Workspace: pick one; Reset Workspace (a built-in one goes back to its starting layout, a user one to how it was last saved); Save Changes to Workspace (user ones); Save as New Workspace; Delete Workspace (user ones, asks first). Reset to Default Layout stays in the Window menu.
- The workspace in use is remembered between runs.
- Each built-in layout has a version; changing a preset later rebuilds saved arrangements of it once, like the Default.

Decision, editor settings: snapping, Automatic Crossfades and the time display format are saved in preferences and restored at start.

## 2026-10-14: Time selection, ripple delete, clipboard, mixdown, markers (P3, second part)

Decision, time selection:
- Drag on the ruler, on empty lane space, or below the last track to select a time range. A plain click moves the playhead and clears the range. The range is saved with the view (not an edit, so not undoable), shows in the ruler and lanes, and Time Info shows its start, end and duration.
- Uses: Zoom to Selection, Clip > Trim > To Time Selection, Ripple Delete, Add Range Marker, and mixdown or bounce of the range.

Ripple Delete (Edit menu):
- Selected Clips: deletes them and pulls later clips on the same track left by the space they took.
- Time Selection in Selected Track / in All Tracks: removes that stretch of time. Clips inside are deleted, clips across an edge are trimmed, a clip spanning the whole range is split around it, and everything after moves left. Fades at the cut are kept or shortened to fit.
- Gap Between Clips: closes the empty space under the playhead on the selected track.
- A locked clip in the way stops the edit with a message; nothing changes.

Cut, Copy and Paste work on clips in the Editor. Paste puts the clips at the playhead, with the topmost on the selected track, keeping their spacing and track layout. The clipboard is the app's own, not the system clipboard, and lasts until the app closes.

Snapping (Edit > Snapping): on/off, to clips, to the playhead, to markers, and to ruler ticks (off by default). Shift while dragging still turns snapping off for that drag. These settings are not saved between runs yet.

Automatic Crossfades (Clip > Fades, on by default): when a move or trim makes clips overlap on a track, the earlier one fades out and the later one fades in across the overlap. A clip fully inside another is left as a layer.

Clip Color (Clip menu and right-click): six palette colors or the track color. Stored as a palette token so it follows the theme.

Mixdown:
- File > Export > Session Mixdown or Time Selection writes a stereo WAV.
- Multitrack > Mix Down to New File does the same and adds the result to the Files panel.
- Multitrack > Bounce to New Track does the same and places the result on a new track where the range started.
- The mix uses the same plan and renderer as playback at the session rate, so it sounds like what plays. The file uses the session bit depth (16 or 24-bit with triangular dither, or 32-bit float). Progress shows in a dialog with Cancel, and a cancelled export deletes its partial file. If the mix went over full scale, the notice says so and by how much.
- MP3 and loudness checks come with Publish Package (P7) and the loudness scan (P5).

Markers:
- Edit > Markers > Add Cue Marker (at the playhead) and Add Range Marker (from the time selection).
- Markers show as flags on the ruler, with dashed lines through the lanes. Drag a flag to move it (it snaps), click it to jump there, right-click it to rename or delete. Right-click empty ruler to add one.
- The Markers panel (Window menu, not in the Default layout) lists every marker. Click to jump, double-click the name to rename, change Cue or Subclip, delete. Chapter, Ad Slot and Social Clip kinds arrive with their phases.
- Core ops: `add_markers`, `remove_markers`, `set_markers`, all undoable. Markers stay sorted by time.

Dev build speed: the audio crates (symphonia, rubato, FFT, cpal, hound) are optimized in dev builds and our own code is built at opt-level 1. Without this, a mixdown in a dev build ran at about twice real time, too slow for a full episode. Other crates stay unoptimized so builds stay quick.

Reason: these are the edits a podcast cut needs day to day (cut out a stretch across all tracks, close gaps, move bits around, export the episode). `hound` moved from a test-only dependency to a normal one for writing WAV, as the stack list already planned.

## 2026-10-14: Clip editing (P3, first part)

Decision: clips can now be edited in the Editor.
- Drag a clip to move it in time or onto another track. Several selected clips move together and keep their spacing. The group stops at the session start and at the first and last track.
- Drag the lower part of a clip's left or right edge to trim it. Trims stop at the ends of the file and never leave a clip shorter than 10 ms.
- Drag the small square handles in a clip's title bar to set the fade in and fade out. New fades start as an S-curve, which sounds smoothest on speech. The fades never overlap.
- Moves, trims and fade drags snap to the playhead, the session start and other clips' edges within 8 pixels. Hold Shift while dragging to turn snapping off.
- Clip menu and right-click: Split at Playhead (Ctrl+K) splits the selected clips under the playhead, or every clip under it when none are selected. Split All Clips at Playhead always splits every track. Also Trim Start/End to Playhead, Mute Clip, Lock Position, Clip Gain, Fade In and Fade Out. The Fade In and Fade Out menu items add a one-second fade or remove it.
- Locked clips cannot be moved or trimmed. Each drag is one undo step and shows its result live while dragging.

Split moves the fade out to the right part and keeps the fade in on the left part.

Not yet: cut, copy and paste of clips, ripple delete, time selection, crossfades, clip color, merging and grouping. Those come next in P3.

Reason: this is the minimum to cut a real episode. The edit rules are pure functions in `src/panels/editor/clip-edit.ts` with tests; the core enforces locks and file bounds again in `set_clips`, so a bad edit from any source is refused.

## 2026-10-14: File > Open takes audio files

Decision: File > Open shows sessions (.altaudio), .sesx sessions and audio files in one dialog, and several files can be picked. A session opens as before; a .sesx is imported. Audio files go into the Files panel and are selected. If the session has no clips yet, each audio file is also placed on its own track at the start and the view zooms to fit, so opening one recording is enough to start listening and editing. Picking two sessions at once opens the first and says which were skipped.

Reason: opening single audio files, not just sessions, is a basic need. The Waveform editor (single-file destructive editing) is not built yet, so the multitrack is where an opened file can be played. When the Waveform editor arrives, opening an audio file can open it there instead; that is a one-line change in `openAudio`.

Clip playback: clip gain, clip mute and fades (linear, logarithmic, cosine) now shape playback. Changing them restarts playback in place, like moving a clip.

## 2026-10-14: Overview strip, Session Properties, clip edit op

Decision: the Editor gets an overview strip above the ruler. It shows every clip in the session, the playhead, and a window over what the lanes show. Drag the window to scroll, drag its edges to zoom, click outside it to jump, wheel to zoom, double-click to fit.

Session Properties (Multitrack menu) edits name, sample rate and bit depth as one undoable step. Changing the rate converts every timeline position (clips, fades, envelopes, markers, view) so nothing moves in time. Undo restores the exact original sample values rather than converting back, so repeated changes never drift. Source files are never touched; they are converted as they play.

The core has a new `set_clips` op (move, move to another track, trim, gain, mute, lock, fades). Locked clips refuse moves and trims, and a trim can never reach past the end of its file. The editor does not use it yet; dragging, trimming and splitting clips come next (P3), along with clip gain and fades in playback.

Reason: P2 is complete apart from bus tracks and sends. Buses are deferred until effects arrive (P5), because without a Voice Chain on a bus they only group volume, while clip editing is what makes the editor usable for a real episode. Buses can move back up if they are needed sooner.

## 2026-10-14: Mixer and Time Info panels

Decision: the Mixer shows one strip per track plus a Master strip. Each strip has pan, Mute, Solo, a fader and a meter. The fader scale runs from -inf to +12 dB with a bend at -24 dB, so the useful range near 0 dB gets most of the travel. Meters use the same scale as the fader so a level lines up with the fader mark. Master volume is a new `set_master_volume` op, so it is undoable and fader drags merge into one undo step like track faders. Double-click a fader to reset it to 0 dB.

Time Info shows the playhead, and Start, End and Duration of the selected clips (their combined span). The footer shows session length, sample rate, bit depth and a size estimate for a stereo mixdown at the session bit depth.

Reason: the Mixer is the second tab of the center group in the Default layout, and Time Info is in Default under Levels. Both read state the core already has, so they cost little and make the Default layout complete apart from P5 panels. Time selection ranges arrive with editing tools; until then the selected clips define the range.

## 2026-10-14: Playback (P2 start)

Decision:
- Space plays and pauses from the playhead. Stop returns the playhead to where playback started; Pause leaves it where playback reached. Go to Start is a toolbar button (no new shortcut). Clicking the ruler or an empty lane while playing jumps playback there.
- Three threads, per the real-time rule: a render thread decodes and resamples every track's clips into one lock-free ring buffer per track, about half a second ahead; the device callback only mixes those rings with the current gains and writes peak meters. It never allocates, locks, or touches the disk.
- Volume, pan, mute and solo apply on the next audio buffer, from atomics. Edits that move audio in time (adding, removing or moving clips or tracks) restart playback from the same spot.
- Pan is a balance control: centered plays both sides at full level and panning turns the far side down, so a centered voice plays at exactly the fader's level. Solo: if any track is soloed, only soloed tracks play; mute always wins.
- Seeking is sample-accurate, and resampled files have the resampler's delay removed, so a 44.1 kHz file and a 48 kHz file line up exactly (tested with a click track).
- Track headers: drag the dB or pan value left or right (Shift for fine steps), double-click to reset. A drag is one undo step. Small level meters show under each track while playing.
- The Levels panel shows the output level, left and right, from -60 dB to 0 dB with peak hold and over lights.
- The editor pages along to keep the playhead in view while playing. The playhead is saved in the session only when playback stops, so playing never marks the session as changed.
- Previewing a file in Drive Manager or Libraries stops session playback, and the reverse.

Reason: P2 playback per CLAUDE.md (cpal, rtrb, real-time thread rules).

## 2026-10-14: .sesx session import (task 11)

Decision:
- File > Import Session (.sesx) reads the user's own `.sesx` multitrack sessions for interoperability. It reads only the XML structure seen in real files; no other product's code or design is reused.
- Imported: tracks (name, mono or stereo, height, mute, solo, arm, volume as dB, pan), clips (position, source range, fade lengths and log or linear shape, lock), and media files. Media is looked for relative to the `.sesx` first, then at the stored absolute path, then by name beside the `.sesx`.
- Not imported, with a count in the status bar: effects (track, clip and master), buses, sends and automation. Effects are rebuilt in Voice Chain in P5.
- Missing media gets a placeholder so its clips still show where they were, marked "Media offline". The Files panel lists the file as an error.
- The import becomes a new, unsaved session named after the `.sesx` file. Saving it writes an `.altaudio`; the `.sesx` is never changed.
- Checked against two real production sessions: every track, clip and position came across. Real sessions live in `samples/`, which git ignores; `cargo test --test sesx_samples -- --ignored --nocapture` imports whatever is there.

Reason: CLAUDE.md task 11 and the P1 exit gate.

## 2026-10-14: Session save, open, autosave and recovery (task 12)

Decision:
- File > New (Ctrl+N), Open (Ctrl+O), Open Recent (last 10), Save (Ctrl+S), Save As, Save All, Close and Close All. With one session open, Save All and Close All act on it.
- New, Open, Close, Import Session and Exit (including the window close button) ask Save, Don't Save or Cancel when there are unsaved changes. The window title shows the session name with a star while unsaved.
- Sessions save atomically (temporary file, then rename). Each media file is stored with its absolute path and, when on the same drive, a path relative to the session file. Open tries the relative path first, so a show folder can move between drives or machines. Files not found show as errors in the Files panel.
- Files in the Files panel that no clip uses yet are saved with the session too, and so are the open libraries, which reopen with it.
- Autosave writes a recovery copy to the app data folder every 2 minutes while there are unsaved changes. Saving or closing cleanly deletes it. A copy found at startup means the app did not close normally: the user can Recover (it opens as unsaved work, saving to the original file) or Discard.
- Save As, or the first save of an untitled session, names the session after its file.
- The dock never saves or restores an empty layout. That could only come from a failed load and would leave the next start blank.

Reason: CLAUDE.md task 12 and charter rule 7 (work is never lost).

## 2026-10-14: Multitrack view (task 10)

Decision:
- The core owns the open session. The interface sends each edit as an op (add track, move track, set track, add media, add clips, remove clips, and a batch of these). The core applies it and returns the new session plus the op that reverses it, which goes on the undo stack. New ids come from the interface, so redo recreates exactly the same tracks and clips. A batch that fails part way changes nothing. This puts the session where the P2 audio engine will need it.
- Waveforms come from a peak cache (`.alpk`) in the app cache folder: min and max of every 256, 4,096 and 65,536 frames, 16-bit, at most two channels. It is a rebuildable cache, so it is compact binary with a version header, not JSON. It is rebuilt when the source file's size or modified time changes, or when it is damaged. Building runs off the UI thread and shows a percentage on the clip.
- Dropping files, Files panel rows, Drive Manager rows or library items onto a track adds them as clips one after another, starting where they were dropped. Dropping below the last track makes a new track. Files are added to the Files panel too. That is two undo steps (Import, then Add Clips).
- New sessions are 48 kHz. Clips from files at other rates are placed at their correct length; the engine resamples in P2.
- Track headers: name (double-click to rename), mono or stereo badge, Mute, Solo, Arm. Volume and pan are shown but adjusted in P2. Drag a header to reorder; every change is undoable.
- Zoom: Ctrl+= and Ctrl+- (the approved shortcuts), Ctrl+mouse wheel around the pointer, Zoom to Session. Shift+wheel scrolls sideways. Click or drag on the ruler to move the playhead.
- Delete in the Editor deletes selected clips. Removing a file from the Files panel keeps any file a clip still uses.
- Not in P1, now marked with their phase in the menus: markers, snapping, timecode display and time selection (P3), bus tracks (P2).
- The task 3 dev sandbox is gone; the Editor panel shows the real view in every build.

Reason: CLAUDE.md task 10, and the architecture rule that the core owns state the engine will play.

## 2026-10-14: Libraries panel (task 9)

Decision:
- A library is a folder with `library.json` and an `audio/` folder. New libraries go in Documents\Altered Audio\Libraries\<name> (home folder if there is no Documents folder). Any folder can be opened as a library.
- Files are copied in by default; the original path is kept. Files already inside the library folder are used in place.
- Every library change (add, remove, rename, tags, notes, color, collections) is one undo step on the same undo stack as the session. Undo saves the previous `library.json`. Removing an item never deletes its file, so undo always works. Library menu > Clean Up Library Folder deletes copied files that no item uses.
- Which libraries are open, and which one is showing, are remembered between runs (preferences, not undo).
- Space previews, Enter or double-click adds to the session, drag adds to the Files panel or onto a track. Drop files on the panel to copy them in. Delete removes selected items when the Libraries panel was clicked last.
- Collections are named, ordered groups; an item can be in several.

Reason: CLAUDE.md task 9. Network-wide libraries (P7) use the existing `scope` field.

## 2026-10-14: Drive Manager and preview playback (task 8)

Decision:
- Drive Manager lists drives (Windows drive letters; on Linux and macOS the root, mounted drives and /Volumes), plus Home, Desktop, Documents, Music and Downloads. Back, Forward, Up, a typed path, a filter box, Audio only, and Auto-play. Hidden and system files are skipped.
- Preview plays through the core, not the web view, to keep the rule that the interface never touches audio. A preview thread decodes and resamples to the output device's rate (rubato) into a lock-free ring buffer (rtrb); the device callback (cpal) only copies. This is the first piece of the P2 engine.
- The selected file's format, codec, rate, channels and length show at the bottom with a progress bar; click the bar to preview from that point.
- Double-click a folder to open it, a file to add it to the session; drag rows onto the Files panel or a track.
- Small interface settings (last folder, toggles, open libraries) are saved in `preferences.alprefs` in the app config folder. Never browser storage.
- Linux builds now need the ALSA headers (`libasound2-dev`) for audio output.

Reason: CLAUDE.md task 8.

## 2026-10-13: Platform priority

Decision: Windows first, Linux (Zorin OS) second, macOS third. Only Windows ships for now, but the code stays portable from the start instead of being ported later.

- Interface differences by OS live in `src/lib/platform.ts`: the reveal command is Show in Explorer, Show in File Manager, or Reveal in Finder, and help text names the right file manager.
- Duplicate detection in the Files panel ignores path case on Windows and macOS only. On Linux `Intro.wav` and `intro.wav` are different files.
- Claude's workspace is Linux, so every Rust test and live check already runs there. Windows is checked by the project lead.
- Left for later, when a second platform becomes a release target: Linux and macOS installers, Linux CI, Cmd for Ctrl on macOS, macOS signing and notarizing, and per-platform checks of audio devices (P2) and GPU features (P7).

Reason: the project lead may move to Zorin OS. Keeping platform differences in one place now costs little; finding Windows assumptions across the codebase later costs a lot.

## 2026-10-13: QuickPass scope

QuickPass is specified by behavior:

- QuickPass scans selected clips (or all clips) for problems and lists each finding with its start time, length, and clip.
- Silence finding uses two rules: audio below a quiet level for at least a set time is silence; audio above a content level for at least a set time is speech. Both levels and times are user settings, with a "suggest levels" button that measures the clip.
- For each finding, or all at once, the user can Remove it (ripple, so the multitrack stays in sync), Split the clip there and leave the pieces selected for review, or Mark it.
- Later scan types (clicks, clipping) follow the same find, review, act pattern.
- The QuickPass presets in CLAUDE.md (Trim Long Silent Pauses, Mark Non-Silent Areas, and the rest) are starting settings for these scans.

Phase: not assigned yet. It needs the multitrack view, clip splitting (P3), and level analysis, so P5 at the earliest.

## 2026-10-13: Files panel (task 7)

Decision:
- Decoding uses symphonia 0.6.1 (MPL-2.0, GPL-compatible). 0.6 has been the stable line since May 2026 and 0.5 is in maintenance. Only the formats we need are enabled: WAV, AIFF, MP3, FLAC, M4A/MP4 (AAC-LC and ALAC), raw AAC, and OGG Vorbis.
- Import measures each file in the core: format, codec, sample rate, channels, bit depth (lossless only), size, and exact length in samples. The first packet is decoded, so a file we cannot play (for example HE-AAC, which symphonia does not decode) fails at import with a clear reason, not later on the timeline. Files with no stored length (some MP3s) are measured by reading packet headers to the end, without decoding.
- Up to 4 files are measured at once, off the UI thread. Each row shows Reading, Ready, or Error, and the error text is in the row tooltip. Right-click > Try Reading Again re-checks a failed file.
- Importing and removing files are undoable commands. Measuring is not an edit, so Undo then Redo of an import does not re-read the files.
- Dropping files or folders from Explorer: the window's native drop events are hit-tested against panels. A drop that misses every panel still imports into the Files panel, so a drop never silently does nothing. Folders are searched up to 8 levels deep, at most 5,000 files per drop. Files already in the list are skipped (paths compared without case).
- The Import dialogs open from the core (tauri-plugin-dialog used from Rust), so the interface gets no file system permissions.
- Delete, Select All, Deselect All, and Find act on the area the user last clicked. For now that is only the Files panel; the Multitrack view joins in task 10. Menus show why these items are disabled elsewhere.
- Columns: Name, Status, Duration always fit at the 250px default; Sample Rate and Channels appear when the panel is wider. Click a header to sort, click again to reverse, a third time for import order.
- Close Unused Media waits for task 10, since "unused" needs clips to exist.

Reason: CLAUDE.md task 7, plus the charter rules on undo, drag and drop everywhere, and never freezing the window.

## 2026-10-13: Keymap (task 6)

Decision:
- Shipped shortcuts live in `src/keymap/default.json` (format `altaudio.keymap`, version 1). It holds exactly the 15 approved shortcuts; a test fails on any other.
- The user's changes are saved to `keymap.alkeys` in the app config folder, holding only what differs from the defaults. New defaults in later versions still reach users who never changed that command. A cleared command is stored as an empty list.
- Chords have one spelling: modifiers in the order Ctrl, Alt, Shift, then the key (`Ctrl+Shift+K`).
- Edit > Keyboard Shortcuts (also Help > Keyboard Shortcuts) lists every menu command with search, Change, Clear, and Default per row, and Reset All. Taking a shortcut that another command holds asks first, then moves it.
- One dispatcher routes key presses to actions. It ignores keys while typing in a text field or while a dialog is open. Held keys repeat only for Undo, Redo, and Zoom; Space fires once per press.
- Release builds block the webview's reload, print, and page-search keys (F5, Ctrl+R, Ctrl+P and similar) so a stray key cannot discard work. This adds no shortcuts.
- Menus and tooltips read shortcut text from the keymap, so a change shows everywhere at once.

Reason: CLAUDE.md asks for an editable keymap as data with only approved defaults. Saving only differences keeps upgrades clean.

## 2026-10-13: Window menu and Help links

Decision:
- The Window menu is back: every panel toggle, Reset to Default Layout, Minimize. View keeps editors, zoom, time format, envelopes, and the Podcast and Voiceover modes. Menu ids `view.panel.*` became `window.panel.*`; `session.*` became `multitrack.*`.
- Help gets The GCRN Website (https://thegcrn.com/) and GCRN Plugins, one link to a single plugins page. The plugins page is not live, so that item is disabled until `LINKS.plugins` in `src/app/links.ts` is set.
- Links open in the default browser through the core, which only allows https links on thegcrn.com.

Reason: Window is where long-time editors look for panels. One plugins page means new plugins never need an app update.

## 2026-10-13: Drive Manager

Decision: the drive and folder browser panel is Drive Manager (`drive-manager`), replacing File Manager. Default layout version 4.

Reason: says exactly what it browses, and reads clearly apart from the Files panel.

## 2026-10-13: Multitrack menu name

Decision: the track and mixdown menu is called Multitrack, not Session. Opens with Alt+M.

Reason: Multitrack is the industry-standard term. The menu contents and the rest of the tree stay our own.

## 2026-10-13: Name cleanup

Decision:
- Panels: the selection readout panel is now Time Info (`time-info`); the media browsing panel is now File Manager (`file-manager`). Default layout version bumped to 3 so saved layouts with the old ids are replaced.
- Effects get our own names: Noise Reducer, Speech Leveler, Dynamics, Limiter, Click Repair, Breath Gate. The full map of old names is kept in the private naming map.
- Presets: four EQ presets become Hum Cut 60 Hz (US), Hum Cut 50 Hz (Europe), Rumble Cut, Subsonic Cut, rebuilt with our own settings. Other preset names stay for now.
- Tests now fail on any of the old names in menus or panel titles.

Reason: CLAUDE.md bans other products' panel and feature names in the UI. The remaining preset names were judged to be ordinary descriptions.

## 2026-10-13: Default layout v2

Decision: right column is Audio FX Panel over Libraries. The selection readout (now Time Info) moves to the right end of the bottom strip, beside Levels. Saved layouts now record which Default they came from (`defaultVersion`); a layout from an older Default is replaced on next launch.

Reason: Libraries needed a home in Default, and pre-release it is better to roll everyone onto the new Default than keep stale arrangements.

## 2026-10-13: Menu bar (task 5)

Decision:
- Seven menus: File, Edit, Session, Clip, Effects, View, Help, grouped around the common podcast workflow. The Window menu is folded into View > Panels; the saved-commands panel is dropped; track and mixdown commands live under Session.
- Items use our names (Voice Chain, Loudness Match, QuickPass, Settings) and our own effect groupings. Branded, video, CD, account and extension items are left out.
- Podcast workflow items sit at their workflow step: Speaker Balance, Transcribe Session and Remove Filler Words under Session; Check Loudness and Publish Package under File; Remove Music under Clip.
- Menus are data (`src/app/menus/model.ts`). Every item names an action id; actions live in one registry (`src/app/actions.ts`). No registered action means disabled, with the reason as a tooltip.
- Only the shortcuts approved in CLAUDE.md appear, each once. A test enforces this and bans other products' feature names.
- Keyboard: Alt plus the underlined letter opens a menu (standard Windows menu access, not an app shortcut). Arrows, Enter and Escape work as in any Windows menu.
- Built now: Undo and Redo (labels name the edit), every panel toggle, Reset to Default Layout, About, Exit. Exit gets an unsaved-changes prompt in task 12.
- The menu bar is drawn in the app, not a native Windows menu, so it matches the dark theme.

Reason: CLAUDE.md allows similar menus but not an identical tree. Folding Window into View and merging the track menus gives a structure experienced editors find their way around in seconds, with a tree of our own.

## 2026-10-13: Panel dragging uses pointer events

Decision: the docking layout uses `dndStrategy: "pointer"`. Floating groups get a Dock button and a Dock item on the tab right-click menu, which return their panels to their home spot. App drag and drop (Files to tracks, clips between tracks) must also use pointer events, not HTML5 drag and drop.

Reason: panels could be dragged out but not back in. On Windows, the webview swallows HTML5 drop events while Tauri's native file drop is on, and native file drop is the only way to get real file paths from Explorer (task 7). Pointer events work with it on.

## 2026-10-13: Visual direction: panel framing and value color

Decision: each panel group now sits in its own frame with a 2px dark gutter. Added a `--value` token (light blue) for editable numbers and actionable names, used from task 7 on. Kept our own tab treatment, icons, and header layout. No per-panel menu icon in the header.

Reason: density, framing, and color for editable values are common to pro audio tools and help in long sessions. Header chrome stays our own, per the originality rules in CLAUDE.md.

## 2026-10-13: Docking shell and Default layout

Decision:
- Use the `dockview` package, not `dockview-core` directly. In version 8, `dockview-core` is marked internal and ships no stylesheet; `dockview` re-exports it with the CSS.
- Default layout: left column 250px (Files on top; Voice Chain, Batch Convert, QuickPass tabbed below), center Editor and Mixer tabs, right column 200px (Audio FX Panel over the selection readout, now Time Info), Levels across the full bottom.
- Markers and History are not in Default and can be turned on. The saved-commands panel is dropped.
- One Default layout, no named presets. The user's arrangement saves itself to `workspace.alworkspace` in the app config folder (never browser storage), 400 ms after the last change. Reset to Default deletes it and rebuilds.
- A damaged or unreadable saved layout falls back to Default with a message instead of blocking startup.
- Turned-off panels come back to a fixed home spot (registry `home` list), falling back to a window edge.
- Layout changes are not session edits and do not go through the undo stack.
- Every group has float and maximize buttons (shown on hover) and a right-click tab menu with the same plus close.
- Panels without features show a plain "not built yet" note naming the task or phase, never fake controls.
- A temporary Panels menu in the top bar holds the toggles and Reset until the View menu exists (task 5).

Reason: puts the common podcast workflow in reach, keeps the theme ours (compact strips, our tokens), and keeps layout state out of the session file.

## 2026-10-13: CLAUDE.md layout conflict resolved

Decision: removed a second, conflicting "Confirmed Default workspace layout" section. Kept the original Default. Dropped the saved-commands panel from the task 5 menu list.

Reason: two Default layouts contradicted each other, and the second one conflicted with the original-design guardrail.

## 2026-10-13: CLAUDE.md rewrite applied to finished work

Decision: aligned tasks 1 to 3 with the revised CLAUDE.md.
- Redo is Ctrl+Y. Ctrl+Shift+Z is not bound (standard Windows shortcuts only).
- `HostProfile` now means a host voice profile (for example Host, Co-host, Guest): gate settings plus the Voice Chain preset to load. It no longer describes podcast hosting export settings. Tracks reference it with `host_profile_id`.
- New scaffolding fields: `Track.role` (Standard or Guest, P5), `MarkerKind::AdSlot` with `contracted_duration` (P5), `MarkerKind::SocialClip` (P7), `Library.scope` (Show or Network, P7), `Session.modes` (podcast and voiceover toggles), `EpisodeMetadata.show_id` (P6).
- Comments use the app's own names (for example Voice Chain).
- The v1 fixtures and schemas were regenerated.

Reason: the format is unreleased and no real files exist yet, so changing v1 costs nothing now. After the first real session is saved, v1 is frozen and changes need a migration.

## 2026-10-13: Undo stack design

Decision:
- Commands may be async. The stack runs every operation strictly one at a time, in call order, so fast repeated Ctrl+Z presses are safe while edits round-trip to the Rust core.
- A command that throws is not recorded. A failed undo leaves the position unchanged.
- Rapid edits of the same kind (fader drags) merge into one entry when they arrive within 750 ms and the command allows it.
- `CompoundCommand` records several changes as one entry and rolls back on partial failure.
- Commands must never call the stack. Synchronous nesting is caught; async nesting would deadlock, so it is a rule.
- History tracks a save point, so the stack can tell whether the session has unsaved changes. Task 12 uses this for the title bar and autosave.
- One stack per open document. In P1 that is a single `sessionHistory`.
- Limit: 1000 entries, oldest dropped first.

Reason: the core is async from day one, and losing or reordering an undo is the worst bug an editor can have.

## 2026-10-13: Dev sandbox

Decision: a dev-only sandbox (`src/app/dev/CommandSandbox.svelte`) drives the undo stack until real editing exists. It is compiled out of release builds. Delete it when the multitrack view (task 10) produces real commands.

Reason: the History panel has nothing to show until there are edits, and the project lead needs to try it to sign off.

## 2026-10-13: Hard-coded undo and redo shortcuts

Decision: Ctrl+Z and Ctrl+Y are bound in `src/app/shortcuts.ts` until the keymap (task 6) replaces them. They are skipped while typing in a text field so the field's own undo works.

## 2026-10-11: File format ids and extensions

Decision: every file has a `format` header (`altaudio.session`, `altaudio.library`, `altaudio.template`, `altaudio.workspace`, `altaudio.host_profile`) next to `schema_version`. Extensions: `.altaudio`, `library.json`, `.altemplate`, `.alworkspace`, `.alhost` (host voice profile).

Reason: the header lets the loader reject the wrong kind of file with a clear message instead of a confusing parse error. `.altemplate` and `.alhost` are new names, not set in CLAUDE.md, and can still change before any are shipped.

## 2026-10-11: Versioning rules

Decision:
- Adding a field with a serde default is not a version bump.
- Renaming, removing, or changing the meaning of a field is. It needs a `migrate_step`, a new fixture folder, and a passing fixture test.
- Files from a newer app are refused with a message to update, never half-loaded.
- A template's nested session is upgraded with the session migrations.

Reason: "never break an old file." Frozen fixtures in `src-tauri/tests/fixtures/v<N>/` are opened by every build, so a breaking change fails CI.

## 2026-10-11: Schemas generated from Rust, checked in CI

Decision: JSON Schemas in `/schemas` are generated from the Rust types with `schemars`. A test fails when they drift. Regenerate with `UPDATE_SCHEMAS=1 cargo test --test schemas`.

Reason: one source of truth. The schemas can never silently disagree with the code. Field doc comments become schema descriptions.

## 2026-10-11: Units in the session model

Decision: timeline positions are integer samples at the session rate. Clip source offsets are integer samples at the source file's own rate. Levels in dB. Pan from -100 to 100.

Reason: integers avoid float drift on long sessions and match how `.sesx` stores positions. Keeping source offsets in the file's own rate means a 44.1 kHz file in a 48 kHz session is never rounded.

## 2026-10-11: Atomic saves

Decision: saves write a `.tmp` file beside the target, sync it, then rename it over the target.

Reason: a crash or power cut during save leaves the old file intact. Needed for autosave (P1 task 12).

## 2026-10-11: Session validation reports, it does not refuse

Decision: `session::validate` returns a list of issues (missing media, unknown bus, routing loops, duplicate ids). Loading does not fail on them.

Reason: a slightly broken session should still open so the user can fix it. Refusing to open work is worse than showing a warning.

## 2026-10-11: Model placement

Decision: `HostProfile` and `Workspace` live in `session/` beside the session model. (`export/` is P3+ and gets no code in P1.) `Library`, `LibraryItem` and `Template` live in `library/`. A `MediaRef` type (source file reference) was added, though not in the CLAUDE.md type list, because clips need to point at a file.

Reason: `session/` owns all file formats and migrations. `export/` is P3+ and gets no code in P1.

## 2026-10-11: TypeScript 5.9, not 7

Decision: pin TypeScript `~5.9`.

Reason: TypeScript 7 (the native rewrite) is out, but `svelte-check` supports 5 and 6 only. Proven over new.

## 2026-10-11: License identifier

Decision: `GPL-3.0-or-later`.

Reason: the FSF's recommended form, and it stays compatible with GPL code from Audacity. Change to `GPL-3.0-only` before the first public release if preferred. It is one find-and-replace plus the header check.

## 2026-10-11: Rust lib name

Decision: the crate builds a library named `altered_audio_editor_lib` plus the app binary.

Reason: the standard Tauri 2 layout. It lets integration tests in `src-tauri/tests` use the core without starting a window, and avoids a Windows name clash between the lib and the exe.

## Windows distribution

The installer (NSIS, per-user or all-users, shows the GPL license) is the main download. A portable zip with the bare exe is attached beside it. Builds run in GitHub Actions on version tags and make a draft release, so nothing goes public until it's tested. MSI is skipped unless someone asks. Files are unsigned until code signing is set up.

## 2026-10-15: Empty-track right-click menu, nested menus, Batch Convert templates

Decision:

- The right-click menu on empty lane space is the session menu: Paste, Ripple Delete, Select, Groups, Track, Insert, Marker, Mixdown Session to New File, Export Mixdown, Bounce to New Track, Session Properties. Clips keep their own menu (clip items plus track items). Right-clicking empty space keeps any chosen clips and makes that lane the chosen track.
- Context menus now nest. A row with an arrow opens its submenu on hover or with the Right arrow key, and Left arrow or Escape closes it. A submenu flips to the left or lifts up when it would run off screen.
- Every entry in the menu runs a registered app action, so it cannot drift from the menu bar. Entries whose feature is not built show disabled, with the reason as a tooltip.
- New Select actions, also added to Edit > Select: Next Clip in Selected Track, Clips to End of Track, Clips to Start of Session, Clips to End of Session, Select Current View Time, Select All Time, Select Time Around Selected Clips, Clear Time Selection. The "clips to" actions count from the earliest chosen clip, or from the playhead when none is chosen. The rules are in `src/app/clip-select.ts` with tests.
- No keyboard shortcuts were added.
- Batch Convert naming is one template with tokens: {name}, {number} (position in the list, at least two digits), {date} (2026-10-15) and {format}. Buttons under the field insert a token at the cursor. Settings saved with the old Before and After boxes become a template on load, so nothing is lost.

Reason: the empty-lane menu is where an editor reaches for track and session actions without going to the menu bar. Templates replace two boxes with something that can number a batch and date-stamp it, which is what people do with a folder of call recordings.

## 2026-10-15: QuickPass silence scan built

QuickPass now scans the selected clips (or every clip on a track that plays when none are selected) and lists silences. The Rust side only measures: `quickpass_levels` renders each clip as it plays (gain and Voice Chain included) and returns its level in 10 ms windows. All the rules live in `src/panels/quickpass/quickpass.ts` so they are unit-tested: a stretch below the quiet level for at least the set time is a silence, and a burst of sound shorter than the speech time does not break it. Suggest Levels sets the two levels from the clip's own noise floor and speaking level. Each finding can be removed (gap closes), split out (the piece is left selected), or marked (range marker); "all" versions of each make a single undo step. Remove closes the gap on the clip's own track by default; a setting closes it on all tracks to keep them in sync. Clicking a row moves the playhead. Settings are remembered. No new shortcuts. Click and clipping scans follow the same find, review, act pattern later. Presets (Trim Long Silent Pauses and so on) wait for P5 with the rest of the effects presets.

## 2026-10-15: first release is v0.1.1

The repository history was rewritten into one clean commit before going public, and the first `v0.1.0` tag and draft pointed at the old history. The first public release is therefore v0.1.1, so the number alone shows it came from the clean history. Version lives in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`; change all five together.

## 2026-10-16: QuickPass layout, key and transport fixes

QuickPass now has a Scan view and a Settings view with a preset list (built-in plus the user's own, saved with a name). Settings hold the silence level and time, the sound level and time, Find Levels, and what fixing does: remove (keeping an optional short pause so speech does not run together, default 100 ms), split, or mark. The Scan view lists findings; click selects a row and moves the playhead, Ctrl+click adds to the selection, Fix and Fix All act, Clear Fixed tidies the list. Fixes: clicking away from a text field (such as the Libraries search box) now releases keyboard focus, so Space plays and pauses again; Stop also ends a recording in progress; the transport buttons sit in the centre of the bar with zoom on the right.

## 2026-10-17: community feature requests

Seven groups of requests came in from the project's Discord group and are filed in `docs/backlog.md`. Decisions made while filing them:

- **VST2 is not planned.** The VST2 SDK can no longer be licensed, so a public VST2 host is a legal risk. VST3 (MIT licensed SDK) comes first, CLAP possibly after.
- **No OpenAPI spec.** The app has no HTTP interface and the privacy rule forbids network access. Schemas and generated TypeScript types cover the same need.
- **MCP endpoint is local and off by default.** It uses a companion program and a local pipe, starts read-only, and every assistant edit is a labelled, undoable History entry.
- **Scripting is sandboxed with no file, network or process access**, calling the same actions as the menus.
- **Alt+Drag for time stretch is not assigned.** New shortcuts need the project lead's approval. Until then stretch is a menu command.
- **Broadcast loudness gets two presets**, -23 LUFS (European) and -24 LUFS (US), instead of picking one.
- **Silence and breath management uses volume envelopes**, not splits, so clips stay whole.
- **Video preview needs its own decoder path.** A web view cannot step exact frames.
- ASIO, FFmpeg, Rubber Band and any plugin SDK need their licence terms checked and credited before they ship.

## 2026-10-17: loudness regions, Loudness Match, Turn Down, low-bit WAV, chapter export

- **Broadcast loudness by region.** The project lead chose the US standard as the default with a region setting. Regions: United States and Canada (ATSC A/85, -24 LUFS, -2 dBTP), Europe and the UK (EBU R128, -23, -1), Australia and New Zealand (OP-59, -24, -2), Japan (ARIB TR-B32, -24, -1). The region is saved once and used by Check Loudness and Loudness Match. A note reminds users that a broadcaster's own delivery spec wins.
- **Loudness Match (Effects menu).** Measures each selected clip on its own, or each track as a whole when nothing is selected, and changes clip gain to reach the target. Track mode moves every clip on the track by the same amount so the balance inside a track stays. Measurement leaves out track volume, pan and the Master (`clip_only`), since the change goes into clip gain. Rows that would push true peaks over the target's limit start unticked; we do not limit yet (that arrives with effects). One undoable step.
- **QuickPass Turn Down.** A fourth fix action lowers each silence with clip volume envelope points (10 ms ramps, default -15 dB) instead of cutting. Existing envelope levels either side are kept, and the dip adds to whatever the envelope was. Clips stay whole. New preset: Turn Down Breaths and Pauses. Breath detection here is level based, so it catches breaths quieter than speech; a dedicated breath detector waits for the effects engine.
- **WAV 8-bit and 4-bit IMA ADPCM export.** 8-bit is dithered PCM through the existing WAV writer. ADPCM is our own encoder for the public IMA format (block sizes 256, 512 or 1024 bytes per channel by sample rate, `fact` chunk with the exact length). Both read back through our own decoder and through FFmpeg in tests. The export dialog notes that both are lossy or noisy and not for masters. These depths only come from an explicit choice; "keep the source's depth" still rounds up to 16, 24 or 32.
- **Chapter markers and export.** Chapter becomes a marker type you can pick, and Edit > Markers > Add Chapter Marker works (at the playhead, or over the time selection). File > Export > Markers and Chapters writes a timestamp list, CSV or podcast chapters JSON (version 1.2.0 of the open format), with a preview and warnings for lists video sites reject (must start at 0:00, at least three chapters, each 10 seconds or more). Embedding chapters inside MP3 files waits for the metadata work (P7). A new `export_text` command writes only to the path the user picks in the save dialog.

## 2026-10-17: QuickPass scan types and the podcast clean-up preset

QuickPass now scans for three things, picked from "Scan for", each with its own presets and settings:

- **Silence**, as before. New built-in preset **Clean Up Podcast Interview**: cuts silences and the junk inside them in one pass. A cough, bump or click shorter than the speech time (300 ms) that sits inside a silence is treated as part of it and removed with it, keeping a 150 ms pause. Starting values: silence below -50 dB for 600 ms, sound above -40 dB. The project lead's own settings from his old workflow can replace these numbers once he has them.
- **Short noises** (coughs, bumps, clicks between sentences): sound above a level, no longer than a set time, with quiet on both sides so short words are never caught. Presets: Turn Down Coughs and Bumps (envelope dip of 30 dB, clips stay whole) and Cut Coughs and Bumps.
- **Clipping**: spots where samples reach a level near full scale (default -0.3 dBFS), found from per-window peaks the core now reports. Mark or split only, since cutting does not fix clipping.

Clicks inside speech need spectral analysis and wait for the effects engine; the option shows disabled with that reason. Preset "Remove Digital Dropouts" is renamed "Trim Short Digital Dropouts" to match the planned preset list.

## 2026-10-17: time stretch, Levels for every kind of playback, waveform checks

- **Time stretch (Clip > Time Stretch, Clip > Remove Stretch).** A clip gains an optional `stretch` field (`ratio`, `keep_pitch`); old files open unchanged and the session schema is regenerated. The clip keeps its source range; only how long it plays changes. Range 25% to 400%, with a note past 25% either way for speech. Two modes: keep pitch, using our own waveform-similarity overlap-add engine (`src-tauri/src/dsp/stretch.rs`, 30 ms frames, similarity search of a quarter frame; tests check length within 2% and pitch within a few hertz), and tape style, which reads the file as if its rate were scaled so pitch moves with speed. Envelope points and fades scale with the clip so they stay on the same words. Quick buttons fit 15, 30 and 60 second slots, and Fit to Time Selection moves the clip into the selection. Looped clips cannot stretch (and stretched clips cannot loop) to keep the model simple. Splits, trims and slips of a stretched clip account for the stretch. The clip title shows the percentage. The drag-the-edge gesture waits for the project lead's approval of a shortcut; until then it is the menu command. If our engine is not good enough on real voices, Rubber Band (GPL) is the fallback.
- **Levels panel follows what is playing.** It only listened to the multitrack transport, so it stayed flat while the Waveform editor or a Files, Drive Manager or Libraries preview played. It now follows the multitrack session, the Waveform editor and previews (the preview player reports peaks), showing whichever played last.
- **Waveform checks.** A clip whose waveform is ready but empty now says "No waveform data" instead of staying blank, raw peak bytes are accepted in any form the webview hands back, and a library row whose waveform failed shows a red line with the reason on hover. These are to pin down the blank MP3 waveforms reported on Windows, which do not reproduce on Linux.

## 2026-10-17: a version for every release

The project lead asked for the version to go up with every update. Each release bumps the last number (0.1.1, 0.1.2, ...); a finished phase bumps the middle number; 1.0.0 marks the first version stable for daily podcast work. Bumps are per release, not per commit. `npm run bump` changes all five files that carry the version at once, without a byte order mark, so they never disagree. The next release is 0.1.2.

## 2026-10-17: Alt+drag stretches a clip

Approved by the project lead. Holding Alt while grabbing a clip's left or right edge stretches the clip instead of trimming it: the audio range stays the same and the clip plays longer or shorter. The edge snaps like a trim (Shift turns snapping off), a label shows the percentage while dragging, and the clip keeps its pitch mode (keep pitch unless it was already set to tape style). Looped and locked clips trim as usual. One clip at a time.

## 2026-10-17: AAC export, Generate, Insert Silence, punch-in, recording offset, marker colors and notes

- **AAC (M4A) export uses the encoder built into the operating system.** The project lead's call: a bundled encoder only where a system has none. Windows has one in Media Foundation (every Windows 10 and 11 install), so AAC export works there at 44.1 or 48 kHz, mono or stereo, 96 to 192 kbps. The calls need `unsafe`, which the app crate forbids, so they live in a separate small crate (`src-tauri/os-aac`) and the app keeps its no-unsafe rule. macOS will use Audio Toolbox the same way; Linux has no system AAC encoder, so it will need a bundled one (FFmpeg's native AAC encoder is the GPL-compatible candidate; the Fraunhofer encoder's licence does not fit GPL). Other systems show AAC disabled with the reason. The Windows code is type-checked against the library's source but can only be compiled and tested on Windows (CI and the project lead's PC).
- **Generate Tone and Noise (Effects > Generate).** Writes a new 32-bit float WAV to a Generated folder beside the session and places it at the playhead on the selected track. Tone: sine at any frequency and peak level, with 5 ms fades so it never clicks. Noise: pink (default, like room tone) or white, at an average level.
- **Insert Silence (Multitrack > Insert Silence at Playhead, Effects > Generate > Silence, and the empty-lane menu).** Makes room at the playhead: clips after it move later and a clip under it is split. On every track by default (markers move too, keeping sync), or only the selected track. Locked clips in the way stop it with a message.
- **Punch-in.** With a time selection, Record starts a pre-roll before it (2 s by default), stops by itself at the end, keeps only the selected range, and replaces what was there on the recorded tracks. Without a selection, recording works as before. Multitrack > Recording Options turns punch-in and replacing off and sets the pre-roll.
- **Recording offset (latency compensation).** A setting in Recording Options trims that many milliseconds from the start of each take, so takes line up with what they were played against. The dialog explains how to measure it with a clap. Automatic measurement can come later.
- **Marker colors and notes.** Markers gain an optional color (the type's color when unset) and the Markers panel edits notes (the existing description field) on a second line under each marker. Click the swatch to pick a color; right-click it to go back to the type's color.

## 2026-10-18: editing batch (silence over a selection, audio types, Add Tracks, ad and social markers, embedded chapters, Playlist)

- **Insert Silence over a time selection.** With a time selection, Insert Silence offers to silence that range in place: clips are split at both edges and the audio inside removed, nothing moves. Without a selection it inserts new silence and pushes audio later, as before.
- **Audio types on clips.** Clips can be marked Dialogue, Music, Sound Effects or Ambience (Clip > Audio Type, and the clip right-click menu). Edit > Select > Clips of Audio Type selects every clip of a type. The Audio FX Panel will use the same field when effects arrive. New optional field `audio_type`; old files open unchanged.
- **Add Tracks dialog.** Several mono or stereo tracks at once, with an optional base name ("Guest" gives Guest, Guest 2, Guest 3), placed after the selected track. Bus tracks wait for effects (they route audio through effect chains), and 5.1 surround tracks stay out of scope for podcast work; both show in the dialog as unavailable with the reason.
- **Ad slot and social clip markers.** Add Ad Slot asks for the contracted read length; the Markers panel shows the duration in red when the slot is off by more than 5 seconds. Add Social Clip marks the time selection. Both types can be picked in the Markers panel.
- **Chapters inside exported files.** Export Mixdown writes chapter markers into MP3 (ID3v2.3 CHAP and CTOC frames) and FLAC (Vorbis comment CHAPTERnnn tags), on by default when the session has chapter markers. A time-selection export carries only the chapters inside it, starting at 0. Our own writer, no new dependency; FFmpeg reads the chapters back from both formats in testing. AAC files do not get chapters yet; Windows' writer has no chapter support.
- **Playlist panel (Window > Playlist).** Range markers in an order you choose, saved in the session (`playlist`, undoable). Play All runs them back to back without moving clips; each row plays from there; Export Each writes one file per range, numbered in playlist order, in the last export format.
- **Already in place and checked:** Paste to New File and Mix Paste (Waveform editor).
- **Not in this batch:** markers in the Waveform editor (they need saving into the WAV file as cue points and moving with cuts), hearing the input through the speakers while recording, ASIO, Batch Convert metadata, AAC encoder-delay trim, `.aup3` import. HE-AAC is not offered by the encoder built into Windows, so it would need a bundled encoder.

## 2026-10-18: Waveform editor markers, input monitoring, Batch Convert tags; `.aup3` parked

- **Markers in the Waveform editor.** Markers are saved inside the WAV file as cue points (a `cue ` chunk with `LIST adtl` names, lengths for ranges and notes), the form most editors and samplers read. Other formats keep markers only while the file is open; the save notice says so. Cuts move later markers left and pull markers inside the cut to its start; pastes and inserts push them right; Crop keeps the ones inside and moves them to the new start. The Markers panel follows whichever editor is in front.
- **Input monitoring.** The Monitor Input button on a track header now works: while the track is armed, its input plays through the default output, mono inputs to both sides. It is software monitoring, so there is a short delay (the device buffers plus 10 to 30 ms); the queue to the speakers is capped at 80 ms so it never creeps up. An interface's own direct monitoring has no delay and is still the better choice when available. The output must run at the input's sample rate; if it cannot, monitoring stays off and the status bar says why. System audio inputs are never monitored (they are already what the speakers play). The tooltip warns to use headphones.
- **Batch Convert tags.** Keep each file's tags and markers (on by default) copies title, artist, album, year, genre, comment and track number into the output, and a WAV's markers when the output is WAV too (scaled to the new sample rate). Fields typed in the panel replace the file's own and can use the name tokens. Written into WAV (`LIST INFO`, placed before the audio so streaming readers see it), FLAC (Vorbis comments) and MP3 (ID3v2.3). Tags and embedded chapters keep each other when both are written. M4A outputs convert fine and show a note that tags are not written yet. Our own writers, no new dependency.
- **`.aup3` import parked.** The project lead's call: keep it as a possible later feature. What it would take is in `docs/backlog.md`.
- **ASIO stays open.** It needs the project lead's interface to test against and a check of the ASIO SDK licence terms before any build that includes it ships.

## 2026-10-18: effects, first batch (P5 starts)

The project lead asked to start the effects phase, with three pages of public help documentation on noise reduction and restoration, time and pitch, and filter and EQ effects as behavior references. They describe what such effects do; our names, controls, layouts and code are our own, and nothing was taken from any product's code.

- **Where effects run.** In the render thread that reads clips ahead of the audio device, never in the audio callback, so the real-time rule holds. Each clip's Voice Chain runs on that clip (after its gain and fades), then each track's chain on the track, before the fader. Export Mixdown and stems use the same renderer, so what you hear is what exports.
- **Look-ahead is compensated.** An effect that looks ahead (the Limiter) delays its output. The renderer reads that track or clip early by the same amount and primes the chain with the frames in between, so a limited track stays in sync with the others. The dry part of the Mix control is delayed to match.
- **Changing a chain restarts playback from the same spot**, like moving a clip. Sliders commit on release, not while dragging, so a drag is one undoable step and one restart. Smooth live tweaking can come later.
- **Effects are data.** The core lists every effect with its controls, ranges and defaults (`effects_catalog`), and the Voice Chain panel draws the controls from that list. Saved chains store values by key; missing keys take defaults and unknown effects stay in the file and are skipped, so old and newer files always open.
- **The first eight effects**, all original code: Parametric EQ (high-pass and low-pass at 12, 24 or 48 dB per octave, two shelves, three bands), Hum Cut (60 or 50 Hz and overtones), Dynamics (a compressor with soft knee), Limiter (look-ahead that never passes the ceiling, -6 dB by default per the loudness defaults), Breath Gate, De-Esser (a high shelf that dips only when S sounds flare), Speech Leveler (rides speech toward a target and holds still in pauses so room noise is not pumped up), and Gain. Filter coefficients follow the published audio EQ cookbook formulas, credited in `ATTRIBUTION.md`.
- **Voice Chain panel.** Clip Effects and Track Effects tabs, 16 numbered slots with colored badges, bypass, reorder and remove per slot, chain power, input and output gain, and Mix (dry/wet). Presets: the Voice chain presets replace the chain; Breath gate and EQ presets add one effect. Built-in presets are our own starting points for the names listed in CLAUDE.md; Noise Reducer is left out of them until it exists. Users can save and delete their own presets (stored in the app's settings).
- **Effects menu.** Dynamics and EQ and Filters list the effects; picking one adds it to the selected clip's chain (one clip selected) or the selected track's, and shows the panel.
- **Not yet:** Noise Reducer and Click Repair (next batch), chain meters, master chain (the master is mixed in the audio callback today; it moves to the render thread with buses), Pitch and Time effects, applying effects destructively in the Waveform editor, and the Audio FX Panel.

## 2026-10-18: effects, second batch (restoration), and the look of QuickPass and Batch Convert

- **The project lead's call on the look:** QuickPass and Batch Convert keep the same layout and control set as the editors podcasters already know (same controls in the same places and order), with our own labels, icons, colors and spacing. No copied styling, wording or panel names. This keeps the legal guardrails and still feels the same in the hand.
- **Full effect set planned, renamed.** The behavior references cover noise and restoration, time and pitch, and filter and EQ effects. Every one gets an equivalent under our own name, built in batches. Restoration came first, at the project lead's choice.
- **Restoration effects built (original code):** Noise Reducer (learns the steady background per frequency from the quietest recent moments and turns it down under the voice), Hiss Reducer (the same, above a set frequency only), Room Reducer (models the decaying tail of a room and turns down what matches it), Click Repair (finds spikes far above the signal's usual sample-to-sample change and fills them with a smooth curve from each side), Clip Repair (rebuilds flattened peaks the same way, then lowers the level), DC Offset Fix. The spectral three share one streaming engine (40 ms frames, 75% overlap, same gains on both channels so stereo stays put). All report their latency, which playback and export already compensate.
- **Presets.** Podcast Voice, Podcast Voice 2 and 4 now start with Noise Reducer, matching the chain order in this file. New: Remote Guest Cleanup, and one-effect Repair presets (Remove Clicks, Repair Clipping, Reduce Hiss, Reduce Room Echo, Fix DC Offset).
- **Still to come in restoration:** Noise Print Reducer and Sound Eraser (both learn from a selection, so they need a capture step in the interface), and Phase Align for stereo.

## 2026-10-18: Batch Convert rework and Music Fit

- **Batch Convert, agreed layout.** The panel is now a file list (Name, Status, Stage), a Process with list (any Voice Chain preset, built-in or the user's, run on every file before it is saved; or nothing), and an Output Settings dialog holding naming (before and after text, or a template with tokens), where files go, replace existing, format, encoder settings, tags, and taking finished files off the list. Sample Settings has its own dialog: rate, channels with how much of each side goes into a mono file, and bit depth with dither on or off. Our own labels and look throughout.
- **Processing in Batch Convert** uses the same effects as playback. Mono files run through the chain as stereo and fold back. The chain's look-ahead delay is trimmed from the start and flushed at the end, so the new file is exactly as long as the old one and in time with it (tested with a limiter plus gain). Files with more than two channels convert but cannot be processed yet; the row says why.
- **Music Fit (Clip > Fit Music to Length).** Asked for on 2026-10-18 for music beds and ad slots. It finds the tempo and beats, fingerprints each beat by its harmony and tone, and finds joins where the music on both sides matches. To shorten it skips one section; to lengthen it repeats one section as many times as needed. The intro and ending are always kept. Whatever is left (under half a beat) is taken up by a stretch of a percent or two. The result goes on the track as grouped clips with 40 ms crossfades, so every join stays editable, and the whole fit is one undoable step. Music without a steady beat is refused with a pointer to Time Stretch. Our own analysis code, no new dependency.

## 2026-10-18: QuickPass repair scans

- **New scan types.** Click Repair (clicks, pops, mouth noise), Clip Repair (flattened peaks), and Mark Audio (the parts with sound, for marking or splitting) join Remove Silence and Short noises. The panel keeps the agreed layout: scan type, presets with save and delete, Scan and Settings views, a findings table (fixed, start, length, clip), Fix, Fix All and Clear Fixed, and a count of what was found and fixed.
- **Clicks and clipping are found in the core**, on each clip as it plays (gain and Voice Chain included), sample by sample, with the same detectors as the Click Repair and Clip Repair effects. Silence, short noises and sound still come from window levels on the interface side.
- **Repair keeps the multitrack non-destructive.** Fixing writes a copy of just the clip's part of its file, with only the chosen spots repaired and every other sample untouched, into a Repaired folder beside the session; the clip then plays the copy. The original file is never changed, and one undo puts every clip back. Looped and stretched clips are skipped with a note, since their timeline positions do not map one to one onto the file. Mark and Split still work for every type.
- **New presets:** Remove Clicks, Remove Mouth Clicks (sensitive), Repair Clipping, Mark Non-Silent Areas.

## 2026-10-18: effects, third batch (filters, pitch), and panel fixes

- **Filter and EQ effects built (original code, cookbook formulas):** Band EQ in 10 (octave), 20 (half-octave) and 30 (third-octave) bands, each band plus or minus 15 dB; Notch Filter with six notches of set depth; Precision Filter with Butterworth, Chebyshev and Bessel designs at 2nd to 8th order, as low-pass, high-pass, band-pass or band-stop; Curve EQ, a smooth zero-phase response through eight points applied in the spectrum; Center Remover, which takes out what sits in the middle of a stereo mix (usually the lead vocal) and keeps the bass. Elliptic designs and a drawn curve editor come later.
- **Pitch Shift built:** changes pitch without changing speed by stretching with the same pitch-keeping engine as Time Stretch, then reading it back faster or slower. Plus or minus 12 semitones with fine tune. Tested an octave up, an octave down and a third up. A fixed 75 ms delay keeps the output steady and is compensated like the Limiter's look-ahead. Formant keeping, Pitch Tune and Pitch Curve come later.
- **Presets.** Batch Convert no longer offers the per-voice track chains (Podcast Voice 1 to 4, Radio Announcer Voice, Voice Over, Remote Guest Cleanup); those belong on a host's track in the Voice Chain. The project lead's call. New one-effect Processing presets for both: Hard Limit to -0.1 dB, Voice-Over Compressor, Raise Pitch, Lower Pitch, Remove Vocals.
- **Voice Chain always shows a chain.** With nothing selected it showed only a hint and looked empty. It now follows the selected track, or the selected clip's track, or the first track, with a Track picker in the panel to choose another.
- **Smaller controls.** QuickPass's Scan and Settings tabs and its action buttons, and Batch Convert's footer buttons, were drawn at the browser's default size and wrapped in a narrow panel. They now use the panel's 11 px text at 20 px tall and stay on one line.


## 2026-10-18: effects, fourth batch: live chains, Waveform apply, learned prints, alignment

- **Live chain edits.** A Voice Chain change no longer restarts playback. The playback layout now covers only timing; chains are tracked apart. When a chain changes and its look-ahead stays the same, the new chain swaps into the render thread and is first warmed on the last stretch of input the old chain heard (the look-ahead plus 100 ms), so it picks up sample-accurate where the old one was (tested: swapping in an identical chain gives identical output). A look-ahead change (the Limiter's look-ahead control, for one) still restarts, since the input is read from a different point. The change is heard once the half second already rendered ahead has played.
- **Sliders apply while dragging**, about ten times a second, and one drag is still one undo step (consecutive edits to the same control merge). Controls that change an effect's look-ahead apply on release, so dragging them never restarts playback over and over; the core marks those controls by building the effect at the control's ends.
- **Chain meters.** Each track chain reports its peak in and out per rendered block, tagged with the block's position; the status call only hands over readings for audio the device has played, so the meters line up with what is heard. Clip chains are not metered yet.
- **Voice Chain in the Waveform editor.** The chain sits on the open file as a preview: it plays, but is not part of the file. Saving, spectral edits and every destructive edit work on the file without it, so a preview can never be saved by accident. Apply renders the selection (or the whole file) through the chain, warmed on a quarter second before the selection, with the look-ahead trimmed and a 5 ms blend at each end, then swaps the stretch in as one undoable edit and clears the preview. Effects menu entries add to the preview chain in the Waveform editor.
- **Noise Print Reducer.** Learn from Selection captures the noise (a pause with only the hum or fan in it) as 128 bands evenly spaced in pitch, stored in the effect's own settings so it saves with the session and travels in presets. The print is learned from the raw audio of that track or clip (no chain, unity gain). With nothing learned it changes nothing. Original code on the shared spectral engine.
- **Sound Eraser.** Same learning step, of the sound to remove (a chime, a siren). Each frame it reads how loud the sound is from the bands where the sound is strong, using one of the lower readings since other sound only adds to a band, and only acts when those bands agree with the learned shape, so a voice across the same bands is left alone. Tested: a chime at a third of its learned level comes out by more than 12 dB over a steady tone, and broadband sound with no chime is kept.
- **Phase Align (Clip > Phase Align).** Select two clips of the same sound on two tracks; the upper one is the reference. The core cross-correlates the raw audio (up to 30 s, 100 ms either way) and the interface moves the lower clip by the lag and flips its track's polarity if the best match is upside down, as one undoable edit, with the match percentage in the notice.
- **Elliptic Precision Filter.** Even orders 2 to 8, with passband ripple and stopband depth controls. Designed by the textbook method (Landen's transformation for the Jacobi elliptic functions), credited in `ATTRIBUTION.md`; tested for ripple, edge and stopband depth at every order.
- **Response graphs.** Every EQ and filter draws its response in the Voice Chain, measured by sending a click through the effect itself, so the graph is exactly what plays. The Curve EQ's points drag on the graph (frequency left and right, gain up and down).
- **Batch Convert normalize and fades.** A level row after the preset: normalize the peak to a level (-30 to 0 dBFS) and fade in and out by milliseconds (an S curve). Normalize and fade out read the file twice (the first pass runs the preset too, so the peak is the processed one); progress runs once across both.
- **M4A tags.** Our own writer for the MP4 item list (`moov/udta/meta/ilst`): title, artist, album, year, genre, comment, track number. Other boxes in the movie header (chapters) are kept. When the header sits before the audio, every chunk offset moves by the size change. Batch Convert now tags M4A files.
- **Music Fit plan choice.** The core returns up to three plans, best first, each joining at least two beats from any better plan; the dialog lists them with where each one cuts or which section repeats.

## 2026-10-18: Edit > Settings

- **Asked for by the project lead** with screenshots of a settings dialog as the behavior reference. Same idea (a page list on the left, one page of settings at a time, OK and Cancel), our own page names, wording, grouping and look. Pages that do not fit this app were left out (another product's memory sharing, video).
- **Pages:** General, Appearance, Audio Devices, Channel Routing, Autosave, Editing, Effects, Cache and Files, Markers, Multitrack, Clips, Playback and Recording, Spectrum Views, Time Display, plus two of our own: Loudness and Privacy.
- **What works now:** Ctrl+wheel zoom step and whether it zooms around the mouse or the playhead; the shape new fades start with; what the space bar does while playing (pause, or stop and go back); color overrides for the waveform, playhead, accent and selected lane; the output device for playback, previews and monitoring (falls back to the system default when unplugged); autosave on or off and its interval; showing the Voice Chain when adding from the Effects menu; exclusive solo (one undoable step); clip auto-crossfade; follow playhead; meter yellow and red levels; the recording latency offset, punch-in and pre-roll; time format; loudness region and target; cache sizes and clearing the waveform drawings (rebuilt on demand).
- **Shown greyed out with the reason**, per the rule for unbuilt controls: channel routing and driver type (with ASIO), buffer size, numbered session backups, edge crossfades in the Waveform editor, pan law (with buses), spectrum detail and range, timecode frame rate, scrubbing, the record countdown, the weekly update check (after the first public release, off by default).
- **One draft.** The dialog edits everything as one draft: OK applies it all, Cancel drops it. Settings that already lived in other places (the editor's time format and crossfade switch, Recording Options, the loudness region) are edited in the same draft and stored where they were, so nothing moved.
- **Cache clearing is limited to waveform drawings.** Edit results, untitled Waveform files and unsaved recordings may still be in use by an open file or session, so Settings shows their size but does not delete them.

## 2026-10-19: 0.1.3, podcast workflow batch

- **Speaker Balance (Multitrack > Speaker Balance).** Step 3a of the podcast workflow. Measures each voice track's integrated loudness raw (no Voice Chain, no fader, no pan), takes the quietest as the reference and brings the others down to it with clip gain, as one undoable edit. Coming down never pushes anyone into clipping. Differences under 0.5 LU are left alone. Voice tracks are the unmuted tracks with clips that are dialogue or untyped; music, sound effect and ambience tracks are skipped. A new `dry` mixdown option measures without any chain.
- **Host profiles (Multitrack > Host Profile).** Close Mic (gate -14 dB), Standard Mic (gate -20 dB) and Remote Guest (gate -30 dB) on the selected track, from the table in CLAUDE.md. Each replaces the track's Voice Chain with a starting chain ending in the profile's breath gate (attack 2 ms, hold 50 ms, release 200 ms): Podcast Voice for the mic profiles, Remote Guest Cleanup plus a 7 kHz low-pass for the guest. Remote Guest also sets the track's role to guest (Guest track mode). The track remembers its profile; Clear Profile removes the mark and leaves the chain. Track changes now carry `role` and `host_profile_id`.
- **Settings items that went live:** numbered session backups (each save first copies the previous file into a Backups folder beside the session, named with the date and time, keeping the newest 1 to 50; only our own timestamped names are ever pruned), a 3 or 5 second count-in before recording (press Record again to cancel), the name new markers start with, and the timecode frame rate (24, 25, 30, 50 or 60).
- **Master chain and buses held back, with a reason.** The fader choice for 0.1.3 was "instant faders, chains before them, the master chain hearing the mix after the faders." Building it showed the last part cannot hold together: faders are instant because the audio thread applies them, and effects can only run on the render thread, half a second ahead (the audio thread must never allocate or wait, and several effects allocate). So a chain that hears the faded mix necessarily hears fader moves half a second late. The real choices go back to the project lead: (a) when the master or a bus has a chain, the mix moves to the render thread and track faders lag half a second while it is on, with the master fader staying instant after the chain; or (b) master and bus chains run before their own faders only, and never hear track fader moves live (export would then also ignore track fader moves for those chains, so playback still matches export).
- **Still greyed out:** Waveform editor edge crossfades (the delete op needs the split pieces' ids to overlap them; planned with the next Waveform batch), scrubbing, spectrum detail and range, pan law (with buses), channel routing and driver type (ASIO), controllers, the update check, a light theme, moving the cache.

## 2026-10-19: fixes found while retaking the README screenshots

- **Edit > Settings did not open.** The dialog copied its starting values with `structuredClone`, which fails on the reactive object Svelte hands it, so the click did nothing. It now takes a plain snapshot first. Checked by opening every Settings page and pressing OK in a browser run of the interface.
- **Batch Convert's Normalize and Fade row took the file list's space.** The panel's grid had one row too few after the row was added, so the list was squeezed and the row stretched. The row now has its own grid row, wraps in two groups (Normalize, then the fades) in the narrow left column, and the panel can no longer grow wider than its column.
- **README screenshots retaken** on the current build with a demo session (generic HOST and CO-HOST names, no real host names): multitrack with the Voice Chain, the Waveform editor previewing a chain with the EQ graph, the spectrum view, Settings, Batch Convert and Export Mixdown. The old per-speaker shot is retired.

## 2026-10-19: Design principles, wording, Settings trim

- **Design principles written down** in `docs/design-principles.md`: keep conventions, make the expression ours; no AI unless the user turns it on; no "clone" or "alternative" wording in public.
- **"Donate" is gone from the code.** The Help item, link setting and action id are now "support" (`help.support`, `LINKS.support`).
- **Settings: Controllers page removed.** Settings only holds things this app has or plans. Channel Routing stays because ASIO is planned.

## 2026-10-19: Experimental Linux and macOS builds

- **Why:** a user asked for Mac and Linux. Windows stays the main target and the only tested one.
- **Linux:** CI job (clippy and tests) plus AppImage and deb on tagged releases. Marked experimental until someone confirms audio on real Linux.
- **macOS:** Apple silicon dmg on tagged releases, ad hoc signed only (no Apple developer account, no cost). Users open it with right-click Open. Experimental, untested by us. Cmd-for-Ctrl and Mac-specific audio checks are still later work.
- **Both** are separate release jobs that add files to the same draft and cannot fail the Windows release.
- **Workflows are delivered as `ci-linux-mac.zip`** because `.github/workflows` is a protected path.

## 2026-10-19: Waveform editor Save includes the Voice Chain

- **Bug report:** "effects and pitch changes did not carry over." In the Waveform editor, effects from the Effects menu (Pitch Shift included) were a preview, and Save left them out without saying so.
- **Decision (project lead):** the chain is on by default and Save writes it into the file, with no question. To save without it, bypass the chain first. Multitrack already worked this way: chains are saved in the session file and included in exports.
- **How:** Save and Save As apply the enabled chain to the whole file right before writing, as one undoable step ("Apply Voice Chain"), then clear the chain so it is not heard twice. A bypassed chain is left out. Cancelling the save dialog changes nothing.

## 2026-10-19: Experimental Linux and macOS builds

- **Why:** a user asked for Mac and Linux. Windows stays the main target and the only tested one.
- **Linux:** CI job (clippy and tests) plus AppImage and deb on tagged releases. Marked experimental until someone confirms audio on real Linux.
- **macOS:** Apple silicon dmg on tagged releases, ad hoc signed only (no Apple developer account, no cost). Users open it with right-click Open. Experimental, untested by us. Cmd-for-Ctrl and Mac-specific audio checks are still later work.
- **Both** are separate release jobs that add files to the same draft and cannot fail the Windows release.
- **Workflows are delivered as `ci-linux-mac.zip`** because `.github/workflows` is a protected path.

## 2026-10-19: Waveform editor warns before dropping a preview chain

- **Bug report:** "effects and pitch changes did not carry over." In the Waveform editor, effects from the Effects menu (Pitch Shift included) go into a preview chain, and Save left them out without saying so. They only reach the file through Apply in the Voice Chain panel.
- **Fix:** Save and Save As now ask "Apply the Voice Chain first?" (Apply and Save, Save Without Effects, Cancel) when the preview chain has an enabled effect. The notice after adding an effect says it is a preview and to press Apply.
- **Still open:** whether Effects menu items in the Waveform editor should apply immediately instead of previewing. Previewing lets you hear and tweak before committing, but it is easy to miss. Left for the project lead to decide.

## 2026-10-19: 0.1.4 user report fixes

A user report on 0.1.3 listed five problems. All five are addressed in 0.1.4.

- **Input picker was one long list.** Track inputs are now grouped: the default input and None first, then Mono and Stereo submenus, each split by device, with channels listed inside. The stored value did not change, so existing sessions keep their inputs.
- **Recording more in the Waveform editor was lost to the session.** The session kept the file's old length, so a clip could never reach the new audio. Now, when the Waveform editor saves a file the session uses, the session reads the file's new length (`set_media`, one undoable step called Update Changed File). Clips that ran to the old end run to the new end; clips past a shorter end are trimmed to it.
- **Effect settings opened below the slot list.** They now open in a floating window (see backlog).
- **Opus.** File > Open ignored `.opus` (the interface's list of audio types missed it), and importing was slow because the `opus-decoder` crate computed one FFT as a direct DFT. A patched copy in `src-tauri/vendor/opus-decoder` uses `rustfft` instead: a 7 minute mono file went from about 89 s to about 1.2 s to decode in our test build. A small `tone.opus` test file checks the length, level and pitch of the decoded tone.
- **Opening another file in the Waveform editor asked to save.** The file open now is set aside with its unsaved edits and undo history, and comes back as it was when opened again. Nothing is written to disk until you save. Quitting with set-aside unsaved files asks before discarding them.

## 2026-10-19: More 0.1.4 report fixes

- **Mouse wheel over the multitrack lanes zoomed nothing.** Plain wheel only scrolled the tracks; zoom needed Ctrl. Now the wheel over the lanes and the Waveform editor zooms (around the pointer or the playhead, per Settings), the wheel over the track headers scrolls the tracks, Shift + wheel scrolls sideways, and Ctrl + wheel always zooms. Edit > Settings > General can turn plain-wheel zoom off.
- **Loop Playback with no time selection played on into silence.** It now loops the whole session, from the start to the end of the last clip. With a time selection it loops the selection, as before.
- **The open session was missing from the Files panel.** It is now the first row (name, unsaved star, length, sample rate). Double-click shows it in the Multitrack view.
- **No way to pick the Waveform editor's recording input.** Edit > Settings > Audio Devices now has "Waveform editor records from" (system default, or any mono channel or stereo pair). Multitrack tracks still pick inputs in their headers. Edit > Settings itself did not open in 0.1.3; that was already fixed for 0.1.4.
- **Time Info did not follow Waveform editor playback.** It only ever showed the multitrack session. It now shows the open file, its selection and its playback while the Waveform editor is in front.
- **ASIO** was asked for again. Still planned with per-channel routing; it needs a real ASIO interface to build and test, and the ASIO SDK's license has to be checked against GPLv3 before it goes in.

## 2026-10-19: 0.1.5: Phase Rotate, effect automation, .sesx export, ASIO

- **Phase Rotate** (Effects > Stereo and Channels): turns the phase of every frequency by -180 to 180 degrees (one channel or both), flips either channel's polarity, and adds a 0 to 20 ms delay. It works on the spectrum with 75% overlapped frames (about 43 ms latency, compensated like every look-ahead). Asked for to fix music that sits partly out of phase.
- **Effect automation on clips.** Any setting of an effect in a clip's Voice Chain can change over time. The file format already had `EnvelopeTarget::EffectParam`; now playback reads it (every 256 frames the chain gets the new values). Effects that can take a new value in place (Phase Rotate, Gain) glide; others are rebuilt at the new value, which can click on fast moves, so Phase Rotate is the one to automate for now. In the lanes: the clip's envelope chip > Effect Setting picks which setting to draw (View > Envelopes > Clip Effect Settings shows them). Points move with the audio on split and trim. Track-level automation is later. Settings that change the look-ahead are not offered.
- **Export as .sesx, not save as .sesx.** Our file stays `.altaudio`, because a `.sesx` cannot hold Voice Chains, envelopes, loops, stretch or markers, and saving to it would quietly lose them. File > Export > Multitrack Session (.sesx) writes a copy with tracks, clips, levels, pan, mute, solo, lock and fades, and says what it left out. Our own importer reads it back the same (tested). Not yet checked by opening it in the other editor; the project lead will try it.
- **ASIO.** The ASIO SDK is dual licensed by Steinberg, proprietary or GPLv3; we use the GPLv3 option, so it fits our license. It is a Cargo feature (`asio`, off by default) because building it needs LLVM and the SDK, which cpal's build script downloads. Release builds turn it on, and fall back to a build without it if that fails. A separate CI job checks the ASIO build. Settings > Audio Devices picks System or ASIO; device lists follow the picked driver. Sample rate and buffer size come from the driver's own control panel. If the ASIO driver cannot open, the system driver plays. We use the name ASIO only to say what is supported, never the logo, per Steinberg's trademark rules. Not tested on real ASIO hardware yet.
- **The 3:17 FLAC that looked 15 minutes long in the Waveform editor.** Probing the file gives the right length (checked with 44.1 and 96 kHz FLAC). The likely cause: the Waveform editor fitted the file to the panel's width while the panel was still narrow, right after switching from Multitrack, so the view showed several times the file's length. It now fits again while the width settles, until you zoom or scroll. Waiting on the reporter to confirm.

## 2026-10-19: Release workflow fix

- **Old v0.1.1 files showed up in the v0.1.5 draft**, and Linux and macOS files never arrived. The `out/` and `portable/` folders from an earlier local packaging run were committed, and the workflow packaged into `out/` in the checkout, so it picked them up. Separately, the Linux and macOS jobs tried to add files to a draft by tag, which can fail silently for drafts.
- **Fix:** every platform builds in its own job, stages files in the runner's temp folder (never the checkout) and hands them over as build artifacts. A last job makes the draft from whatever built, with one `SHA256SUMS.txt` for all files, and warns when the installer was built without ASIO. `out/` and `portable/` are gitignored and removed from the repository.

## 2026-10-20: 0.1.6 tester report fixes

- **Only the first recorded take got the effects.** Not the chain: the Noise Reducer (and Hiss and Room Reducer, which share its noise tracker) learned the background from the digital silence between clips, decided the noise was zero, and stopped working from the second clip on. They now ignore digital silence and keep what they learned. A test renders three clips with gaps through Gain, Limiter and Noise Reducer and checks every clip is processed.
- **Waveform playback ran on past the end.** Playback in both editors now stops at the end of the last clip (not while looping, and never while recording).
- **Loop over a time selection did nothing.** The selection reached the core up to 300 ms late (view changes are batched), so pressing Play right after selecting looped nothing. Play now sends any waiting view change first.
- **The quick level box in the Waveform editor** only responded on its small dial. The whole box is the handle now, and up or right turns it up.
- **QuickPass in the Waveform editor** said there were no clips: it only looked at the multitrack session. It now scans the open file (Repair stays a multitrack action; the Waveform editor has the Click and Clip Repair effects).
- **Files panel right-click** gains Edit File and Insert into Multitrack (at the playhead on the selected track, or as new tracks) and Remove Unused Files.
- **Right-click menus:** a disabled row in a submenu closed the whole submenu, so a menu seemed to vanish over unbuilt items. It now stays open. The space below the last track gets the session menu too.
- **Envelope points** snap to 0 dB and center (hold Alt to place them freely).
- **Custom loudness target** (Settings > Loudness): your own LUFS and true peak, offered as Custom in Check Loudness and Loudness Match.
- **Meter response** (Settings > Playback and Recording): Fast, Medium or Slow. The peak marker always shows the true peak.
- **Blocky waveforms when zoomed in on short files.** The finest waveform drawing was one value per 256 samples. Files up to 10 minutes now get a 32-sample level, up to an hour a 64-sample level (about 22 MB at most). Cache format version 2; old caches rebuild by themselves.
- **Stretch in the Waveform editor** (Effects > Pitch and Time, or Clip > Time Stretch while the Waveform editor is in front): type the new length of the selection or file (30, 14.99, 1:02.5) and it is re-timed with the pitch kept, and what follows moves up or back. For spots that must hit an exact length.
- **Clip > Move to Track Above / Below.** Asked for as Alt+Up and Alt+Down; they have no default shortcut until the project lead approves one (Edit > Keyboard Shortcuts can set them now).
- **CI:** Linux jobs pinned to ubuntu-24.04 (ubuntu-latest moves to 26.04), and the Linux release build gets the extra packages and the extract-and-run setting the AppImage tools need without FUSE.

## 2026-10-20: roadmap audit and the second round of tester documents

- **Why:** the project lead asked whether the items in a new set of tester documents, and a seven-part request for scripting, time stretch, plugin hosting, ASIO, MIDI, markers and video, and podcast tools, were built, planned or new. No credits were available to build, so this was an audit and a documentation update only. No code changed.
- **Result:** most of the seven-part request was already planned in `docs/backlog.md` (added earlier this month) and several parts are now built. The backlog has a status table and a list of what remains. The document's menu screens, settings walk-through and manual pages became a grouped list of behavior to match, in our own names and layout.
- **Decisions kept:** VST2 stays out (the SDK is no longer licensed). No OpenAPI spec, because there is no HTTP API and adding one would break the no-network rule. The `.altaudio.session` style file names in the request are not used; our extensions are `.altaudio`, `.altemplate`, `.alworkspace`, `.alhost`, `.alkeys`, `.alprefs` and `library.json`. Scripting and the assistant endpoint stay off by default and local only. Rubber Band is a candidate for time stretch only after listening tests, with an `ATTRIBUTION.md` note, because it is GPL.
- **Decisions made:** Monkey's Audio and Windows Media formats are not planned (no maintained Rust codec for the first, proprietary and Windows-only for the second); the format menu says so. Control surface protocols come after generic MIDI-learn because they cannot be tested without the hardware. Other products' keyboard shortcut sets are not shipped as presets, and no new shortcut is added without the project lead's approval; candidates are listed in the backlog.
- **Open for the project lead:** what the window shows on launch (empty, last session, or new session); the master chain and fader behavior (unchanged from the earlier question); which shortcut candidates to approve.

## 2026-10-20: approvals and the third round of tester documents

- **Shortcuts approved.** The project lead approved using the common defaults tester documents listed ("the same shortcuts"). They are listed in `CLAUDE.md` and each is assigned when its feature ships. Shortcuts are not protected expression, and we still do not ship another product's full set or preset names. Where a key clashes with an existing one (Up and Down, Alt+Up and Alt+Down), the Waveform editor or Multitrack context decides, and Edit > Keyboard Shortcuts still lets users change them.
- **Launch window:** empty, not a session with tracks. A setting offers last session or new session.
- **Master and bus chains (the project lead had no preference, so the recommended option was taken):** chains run on the render thread half a second ahead; track faders then lag that long when a chain follows them; the master fader stays on the audio thread and is instant. Reason: the audio thread can never allocate or lock, and the other option (chains before their own fader) makes the fader feel dead on the track it controls. Reversible before any code is written.
- **Third document:** 228 pages of effect and multitrack manual text. Grouped into the backlog by effect family and by area, with status. OMF and Final Cut interchange export is not planned for now. Surround and the guitar amp suite are out of scope. Everything else is either built, listed, or part of the video phase.

## 2026-10-20: reference documents stay private; podcast AI feasibility

- **Rule.** The tester documents and their screenshots are private working material. They are never committed or published: `/reference/`, `*Bugs*.pdf` and `/private/` are in `.gitignore`, and `scripts/check-private-terms.mjs` (part of `npm run check`) fails the build if any term in `private/banned-terms.txt` appears in a tracked file. The list lives in `private/` so the names in it never reach the repo. Public docs describe features in our own words and name no other product. The source-to-feature mapping is kept in `private/`.
- **Podcast AI features** (text-based editing, filler and retake removal, studio sound, voice tools, clips, chapters, show notes, soundboard, remote recording) were checked for feasibility; the table is in `docs/backlog.md`. Decisions: everything stays local and optional; local transcription with word timestamps comes first because most of the rest depends on it; remote recording is not compatible with the no-network rule, so we keep Guest tracks and import separate recordings; language-model writing features stay outside the core and go through the optional local assistant endpoint rather than a bundled model; voice cloning needs consent, a visible label and no watermark removal before any code.
- **Update, same day:** the project lead judged the podcast AI ideas not feasible for this project. They are parked in `docs/backlog.md`, not scheduled, and dropped from the order of work. The earlier P7 plans (local transcription, filler word removal, Publish Package, Social clip markers) are unchanged.
- **Final scope, same day:** the project lead agreed with the feasibility advice. Planned: local transcription, transcript export, text-based editing, filler word removal, a Clean Voice preset, the soundboard. Shelved: everything else in that table (retake finder, edit for clarity, clip finder, chapter suggestions, language-model writing, text to speech, cloning, translation, studio sound model, remote recording, publishing, video layouts, built-in libraries).

## 2026-10-20: Settings > Devices crash logged as the first item for the next round

- Reported by a tester and reproduced by the project lead, also without an audio interface: the app closes when the Devices page opens. Cause not confirmed. Leading suspect is the ASIO driver list, which makes every installed driver load itself, and a native crash in a driver cannot be caught. Fix plan and the information needed are in `docs/backlog.md` ("Next round"). No code changed yet.
- Next-round order: this crash, the small certain items, assigning the approved shortcuts, then the larger build order.

## 2026-10-20: cache and files go to a default folder in Documents, changeable

- First asked for with nothing preselected, then revised the same day by the project lead: default to a designated folder in Documents and let people change it. Reason for the revision: a setup screen is friction, a visible default is easy to find, and Settings still offers Browse for every location. Logged for the next round, second after the Devices crash. The main risk is Documents being redirected into a sync service, so first run checks for that and warns. Layout, folders and trade-offs are in `docs/backlog.md` ("Next round"). Nothing built yet.

## 2026-10-20: live recording waveform must match the final waveform

- Reported as: Waveform editor recordings show nothing until Stop. The project lead then widened it: any recording, in either editor, must show exactly how the finished waveform will look, with no approximate picture swapped out afterwards. Reading the code showed why the multitrack picture changes at Stop: live blocks are 1024 frames and drawn by a separate path, while the finished file uses the peak cache (32 or 64 frames) and the normal renderer. Decision: build the peak cache incrementally while recording and draw it with the normal renderer, so live and final are the same data. Third in the next-round order. Not built.

## 2026-10-20: macOS microphone permission fix prepared

- Cause (from the tester's report): no microphone usage string in `Info.plist` and no audio-input entitlement, so macOS silently denies input and recording is flat. Prepared: `src-tauri/Info.plist`, `src-tauri/Entitlements.plist` and `bundle.macOS` in `tauri.conf.json` (checked against Tauri's config schema; not built or run on a Mac). Kept the ad hoc signature: there is no Apple Developer ID, so no notarization. A suggested `infoPlistPath` key was not used because the real key is `infoPlist` and a wrong key fails the build. Also planned: a clear note when input is silent. Listed as item 1b of the next round.

## 2026-10-20: long-job bugs logged (apply across files, export blocks the app)

- Reported: switching files during Apply to Whole File breaks the job; a multitrack export locks the app and a second copy offers to recover the session being exported. From the code: results are applied to the current file instead of the file the job started on; the export's progress overlay is modal although the work runs on a worker thread; recovery cannot tell a live copy's autosave from a crashed one. Fix plan (background tasks bound to their file, non-blocking export progress with Cancel, per-instance lock files for recovery) is item 3b of the next round in `docs/backlog.md`. Not built.

## 2026-10-21: creator feedback on chapters, ad markers and filler removal

- **Chapters:** embedded chapters and chapters JSON already exist. Added to the plan: chapter links and images, a title-length warning based on the major directory's published guidance, and saving the chapters JSON beside an export. Hosts differ on whether they read embedded chapters, so the plan does not promise a universal feed import.
- **Ad markers:** no standard exists; export them as a separate timestamp file, never as chapters.
- **Filler removal:** changed from "select all and cut" to a reviewed, per-item keep or cut with a minimum pause kept, short crossfades, shorten and turn-down options, and Cut All only after review. Reason: fillers can carry emotion and mass cuts break cadence. `CLAUDE.md` updated.


## 2026-10-21: 0.1.7 Devices crash fix and contributor kit

- Settings > Devices now runs its three lookups one after another, and the ASIO driver list is read from the registry instead of loading every driver. A driver is only loaded when the user turns ASIO on. Reason: loading drivers in parallel is the leading suspect for the instant close; unverified until run on Windows. If it still crashes, the next step is the crash-guard marker file.
- Added a contributor kit (CONTRIBUTING, ROADMAP with starter issues and project goals, issue forms, PR checklist). The goals section describes what users should be able to do and names no outside sources or products.

## 2026-10-21: 0.1.8 scope and choices

- Kept 0.1.8 to fixes that are small and certain; Invert and Reverse, the live recording waveform, export progress and the track-header wheel work went to contributor issues (ROADMAP.md). Reason: limited build credits, and each needs its own design pass.
- Crash recovery skips autosaves whose writing copy has a lock file touched in the last 150 seconds. Cost: after a real crash, restarting within 2.5 minutes hides the recovery offer until the lock goes stale. Chosen over a process-list check to stay portable with no new dependency.
- Apply Voice Chain checks that the same file is still open when the result arrives and refuses otherwise. A true background task with progress and Cancel is a contributor issue.
- Working files default to `Documents\Altered Audio Editor\Cache`, picked folder stored in `cache-folder.txt` in the app config folder. Recordings and untitled files stay inside that cache folder for now (not the Sessions and Templates subfolders planned earlier). Existing files are not moved; drawings rebuild.
- Silent-recording note triggers when the loudest input peak for a take of one second or more stays under about -66 dBFS.
- GitHub and Discord added to the Help link allow list (github.com and discord.gg hosts). The app still makes no network requests of its own.

## 2026-10-21 (second pass): live waveform, Invert and Reverse, export card, bulk selection

- Live recording waveform now uses the finished file's own peak form: the core reports 32-frame min and max blocks per channel, 16-bit, scaled and rounded as the peak builder does; the interface merges to coarser levels at 10 and 60 minutes and picks levels the way a finished file does; both editors draw it with the same code as clips. It replaces the old 1024-frame block path. Reason: nothing may be swapped or redrawn at Stop. The live data is not byte-compared against the finished cache yet (a contributor test); a partial last block under 32 frames is not drawn live.
- Reverse and Invert render the range to a new file in the working folder and replace it, like Stretch, so they are undoable and destructive only to the file's edit list.
- Export, bounce, stems and playlist export use a corner progress card (the core exports a cloned session). Other long jobs (save audio, merge, repair) keep the blocking dialog because they change the open session.
- Ctrl+wheel over track headers (one track) and Ctrl+Shift+wheel (all tracks) were requested by the project lead and are approved.
- Closing the session and working without one is logged as backlog 3e with a proposed start screen; not built.

## 2026-10-22: bug round 5

- Playback and previews open the chosen driver first, then the system driver if the chosen one has no output. Reason: ASIO drivers open for one program at a time and a recording holds it.
- Device lists keep the last non-empty result per driver (note shown). Reason: a busy driver returns an empty list, which looked like all devices were forgotten.
- Mouse wheel rule set by the project lead: lanes scroll tracks, ruler zooms time, track headers resize tracks. The "wheel zooms over the timeline" setting is gone.
- A blank, untouched session shows a start screen. The core still holds a blank session; a real Close Session command is backlog 3e.

## 2026-10-22: shortcuts and Space bar queued

- The project lead asked for every approved shortcut to be assigned, including for unbuilt features (disabled items with the chord shown), and for Space to play, or stop when playing or recording. Queued as backlog 3g for the next round, to be done at the desk. No shortcut changes were made yet.

- 2026-10-22: ruler drag becomes a pan, not a selection (backlog 3h), queued for the next round with 3g.
- 2026-10-22: middle-button drag pans the view (backlog 3i), queued with 3g and 3h.

- 2026-10-22: multitrack tab/panel menu and Track Visibility (hide, show only by kind or clips, presets) queued as backlog 3j. Needs a track `hidden` field.

- 2026-10-22: clip envelope selector menu (Add/Remove Effects, Rack Input, Output, Mix, Power) queued as backlog 3k.

- 2026-10-22: fade handles move below the clip title bar and get bigger and clearer (backlog 3l).

- 2026-10-22: clip waveforms get much higher contrast against the clip body (backlog 3m).

- 2026-10-22: ruler gets finer labels and ticks at every zoom; timeline extent defaults to 4 hours and grows only for longer content (backlog 3n).

- 2026-10-22: dragging a clip near the lane edge auto-scrolls the timeline with it (backlog 3o).

- 2026-10-22: Alt+wheel over lanes resizes track height with a vertical scroll view, never drawn as peaking (backlog 3p); conflicts with Alt amplitude zoom, resolve at the desk.

- 2026-10-22: Voice Chain rows get the power toggle left of the name, gear and remove at the right, no arrows or color codes, drag to reorder (backlog 3q). Effect editors get Presets and centered pop-ups; Dynamics becomes a transfer-curve editor with a segment table (backlog 3r).

- 2026-10-22: presets for most effects, with the Limiter editor layout as the pattern (backlog 3r d).

- 2026-10-22: added character voice chains (Godlike, Demonic, Evil Voice, Ethereal, Angelic) to the Voice Chain presets using pitch, EQ, dynamics and limiter. Reverb, delay, modulation and stereo imaging are queued as new effects (backlog 3s). Preset values from other tools are not importable and are not copied.

- 2026-10-22: fades get a drag handle to bend the curve freely (backlog 3t); the existing `curve` field is wired through the drawing, engine and export.

- 2026-10-22: while playing, a click in the lanes no longer moves the playhead; only the ruler does. The ruler triangle now sits on the same pixel column as the playhead line (multitrack and Waveform editor). The bottom horizontal scrollbar in the multitrack is removed, because the overview strip at the top scrolls and zooms the same range.

- 2026-10-22: panel tab strips lose the float and maximize buttons; active tab gets a menu icon, overflow gets a chevron list (backlog 3u). Float and maximize stay in the panel menu (3j).

- 2026-10-22: the panel menu opens from the active tab's menu icon or a tab right-click, with a Panel Group Settings submenu (backlog 3u, shared with 3j).

- 2026-10-22: QuickPass gets a Fix by dropdown on every scan and stops closing gaps by default (backlog 3v); Editor tab before Mixer (3w); left panels never scroll sideways (3x, partly done: QuickPass columns, Voice Chain, Batch Convert).

- 2026-10-22: QuickPass results list detail (verb buttons follow the Fix by choice, sortable Start, check marks, counts, rounded scan time) added to backlog 3v.

- 2026-10-22: Split Silence and Strip Silence behavior spelled out (backlog 3v); clip right-click menu audit queued (backlog 3y).

- 2026-10-22: Libraries get an Empty start option, deletable and reorderable bins, saved custom starter sets and an empty-state message (backlog 3z).

- 2026-10-22: built the 0.1.9 batch (backlog 3g, 3h, 3i, 3l, 3m, 3n, 3o, 3p, 3u, 3v, 3w, 3x). Space stops when playing or recording and the Pause/Stop setting was removed. Alt+wheel over lanes resizes the track and Alt+Shift+wheel zooms the waveform (Waveform editor keeps Alt+wheel for amplitude). Strip now leaves the gap open by default; closing gaps is an opt-in setting. Panel group buttons removed; floating lives in the panel menu. Small Tabs lasts for the session only. 3t (fade bending) and 3z (libraries) were left for a later version by the project lead.

- 2026-10-22: added a `hidden` field to tracks (backlog 3j). Hidden tracks leave the Editor and Mixer lists but still play and mix; the flag is optional and left out of the file when off, so old files load unchanged. Voice Chain rows reworked with drag reorder (3q). Clip envelope menu gained Add/Remove Effects; chain-level envelopes stay disabled until the engine supports them (3k).

- 2026-10-23: the web view's own right-click menu (Back, Refresh, Print, Share, in the system language) is blocked everywhere except text fields (bug report from a tester). The app's own menus are unaffected.

- 2026-10-23: the multitrack right-click menu for empty space now also opens when no session is open (items that need a session are disabled with a reason). File > Export > Markers and Chapters works on the open file in the Waveform editor, using its markers, and saves beside that file by default (tester request).

- 2026-10-23: empty-space right-click menu lined up with the tester's reference: Groups and Bounce gain the missing entries (shown disabled until built), Ripple Delete gains Time Selection in Selected Clips (disabled), the video track entry is removed. Our Marker entries keep their own names (Range, Chapter, Ad Slot, Social Clip).

- 2026-10-23: broadcast and CD features stay in the plan (project lead). Added CD Track and Cart Timer marker kinds (Rust enum, schema, TS, Markers panel, menus), Add Subclip Marker, and 16 new shortcuts assigned to the items we have (CLAUDE.md table). Disabled items hold their chords: Alt+B, Ctrl+Shift+G, Alt+Backspace. Video and 5.1 stay out.

- 2026-10-23: the right-click and marker work ships as 0.1.10, a bug-fix release on top of 0.1.9 (docs/release-0.1.10-plan.md). FCPXML export and MCP control noted in the backlog for later.

## 2026-10-24: bus tracks, sends, master chain and sidechain

Built on the 2026-10-20 decision (chains on the render thread, track faders lag, the master fader stays instant). Proposed as a contribution; the project lead decides whether it goes in.

- **Mix graph (`engine/graph.rs`).** Everything between a track's clips and the master fader now runs in one place on the render thread: track chains, track faders and pans, outputs, aux sends, bus chains and faders, and the master chain. The renderer now only renders clips (with their clip chains). Mixdowns, stems, loudness checks and Speaker Balance use the same graph, so export still sounds exactly like playback.
- **Two shapes, so plain sessions lose nothing.** With every track going straight to the master, no sends, no master chain and no sidechain, the graph runs only the track chains and hands each track to the audio thread, which applies faders as before (instant). Routing a track to a bus, turning on a send, putting an effect on the master or using a sidechain moves the whole mix to the render thread; track and bus faders are then heard about half a second late and the master fader stays instant. The Mixer says "Faders lag ½ s" under the master while that is so. Level changes ramp over one block instead of stepping, so faders never click. A bus that nothing feeds changes nothing.
- **Look-ahead stays sample exact.** Each chain may delay its sound (limiters, spectral effects). The graph holds every route back just enough that all of a bus's inputs arrive in step, so the mix is delayed by one fixed amount; the renderer runs that far ahead and the graph is primed with it. A click on a direct track and the same click through a bus with a 5 ms limiter land on the same sample (tested).
- **Routing order and loops (`session/routing.rs`).** Tracks and buses run in an order where everything comes after what feeds it, counting outputs, sends and sidechain keys. Any edit that would feed sound back into itself is refused ("That routing would feed the sound back into itself.") and changes nothing; the Mixer and Voice Chain lists leave such choices out. A damaged file with a loop still plays: the edge that closes the loop goes to the master.
- **Sends.** Each send has a level, its own pan, on/off and pre- or post-fader. Post-fader follows the fader but not the track pan. Pre-fader ignores fader and pan. Both follow mute and solo. Turning a send on or off needs no restart; adding, removing or retargeting one restarts playback in place, like other routing changes.
- **Solo through buses.** Soloing a bus plays it and everything that feeds it. Soloing a track plays it through its bus and its send buses. Exclusive solo (Settings) unsolos tracks and buses alike.
- **Sidechain.** A new optional `sidechain` field on an effect slot names the track or bus whose sound drives the effect's detector. Dynamics and Breath Gate use it (for example, music ducked under a voice, or a gate opened by another mic). The key is the source's sound after its Voice Chain and before its fader, so turning the voice down or muting it does not change the ducking. Inside a chain the key is held back by the look-ahead of the effects before it, so key and signal stay lined up. Clip chains do not take a key. This is the live version of auto-ducking the backlog mentions; the one-setting Auto-Duck dialog is still to do.
- **Ops (all undoable).** `add_bus`, `remove_bus`, `restore_bus`, `move_bus`, `set_bus`, `set_sends`, `output` on `set_track`, and a `bus` target for `set_effects`. Removing a bus sends what fed it to the master, drops sends to it and unkeys effects listening to it, and undo puts all of it back in one step. A key may keep pointing at a removed track, so undoing the removal brings the ducking back.
- **File format.** `sidechain` is optional and left out when unset; buses, outputs and sends were already in the format (scaffolding since P1), so old files open unchanged and there is no version bump. Schemas regenerated.
- **Interface.** Multitrack > Add > Bus Track (Alt+B, approved 2026-10-23). The Mixer shows bus strips after the tracks (tinted, marked BUS; right-click or double-click to rename, right-click to delete), and every track and bus strip has an output picker and its sends (drag a send's level sideways, P for pre-fader, the dot for on and off). The Voice Chain's Track Effects picker lists buses and the Master, and clicking a bus or the Master strip shows its chain, with meters. Dynamics and Breath Gate settings windows gain "Listen to". `.sesx` export writes every track straight to the mix and says how many tracks and buses lost their routing.
- **Editor (after the first tester run, who expected buses on the timeline and a send on every track).** Bus tracks show as rows below the audio tracks: a header with mute, solo, volume and pan, and a lane that holds no clips and says how to route to it. The header view switch's Sends button now works: every track and bus header shows its output and its sends, with "+ Send" on each. The track right-click menu gains Output To and Add Send To. The Add Tracks dialog can add stereo buses, the Editor toolbar has "+ Bus", and the right-click Track submenu adds mono or stereo buses. One send and output control (`SendList.svelte`) serves the Mixer and the Editor.
- **Not in this change.** Reordering or resizing bus rows in the Editor, Show Only Bus Tracks, hardware outputs in the output picker, track automation of bus levels, learning a noise print on a bus, and the Auto-Duck dialog.
- **Alt+Up and Alt+Down on clips (tester report).** Move to Track Above and Below (built in 0.1.6) did nothing when a Voice Chain row still had keyboard focus: the Editor's lanes stop the browser's default on press, so clicking a clip never took focus away, and the row used Alt+Up and Alt+Down to reorder its effects. A click on anything that cannot take focus now leaves the focused row, button or list, as a plain web page would (containers marked `tabindex="-1"` do not count as focusable). Moving clips also skips hidden tracks, the same as the Editor lists them.
- **Editor rows (tester requests).** Each track and bus header has a grab strip on its left edge, in the track's color, to drag it up or down (the whole header still drags too). Buses sit anywhere among the tracks: a bus stores how many tracks are above it (`position`, optional, display only), so moving a track never drags a bus along, and dragging a bus between two tracks is one undoable step. Tracks on the default color used to change color when moved, since the default follows the row; moving a track now fixes the current color of every default-colored track in the same step. Right-click a track for Track Color..., a bus for Bus Color... (the palette or any custom color). A Master row closes the Editor: its volume, its meter, and an fx button that shows the master Voice Chain.
- **Mono buttons (tester request, for mastering).** Tracks and buses have a Mono button (MO in the Editor and Mixer): it folds left and right into one centered signal after the Voice Chain and before the fader, pan and sends, so pan still places it. It is part of the mix (saved as an optional `mono` field, heard in exports) and applies live without a restart. The master's Mono button is a listening check instead: the audio thread folds everything that plays to mono, at once. It is not saved and never changes an export, so a check cannot end up as a mono master by mistake; a mono file is still a choice in Export.

## 2026-10-24: VST3 plug-ins

Proposed as a contribution, with the rest of the bus work. Follows the plan in `docs/backlog.md` ("Plugin hosting"): VST3 only, in a separate process.

- **Crash isolation.** Every plug-in runs in a host process of its own: the editor's executable started again with `--plugin-host` (or `--plugin-scan <bundle>` to read a bundle). No second program to install or sign. The hosting code is its own crate, `src-tauri/plughost`, because talking to plug-ins needs unsafe code, which the app crate forbids. A plug-in that crashes, or does not answer a block within 4 seconds, is bypassed (its dry sound still delayed by its look-ahead, so the track stays in time), the slot shows why in red, and Restart Plug-in starts it again. Scanning works the same way: a bundle that crashes while being read is listed as failed.
- **Pipes, not shared memory.** Audio goes over the host's stdin and stdout in blocks (a kind byte, a length, then interleaved f32 plus any control changes; JSON for everything else). Playback renders half a second ahead on the render thread, so a pipe round trip per block costs nothing audible, and it needs no unsafe code on the editor's side.
- **Live, and kept running.** Playback keeps one host per chain slot for as long as the session uses it, so a plug-in's window, settings and inner state carry on across stops, starts and chain edits (only the values that changed are sent; a chain holding a plug-in is not warmed up again on audio the plug-in already heard). Mixdowns and stems get fresh hosts of their own, loaded with the slot's saved settings, so an export never disturbs playback. Removing a slot from the session stops its host.
- **Settings.** Plug-in controls are stored like any effect's (`p<parameter id>`, normalized 0 to 1, so they can be automated); everything else set in the plug-in's own window is saved as `plugin_state` on the slot (the plug-in's component and controller state, base64; an optional field, so old files are unchanged). The interface polls running plug-ins twice a second: controls moved in a plug-in's window, and its settings after a change or when its window closes, come back into the session as one undoable "Plug-in Settings" step. Save, Save As and exports fetch every running plug-in's settings first. Values given at load or moved while stopped are handed to the plug-in with a zero-length block (allowed by VST3), so they count in saved settings before playback starts.
- **Latency.** Each plug-in reports its look-ahead; the mix graph compensates it like any effect's. When a plug-in changes it (switching modes), playback starts again from where it is so every track stays aligned.
- **Plug-in windows.** On Windows the host opens a plain window and lets the plug-in draw into it; a window that hangs never freezes the editor. Elsewhere, and for plug-ins without one, the plug-in's controls show in the effect window as sliders. Linux and macOS plug-in windows are later work, and macOS bundles are not loaded yet.
- **Plug-in Manager (Effects menu).** The standard VST3 folders (which can be turned off) and the user's own (Add Folder, Remove), Scan for new and changed bundles, Rescan All, every plug-in found (maker, kind, look-ahead, whether it has a window) and every bundle that failed, with the reason. Results are cached in `plugins.json` in the app's config folder. Plug-ins show in the Voice Chain's Add effect list under Plug-ins (VST3), on clips, tracks, buses and the master.
- **Not in this change.** Monitoring a live input through a plug-in while recording (it needs a low-latency path of its own), sidechain inputs on plug-ins, instruments (they need notes: added the same day with MIDI tracks, below), plug-in presets in the Voice Chain preset list, and immersive formats (an object panner such as an Atmos one needs multichannel buses and a renderer; the app is stereo).
- **Tests.** A test plug-in built from the `vst3` crate's example (gain, a 32-sample look-ahead, saved settings and a crash switch) is built by the tests and put in a real bundle. The host crate's test scans it, processes audio, checks look-ahead, control changes, block splitting, saved settings and a crash; the app's test scans a user folder, mixes through the plug-in with the editor's own executable as host, checks timing and level, restores settings in a fresh host, and checks that a crash or a missing plug-in leaves the mix playing. The Windows window code is checked to compile for Windows; it has not run on Windows yet.

## 2026-10-24: MIDI tracks and VST3 instruments

Proposed as a contribution with the bus and plug-in work (the tester asked for MIDI tracks with tempos, notes and a VST3 instrument).

- **MIDI tracks are tracks.** A track has a kind (`kind: "midi"`, left out of the file for audio tracks), so a MIDI track has everything a track has: Voice Chain, fader, pan, mute, solo, mono, output, sends, and it can key a sidechain. It holds MIDI clips instead of audio clips (the core refuses the wrong kind with "Audio clips go on audio tracks, MIDI clips on MIDI tracks.") and one optional instrument, an effect slot holding a VST3 instrument.
- **Beats, not samples.** MIDI clips and their notes are placed in beats (`start`, `length`, and each note's `pitch`, `velocity` 1 to 127, `start` and `length`), against one session tempo and meter (`tempo`, default 120 BPM in 4, left out of the file at the default). Changing the tempo moves the notes with it. Tempo changes along the timeline are later work.
- **Ops (all undoable).** `add_track` with `midi: true`, `set_midi_clips` (the whole list for a track: every clip edit, the piano roll's included, is one step) and `set_tempo`; `set_track` takes `instrument` and `clear_instrument`. Old files open unchanged; schemas regenerated.
- **How notes become sound.** The renderer plays each MIDI track's notes through its instrument, and what the instrument plays is the track's sound, before its Voice Chain. Notes go to the instrument as early as its look-ahead, the way clip chains read early, so an instrument with look-ahead stays in time and the mix graph does not change. Starting or jumping into the middle of a note plays it from there; a loop or a skip lets every note go. The instrument is told the tempo. Mixdowns get a fresh instrument of their own, like any plug-in. The session's end counts MIDI clips, so playback and export reach the last one. A track without an instrument (or with one that is missing or crashed) is silent and nothing else breaks.
- **Host protocol.** Audio blocks to the plug-in host now carry note events (frame offset, on or off, pitch, velocity); the host hands them to the instrument as VST3 events, split across blocks at their frames. `Tempo` is a new request. A plug-in with a note input and no audio input counts as an instrument (also any whose category says Instrument): the scan now lists instruments instead of skipping them, the Voice Chain's effect list leaves them out, and MIDI tracks offer them. `plugins.json` goes to version 2 so the next scan reads every bundle again and finds instruments in bundles scanned before.
- **Interface.** Multitrack > Add > MIDI Track, "+ MIDI" in the Editor toolbar and MIDI in the Add Tracks dialog (no new shortcuts). A MIDI track's header picks its instrument from those scanned and opens the instrument's window; Record and input monitoring are hidden on MIDI tracks. The lane shows bars and beats and each clip's notes; double-click empty space for a one-bar clip, drag a clip to move it and its right edge to change its length (both snap to beats), right-click for Edit Notes, Duplicate, Mute, Rename and Delete; Delete and Select All work on MIDI clips too. The piano roll (double-click a clip) adds a note on click, moves and resizes notes on a chosen grid (bar, beat, 1/8 to 1/32, triplets, or off), deletes with double-click, right-click or Delete, and sets velocity in its own lane or for the picked notes; Done or Escape keeps the changes as one step, Cancel drops them. The tempo and beats per bar sit in the Editor toolbar. Settings changed in an instrument's window come back into the session (as for effects) and are saved with it.
- **Not in this change.** See the backlog: a free (non-snapping) stretch for MIDI clips once clip stretching covers them (requested by the tester), hearing notes while drawing, recording from a MIDI keyboard, `.mid` files, tempo changes along the timeline, quantize, controllers and sustain.
- **Tests.** The test plug-in gains a test instrument ("Altered Test Synth": no audio input, a note input, a steady level while any note is held, from the note's exact frame). The host crate's test plays notes across blocks and through a reset; the app's test scans the bundle (the synth is listed as an instrument and kept out of the effect list), mixes a MIDI track through it and the gain plug-in on its Voice Chain, and checks the note starts and stops on the exact sample, plays from the middle, follows a tempo change, sets the session's end, and that no instrument means silence.

## 2026-10-24: plug-in scans no longer hang

- **Tester report:** a scan sat for ten minutes and found nothing; a single bundle "took too long to load and was skipped". The scanner waited for each scan process to finish before reading what it printed. A plug-in with many controls prints more than a pipe holds (about 4 KB on Windows), so the scan process waited for the editor to read it while the editor waited for it to finish, until the one-minute limit. Real plug-ins nearly all have that many controls; the test plug-in has two, so the tests never filled the pipe. The scanner now reads while the scan runs (tested with a 300 KB answer).
- Bundles are now read four at a time, the Plug-in Manager shows how far the scan is and which bundle it is reading, and plug-in processes start COM and OLE on Windows, as Windows plug-ins expect of their host.
- **Second report: "It crashed while loading, so it was skipped."** The scan opened a bundle once to list its plug-ins and again inside each plug-in it read; closing that second copy told the plug-in's code to shut down (`ExitDll`) while the first copy still used it. The VST3 SDK counts these calls, so the test plug-in never minded, but plug-ins that do not count them (JUCE-based ones, which is most commercial plug-ins) shut down under the scan and crashed it before it answered. A bundle is now opened once and shared, and nothing is closed before the scan answers.
- **Scans keep what they got.** The scan process now prints one line per step: the bundle's plug-ins by name first, then each one as it is read. A plug-in that crashes part way is still listed (marked ⚠ in the Plug-in Manager, with where it stopped and the Windows crash code in words, such as "a file it needs is missing (DLL not found)"); its controls and look-ahead are learned when it runs. A bundle that dies before naming anything fails with the same detail. Text plug-ins print themselves is ignored. Failed and partly read bundles are tried again at every Scan, and `plugins.json` version 3 rereads everything once.

## 2026-10-24: plug-ins open in their own window, a shorter Add effect menu

- **Tester requests, restated as behavior** (the request compared both to another editor; per the guardrails, reframed here): opening a plug-in should show the plug-in's own interface, not a list of sliders; and adding an effect should be a short menu of families that open as submenus, not one long list.
- Clicking a plug-in's name or its settings button (and adding a plug-in) opens the plug-in's own window. The slider list is still there: the ☰ button, or Show Controls in the slot's right-click menu. A plug-in with no window of its own, or one that stopped (so Restart Plug-in can be reached), opens the slider list as before.
- "+ Add effect" is now a button that opens the effect families as submenus (the same menu as the slot right-click's Replace With). Plug-ins sit under VST3, grouped by type (from the plug-in's own categories, such as Reverb or Dynamics; Other when it has none) and then by maker. VST (version 2) stays listed as not supported.
- Add MIDI Track is in the Editor's right-click menus too: the empty lane's Track submenu and the track header menu.

## 2026-10-24: plug-in scans can be stopped, and no longer wait on helper programs

- **Tester report: a bulk scan froze.** A scan process that took too long was ended, but the editor then waited for its output pipes to close. Some plug-ins start helper programs (licence checks, background services) that inherit those pipes and keep them open after the scan process is gone, so the wait never ended and the scan never finished. Now, once a scan process has ended, the editor takes what it printed within two seconds and moves on.
- A scan process that has printed its last line is done, even if the plug-in then hangs while the process closes (some do, unloading their code); the editor ends it at once instead of waiting out the minute.
- **Stop Scan.** The Plug-in Manager's Scan button becomes Stop Scan while scanning, and closing the window during a scan stops it too. Running scan processes are ended, no new bundles start, everything read so far is kept, and bundles not reached are read at the next Scan.
- Tests: fake scanners that leave a helper holding the pipes, hang after finishing, or are stopped part way all return within seconds.

## 2026-10-24: tester round (track picking, Waveform zoom, file-only imports, piano roll, instrument picker)

- **Instrument picker did nothing.** Pressing on a track header starts a row drag unless the press is on a control, and dropdowns were not counted as controls, so the instrument list (and the automation mode list) never opened. Dropdowns, labels and links now count.
- **Which track the Voice Chain works on.** The row whose chain the Voice Chain panel shows (track, bus or master) gets an accent outline and an FX tag, so it is always clear where effects go. Picking a track in the panel's own list now selects it in the Editor too. The grab strip's tooltip says a click selects the track.
- **Waveform editor zoom.** Zooming out stops when the whole file fills the view, instead of showing empty time past the end. Zooming in now goes down to 32 pixels per sample. Past 8 source frames per pixel the waveform is drawn from the file's exact samples (read by a new core command, `media_frames`), as a line through them with a dot on each sample once they are 6 pixels apart, through each piece's gain, envelope and fades. Looped and stretched pieces keep the peak drawing. The ruler gains steps down to 0.1 ms.
- **Importing files without a session.** The app always has a session to hold the Files panel. A never-saved session with nothing in it (no clips, MIDI clips, markers, buses or effects) is no longer listed as Untitled Session, marked unsaved, autosaved or offered for saving, even after files are imported; the files simply sit in the Files panel for the Waveform editor. As soon as something goes on the timeline it is a session as before.
- **Piano roll.** A Snap switch next to the grid size. Dragging near a note's left end stretches its start (the end stays), near its right end its end, the middle moves it. Shift-click and Shift-drag (a box) pick several; moving or stretching one picked note does the same to all of them, with the grabbed note landing on the grid. Copy, Cut and Paste (buttons and Ctrl+C, Ctrl+X, Ctrl+V, standard shortcuts inside the dialog) paste at the last spot clicked, marked with a line; pasting past the clip's end lengthens the clip to the next bar line. The grid runs two bars past the clip's end, shaded, so notes there can be seen and moved back.
- **Fix (same day, tester report: no tracks could be added).** The Editor's start screen showed for any never-saved session without audio clips, markers or buses, hiding its track rows. So after File > New Session, or after adding mono, stereo or MIDI tracks, the tracks never appeared and files dropped there did not land. The empty-session rule above had also applied to new sessions made on purpose. Now both apply only to the blank session the app opened with (`SessionStore.isScratch`): never saved, its tracks as they were at launch, and nothing in it. A session made with New Session or opened from a file, or any track or clip added, shows the tracks and counts as a session to save.
- **Fix (same day): zoomed-in Waveform went blank on Windows.** Raw bytes from the core come back to the interface as an ArrayBuffer in some webviews and as a typed view or plain array in others (the peak cache already allowed for this); the exact-samples reader assumed an ArrayBuffer, failed on Windows, and drew nothing in its place. A shared `rawBytes` helper reads all three. The peak drawing now stays up whenever exact samples are not ready or cannot be read, and a file that cannot jump to a spot is read from the start instead.
- **Voice Chain tint (tester request).** The Voice Chain panel takes the color of what it is working on: the selected clip's color (or its track's) on Clip Effects, the track's or bus's on Track Effects, the accent for the master. Its surfaces and outlines are mixed from the theme's dark values and that color, so the lighter and darker parts keep their order. There is one dark theme; a light theme would use a lighter mix. The FX tag in the Editor stays.
- **Fix (same day): new tracks looked like they were not added.** They were, below the rows on screen, and the Editor did not scroll to them, so with six tracks filling the view nothing seemed to happen. A newly selected track or bus (a new one is always selected) now scrolls into view when it is off screen. Found by running the real app under a virtual display and driving it over WebDriver (tauri-driver).
- The Editor's FX tag sits on the grab strip, written sideways, so it no longer covers the automation list.

## 2026-10-24: Phase Wheel panel (tester request)

- **Restated as behavior** (the request pointed at another editor's panel): a panel that shows, while audio plays, how two signals line up frequency by frequency, with a choice of which two and of how many samples are analysed, so slower computers can use a lighter setting. Our own design and name: Phase Wheel, in the Window menu, not in the Default layout.
- **Capture.** The audio thread writes each output frame (after the master fader and the mono check) into a fixed ring of 32,768 frames of atomics: two relaxed stores a frame, no allocation, no locks (`engine/scope.rs`). Reads may tear against a write; for a picture that is fine.
- **Analysis (core, `scope_phase`).** The newest 1,024 to 16,384 frames, Hann-windowed, both signals through a real FFT. Per bin: the phase of one relative to the other, the level (0 at -72 dBFS, 1 at full scale) and the pitch on a log scale. Plus the correlation over the window. Signals: left, right, mid (L+R) and side (L-R); the app plays in stereo, so there are no surround channels to pick yet.
- **Picture.** Each frequency is a dot: its angle is the phase (straight up when the two move together, straight down when opposite and they would cancel in mono, ±90° to the sides), its distance from the center its level, its color its pitch (red low to violet high). Dots leave a short trail. A correlation bar under the wheel runs from -1 to +1. It refreshes about 15 times a second while anything plays (session or Waveform editor) and holds the last picture when stopped.
- Tests: in-phase, opposite and quarter-turn tones give 0, half a turn and +90° at the right level, correlations +1, -1 and 0; the ring returns the newest frames in order after wrapping.
- **Vertical zoom bar (tester request).** The track scrollbar on the Editor's right edge always shows its thumb (full height when every track fits), with grips on both ends. Dragging an end zooms vertically like the timeline's zoom bar: the thumb covers the share of the session that fits the view, and every open track takes the same height so that it does, previewed live and applied as one undoable step, keeping the other end in place. Double-click the bar to fit every track in the view at one height. Before, the thumb only appeared when tracks overflowed and its ends scaled each track by its own size.
- **New Session names.** The suggested name is the first "Untitled Session N" with no session folder of that name in the folder it will be saved in (`session_next_name`), instead of a count since the app started.
- **Fix (same day): adding a MIDI track froze the Editor (tester report).** A stereo track added fine; a MIDI track did not show, nothing after it updated, and the track headers stopped drawing part way down. The MIDI track header lists the instruments, keyed by effect id, and Svelte throws on two items with one key. On a real system the same plug-in is often installed twice (two scanned folders, or an old copy beside a new one), giving two registry entries with one class id. The registry now keeps one entry per class id (the fully read copy, else the first), on every scan and when `plugins.json` loads; the Editor's instrument list also drops duplicates, and the Plug-in Manager's lists no longer key by id, so a list can never break on them again.
- Errors nothing else catches (in an interface update, an effect or a promise) now show in the status bar, and a session listener that throws is reported and skipped instead of stopping the others, so a fault is visible and the rest of the interface stays current.

## 2026-10-24: instruments that load slowly, wheel zoom, analysis colors (tester round)

- **No sound from VST instruments (tester report).** Play waited 5 seconds for playback to be ready, and the first time an instrument (or any plug-in) played, it was loaded inside that wait. Real instruments often take longer (licence checks, sample libraries), so playback gave up ("The audio device did not respond") and nothing sounded. The test instrument loads in milliseconds, so the tests never saw it. Now plug-ins and instruments start loading in the background as soon as they are in the session (every edit, and opening a session; `instance::warm`), one start per slot (a second request waits for the first rather than starting another), and Play waits up to 30 seconds when the session has plug-ins. Tested: an instrument warmed in the background is running without playback asking for it.
- **Mouse wheel (tester request).** Over the Multitrack lanes the plain wheel now zooms in time around the pointer, as in the Waveform editor; over the track headers it scrolls the tracks; Alt+wheel still resizes tracks and Shift+wheel scrolls in time. A switch for it (on by default, shared with the Waveform editor) is now in Settings > General.
- **Phase Wheel.** It redraws on each screen refresh while playing, up to 60 times a second (Refresh: 15, 30 or 60, for slower computers), one request in flight at a time. Dot colors now follow real frequency, 20 Hz to 20 kHz on a log scale (the output rate is noted when playback starts), and run around the whole color wheel from red to magenta by default, with a key under the wheel.
- **Analysis colors (tester request).** The spectrogram (quiet and loud) and the Phase Wheel (low and high notes) each take two colors, blended around the color wheel between them: right-click the spectrogram (Spectrogram Colors...) or the Phase Wheel, or Edit > Settings > Appearance. Saved with the app's settings; Default goes back to the built-in colors (the spectrogram keeps its original palette until a color is picked).

## 2026-10-24: tester round (Phase Wheel layout, follow playhead, close zoom, drop preview, copy by dragging)

Several requests compared behavior to another editor; per the guardrails they are restated here as behavior, in our own design.

- **Phase Wheel, laid out by frequency.** On the tester's phase-shifted file the right channel's phase against the left rose smoothly from about 4° at 20 Hz to 133° at 20 kHz, fully coherent. The first wheel put loudness on the radius, drew one dot per FFT bin and measured the angle the other way, so a clean relationship came out as a scattered cloud turning the wrong way, with the colors not matching the key. Now the distance from the center is the frequency (20 Hz in the middle to 20 kHz at the ring, the same log scale as the color key), the angle is the phase of "Compare to" against "Signal" (clockwise when it leads), and the spectrum is split into 128 log-spaced bands, each measured over all its bins (summed cross spectrum), so a steady relationship is one steady dot per band and a delay or phase shift draws one clean curve. Brightness is the band's level; dot size how steady its phase is across the band (coherence).
- **Follow Playhead lets go.** During playback, moving the view away from the playhead by hand (overview bar, wheel, scrollbar, zoom) pauses following until the playhead is back in view or playback starts again, in the Multitrack and the Waveform editor. The follow's own paging does not count as a move.
- **Playhead triangle.** The ruler canvas was set to the lanes' width but stretched by `flex: 1` to fill its row, which became wider than the lanes when the track scrollbar grew; the triangle drifted right of the line, more so toward the right. The ruler now keeps exactly the lanes' width.
- **Close zoom in the Multitrack.** It now zooms in to 32 pixels per sample, as the Waveform editor does, and past 8 source frames per pixel each clip is drawn from its file's exact samples (a line, with a dot on each sample once they are 6 pixels apart). Exact samples come from one shared cache (`stores/exact-samples.ts`); looped and stretched clips keep the peak drawing.
- **Drop preview.** While files dragged from the system or within the app hover over the lanes, the Editor shows where each would land: a box per file, one after another from the pointer, on the track under it or as a new track below the last, with its name and length. The drop router passes the hover position and the files to the panel (`onMove`). Audio dropped on a MIDI track now goes to a new track instead of failing.
- **Copy by dragging: Alt+drag a clip (proposed; a new mouse gesture, for the project lead to approve).** Holding Alt when releasing a clip drag puts copies where it was dropped, on any track, and leaves the originals; grouped clips copy as a new group; "Copy" shows while dragging. Alt on a clip edge still time-stretches it. One undoable step.
- **Files panel (tester request).** Removing files that clips on the timeline use asks first: remove them with those clips (one undoable step), keep them (removing only the unused ones), or cancel. Before, used files were silently skipped, so nothing could be removed from a session in use. Dragging from empty space in the list draws a box that picks every file it touches (Ctrl or Shift adds to the picked ones). The panel's toolbar gains New Session and New Audio File, before Import Files and Import Folder.
- **CI gate.** The privacy check flagged a method named `fetch` in the exact-samples cache (a name only; it calls the local core). Renamed; local checks now gate on each command's exit status rather than on its printed summary.

## 2026-10-24: clip pitch and stretch types, Phase Wheel follow and density, ruler scrub, files kept across sessions

- **Restated as behavior** (the request pointed at another editor's clip settings): a clip can change its pitch in semitones (-24 to +24) without changing its length, and its stretch picks how it works: keep the pitch for a single voice or instrument (monophonic), keep it for music with several notes at once (polyphonic, longer analysis frames), or change pitch with speed like tape (varispeed). A clip can also be marked to be worked out ahead (rendered) rather than live. Playback and export sound the same either way here, so the mark only keeps files from other editors intact.
- **How pitch works.** The file is read faster by 2^(semitones/12), which raises the pitch and shortens it, then the pitch-keeping stretch lengthens it by the clip's ratio times that factor, so it fills the clip again. The stretch range for this inner step is 1/8 to 8. Finer settings (analysis window, transient sensitivity, formants, precision) are stored; only the window changes the sound for now.
- **Phase Wheel.** 1,024 bands instead of 128, so the wheel fills in like a cloud; dot size follows the level. While stopped it analyses the mix (or the Waveform editor's file) just before the playhead, rendered in the core (`scope_phase_at`), and follows the playhead as it moves. Its choices (signals, samples, refresh up to 120 a second) live in the settings, so every file and session uses them.
- **Ruler.** Click or drag on the ruler and the playhead follows the mouse (both editors). While playing, playback moves there on release. The middle mouse button still slides the timeline. Replaces the earlier drag-to-slide on the ruler, at the tester's request.
- **Files across sessions.** A new or opened session adds its files to the Files panel and keeps those already there, instead of replacing the list. A file listed twice takes the session's id, which its clips use. Close Unused Files clears the rest.

## 2026-10-24: .sesx plug-ins, buses and stretch; drop a session to open it

- **Import.** VST3 plug-ins in effect racks on clips, tracks, buses and the master come across in slot order with their saved settings and on/off state. In the file, a plug-in's settings are base64 of the component and controller lengths as 64-bit numbers, then both parts; we turn that into our own form (a 32-bit component length, then both parts). The class id is written as four groups of eight hex digits; its byte layout differs by platform, so the importer tries both against the scanned plug-ins and otherwise uses this platform's. Plug-ins not scanned on this computer still come in, keep their settings and are named in the notice, so a scan or install brings them back. The other editor's built-in effects cannot be hosted; they are counted and named in the notice.
- **Buses and routing.** Bus tracks become buses (name, level, pan, mute, effects), placed where they sat among the tracks, and each track's output follows. Master level and effects come across. Automation is still left out.
- **Stretch.** A clip's stretch element brings its ratio, pitch (semitones), type (single voice, several notes, or varispeed), live or rendered mode and its finer settings. A stretched clip's source points are stored in stretched time in that format, so the importer divides them by the ratio (checked against the tester's session, where the same take appears stretched and not).
- **Export** (File > Export > Multitrack Session) writes the same things back: stretch and pitch, and VST3 plug-ins with their settings on clips, tracks and the master. Built-in effects, buses and loops are still left out and counted.
- **Drop to open.** Dropping a `.altaudio` or `.sesx` file anywhere on the window opens it, with any audio files dropped along with it, the same as File > Open.

## 2026-10-24: Properties panel and stretch handles

- **Restated as behavior** (the request pointed at another editor's clip settings panel and its corner handles): a panel that shows the selected clip's details (start, length, natural length, file) and its stretch and pitch: mode (Off, Live, Rendered), type (Single voice, Several notes, Varispeed), stretch in percent, pitch in semitones with -1 and +1 steps and Reset, and finer settings under Advanced. With several clips selected a change applies to all of them, as one undoable step. Our own name: Properties (Window menu, and Clip > Stretch and Pitch..., also on the clip right-click menu).
- **Stretch handles.** A toggle in the timeline options (beside Follow Playhead) shows a white triangle in each top corner of every clip that can stretch. Dragging one stretches the clip from that side, the same as Alt+drag on the edge, without holding Alt. The setting is saved with the other editor settings. Clip titles show the stretch and any pitch change ("125% +3 st").
- **Fix.** Changing a clip's length kept only the speed and pitch mode and dropped any pitch change and type; it now keeps them. Remove Stretch (and Mode Off) returns the clip to natural speed and pitch.

## 2026-10-24: loading speed, plug-ins side by side, clips on top cover, wheel

- **Loading.** Measured on a stand-in for the tester's session (80 files, 23 tracks, 259 clips, 27 plug-ins): reading the `.sesx` takes about 30 ms and building every waveform cache about 1 to 2 seconds in the core. The slow part was the interface: each file's waveform arriving redrew every track lane (80 files times 23 lanes). A lane now redraws only when one of its own files changes, which halved the time the window was busy. Waveform caches build up to four at a time (was two). Plug-ins in an opened or imported session start loading a few at a time (half the processor's cores, two to six) instead of all at once, so dozens of heavy plug-ins no longer slow the whole computer while the session opens.
- **Plug-ins side by side.** Each plug-in runs in its own host process and the mix waits for its answer. Tracks whose chains hold a plug-in, and that listen to no other track, now run their chains at the same time on separate threads before the rest of the mix, so a session with many heavy plug-ins no longer adds up their times every block. Buses, sidechained tracks and the master still run in order.
- **Restated as behavior** (the request pointed at another editor's overlapping clips): on one track, a clip on top covers the clips under it. They come through only where the top clip fades: under its fade in they fade out as it fades in, and under its fade out they fade back in. Where the clip underneath has its own fade across the same span (a crossfade, as the automatic crossfade makes), both play through their own fades. A later clip in the track's list is on top, as drawn. This replaces layering, where a clip fully inside another played along with it. Imported `.sesx` clips keep their stacking order.
- **Wheel.** Over the ruler and the overview the wheel zooms in time; over the lanes and track headers it scrolls the tracks up and down; over the track scrollbar it zooms vertically (every open track the same height, one undoable step once the wheel stops). Ctrl + wheel still zooms in time anywhere, Alt + wheel still resizes the track under the pointer. The "wheel zooms" setting now applies to the Waveform editor only.

## 2026-10-24: track header Effects view, routing in the header, effects in any slot

- **Restated as behavior** (the request pointed at another editor's track header): the header's Mix view grows with the track. A taller track shows its input, then its output (the master or a bus), then its automation mode, then its sends with "+ Send". The separate Sends view is gone; buses show their output and sends the same way.
- **Effects view** (the fx button above the headers): each track's and bus's Voice Chain in its header: the chain's power, then the numbered slots, each with its power and name. Click a name to open the effect (a plug-in in its own window), the arrow or an empty slot to add, replace, bypass or remove. Drag an effect to another slot to move it, or onto another track's or bus's slots (or the Voice Chain panel) to copy it with every setting, plug-in settings included. Each change is one undoable step.
- **Effects in any slot.** An effect can sit in any of the 16 numbered slots, with gaps (`position` on the slot; effects still run top to bottom in list order). Every empty slot in the Voice Chain offers "+ Add effect" for that slot (later ones show on hover). Moving an effect into a taken slot moves the one there down. `.sesx` import keeps each plug-in's slot number, and export writes it.
- **Dragging uses pointer events**, not the browser's drag and drop: the window's file drop turns that off on Windows, which is why reordering effects in the Voice Chain did not work there.

## 2026-10-24: instrument picker grouped like the Add effect menu

- A MIDI track's instrument is picked from a menu instead of a flat list: instruments by type (Piano, Synth, Sampler and so on, from the plug-in's own categories), then by maker, with No Instrument at the end and the current one checked. The word "Instrument" is left out of the type, as "Fx" is for effects, since every entry is one.

## 2026-10-24: hear notes while editing, right-drag erase, menus that fit the window

- **Hearing notes.** In the piano roll a note sounds through the track's instrument when it is drawn, picked up, moved to another pitch or given a new velocity, and the keys on the left play their note when clicked. The core plays it on the preview output (`midi_audition`): note on, held for the note's length (0.15 to 1 second), note off, then a short tail. The preview player now takes generated sound as well as files. A note cut short by the next one is still let go, so it never hangs on. Nothing plays while the session is playing, since the instrument is busy then. The note is heard without the track's effects.
- **Erasing.** Holding the right button and dragging over notes erases each one it passes over (a right-click still deletes one). The changes stay one undoable edit with the rest of the piano roll's.
- **Menus.** A menu or submenu taller than the window scrolls instead of running off the bottom, and submenus are placed against the window beside their row (left of it when there is no room on the right). Scrolling a menu closes its open submenu. Plug-in names sort with numbers in order ("2" before "10").

## 2026-10-24: a reverb plug-in on a MIDI track (tester report)

- The tester heard no reverb from a VST3 reverb on a MIDI track during playback, while built-in effects worked. Checked with a real open-source reverb (Dragonfly Plate Reverb, from the distribution's packages, unpacked into a scratch folder) after the test synth on a MIDI track: its tail rings in export and in playback alike, so the mix path itself is sound (`a_real_reverb_rings_on_a_midi_track`, run with REVERB_DIR set).
- One weak spot fixed on the way: when a plug-in reported an error from its processing call, the host passed the dry sound through even if the plug-in had filled its outputs. Some plug-ins do report errors while processing normally, so the host now keeps whatever the plug-in wrote, and passes the sound through untouched only when nothing was written.
- Still open: which reverb and instrument the tester used, to reproduce it on Windows.

## 2026-10-24: mixdown with only MIDI tracks

- Multitrack > Mix Down to New File (and File > Export > Session Mixdown, Go to End, Zoom Out Full) measured the session by its audio clips only, so a session with only MIDI tracks showed them greyed out ("no clips yet"). They now use the session's end with MIDI clips included, as the core's mixdown already did.

## 2026-10-24: Mix Down to New File opens in the Waveform editor

- **Restated as behavior** (the tester asked for the mixdown to open as a file in the app rather than asking where to save it): Multitrack > Mix Down to New File (Entire Session, Time Selection, Selected Clips) renders the mix into a new untitled 32-bit float file and opens it in the Waveform editor, with no save dialog, the same way Paste to New File works. It counts as unsaved work; Save As puts it on the drive. Plug-ins' latest settings are saved into the session first, progress shows as for an export, and it can be cancelled. File > Export > Session Mixdown still writes straight to a file of your choice.

## 2026-10-24: drag MIDI clips between MIDI tracks

- A MIDI clip dragged up or down onto another MIDI track moves there, keeping its notes and its place in time (plus any sideways move, snapped to beats); that track's instrument plays them. With Alt held it is copied instead (new ids), the same proposed Alt+drag copy gesture as audio clips, which still needs the project lead's approval. The track under the pointer is outlined while dragging. Both tracks change in one undoable step. Audio and bus rows are not drop targets for MIDI clips.

## 2026-10-25: spectrum zoom and options, zoom toward the pointer

- **Restated as behavior** (the request pointed at another editor's spectral display and its right-click menu): the mouse wheel over the Waveform editor's spectrum zooms time, as over the waveform. Over its frequency scale it zooms the frequencies, keeping the frequency under the pointer in place, never past 0 Hz or the Nyquist frequency of the file's sample rate, never narrower than 40 Hz. A right-click on the spectrum offers Zoom In, Zoom Out and Zoom Reset (Frequency); Hertz or Notes on the scale; More Logarithmic, More Linear, Full Logarithmic and Full Linear (five steps between linear and log); Increase and Decrease Spectral Resolution (analysis frames from 512 to 16,384 samples, 2,048 by default); and Spectrogram Colors. No shortcuts are added. The view settings last for the run of the app, across files.
- **How.** The core's spectrogram takes the view's row edges in Hz and a frame length; without them it draws the edits' fixed log axis as before. Spectral selections stay on that fixed axis, so a box, lasso or brush stays on the same sounds while the view zooms or changes scale.
- **Zoom toward the pointer.** Over the time ruler (Waveform editor and Multitrack) the wheel always zooms toward the pointer, whatever the wheel anchor setting says for the lanes, and the Waveform ruler zooms even with "wheel zooms" off. The wheel over the track scrollbar zooms the track heights toward the track under the pointer: that spot keeps its place in the view.

## 2026-10-25: Parametric EQ graph editor

- **Restated as behavior** (the request pointed at another editor's parametric equalizer): the Parametric EQ's window opens with a large graph of its response over the sound. Each band is a node: drag it sideways for frequency and up or down for gain (the high-pass and low-pass move sideways only). A band's width (Q) shows as a dashed span with a handle at each end; drag a handle, or turn the mouse wheel over the node, to narrow or widen it. Double-click the high-pass or low-pass node to switch it on or off. Hovering or dragging shows the band's frequency, gain and Q. Behind the curve is the spectrum of what is playing (after the whole chain and the master, 20 Hz to 20 kHz, -96 to 0 dBFS), live while playing; when stopped, the audio just before the playhead (the session's, or the Waveform editor file's). Each drag is one undoable step, the same as the controls below the graph.
- **How.** A new core command (`scope_spectrum`) analyses the playback ring while playing, or renders the mix before the playhead while stopped (the same render the Phase Wheel uses when stopped). The graph is its own component (`EqGraph.svelte`, maths in `eq-graph.ts`); the Voice Chain panel only chooses it for the Parametric EQ, so the change there is small.

## 2026-10-25: Phase Curve

- **Restated as behavior** (the request pointed at another editor's per-frequency phase shifter): a new effect, Phase Curve (Filter and EQ), turns the phase of each frequency by its own angle along a curve of up to eight points, leaving every frequency's level as it was. It is for restoration: lining up a second mic that is out of phase only in some frequencies, or undoing a filter's phase smear. Our own name and layout.
- **The curve.** Points are frequency and angle. The curve runs straight between them on a log frequency scale, or smoothly through them with Spline on (the default), and holds level beyond the first and last points. Range sets ±180° or ±360°. Channel picks both, left or right. FFT size (1,024 to 65,536 samples, 8,192 by default) sets the frame length: longer follows the curve more finely in the low frequencies, at more look-ahead (reported as latency, so the chain stays in time).
- **Ready for automation.** Each point is two ordinary numeric controls (`p1_freq`, `p1_phase` up to `p8`), plus `points` for how many are in use, so the points can be automated like any other control once automation recording arrives; the effect already takes new points, spline, range and channel while playing. Only a new FFT size rebuilds it.
- **Graph editor.** Drag a point; click empty space to add one; double-click or right-click a point to remove it (two always stay); Reset gives a flat curve at 0°. Controls for unused points are hidden below the graph.
- **.sesx import.** The other editor's phase shifter now comes across as Phase Curve. Its saved settings, read from the tester's sessions, are 32-bit floats: the sample rate; the axis ends at 6 and 7 as shares of half the sample rate; the point count at 10; then each point as two numbers from 0 to 1, position along the axis and phase (0.5 is 0°). The axis is taken as a log scale and the phase range as ±180°; neither is confirmed, and two other settings in it are not yet understood, so channel and FFT size keep our defaults. More than eight points are thinned evenly. The import notice says how many came across.

## 2026-10-25: Effect windows use dials and value fields

- **Restated as behavior** (the request asked for effect windows that look more like another editor's): an effect's settings window shows each setting as a small dial with a number box under it, one column per band or group (High-pass, Band 1, Point 1 and so on), instead of a long list of sliders. A whole Parametric EQ fits under its graph with no scrolling. The look is our own: the same dials as the track headers.
- **Using them.** Drag a dial or its value up or down to change it (Ctrl for fine steps, Shift for coarse), or turn the mouse wheel over the value. Click the number box to type a value ("1.5k" is 1,500 Hz); Enter sets it, Escape leaves it. Double-click a dial for its default. A group's On switch is the dot in its heading; switched off, its controls dim. Choices stay drop-down lists and other switches are On/Off buttons. A drag or a wheel turn is one undoable step.
- **How.** `EffectControls.svelte` and `ValueField.svelte`, with the maths in `effect-controls.ts`. The Voice Chain panel only swaps its slider list for the new component, so the change there is small.

## 2026-10-25: Sessions keep their audio in their own folder

- **What.** A saved session can hold every audio file it uses in its own folder, so the folder can be moved, backed up or handed over whole. Importing audio into a saved session copies it into an `Audio` folder beside the session file, and the session and the Files panel use the copy. The original is never moved or changed. Files already anywhere inside the session folder (recordings, earlier copies) stay where they are. Importing the same file again finds its copy (same name, size, and first and last 64 KB) instead of copying twice; a different file with the same name gets a numbered name. A copy is written under a temporary name first, so a copy cut short never looks finished.
- **Settings > Session Media.** Copy imported audio always (the default), ask each time, or never; the folder name (Audio by default); "When saving, copy in audio still outside the session folder" (on by default), which catches files imported before the first save and working files such as untitled mixdowns; and "Leave library files where they are" (on by default, since libraries keep their own copies).
- **File > Collect Session Media** copies everything the session and the Files panel use into the folder at any time, library files included. Collecting is one undoable step: Undo points the session back at the originals (the copies stay on disk).
- **How.** The core copies (`media_copy_in`, `session/collect.rs`). The Files panel gets a placing step that runs before new files are listed, so every import path (the Files panel, drops on tracks, Insert Files, bounces) uses the copy. Moving existing media is `set_media` ops plus a Files panel repath, combined into one command. Waveform drawings are rebuilt once for the copies, since their cache is keyed by path.

## 2026-10-25: Editing one channel in the Waveform editor

- **What.** A stereo file shows an L and an R button beside the level scale. Switching one off dims that channel, and edits then reach only the channel still on: Amplify, the level HUD, Normalize, Silence, fades, Reverse, Invert, Voice Chain apply and spectral edits. Delete and Cut silence the active channel instead of removing time, since one channel can't get shorter than the other. Copy with one channel on remembers which channel it came from. Paste with one channel on writes over that channel from the selection or playhead, keeping the file's length, so copying the left channel onto the right is: right off, select, Copy; left off, right on, Paste. A one-channel copy pasted with both on fills both. Edit > Activate Left Channel and Activate Right Channel (Up and Down, from the approved table) pick one channel alone, or both again. Switching both off is not possible: switching off the last one switches the other on. Each file opens with both on. Playback is not affected.
- **How.** The Waveform editor's file is an edit list of clips, so a one-channel edit is rendered: the core (`wave_channel_edit`) applies the edit to a copy, renders the range before and after, and takes each channel from one or the other; the interface swaps that range in as one undoable step. Stretch and Crop change length and still work on both channels.

## 2026-10-25: Time Info times can be dragged and typed

- **What.** Time Info shows the playhead, then a small table: Start, End and Duration for the time selection and for the part of the timeline in view. Drag any of these left or right to move it (one pixel moves as far as one pixel of the timeline; Ctrl for fine steps, Shift for coarse), turn the mouse wheel over it, or click it to type a time in the current display format (Enter sets it, Escape leaves it). Moving the playhead works while playing. Changing the View times scrolls or zooms the editor. When there is no time selection but clips are selected, the row reads Clips and shows their span, read-only. Works in both editors.
- **How.** `TimeField.svelte` and `time-info.ts`. The editors report their lane width (`laneWidths`) so the View row can show and set the visible range. Selection and view changes are view changes, not edits, as everywhere else.

## 2026-10-25: Shaped Noise Reducer

- **Restated as behavior** (the request pointed at another maker's noise reduction plug-in): a new effect, Shaped Noise Reducer (Restoration). Learn from Selection takes a print of the noise, as the Noise Print Reducer does. Then each frequency works as its own gate: louder than its threshold, it passes; quieter, it is turned down by up to Reduction. The threshold follows the learned noise, moved up or down by Threshold and by five shaping bands (a bell, low shelf or high shelf each, with frequency, gain and width), so more can be removed in one range (rumble, hiss) and less in another (the voice). Knee softens the step at the threshold. Attack and Release set how fast each frequency opens and closes. Adaptive lets the print follow a noise that drifts, learning only from frames that sound like noise alone. Output can be the cleaned audio or only what is being removed, to hear what is lost. Our own name, layout and look.
- **Graph.** The window shows the sound (filled), the learned noise (grey) and the threshold (orange) on one dB scale. The core lines the print up with the spectrum view, so a steady noise reads the same on both. Each shaping band is a node on the threshold: drag it sideways for frequency and up or down to raise or lower the threshold there, turn the mouse wheel over it for width, and double-click to switch it on or off. Each drag is one undoable step.
- **How.** `dsp/effects/shaped.rs` builds on the Noise Print Reducer's print and frame engine (75% overlap, both channels share gains). The graph's curves come from the core (`shaped_noise_curves`), so the drawing and the sound use the same maths. Every control is an ordinary setting, so presets, `.sesx`-style automation later and undo all work as for other effects.

## 2026-10-25: Opto Leveler and Tube Preamp

- **Restated as behavior** (the request pointed at a classic optical tube leveling amplifier and a 1960s studio's tube console preamp): two new effects with our own names, maths and look.
- **Opto Leveler** (Dynamics). A leveling amplifier whose gain cell behaves like a light-dependent resistor: it catches in about 10 ms and lets go in two stages, the first half in about 60 ms and the rest in 0.5 to 3 s, more slowly the longer it has been working (program-dependent release). Controls: Peak Reduction (how hard it works), Gain (makeup, 40 is unity), Compress or Limit (3:1 with a wide knee, or 20:1 with a narrow one), Emphasis (listens harder to high frequencies, to tame sibilance), Tube (the warm, mostly even harmonic color of a tube output stage), Mix, and a meter switch for gain reduction or output.
- **Tube Preamp** (Amplitude). Input gain into an asymmetric tube stage run at twice the sample rate (mostly even harmonics, low aliasing), an output transformer (a little weight around 70 Hz and a soft top, switchable), broad Low and High tone controls, Output and Mix.
- **Windows.** Both draw their own panel: big numbered dials for the main controls, a needle meter fed by the effect itself, lever switches, small dials for the rest, and a number box under every dial. A charcoal faceplate with brass trim, our own design.
- **Effect meters.** Effects can now report their own meter (gain reduction and output) by slot: the processor gets a pair of atomics when it is built (off the audio thread), the audio thread only stores into them, and the window reads them with `effect_meter` about 30 times a second.

## 2026-10-25: Real-time mixdown for plug-ins that need it

- **What.** Settings > Effects > "Mix down sessions that use VST3 plug-ins in real time" (off by default). With it on, any mixdown, stem export or Mix Down to New File of a session with VST3 plug-ins runs at playback speed, a block ahead of the clock, as playback does. Some plug-ins only work at normal speed: pitch correction, immersive panners and other tools that follow the clock or talk to each other while playing. Mixdowns already save every plug-in's latest settings first; this changes only the pace. Built-in effects never need it, so it stays off by default: with it on, an hour of audio takes an hour to export.
- **Not covered yet.** Plug-ins whose output has more than two channels (immersive beds and objects) are heard as stereo, as in playback; hosting multichannel buses is a separate piece of work.

## 2026-10-25: Clip stacking when dragging, and leftover crossfades

- **Stacking.** Where clips on a track overlap, the one higher in the stack plays and the ones under it come through only where it fades. A clip you drag lands on top of the clips it now overlaps, unless it was already tucked under one of them on the same track; then it stays under. Drag it off and back on and it goes over. Clip changes carry an optional `layer` (place in the track's stacking order), which undo restores; moving a clip to another track and undoing now also puts it back in its old place in the order.
- **Crossfades.** An automatic crossfade made while a clip overlapped another's edge is taken away once that overlap is gone, for example when the clip is dragged wholly inside the other (it then simply covers it) or clear of it. Only fades exactly as long as the old overlap count as automatic, so fades you set yourself stay.

## 2026-10-25: Several sessions open at once

- **What.** New Session, Open and Import Session no longer close the session in front: it is set aside with its unsaved changes and its undo history, and listed under it in the Files panel (in plain text, status "Open"). Double-click a session row to bring it to the front. Click a session row and press Delete, use the remove button, or the × on the row, to close it; it asks to save first if it has changes. Closing the session in front brings the last one set aside forward, or starts a blank session when none is left. File > Close All closes them one by one, asking for each. Opening a session that is already open brings it forward instead of opening it twice. Quitting warns about sessions set aside with unsaved changes. A blank session nobody has touched is dropped rather than kept.
- **How.** The core keeps the sessions set aside (`ParkedSessions`, `session_park`, `session_resume`, `session_forget`); the interface keeps their undo histories (`UndoStack.stash`/`restore`, as the Waveform editor does for its files). Setting a session aside first fetches plug-in settings and writes a recovery copy if it has unsaved changes. Playback stops on a switch. The Files panel keeps every session's audio files listed, as before.

## 2026-10-25: Effect and plug-in windows stay put

- **Effect windows.** While an effect's window is open, the Voice Chain panel stays on that effect's chain, so clicking another track or clip in the editor no longer takes the window away. Closing the window lets the panel follow the selection again.
- **Plug-in windows (Windows).** A VST3 plug-in's own window now belongs to the main window: it stays above it when you click in the main window, and minimizes with it. The plug-in host gets the main window's handle with the open request (`owner`).

## 2026-10-25: Clip level handle, and session rows' own menu

- **Clip level handle.** Every clip has a small handle in its bottom-left corner (an up-and-down arrow, our own glyph). Drag it up or to the right to raise the clip's level, down or to the left to lower it (0.1 dB a pixel; Shift for 0.02). The level shows beside the handle while it is not 0, and while dragging. One undoable step ("Clip Level"). The level was already a clip setting; it no longer repeats in the title bar.
- **MIDI clips** get a level too (`gain_db` on MIDI clips, saved only when not 0). It makes the clip's notes play harder or softer by that much (their velocity is scaled), up to the hardest a note can play, since an instrument's sound follows how hard it is played.
- **Files panel.** Right-click a session row for Switch to This Session (or Show in Multitrack), Save, Close Session and Close All Sessions.

## 2026-10-25: Bars and beats, more time formats, ruler menu

- **Time formats.** Times can show as Minutes and Seconds, Seconds (65.250), Bars and Beats (bar.beat.tick from 1.1.000, 960 ticks to a beat, at the session tempo), Samples or Timecode. Pick one by right-clicking the time ruler (Time Display), from View > Time Display, or by clicking the transport clock to step through them. The ruler, Time Info and the transport clock all follow it; in Bars and Beats the ruler marks bars (or beats when zoomed in), and typed times in Time Info read bar.beat.tick, bar.beat or a bar number.
- **Snapping to the ruler.** Snap to Ruler Marks is on for new settings and on the ruler's right-click menu (also Edit > Snapping > To Ruler Ticks). It snaps to the marks the ruler shows, so in Bars and Beats clips and selections snap to bars and beats.
- **Following the playhead.** Scrolling or zooming by hand during playback now stops the following outright, until playback starts again, the playhead is moved, or Follow Playhead is switched on again. Before, it only stopped when the playhead went off screen, so small scrolls were pulled back.
- **MIDI clips between tracks.** Moving and Alt-copying MIDI clips onto another MIDI track failed: the copy step could not copy the editor's live state. Fixed there, and in copying an effect between track headers and duplicating tracks, which used the same step.

## 2026-10-25: Plug-ins that talk to each other, and their song position

- **Run together.** Some plug-ins talk to each other while they play: an immersive panner on each track sends to a renderer on the mix, which lists every panner it finds. That only works when they share one process, and until now every plug-in ran in a process of its own (so one crash never stops another). Now a host process can run several plug-ins: each frame on its pipes is marked with which plug-in it is for (a `SELECT` frame before frames for another plug-in; a process with one plug-in never sends one, so nothing changes for it), and `Unload` removes one plug-in while the rest carry on. Plug-ins from a maker set to run together share one process for playback, and each mixdown gets one of its own. The Plug-in Manager lists each maker with a Run together box; Fiedler Audio is ticked by default, since its immersive panner and renderer need it. Changing a maker's box saves its running plug-ins' settings into the session, then restarts them the new way at the next playback. The cost: if one plug-in in a shared process crashes or hangs, the others from that maker stop with it.
- **Song position.** Plug-ins were told the song started at 0 every time playback started. Every audio block now carries the timeline position of its first frame, and the plug-in is told that as its song position, so it follows the playhead, jumps and loops, in playback and in mixdowns (timing tools, start and end points in renderers, synced effects).
- **Maker names.** A plug-in that leaves its maker blank now takes the maker its file declares, so grouping by maker (and the Maker column) works for it.
- **Not yet.** Plug-ins are still connected in stereo. Immersive renderers may also want a multichannel bed; that is the next piece if they still hold back.

## 2026-10-25: Metronome, and tempo and time signature changes

- **Restated request.** Asked for "the metronome like in" another editor, with tempo and time signature beside it, and time signatures that can change partway through a song. Restated as behavior: a click on every beat, heard during playback and recording and never in an export, turned on from the transport bar, with its sound, level, accented first beat and clicks between beats in a menu beside it; the tempo and time signature where the playhead is sit next to it; and a tempo map holds changes at bars.
- **Tempo map.** The session's `tempo` gains `beat_unit` (the bottom of the time signature, 4 when left out, so older files read as before) and `changes`: each a bar number (from the session start) with its own tempo and time signature. Changes sit on bar lines, so a change before it never leaves a later one mid-bar, and the map is simple to lay out: each stretch starts where the previous one's bars end. Beats stay quarter notes everywhere (MIDI clips and notes), so a bar of 6/8 is three of them. `set_tempo` now replaces the whole map as one undoable step, tidied in the core (one change per bar, in order, values in range, power-of-two bottoms). Ramps between tempos are not supported.
- **What follows it.** MIDI notes, Bars and Beats times, the ruler marks, MIDI lane grids, new MIDI clips (one bar where they are made), the piano roll's bar grid (the bar length where the clip starts), and plug-ins: each block tells a plug-in the tempo and time signature playing there, and where that stretch began, so its musical position and bar position are right across changes.
- **Where it is.** The metronome button, its settings arrow, and the tempo and time signature fields sit beside the transport clock in the Multitrack editor. The fields show and change the stretch the playhead is in. The old tempo fields in the track toolbar and the disabled metronome button by the ruler are gone. Changes show as flags on a Tempo strip under the ruler (shown when there are changes or the time display is Bars and Beats): drag a flag to another bar, double-click it to change it, right-click it to delete it, double-click an empty spot to add one; the ruler's right-click menu and the metronome menu also add one at the nearest bar.
- **How the click plays.** The render thread makes the click for each block at that block's timeline position, so it follows loops, skips and tempo changes, and hands it to the audio thread on a ring of its own. The audio thread adds it after the master fader, the meters and the scope, so it is never metered and is untouched by faders. Settings are atomics read once per block, so turning it on is heard after the half second already rendered ahead. Sounds (Tick, Wood, Beep, Shaker) are made in code, our own. Settings are kept with the app settings; "Recording only" is switched on just before a recording's playback starts, so the first beat is heard.

## 2026-10-25: Piano roll zoom and scroll

- **Restated request.** Asked for the piano roll's zoom and scroll options to be "more like" another app's. Restated as behavior: get around a long clip quickly with the mouse alone, zoom time and key height separately, and always see which bar you are in.
- **What it does.** A plain wheel scrolls up and down the keys; Shift and the wheel scrolls along time; Ctrl and the wheel zooms time around the pointer; Alt and the wheel makes the keys taller or shorter around the pointer (key height is kept while the app runs). Dragging with the middle button pans both ways. A bar ruler runs along the top (session bar numbers, through tempo and time signature changes at the clip's start), and the wheel over it zooms time. Under the roll, a zoom bar shows the part of the clip in view: drag it to scroll, drag either end to zoom with the other end held, click beside it to jump, double-click to fit the clip. The header gains Fit and taller and shorter key buttons beside the time zoom buttons. These are pointer gestures and buttons only, no new keyboard shortcuts.
- **Note colors.** MIDI notes take their clip's color, which is the track's unless the clip was given its own: in the clips on the lanes (full color with a light outline, instead of a pale tint) and in the piano roll (notes and velocity bars, instead of the app's accent color).

## 2026-10-25: Amp Model (neural amp captures)

- **Request.** Play neural amp captures (`.nam` files), which are widely shared now. The format and its reference player are open (MIT, Neural Amp Modeler by Steven Atkinson). The free official player is a VST3 and already runs through the plug-in host; this is a built-in effect so a capture works in any chain, preset or session without a plug-in.
- **What it plays.** WaveNet captures (the standard, lite, feather and nano sizes, all one layout) and LSTM captures, which are nearly all the captures in circulation. Newer kinds (the 2026 "A2" WaveNet, slimmable and container files) are refused when loading, with a plain message, rather than played wrongly. Captures are mono: both channels are summed going in and get the result.
- **Checked against the reference.** The forward passes were written fresh in Rust from reading the reference code (an algorithm port; credited in ATTRIBUTION.md, MIT, so compatible). A test runs the reference's example captures and compares with output from the reference built from source on the same input: the largest difference is about 6e-6. That test needs the reference on disk (`NAM_REF`), so it is ignored by default. It uses the same quick tanh as the official plug-in, so captures sound the same as there.
- **Speed.** The math runs along time in blocks of 256 frames, so the compiler vectorizes it. A standard capture takes about 11% of one core in a release build (10 s of audio in 1.1 s); smaller captures much less. A later step could use wider vector instructions where the processor has them.
- **Around the capture.** A noise gate before it (on by default at -80 dB, as players usually have), Input (drive), Bass, Middle and Treble after it, Output, and Level Match, which uses the loudness a capture file records to bring every capture to about -18 dB, so switching captures is a fair comparison. A cabinet impulse response (WAV, AIFF or FLAC, mixed to mono, up to a second) can be loaded too; it runs through a partitioned FFT convolution that adds 128 frames of delay, which the chain's delay matching handles. Turning the Cabinet switch off keeps that delay, so the sound never moves in time.
- **Where the files live.** The capture and cabinet are read into the slot itself: a new `data` field on effect slots (JSON text, absent for every other effect, so older files are unchanged). A session or preset then carries its amp with it and never points at a file somewhere else, which suits keeping sessions self-contained. A capture is a few hundred KB of text; a parsed capture is cached while the app runs, so rebuilding a chain does not read it again. Loading a capture or cabinet is one undoable edit.
- **Rates.** A capture plays at the session's rate. When it was made at another rate (usually 48 kHz), the window says so: its tone shifts a little until the session matches. Resampling around the capture is a possible later step.
- **Clippy.** The new field made a track change too big for its enum, so a track change now holds its instrument boxed. Nothing outside the core sees the difference.

## 2026-10-25: Metronome track, and waveforms follow clip level

- **Restated request.** The metronome button was hard to find, and the person asked for the metronome to be "its own track up top", as another editor does. Restated as behavior: a metronome button by the ruler turns the metronome on and off; while it is on, a Metronome track sits above the session's tracks with its mute, level, sound, and the tempo and time signature where the playhead is; turning it off removes the track.
- **Built as.** A fixed row under the ruler (and the Tempo strip), not a row in the track list, so it never moves clips, drop targets or track numbering, and is never saved in the session. Its lane shows bar lines (accent color) and beat lines. Its header: Mute (silent, track stays), level dial, sound, tempo and time signature boxes; its menu (the dots button or a right-click) has Playback and recording or Recording only, clicks between beats, the higher first beat, Add Tempo Change, and Turn Off. The metronome group beside the transport clock is gone, so the controls live in one place. Settings stay with the app settings (kept between runs); the click is still never in a mixdown or export.
- **Waveforms follow clip level.** Dragging the level handle in a clip's corner now grows or shrinks its waveform as it goes (the drawing is scaled by the clip's level), so what you see matches what you hear.

## 2026-10-25: 3D Space sessions

- **Restated request.** The person mixes for an immersive speaker format, where tracks are placed in a room rather than panned, and its plug-ins need the host to run them live. They asked for a session type where every track is one point (mono) or two (stereo) in a 3D room, set in a window of its own instead of a pan, with the speaker format easy to change, every format exportable from one project (5.1, 7.1.2, 2.0 and so on), and a binaural mix for headphones. Real-time export of plug-ins stays as it was, for plug-ins that need it.
- **The room.** A 3D Space session (`session.space`) has a speaker layout (2.0, 5.1, 7.1, 5.1.2, 7.1.2, 5.1.4, 7.1.4) and a listening mode. Each track and bus feeding the master has a spot (`space`: x left to right, y back to front, z floor to ceiling, size, and width for stereo sources). Spots are kept in room terms, never as speaker levels, so changing the layout moves nothing and any export can pick any layout. Turning 3D Space off keeps the spots, and the session plays as plain stereo with its pans again.
- **Placing a sound (our own method, `dsp/space.rs`).** One axis at a time: between the floor and ceiling speakers by height, then between the two rows of speakers either side of it (front, side, back), then between the two speakers either side of it in each row, sharing power at each step (cos and sin) so loudness holds as it moves. Size blends toward every speaker of the layers in use, keeping power at one. Stereo sources are two points, width either side of the spot. Sounds are never placed in the low-frequency channel (it stays silent for now).
- **Engine.** A 3D session always mixes on the render thread. Where a track or bus feeds the master, its sound goes into the speaker bed by its gains instead of being added in stereo; the gains travel with the live levels, so moving a dot is heard during playback without a restart. Faders, mutes, solos, sends, buses and chains work as before; a bus feeding the master is placed as one source, and tracks into it play where it sits. The master chain is not run in a 3D session (it would only change the listening mix, not the speakers), and track pans do nothing there; the track headers and the Mixer show a 3D button or tag in place of the pan. Plain sessions never touch any of this: one check per route per block.
- **Listening.** Stereo folds each sound to left and right by where it sits. Headphones (binaural) renders the layout's speakers as if around a head (`dsp/binaural.rs`): a spherical head model, the textbook one, with the far ear's extra delay and a head-shadow filter that dulls it more the further it faces away, and a gentle top lift for height speakers. No measured head responses are used, so there is no outside data to license. It is a first version: it places sounds left, right, front and back well; height and front-back are weaker than a measured set would give.
- **Export.** The Export Mixdown dialog has a Speakers choice in 3D sessions: any layout, or Headphones (binaural stereo). Layouts with more than two speakers are written as WAV with the extensible header and channel mask in the standard channel order (L R C LFE, surrounds, heights), by our own writer (`export/wav_multi.rs`), since the general encoder stops at eight channels. 2.0 and binaural go through the normal encoder in any format. Stems and clip mixes stay stereo.
- **The window.** Window > 3D Space (or Multitrack > 3D Space, or a track's 3D button) opens beside the Editor. It shows the room from above, front at the top, with the layout's speakers (height speakers as dashed diamonds) and a dot for each source (a linked L and R pair for stereo). Drag a dot to move it; the wheel over it raises or lowers it. The picked source's height, size, width and position have sliders beside the room (below it when the panel is narrow). Moves are live and merge into one undo step per drag. In an ordinary session the panel offers to make it a 3D Space session.
- **Later.** Automation of spots over time, the low-frequency channel, a surround view in the Phase Wheel, playing to a multichannel output device, and an object-based export format.

## 2026-10-25: Newer amp captures

- **What failed.** A capture made by a current trainer (the 2026 layout: a "slimmable" container holding the same capture at two sizes, each a WaveNet with its own kernel size and activation per layer, a narrower bottleneck and a 16-tap head) was refused on loading, and the window only said nothing was loaded.
- **Now.** The WaveNet reader takes both layouts: per-layer kernel sizes, dilations and activations (LeakyReLU with its slope), a bottleneck narrower than the layer width, gated layers, and a head that is a short causal filter rather than a single tap. A container plays its full-size capture, as the reference player does by default. Features no shared capture uses yet (conditioning models, FiLM, grouped convolutions, per-layer slimming) are still refused with a plain message. Checked against the reference player built from source: its example in the new layout and the person's own capture match within about 3e-7. A failed load now says why in the amp's window.

## 2026-10-25: 3D Space: rotation, low-frequency send, lower heights, every channel metered

- **Rotation.** A stereo source's two sides can be turned around its spot (`space.rotation`, -180 to 180 degrees, clockwise seen from above), so a stereo instrument can face any way in the room, not only left to right. Set it with the Rotation slider, Alt+drag on either side's dot, or Shift+wheel over the source. These are pointer gestures inside the 3D Space room, not keyboard shortcuts.
- **Low-frequency send.** Each source has an LFE send (`space.lfe`, 0 to 1). Its sound goes to the layout's low-frequency channel at that level through a 120 Hz low-pass (two biquads), on top of where it is placed. 2.0 has no such channel, so the send does nothing there. This is the easy way to feed a kick or a bass into the sub.
- **Lower heights.** Height runs from -1 (below ear level) to 1 (ceiling). No layout has speakers below the ear, so a low sound plays from the ear-level speakers, and the headphone mix tilts it down a little.
- **Meters.** In a 3D Space session the core reports the master's level per speaker channel, after the master fader. The Levels panel shows a row per channel with its name (L, R, C, LFE, then surrounds and heights in file order), sharing the panel's height so all of them fit; the Mixer's master strip and the Editor's Master row show a narrow bar per channel, named on hover. They show every channel while stopped too, so the layout is visible before playing. Plain sessions keep the two-channel meters.
- **Exporting a layout.** The 3D Space panel has an Export Mixdown button beside the layout; the export window's Speakers list picks the layout for that file (any of them, or headphones), whatever layout the session is set to.
- **The window.** The room view was tidied: a lit floor with distance rings, the listener's head, speaker names outside the room, tags for each source, and a single dot when a stereo source has no width.

## 2026-10-25: Track automation lanes

- **Restated request.** The person asked for a toggle under each track that opens automation, working like volume and pan elsewhere in a session. Restated as behavior: each track can show lanes under it, one per setting (volume, pan or an effect control), each a line of points over the session's time that the track follows while it plays.
- **Showing lanes.** The lanes button (a small line-and-points icon) sits on the automation mode row of a tall track, or on the output row of a shorter one; the track's right-click menu has Show or Hide Automation Lanes too. Whether lanes show is saved with the track (`show_automation`), so it is undoable like other header changes.
- **Lanes.** A track with no automation shows one Volume lane to start; it is saved only once a point is added. Each lane's header picks its setting (Volume, Pan, or any gliding control of the track's Voice Chain: on/off switches, lists and controls that change look-ahead are left out), turns the lane on (Read) or off, adds a lane (+) or removes it (x). A lane whose effect was removed from the chain stays, named "Removed effect", and plays nothing until it is pointed at another setting or removed.
- **Editing.** Click a lane to add a point and drag to place it; drag a point to move it (Shift keeps its time); double-click a point to remove it. Volume snaps to 0 dB and pan to center when close; Alt places freely. Right-click: Delete Point, Add Point at Playhead, Delete Points in Time Selection, Clear Lane, lane on or off, Add Lane, Remove Lane, Hide Automation Lanes. Each finished change is one undoable step (`set_envelopes`). Frequencies and other log controls are drawn on a log scale.
- **Mode.** The header's mode list now works: Read plays every lane that is on; Off turns every lane off (the fader, pan and effects hold still). Write, Latch and Touch (recording moves while playing) stay listed and disabled until built.
- **Playing.** Unchanged from the engine work: a volume lane takes the fader's place, a pan lane the pan's, effect lanes move their control every 256 frames. Editing a lane while playing carries on from the same spot.
- **Later.** Lanes for bus tracks (the engine plays bus envelopes already; buses need the show setting), curve shapes between points, recording moves (Write, Latch, Touch), and the fader and pan dials following the lanes while playing.

## 2026-10-25: Fixes and small additions after the 3D Space test

- **Delete Track (Ctrl+Alt+Backspace).** The Files list, when a session row was picked, took Backspace with any modifiers and closed the session instead; it now takes only plain Delete and Backspace. Delete Track also removes a picked bus, and the track of the picked clips when no track is picked.
- **Close Session from the right-click menu** did nothing: the menu closed (clearing which session it was for) before its item ran. The item now keeps the session's id.
- **Dropping files near the start.** With snapping on, files dropped close to the session start land at 0 (within three times the usual snap distance), and elsewhere they snap to the usual targets.
- **Several files at once.** Edit > Settings > General: several files dropped on the Editor go one after another on one track (as before), or side by side, each on its own track from the same time: the first on the track under the pointer, the rest on the audio tracks below it, then on new tracks named after the files.
- **The wheel over track headers** now makes every track taller or shorter, as over the track scrollbar (Alt+wheel: only the track under the pointer). Over a header's effect slots it still scrolls the tracks.
- **Effects > Plug-ins** listed nothing (a placeholder from before plug-ins were hosted). It now lists every scanned effect plug-in, by maker then name, and adds it to the selected track or clip like the built-in effects.
- **Low-frequency channel.** The 120 Hz low pass is now 8th order (48 dB per octave, was 24) and a 20 Hz high pass (24 dB per octave) cuts rumble below hearing, so the LFE send carries only the band the subwoofer is for.

## 2026-10-25: 3D Space: Spread, Spatialize, pair handles, faders, snapping

- **Restated requests.** "Spread sounds acoustically" and a "spatialize" setting that works like two named upmixing products were restated as behavior: (1) a control that turns part of a source into a diffuse field around the listener, not just a wider blend between speakers; (2) for stereo sources, a control that opens the sound out into the surround and height speakers by sending what differs between its sides there with delay and phase changes, and what the sides share to the center. No other product's method was looked at; both are our own, built from textbook parts.
- **Spread** (`space.spread`, 0 to 1, any source): part of the source (its mid, left plus right) goes through a decorrelator (`dsp/diffuse.rs`: four paths, each a 5 to 19 ms pre-delay and three all-pass sections, so level is kept and the copies no longer line up) to every speaker of the layers its height uses, equal power, never the LFE. The placed sound gives up the same power, so the total stays as loud. Size stays as it was: a level blend toward more speakers.
- **Spatialize** (`space.spatialize`, 0 to 1, stereo sources): the side sound (left minus right) goes through a second decorrelator to every speaker that is not in the front row, left ones in phase and right ones inverted; the mid is fed to the center speaker at half the amount; the front pair gives up 30% at full. In 2.0 it only adds the decorrelated side to the fold.
- **Listening.** Both are part of the speaker mix (so exports and the headphone mix have them) and are folded to stereo by each speaker's left-right place when listening on stereo speakers. The decorrelators are made when the mix is built, so playing never allocates.
- **Handles.** A stereo pair now has a center diamond that moves the pair. Dragging its L or R dot sets the width by its distance from the center (Alt also turns the pair toward the pointer). A picked pair shows two turn arrows across its axis: dragging one rotates the pair around its center. A pair with no width still shows its L and R dots a little apart so they can be grabbed.
- **Snapping** (the Snap button on the 3D Space bar, saved in settings, on by default): spots snap onto speakers, the center lines and a quarter-room grid, widths onto quarter steps, turns onto 15 degrees. Alt places freely.
- **Faders.** The side controls are horizontal faders in the Mixer's fader style (a recessed slot with ticks and a block cap with a center line), with drag, Ctrl for fine steps, the wheel, arrow keys and double-click for the default. The bar's lists and buttons use the app's flat, square-cornered controls.

## 2026-10-25: Meters in mixed sessions stopped flickering

- **Why they flickered.** In sessions mixed on the render thread (every 3D Space session, and any with buses, sends or chains), track, bus and speaker meters come from a queue of mixed blocks matched to what the device has played. A status read that arrived before the next block was played got an empty reading, so the meters blinked toward silence between reads.
- **Now.** A read with no newly played block gives the last reading again, and the Levels panel keeps its speaker rows if a reading arrives without them.

## 2026-10-25: Editor layout and selections from bus lanes

- **Gap under the Metronome track.** The Editor laid its strips out on a grid with a fixed number of rows, so when the Metronome track (and the Tempo strip) showed, the stretching row fell on the Metronome track instead of the tracks, leaving an empty band. The Editor is now a column where only the tracks stretch.
- **Selections from bus and master lanes.** A drag that starts on a bus lane, the master lane or the space below the last track now makes the same selection as one on empty track space (a box with the Move tool, a time range with the others), so a selection can sweep over every track when zoomed in. A click there still picks the bus or master.

## 2026-10-25: 3D Space automation, playhead follow, wheel zoom anchor

- **3D Space settings over time.** In a 3D Space session a track's automation lanes can move its spot: left/right, back/front, height, size, spread, LFE send, and for stereo sources width, rotation and spatialize (new envelope target `{"type":"space","param":...}`). Pan is left out of the picker there (it does nothing in such a session). The core works out the spot's gains once per mixed block, at the middle of the block, from the spot with the automated settings in place; settings without a lane keep the spot's own value, so moving a dot in the 3D Space window still works for them. This is done on the render thread, which mixes about half a second ahead of the speakers, so working out new gains there (a few small lists a block) never touches the audio device's thread.
- **Leaving the playhead behind.** Follow Playhead was switched back on by any change to the Editor's state (selection, tool, view), not only when you turned it on, so dragging the overview while playing was pulled back to the playhead at once. Now it picks up again only when switched on, when playback starts or the playhead is moved, or when you have stopped moving the view for a moment with the playhead in view.
- **Wheel zoom anchor.** Edit > Settings > General adds "Between the pointer and the middle of the view": the zoom centers 40% of the way from the pointer to the middle, so zooming drifts less. It is the default for new settings; the pointer and the playhead stay as choices. Over the rulers the zoom always centers on the pointer.
- **Phase Wheel and 3D sessions.** Rather than one view of every speaker, the plan is to let the Phase Wheel compare any two signals: two speaker channels of a 3D session, or two tracks.

## 2026-10-25: Audiform, a fork of Altered Audio Editor

- **Why.** Broman (nwh212) wants to keep shipping updates without waiting on reviews, while keeping everything open for Altered Audio Editor to take back. So this repository becomes Audiform: its own project, still a GitHub fork of Altered Audio Editor, with Broman as project lead.
- **Name and artwork.** Altered Audio Editor's trademark notice welcomes forks under their own name and icon, and allows saying a fork is based on it. Audiform has a new name, a new icon and logo (a sound wave inside the rings of the 3D Space room, `brand/`), its own app id (`io.github.nwh212.audiform`), executable, installer names, window title and default folders (`Documents/Audiform/...`). The original logo files are removed.
- **Credit.** Altered Audio Editor and its authors, Steve Phillips and The GeekCast Radio Network, LLC, are credited prominently: a "Built on Altered Audio Editor" section at the top of the README, a credit block in the About box, a Help > Based on Altered Audio Editor submenu (the original project's GitHub page, The GCRN website and plugins page, Steve's book and a link to support him), NOTICE.md, CONTRIBUTORS.md and the GitHub Sponsor button. Every file from Altered Audio Editor keeps its copyright line and now says it is part of Audiform, based on Altered Audio Editor. The About box and README say Audiform is not made or endorsed by The GCRN.
- **Reports.** Help > Report a Problem, Check for Updates, the source link, issue templates and CODEOWNERS point at Audiform (github.com/nwh212/audiform). Audiform has no Discord yet, so that item is disabled.
- **Compatibility.** File formats, extensions and ids stay as they are (`.altaudio`, `altaudio.*` effect ids and so on), so sessions, templates, libraries and shortcut files open in both apps and changes can be merged back. On first start Audiform copies Altered Audio Editor's settings folders (only read, never changed) into its own, so preferences, shortcuts, workspaces and the plug-in list carry over; both apps can be installed side by side. Plug-ins see the host as "Audiform".
- **Giving back.** Everything contributed to Audiform is also offered to The GCRN under Altered Audio Editor's CLA, so any of it can be merged there (README, CONTRIBUTING.md).
- **Builds.** Test builds run on every push to `main` (Windows), and Linux on request; releases are named Audiform.

## 2026-10-26: Record shortcut, and the playhead lets go for real

- **Record is Shift+Space.** Approved by the Audiform project lead; added to the shortcut table in CLAUDE.md. It starts recording the armed tracks, and stops when recording.
- **The playhead kept pulling the view back.** An older rule from before Follow Playhead was still in the Editor: whenever the playhead went off screen while playing, it moved the view back to it, whatever you had done and even with Follow Playhead off. It is removed; Follow Playhead alone pages the view now, and lets go when you move the view yourself (it picks up again when you leave the view alone with the playhead in it, when playback starts, or when the playhead is moved).

## 2026-10-26: Voice Chain panel: owner color, any row's chain, empty slots, mono icon

- **Whose chain it is.** The panel's top area and its tab underline take the color of whose chain it shows: the track's (or clip's), a bus's (its color, or the bus purple), or the app accent for the master.
- **Bus, MIDI and master chains.** Picking a row now always shows that row's chain: picking a bus, the master, or a track without a clip switches the panel to Track Effects, and picking another row closes an open effect window (which used to hold the panel on its chain, so clicks on other rows did nothing).
- **Empty slots** are drawn as empty dashed boxes, so the 16 slots read as a rack with room in it; later empty slots say "+ Add effect" on hover.
- **Mono buttons** (track and bus Mono, the master's mono check) show an icon of two channels joining into one, instead of "MO" or "MONO".
- **Name.** A request to rename the panel to a name another audio editor uses for its effects panel was set aside: the naming rule keeps other products' panel names out of the app. The project lead is choosing a name of our own.
- **Name and handle (same day).** The project lead picked **FX Chain** as the panel's new name (on screen and in the docs; the panel id stays `voice-chain` so saved workspaces keep working). Track headers' grab strip is now a slim 4 px bar in the track color that widens and shows grip dots when pointed at; the area you can grab stays 10 px wide.

## 2026-10-26: Source Channels for files of any channel count

- **Files keep every channel.** A 5.1, quad or 7.1.4 file stays as it is. Its clips draw one row per channel (named L, R, C, LFE, Ls, Rs and so on when there is room), and the waveform cache keeps every channel (up to 64). Caches made earlier for such files, which held two channels, are made again.
- **Source Channels, per clip.** Properties has a grid with one row per channel of the clip's file and a Left and a Right amount for each (-100% to 100%; negative, or the polarity button, flips it). The track hears the sums. One grid covers every case: the standard fold-down, a fold-down with the LFE, one pair (L and R, Ls and Rs, the heights...), one channel alone to both sides, a mono sum, or swapped sides; those are in its Preset list. A change is one undoable step and applies to every selected clip whose file has the same channels. Reset goes back to the default.
- **Defaults.** Mono plays on both sides and stereo as it is, as before. A file with more channels plays the standard fold-down: fronts straight through, center and surrounds 3 dB down, the LFE left out (mastering practice). Channel names come from the channel count in the standard order of multichannel WAV files; a count with no standard order is numbered and plays its first two channels.
- **Name.** "Source Channels" rather than the name another editor gives its similar effect, under the naming rule (and Settings keeps "Channel Routing" for the audio device's channels).
- **Later.** Sending each channel of a multichannel clip straight to its speaker in a 3D Space session, and showing every channel in the Waveform editor, build on this.

## 2026-10-26: Channel Routing: speakers on their own outputs

- **Speakers listen mode.** 3D Space's Listen menu gains "Speakers (each on its own output)", next to Stereo speakers and Headphones. Playback then sends each speaker of the session's layout to its own output of the audio device, with no fold-down, so a room with a real 5.1, 7.1.4 or other layout hears every speaker 1:1. The master fader applies; the metronome plays on the stereo pair. Meters, the Phase Wheel and the scope still read the stereo fold-down.
- **Settings > Channel Routing** (until now a placeholder) picks the outputs: the stereo mix's Left and Right outputs (1 and 2 by default), and an output for each speaker name a layout uses. A speaker left at "Its place in the layout" plays on the output with its number in the layout's order (L 1, R 2, C 3, LFE 4, then surrounds and heights, the order of multichannel WAV files and most interfaces).
- **Opening the device.** Playback opens the device in its default setting when that has enough outputs, otherwise in its widest setting at the same rate. File previews and input monitoring still play on outputs 1 and 2.
- **Exports** are unchanged: a 3D session set to Speakers exports in its layout as before (or stereo when picked).

## 2026-10-26: Multichannel files in the Waveform editor

- **Every channel shown.** A 5.1, 7.1.4 or other multichannel file opens with one lane per channel, each named (L, R, C, LFE...) on its on/off button beside the level scale. The file info line says how many channels it has.
- **Edits keep every channel.** Edits that render audio (FX Chain Apply, Stretch, Reverse, Invert, spectral Remove and Repair, edits limited to some channels) render the file one channel at a time and write the result back with all its channels. Before this, they and Save went through the stereo mix and would have written a multichannel file back as stereo. Effects run a pair of channels at a time (1-2, 3-4...), each pair through its own fresh copy of the chain, so stereo effects behave as they do on a stereo file.
- **Some channels off.** The per-channel buttons work for any number of channels: switch any set off, and edits, effects, copy and paste reach only the channels still on. Up and Down still pick the left or right channel alone.
- **Saving.** Multichannel files save as WAV with every channel, a few seconds at a time so long files never fill memory. FLAC and MP3 are refused for them, with a message, until those writers handle more than two channels.
- **Playback** in the Waveform editor plays the standard fold-down (or each speaker on its own output in nothing but 3D Space sessions; the Waveform editor is not one).
- **CI.** CI runs the newest stable Rust (1.99), whose clippy asks for `as_chunks` over fixed-size `chunks_exact`; changed throughout. CI had failed on every commit since the fork for that reason alone.
- **README** now says plainly that Audiform is the experimental flavor and Altered Audio Editor the more stable one.

## 2026-10-26: Multichannel clips play each channel from its speaker in 3D Space

- **Default in 3D Space sessions.** A clip of a file with more than two channels plays each channel straight from the speaker of the same name (L, R, C, LFE, Ls...), not folded and placed as one object. A channel whose speaker the layout lacks (heights in a 5.1 session, say) plays from where that speaker would stand; an LFE channel with no LFE speaker is left out, as in a fold-down. Source Channels has "Each channel from its own speaker" to turn this off (the grid then applies) or on for any clip, stereo included. Picking a preset in a 3D session turns it off.
- **How.** Before playback or export, each such clip's channels play from hidden copies of its track (same fader, mute, solo, FX Chain, output and sends; no pan or room spot of their own), each pinned to one speaker, unit gain. The copies exist only while playing or exporting and never reach the session file. Their ids are stable, so live fader and chain changes reach them.
- **Known limits.** The track's own meter does not show these channels yet, and stems by track leave them out.

## 2026-10-26: New mark, icon and logo

- **The project lead designed Audiform's mark:** a large glass wave arching over a smaller one, blue through violet and pink to gold and orange, with leaves. It is now the app icon at every size (Windows, macOS, Linux), the logo's mark in the README and the About dialog, and the source file in `brand/mark/`. The "Audiform" wordmark and its line below stay exactly as they were.
- The previous mark (a wave inside the 3D Space rings) and its vector files are removed. The new mark is raster artwork, so the brand pack keeps full-size PNGs in place of SVGs.

## 2026-10-27: Audio FX, no placeholder panel, a Music workspace

- **Audio FX.** The project lead renamed the effects panel (Voice Chain, then briefly FX Chain) to **Audio FX**, taking the name of the planned Audio FX Panel. A track's or clip's chain is "the Audio FX chain" in sentences. The panel id stays `voice-chain`, so saved workspaces keep working.
- **The Audio FX Panel placeholder is gone.** It never had content ("Not built yet"). Libraries now has the Default layout's whole right column (Default layout version 7, so saved Default layouts are rebuilt), the Phase Wheel's home moves to Libraries' group, and a saved workspace that still holds the old panel drops it when it loads.
- **Music workspace** (Window > Workspace), laid out by the project lead: Files over Audio FX (with QuickPass, Properties and Batch Convert) over History on the left; Editor and Mixer in the center over Levels; Phase Wheel over 3D Space (with Libraries) on the right, Time Info beside Levels.
- **Window title** says Audiform (it still named the original app), and two Settings notes too.

## 2026-10-28: Aero themes, 3D Space off switch, bouncing, fade shapes, plug-in fixes

- **Aero themes** join main from the trial branch: Meadow Aero, Aqua Aero and Moonlight Aero (Settings > Appearance > Theme, with an Apply button to preview). Dark stays the default. Meadow and Moonlight use real photos (credited in ATTRIBUTION.md), graded to each theme; the pointer turns into a glass arrow only in these themes.
- **Saved workspaces get current tab names.** A saved layout kept the tab titles of the version that saved it, so Audio FX still read "Voice Chain"; panels now take their names from the app when a layout loads.
- **3D Space turns off and on.** A "3D Space" switch on the 3D Space panel's bar and Multitrack > 3D Space Session make the session stereo again (tracks use their pan) or bring 3D Space back with its last layout. Spots and pans are both kept, so one session holds a stereo mix and a surround mix. Undoable.
- **Bounce to New Track** is complete: Selected Track (its clips with its own chain and fader, over the time selection or all of its clips), Time Selection (the whole mix), Selected Clips in Time Selection, and Selected Clips Only. In the Multitrack menu, the session right-click menu, and a track header's right-click menu. No save dialog: the file goes to a Bounced folder beside the session (32-bit float, never overwriting) and lands on a new track at the same time. The original tracks stay as they are.
- **Fade shapes.** Right-click a clip's fade handle for Linear, Logarithmic (fast start), Dip (slow start), S Curve and Equal Power (even level through crossfades), and Remove Fade. Applies to the other selected clips too. Dip and Equal Power are new shapes in the engine and in Settings > General.
- **MIDI track instrument box:** clicking the instrument's name opens its window; the arrow changes the instrument. The separate open button is gone.
- **Plug-ins open in their own window** when added from the Effects menu or by Replace With; before, those paths showed the built-in controls list first.
- **No network question for plug-ins.** Some plug-ins listen on the network inside Audiform's plug-in host (licence checks, or one maker's plug-ins finding each other), and Windows then asks whether Audiform may use public networks. The Windows installer now adds a firewall rule blocking incoming network connections to Audiform, so Windows does not ask. Traffic within the computer is not filtered, so plug-ins still find each other. A per-user install without administrator rights cannot add the rule; there, answering Cancel once has the same effect.

## 2026-10-28: Piano roll note tools and MIDI channels

- **Tools** in the piano roll (the Tools button) act on the picked notes, or every note when none are picked: set, scale (percent) or randomize velocity; set or randomize pan; and **Humanize**, which moves each note's start up to a set number of milliseconds earlier or later at random (lengths stay). All of it is part of the piano roll's one undoable edit.
- **Per-note pan.** Notes carry their own pan (-1 left to 1 right), shown and drawn in the lane under the roll (Lane: Velocity or Pan). It reaches the instrument as a per-note pan expression, which instruments that support it follow; others ignore it.
- **MIDI channels as note colors.** Each note has a MIDI channel (1 to 16), shown as its color (channel 1 keeps the clip's color). The Channel picker sets the channel for new notes and the picked notes. Instruments with several parts (a split keyboard, an organ's upper and lower manuals) play each channel with its own part. Auditioned notes use their channel too.
- Both are stored only when not the default, so older sessions and files are unchanged.

## 2026-10-28: Effect presets

- **Every effect window has presets:** a list to apply one (one undoable edit), Save As (a new name, or replace one of the same name) and Delete. Built-in effects keep theirs in the preferences, by effect; Default settings puts every control back.
- **VST3 plug-ins use preset files** (`.vstpreset`) in the standard VST3 preset folders, which every VST3 program reads and writes. Presets saved for a plug-in in another program show up here, and presets saved here show up there. Save As writes to the user's own folder (under the maker and plug-in name); only presets there can be deleted from Audiform. A running plug-in takes a preset at once, without restarting, and undo puts its previous settings back the same way.
- **Copying another editor's presets for its own built-in effects** was looked at and left out: those effects and their settings do not line up with Audiform's, so the presets would not mean the same thing. Its VST3 plug-in presets need no copying, since they are the shared preset files above.
- **Moonlight transport orbs** have a soft outer edge; the left side of the rim was hard and bright.

## 2026-10-28: Prism theme

- **Prism** (Settings > Appearance > Theme), after Audiform's mark: neutral dark graphite glass over a backdrop of glowing ribbons in every color of the mark (made for Audiform). No one color leads; each has its own job: blue picks and selects, green turns things on, gold is the playhead and the clock, orange the values, violet the faders, pink the knob pointers, red recording. The transport orbs are teal green glass (record stays red), meters go from blue through green and gold to red, and the spectrum runs along the top of each pane and under the menu bar. Dark stays the default.

## 2026-10-28: Effect channel routing, and the master chain on a 3D Space master

- **Every effect has a channel routing** (the routing button in its window's title bar): for each of the effect's inputs, the chain channel it hears, and for each output, the chain channel it replaces. Chain channels no output goes to pass through untouched, held back by the effect's look-ahead so everything stays in time. By default channels go by speaker name, so a stereo effect works on L and R. Routings can be saved as presets (kept in the preferences) and used on any effect; "Match by Name" goes back to the default. Every change is undoable.
- **Plug-ins can run with their own channel layout** ("Use the plug-in's own channels"): a 5.1 or 7.1.4 plug-in then gets all its channels, named by speaker. Plug-ins added to the master of a 3D Space session start this way. A 5.1 plug-in on a 7.1.4 master works on L, R, C, LFE and the side surrounds; one with both rear and side surrounds lines up with both.
- **The master chain runs on the speakers in 3D Space sessions.** Before, a 3D master had no chain at all (a chain on the stereo listening mix would not have reached the speakers). Now it runs on every speaker channel, and the stereo listening mix is folded from the processed speakers, so what you hear matches what exports. Headphone (binaural) listening and Speakers listening use the processed speakers too. The master meters show L and R.
- Under the hood: the plug-in host's audio frames carry their channel count, and the host asks a plug-in for its own layout only when the slot says so (others stay stereo, as before).

## 2026-10-28: Shorter notices; Stereo effects; Phase Rotate retired

- **Notices.** The About window and the README now say in one line that Audiform is an independent project, not made or endorsed by the makers of Altered Audio Editor (linking their trademark notice, kept in TRADEMARKS.md). The longer trademark sentences and the line naming another company are gone at the project lead's request; the GPL needs only the copyright and license notices, which stay.
- **Stereo** is a new effect family (Effects > Stereo and the Add effect menus): Phase Curve and the center remover.
- **Phase Rotate is retired**: Phase Curve does what it did and more. It is no longer offered anywhere; sessions that already use it still play it.

## 2026-10-28: Scrubbing, bounce placement, instruments in mixdowns, themed drop-downs

- **Held Left and Right arrows scrub** back and forward at four times normal speed (keys chosen by the project lead). Stopped, the playhead glides silently; playing, playback skips along at that speed so you hear where it is. Letting go leaves the playhead there. Arrows typed into a field, list, slider or menu, and under an open dialog, are left alone. Ctrl+arrows keep their jobs.
- **Bouncing a track** (MIDI tracks included, with the track's own effects) puts the new track right below it, named after it. A mono track bounces to a mono file; others to stereo.
- **Instruments in mixdowns.** An export starts its own copy of each instrument so playback is never disturbed. Instruments that load their sounds in the background were still loading when the render began, and came out silent. A copy started for an export now runs on silence for about two and a half seconds first, at about the pace of playback.
- **Drop-down lists** (effect presets, routing, and every other list) open in the theme's own colors instead of the system's gray.

## 2026-10-28: Fades: straight or curve, bent by dragging

- **Two fade shapes**, as the project lead asked: Straight and Curve (smooth at both ends). New fades start Straight (Settings > General can make them start as Curve).
- **Skew, -100 to 100:** drag a fade's handle up or down to bend it (up rises sooner, down later); across still sets its length. A tip shows the length, shape and skew while dragging. The handle's right-click menu has Straight, Curve, the skew, Reset Skew and Remove Fade. Saved in the fade's `curve` (-1 to 1), heard in playback and exports.
- Fades made with the other shapes earlier keep sounding as they did.

## 2026-10-28: Parametric EQ window, laid out as the project lead asked

- **Five bands** (were three), plus the low and high shelves, the high-pass and low-pass, and an **overall gain** (-80 to +40 dB, the slider left of the graph). Every band and shelf has its own on switch (the Band row). New EQs start with the bands spread across the range (50, 200, 800, 3,200 and 12,800 Hz); EQs already in sessions keep their settings.
- **Graph:** a frequency ruler with a line at every step of each decade and labels where they fit (Hz, 30, 40 ... 100, 200 ... 1k ... 10k, 20k), and the gain scale on the right. **Range** shows 30 dB (plus and minus 15) or 96 dB (plus and minus 48).
- **A column per band** under the graph: Frequency, Gain (Slope for the high-pass and low-pass), width, and its switch. **Constant: Q or Width** shows the width as Q or in octaves; both drag and type like every other value.

## 2026-10-28: Reverse and Invert Polarity on clips

- **Right-click a clip: Reverse and Invert Polarity**, each a checkmark that turns on and off, for every selected clip, undoable. Neither changes the file.
- **Reverse** plays the clip backwards from a reversed copy of its file, made once in the working files folder (Settings > Cache and Files; a "Reversed" folder) and reused after that. While reversed, the clip's region counts from the file's end, so trimming, splitting and moving work as they look. A session opened where the copy is missing makes it again when it plays. The waveform is drawn back to front.
- **Invert Polarity** flips the clip's waveform upside down (drawn that way too).
