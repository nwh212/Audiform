# Audiform 0.5.1

Linux builds have not been thoroughly tested. Windows is the main target; if something is off on Linux, please report it.

## New

- **Open `.aup3` projects.** File > Import Session, File > Open, or dropping the file brings a `.aup3` project in as a new, unsaved session: tracks with their level, pan, mute and solo, and clips with their position, trims, time stretch, pitch shift and volume lines. Stereo pairs become one stereo track. The project file is only read, never changed. Its audio is written as WAV files into a folder named "<project> audio" beside it, so keep that folder with the session. Effects are not brought across.
- **Session tabs in the Editor tab.** Open sessions show as chips in the Editor tab, in a steady order. Drag a chip to reorder, click to switch, close from the chip.
- **Visualizer panel** (a tab behind the Phase Wheel in the Music layout). A lava lamp and other presets that move with the music.
- **Audio Separation:** new Denoise tab with two denoise models (regular and aggressive), and a new Mel-Band Roformer vocal model.
- **Grey Desktop '95 theme**, with a pixel font.
- **Piano roll:** Draw and Select tools, placed notes drag on both axes without stretching, and Ctrl+Z undoes piano roll edits.

## Fixed

- Plug-in windows open at the right size on screens set to more than 100 percent scaling.
- The metronome no longer carries over into a plain audio file you play afterwards.
- Time Info condenses to a shorter strip.
- Separation recognises the engine under its new name.
