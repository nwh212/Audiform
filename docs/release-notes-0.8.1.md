# Audiform 0.8.1

Windows is the main target. Linux and macOS (Apple silicon, unsigned) builds are experimental; if something is off there, please report it.

## New

- **Measure Scores** (Audio Separation, next to the engine). Scores your downloaded separation models on this computer against the MUSDB18-HQ test songs, which you download and pick: each result is compared with the song's own recorded part, and denoise, reverb and bleed models with the songs' vocals with noise, a room or leakage added. The results are saved as scores.json, scores.txt and run.log in a folder you can open from the window. Models the test set cannot judge (lead and backing vocals, single instruments, center and wide) are listed with the reason. It uses the models already downloaded and keeps no copies.
- **Save Space** (File menu). Makes smaller copies of the session's audio that sound exactly the same: WAV and AIFF files are stored as FLAC at its smallest setting, and stereo files whose two sides are identical become mono. Each copy is checked against the original sample for sample before the session uses it; then the session is saved and the originals go to the Recycle Bin (or stay, if you choose). Works on the files in the session's folder; one step to undo.
- Audio Separation: becruily's De-limiter (Denoise tab), which undoes the limiter on a brickwalled master and gives back its dynamics. 102 MB; free for non-commercial use (CC BY-NC 4.0).
