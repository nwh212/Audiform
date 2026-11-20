# Handoff guides for Audiform 2

Audiform 2 is a rewrite of Audiform in pure Rust. These guides carry over what Audiform 1 learned, read from its code and `docs/decisions.md` at version 1.0 (commit `042577e`). Each one gives the design, the exact behavior with code links, the bugs found and how they were fixed, open issues spotted while reading, a recommended design for Audiform 2, an ordered checklist with tests, and a file map.

| Guide | Covers |
| --- | --- |
| [asio-and-audio-io.md](asio-and-audio-io.md) | Device I/O, ASIO (driver picking, the patched cpal, sample rate and buffer handling, the silence-before-close fix, stalls), WASAPI and ALSA, multichannel output mapping, building with ASIO |
| [vst3-hosting.md](vst3-hosting.md) | The out-of-process plug-in host and its pipe protocol, one process per maker, scanning, processing and latency, surround layouts, sidechain inputs, editors and DPI, state and presets, MIDI mapping, automation |
| [3d-space.md](3d-space.md) | Object-based surround: data model, speaker layouts and channel masks, the panning law, Spatialize and Place Elements, signal flow, 3D buses, binaural listening, spot automation, export, the room view |
| [buses-master-sidechain-midi.md](buses-master-sidechain-midi.md) | Buses, sends, outputs, the master track, the mix graph and delay compensation, sidechain keys, solo and mute rules, automation write modes, MIDI tracks, instruments, live input, recording, controllers, tempo, metronome |
| [zoom-and-scroll.md](zoom-and-scroll.md) | Every zoom and scroll gesture by area, horizontal and vertical zoom, anchoring formulas, wheel weighting, follow playhead, the rendering tied to zoom |

Read them as findings, not orders: where a guide says something could not be settled, it says so. Audiform 1's code is GPL-3.0-or-later; code copied from it keeps its licence and copyright lines (some files carry The GeekCast Radio Network's, from Altered Audio Editor).
