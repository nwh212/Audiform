# Audiform 1.0

The final version of Audiform built on Altered Audio Editor's code. Work continues in Audiform 2, a rewrite in Rust; this version keeps working as it is, and its Audio Separation keeps using the engine it has (Audiform 2 uses a new one).

Windows is the main target. Linux and macOS (Apple silicon, unsigned) builds are experimental; if something is off there, please report it.

## New

- **Measure Scores** (Audio Separation, next to the engine). Scores your downloaded separation models on this computer against the MUSDB18-HQ test songs, which you download and pick: each result is compared with the song's own recorded part, and denoise, reverb and bleed models with the songs' vocals with noise, a room or leakage added. The results are saved as scores.json, scores.txt and run.log in a folder you can open from the window. Models the test set cannot judge (lead and backing vocals, single instruments, center and wide) are listed with the reason. It uses the models already downloaded and keeps no copies.
- **Save Space** (File menu). Makes smaller copies of the session's audio that sound exactly the same: WAV and AIFF files are stored as FLAC at its smallest setting, and stereo files whose two sides are identical become mono. Each copy is checked against the original sample for sample before the session uses it; then the session is saved and the originals go to the Recycle Bin (or stay, if you choose). Works on the files in the session's folder; one step to undo.
- Audio Separation: becruily's De-limiter (Denoise tab), which undoes the limiter on a brickwalled master and gives back its dynamics. 102 MB; free for non-commercial use (CC BY-NC 4.0).

## Known issues

- MIDI recording: an instrument may not sound while you record MIDI on some ASIO drivers. Audiform reopens the live player when it stalls, but this is not confirmed on hardware.
- Surround plug-ins in the Waveform editor hear left and right only while playing; Apply runs them on every channel. In 3D Space they get every channel.
- On ASIO, very small buffers at 88.2 or 96 kHz can make some interfaces' drivers ask for restarts, heard as short gaps. Use a larger buffer at high sample rates.
