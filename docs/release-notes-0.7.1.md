# Audiform 0.7.1

A stability and speed update. Large sessions use far less memory and stay smooth while you zoom and scroll, sessions from the other editor sound the way they did there, and the Waveform editor no longer loses or hides your work.

Windows is the main target. Linux and macOS (Apple silicon, unsigned) builds are experimental; if something is off there, please report it.

## New

- **Five clipboards** (Edit > Set Current Clipboard): keep up to five copied pieces and switch between them before pasting.
- **Snap Selections to Zero Crossings (Always)** (Edit menu) makes every selection you drag in the Waveform editor land on zero crossings, so cuts never click.
- **Link Media** (Files panel right-click, and the clip right-click menu for clips whose file is missing) points a moved or renamed file at its new place; the clips keep their edits.
- **Add Retake Marker** (Edit > Markers) flags a spot to redo, numbered Retake 1, Retake 2 and so on.
- **Markers panel picking**: Ctrl+click and Shift+click pick several markers, then Delete removes them; Delete All clears the list. Edit > Markers also has Rename Selected Marker, Delete Selected Markers and Delete All Markers.
- **Silence Time Selection in Selected Clips** (Edit menu and the lane right-click menu) mutes part of a clip with a volume dip you can still reshape or remove, leaving the audio untouched.
- **Show Only Bus Tracks** (Multitrack > Track Visibility) hides the other tracks so the buses are all you see.
- **Bring Clip to Front** and **Send Clip to Back** (Clip menu and the clip right-click menu) choose which of two overlapping clips plays on top.
- **Ripple delete in selected clips** (Edit > Ripple Delete > Time Selection in Selected Clips, Alt+Backspace) cuts the time selection out of the selected clips only and closes the gap after them.
- **Five new effects** from Altered Audio Editor. **Expander / Gate** (Dynamics) turns quiet sound down gently or shuts it off. **Dynamics Curve** (Dynamics) draws the whole response as one curve on a graph, with 19 starting curves. **Multiband Compressor** (Dynamics) compresses low, middle and high bands on their own. **Stereo Expander** (Stereo) widens or narrows the picture while keeping the bass in the middle. **Distortion** (Amplitude) has soft clip, hard clip, tube and fold shapes with a tone control. Expander / Gate and Dynamics Curve can listen to another track as a sidechain.
- **Waveform editor: Snap Selection to Zero Crossings and Paste with Crossfade** (Edit menu). The first moves both edges of the selection to the nearest spot within 20 ms where the sound crosses zero, so a cut there does not click. The second pastes and crossfades both joins over a length you choose, using the audio that ran on in its own file. Taken from Altered Audio Editor.
- **Audiobook delivery check.** File > Check Loudness has an Audiobook (ACX) choice that judges what narration is judged on: average level (RMS) between -23 and -18 dBFS, peaks at -3 dBFS or lower, and a noise floor (the room tone between lines) at -60 dBFS or lower. It flags digital silence, says what to fix first, and offers the master gain that brings the level into range when peaks leave room. Taken from Altered Audio Editor.
- **Plosive Tamer** (Effects > Dynamics) pulls down the low thump of P and B sounds only while it happens, comparing the low end with the body of the voice so loud vowels and deep voices pass untouched. Presets for a close mic, a standard mic and a light touch. Taken from Altered Audio Editor.
- **Loudness Match by level.** Besides a loudness target, Loudness Match can bring each clip or track to a level by its peak, its true peak or its average (RMS), with the level of your choice (remembered for each). A Tolerance box leaves alone anything already that close. It is also on the clip right-click menu. Taken from Altered Audio Editor.
- **MIDI editing.** The piano roll's Tools quantize the picked notes (or all) to the grid, with a strength so some feel can stay and an option to move note ends too; Legato runs each note on to the next; Transpose moves notes by a semitone or an octave. Step Input adds notes by playing them on a MIDI keyboard: each key or chord lands at the last spot clicked, as long as the last note drawn, and the spot moves on. A MIDI clip's right-click menu quantizes whole clips (a recorded take, say) without opening the piano roll.
- **Play and record MIDI from a keyboard.** MIDI tracks have an arm button (R) and a MIDI input picker (All MIDI Inputs, one input, or None). An armed MIDI track plays its instrument as you play, through its effects, fader and pan, whether or not the session is playing, and the sustain pedal holds notes as on a piano. Record records what you play on every armed MIDI track: each take becomes a MIDI clip from the bar the first note starts in, one undoable step with any audio recorded alongside. A punch-in keeps the notes started inside it. The track's existing MIDI clips keep playing while you record over them.
- Spectral display: zoomed in on the frequencies, drag the frequency scale up or down to move through them, keeping the same zoom.
- Check for Updates. Help > Check for Updates now asks Audiform's releases page which version is newest and, if it is newer than yours, offers to open its download page. Settings > General has a box to check once a week on its own (off unless you tick it); that check never runs while you play or record, and shows one line when there is something new. Nothing is downloaded or installed by the app.
- Audio Separation: Mega 53, which names 53 instruments at once (heavy), and the same model cut into one small download per instrument, folded under one heading. A duet model that pulls two singers apart. gilliaan's lead-vocal, drums, de-effect (reverb and delay off a vocal) and clean-invert models, and two of gilliaan's center and wide models on a new Stereo tab, which split what sits in the middle of the mix from what is panned out.
- Add-ons: a folder you put in the Add-ons folder (Window > Add-ons > Open Add-ons Folder) gives Audiform a new window, with a page of its own and, if it does work, a program Audiform runs on request and relays the output of. Add-on pages cannot reach the network or the rest of the app; they only talk to Audiform through a small set of messages. Audiform ships no add-ons and never downloads any. For people making one, the contract is in docs/addons.md.
- Audio Separation: four more of X-LANCE's single-instrument models, each fine-tuned from the six-stem SW model: orchestra, percussion (apart from the drum kit) and synth on the Instruments tab, and a vocals model on the Vocals tab. MIT licence, about 200 MB each.
- Audio Separation: the list folds models that make the same split (every vocals and instrumental model under one heading, and so on), so more of each kind can be offered without a long list; the group holding the picked model starts open. Each model takes a memo, a line or two on what it works best for, kept in your settings.
- Audio Separation: seven more models. A guitar model trained for the job (MIT licence, 277 MB), a bowed strings model (the only public one; non-commercial licence), two lead-vocal models that pull the lead apart from backing vocals (one GPL; run them on a vocal stem to get the backing vocals as the rest), a bleed suppressor that cleans what other stems left in an instrumental, another vocals and instrumental model (GPL), and a reverb remover that pulls the dry sound out of a reverberant recording (GPL). The choices behind the list, and what was looked at and left out (drum kits, choir parts, doubled vocals), are in docs/stem-models.md.

- Automation writing. Each track's automation mode picker now offers Write, Latch and Touch beside Read and Off. While the session plays, moving the track's fader, pan, its spot in 3D Space or any effect setting (built-in or plug-in) records the move into the matching lane: Touch while the control is held, Latch from the first move until play stops, Write every lane from the moment play starts. A lane is made when the track has none for that control. The written stretch is thinned to the points that shape it, and a whole pass is one undo step. The picker turns red while a track is recording.
- Automation lanes size like tracks: each lane starts at half its track's height and can be dragged between the same limits as a track.
- CRT theme: a color tube monitor, with scanlines over red, green and blue phosphor stripes, bright things glowing onto the glass, darkened corners and a slow flicker.
- Visualizer: two new looks, Tunnel (shapes flying at you, bending with the bass) and Electric (lightning arcing between drifting points, forking on the treble, striking on the beat). A Detail choice draws the soft looks (Lava Lamp, Plasma) finer, at more of the computer. The Radial look is gone. The control bar is grouped and tidier.

- Sixty-odd built-in effect presets: Parametric EQ shapes for acoustic strum, fat snare, kick, crunch guitar, vintage radio and a vocal lift; Band EQ curves; Precision Filter and Notch Filter starting points; compressor, limiter, gate and Opto Leveler settings for vocals, drums, bass and the mix bus; eight reverbs from a small room to a cathedral; chorus, flanger, phaser and double-track settings; Tube Preamp, Vox Strip and Stereo Width starting points. Each shows in the Effects Rack preset menu under its group and in the effect's own window under Built in.

- Theme Maker: a web page (Settings > Appearance > Make a Theme, or docs/theme-maker/index.html) where you pick colors, fonts, a background picture and pointers on a mock of the Audiform window and save a .audtheme file to share. It lives outside the app, so Audiform stays small and offline.

- **Repeat Last Command** in the Waveform editor. Shift+R applies the last effect or plug-in again with exactly the same settings, to the selection or the whole file. Ctrl+R opens it again with those settings, to adjust before Apply. The Edit menu names it, for example Repeat Parametric EQ.
- **Linear fades.** Effects > Fade In (Linear) and Fade Out (Linear), next to Invert and Reverse, fade the selection in a straight line, or the whole file when nothing is selected. The curved fades under Process are unchanged.
- **FLAC compression levels.** Save Audio As, Export and Batch Convert offer Compression 0 to 8 for FLAC (5 by default). Every level is lossless; higher levels make smaller files and take longer to save.
- **Save Audio As remembers your settings:** format, compression, bit depth, sample rate and channels carry over to the next save. Typing a name ending in .flac, .mp3 or .wav switches the format to match.
- **Saving a session keeps unsaved Waveform edits.** If the session uses a file you edited and have not saved, saving the session saves those edits: into the file itself when it is already in the session folder, otherwise as "name (edited).wav" in the session's Audio folder, leaving the original untouched. The session, the Files list and the Waveform editor move to that copy as one undo step.
- **Plug-ins from one maker share one process,** which saves a lot of memory on sessions with many plug-ins. If a maker's plug-ins misbehave together, untick them under Effects > Plug-in Manager > Run together.

## Faster

- Less memory for waveforms. The editor used to load every detail level of every file's waveform peaks as soon as a session opened; the finest level of an hour-long stereo file alone is about 20 MB, so a session of long files could hold hundreds of megabytes. Now only the coarse level loads with the file, finer levels load when you zoom in far enough to use them, and the least recently drawn fine levels are dropped past a 96 MB budget. A session of 24 forty-minute files went from 368 MB of peaks to 32 MB on open. About shows the peaks' share of the page's memory.
- The spectral display colors its picture once per analysis, not on every repaint while the playhead moves.

- **Much less memory in big sessions.** The effects list was kept once per track and bus; it is now kept once. On a 32-track session the editor view went from about 3.8 GB to about 0.7 GB.
- **Smooth zoom and scroll.** The wheel zooms and scrolls once per screen refresh, lanes keep their pictures between frames, and waveforms draw in one pass.
- **Moving the playhead** or the time selection redraws only the timeline, not every panel.
- **No more plug-in restarts** while you move around a session: the pictures drawn at the playhead borrow playback's plug-ins instead of starting their own, and plug-ins started ahead of Play keep the rate playback uses.

## Fixed

- Audio Separation refused to run from the test build's portable copy ("This engine works only with Audiform"), because that copy was renamed with its build number and the engine checks the program's name. The portable copy is now Audiform.exe in a folder named for the build, as in releases, and the message says what to do if a renamed copy is used.

- Plug-in windows at in-between Windows display scales, such as 115%, now fill their window instead of drawing small in one corner with black around them. At the usual scales (100%, 125%, 150% and so on) nothing changes.

- Audio Separation downloads use Windows' own curl by its full path, so a broken copy left on the PATH by other tools cannot stop them.

- Alt-dragging clips to copy them now shows both the originals, staying put, and the copies moving under the pointer.

- **Save Audio As never saved,** in any format, and showed "Cannot read properties of null (reading 'resolve')". It saves again, and the Files list shows the new file in place of the old one.
- **Sessions from the other editor played about 18 dB too loud.** Compressors with a sidechain input heard silence on it and never compressed. A plug-in's sidechain input now stays off until a key is actually routed to it, so this is fixed for every plug-in that has one.
- **Phase shifter from .sesx sessions:** it now turns the channel the session set (not both alike, which changed nothing you could hear), keeps every point of the curve (up to 16), and follows the drawn curve without overshooting.
- **The Waveform editor froze** ("effect update depth" error) when playback or recording reached the end of the file with the whole file on screen, which every new recording did.
- **ASIO recording at higher sample rates stopped after a few seconds** when the interface's driver asked for a restart. Recording now restarts the input and fills the gap with silence, so the take stays in time.
- **Unsaved Waveform edits and sessions:** a session now hears your unsaved edits even when the file was added after you edited it, and a clip comes in at the edited length. Dragging a recording's take into a session brings the edited file (a new file is saved first, and a note says where). Opening the take from the Files panel opens the file with your edits instead of the raw recording.
- **Spectral selections** no longer jump when you drag past the top or bottom of the view, or into the other channel.
- An "each_key_duplicate" error in Help > About, and a session file listed twice in the Files panel.
- **Copying one channel of a surround file into another file.** With one channel of a 7.1 (or any multichannel) file switched on, Copy then Paste into a new stereo or mono file pasted nothing, and Paste to New File mixed every channel together. The copied channel now pastes on its own into a file with another number of channels (on both sides of a stereo file), and Paste to New File makes a mono file of just that channel.
- **Surround plug-ins on surround clips.** In a 3D Space session a 5.1 or 7.1.4 clip plays each channel from its own speaker, and its effects ran on each channel apart, so a plug-in set to its own layout never heard the whole clip. When the clip's or its track's Audio FX hold such a plug-in, the channels now meet in a hidden 3D Bus that runs those effects on every speaker at once, with the track's fader, mute and sends. This needs the track to output to the master.
- **.sesx import** brings the other editor's limiter in as Audiform's Limiter, at its exact ceiling (input boost and channel link estimated). Taken from Altered Audio Editor.
- **Plug-ins that change their delay after loading** (some report one look-ahead at load and another once they run, or when a setting changes) were lined up with the old figure, so their track landed off. The new figure is now used: playback starts again at the same spot to line up, in the Waveform editor too, and Apply in the Waveform editor lets plug-ins settle before reading it. Taken from Altered Audio Editor.
- **Vocal aligners keyed from another track line up again.** A plug-in such as VocAlign that reports a different delay once its sidechain input is on was lined up with its delay from before the key arrived, so the aligned track came out late. Audiform now measures both delays when it loads the plug-in and uses the keyed one for a slot with a key routed.

## Changed

- The mouse wheel over a session's clips and track headers scrolls the tracks up and down again (Alt+wheel still resizes the track under the pointer, and the wheel over the track scrollbar resizes them all). Ctrl+wheel, or the wheel over the ruler, zooms in time; Shift+wheel scrolls sideways. The Settings box for wheel zoom now applies to the Waveform editor only.

- The Help menu's Based on Altered Audio Editor submenu is gone. The credit, with links to the original project, The GCRN, Steve Phillips's book and a way to support him, is in Help > About Audiform.

- Recording over a track's existing clips no longer plays them: while a track records, its clips are silent, and they are heard again the moment its take ends (disarming it while recording, or stopping). The other tracks play as before, and the session itself is untouched.

- The colored drag strip on each track header is always at its full width with its grip dots showing. It used to be a thin line that widened only when pointed at.

- Selection edges can be dragged, and a loop follows a new selection while it plays.
- Live edits keep 5.1 clip channels in 3D Space sessions.

## Known issues

- On ASIO, very small buffers at 88.2 or 96 kHz can make some interfaces' drivers ask for restarts, which you hear as short gaps. Use a larger buffer, or the driver's own setting, at high sample rates. Playback does not restart on its own yet; stop and play again.
- A new file you have never saved can go into a session only by dragging its recording from the Files panel (which saves it first).
- Files you switched away from in the Waveform editor with unsaved edits are heard in a session as saved until you open them again.
