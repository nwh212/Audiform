# Audiform 0.6.0

Windows is the main target. Linux and macOS (Apple silicon, unsigned) builds are experimental; if something is off there, please report it.

## New

- **Effects.** A Modulation effect (Chorus, Flanger, Phaser and Double Track, a tape-style doubler with source and copy strips). Delay and Echo with Digital, Tape, Analog and Simple kinds, and an Analog knob on Tape and Analog for an old machine's character. A new Reverb with kind tabs, a Convolution Reverb that fills the whole room on surround, a Channel Mixer, and a Limiter with true peak. Band EQ's 10, 20 and 30 bands are tabs of one window, keeping the curve's shape.
- **Every built-in effect runs on every speaker** of a surround or 3D chain, up to 7.1.4. Modulation and Delay spread round the room.
- **Effect windows** have a power button (the symbol only), how-to text behind an (i), and value boxes that open for typing on a double-click.
- **3D Space.** Spatialize has a new Place Elements way: each part of a stereo sound goes where it sits in the picture, frequency by frequency. A 3D Bus places the tracks feeding it in the room. Mix Down to New File from the 3D Space panel. Empty tracks are left out, track buttons are colored, and dots follow their automation.
- **Sidechains.** VST3 plug-ins get their sidechain input, and any track can send itself into another track's sidechain.
- **Save Audio As** opens Audiform's own window: file name, location, format and its settings, Include markers, and the estimated file size.
- **Track Details:** edit a file's tags and cover picture (WAV, FLAC, MP3, M4A).
- **Report a Problem** (Help menu): a short form that fills in a report on the project's issue page in your browser. Nothing is sent from Audiform.
- **Settings backup and restore** (.afbackup), and Export Mixdown remembers its settings per format.
- **Editing.** Record while playing with no stop or gap. Alt+Up and Alt+Down copy the selected clips to the next track (a new track below the last). Linked crossfades bend both fades together. Trimming several clips lines their edges up. Fades shape the drawn waveform. Clips not heard (muted, or others soloed) turn grey. Dragging near the top or bottom edge scrolls the tracks.
- **Smaller touches.** The track header column can be dragged wider or narrower. Ctrl+Shift+S is Save As. Import effects rack presets. Subtle rounded corners. Dials point straight up at their default. Themes with their own pointers have their own clip trim pointers.

## Changed

- **Place Elements sounds closer to the source.** The lows stay on the front pair untouched, nothing goes missing across the speakers, parts no longer flicker between speakers, a voice keeps its air and s sounds, and ambience round the room is no longer smeared. At in-between Spatialize amounts there is no longer a dip in level.
- **Headphones listening** in 3D Space keeps the top of a sound straight ahead, so a centered voice is no longer dull.
- **Switching a channel off or on** in the Waveform editor is heard while the file plays, without stopping and starting.
- The open sessions no longer show as chips in the Editor tab. Switch sessions from the Files panel.
- QuickPass was removed.
- The Effects menu lists the same effect families and names as the Audio FX rack, and adds to the first empty rack slot.

## Fixed

- Plug-ins busy opening their window during playback catch up instead of being stopped, pause instead of switching off on a jump, and no longer jitter when a knob is turned quickly.
- Recording never loops back at the end of the session.
- Save As writes the chosen format settings (bit rate, bit depth, sample rate, mono).
- Float speaker-layout WAVs carry a fact chunk, so more players open them.
