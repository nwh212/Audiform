# Audiform 0.8.1

Windows is the main target. Linux and macOS (Apple silicon, unsigned) builds are experimental; if something is off there, please report it.

## New

- **Measure Scores** (Audio Separation, next to the engine). Scores your downloaded separation models on this computer against the MUSDB18-HQ test songs, which you download and pick: each result is compared with the song's own recorded part, and denoise, reverb and bleed models with the songs' vocals with noise, a room or leakage added. The results are saved as scores.json, scores.txt and run.log in a folder you can open from the window. Models the test set cannot judge (lead and backing vocals, single instruments, center and wide) are listed with the reason. It uses the models already downloaded and keeps no copies.
