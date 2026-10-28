# Audiform 0.5.0

Linux builds have not been thoroughly tested. Windows is the main target; if something is off on Linux, please report it.

## New

- **Audio Separation** (Effects > Audio Separation, a tab beside 3D Space in the Music layout). Splits a file or clips into vocals and instrumental, or into six stems (vocals, drums, bass, guitar, piano, other), or pulls out one instrument (bass, drums, guitar, piano). Vocals and Instruments each have their own tab, with the published SDR score shown for each model. It only pulls the necessary model for demixing; audio separation is handled locally on user hardware and speeds / results depend on your hardware. The separation engine and each model are downloaded only when you click Download, can be removed or updated from the panel, and can use the graphics card (Settings > Audio Separation).
- **Pre-render marker.** The Pre-render button now looks different when it is on, rendering and rendered, and a dot on the track handle shows the same state.
- **Pick one channel of a file.** A small button beside multichannel files in the Files panel picks left only, right only, or any single channel. Dropped clips play only that channel, and the Waveform editor opens the file with only it on.
- **Panels resize together.** Drag where several panel handles meet to move them all at once.
- Clip edges that run under another clip (overlaps and crossfades) now show where they end and can be dragged to shorten them. Clip edges show a double-arrow pointer.
- Mix down, bounce and the other renders work on an imported session without saving it first; files go in a folder beside the imported session file.

## Importing sessions from the other editor

- Per-track render switches come across as Pre-render.
- Clips that use only the left or right of a stereo file keep to that channel.
- The vocal strip plug-in some sessions use comes across as Vox Strip, with its settings.
- Parametric Equalizer effects come across as Parametric EQ (bands, shelves, filters, gains and Q).
- Volume lines drawn on clips now come across, placed and cut to each clip (some clips came in far too quiet before).

## Fixed

- Effects rack: effects stay in the slot you put them in when you replace or move them; every empty slot says "Add effect".
- Four-corner panel drag no longer snaps back when you let go; the panel tab menu draws solidly in every theme.
- Plug-in windows can always be resized, and they re-read their size once open (fixes windows drawn too small).
- Heavy sessions start fewer plug-ins: none for pre-rendered tracks or tracks with nothing on them.
- Spectrum frequency marks fill in as you zoom.
- Mall Pastel '95 pointers are 16 by 16 pixels.
