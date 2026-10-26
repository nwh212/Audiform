# Attribution

Code ported or adapted from other projects is credited here. Each entry lists the project, the source file, its license, and what was used. Required license notices are kept in the adapted files.

| Project | Source file | License | What was used | Our file |
| --- | --- | --- | --- | --- |
| NeuralAmpModelerCore (sdatkinson/NeuralAmpModelerCore, Copyright (c) 2023 Steven Atkinson) | `NAM/wavenet/model.cpp`, `NAM/lstm.cpp`, `NAM/conv1d.cpp`, `NAM/dsp.cpp`, `NAM/activations.h` | MIT | How a `.nam` capture's weights are laid out and played (the WaveNet and LSTM forward passes, written fresh in Rust), and its quick tanh formula, so captures sound as they do in that project's plug-in. Our output was checked against theirs on their example captures (largest difference about 6e-6). | `src-tauri/src/dsp/nam.rs` |
| `vst3` crate (coupler-rs/vst3-rs, Copyright (c) Micah Johnston) | `examples/gain.rs` | MIT or Apache-2.0 | The example gain plug-in, as the base of a test plug-in (look-ahead, saved settings and a crash switch added). Used by the plug-in tests only; never shipped. | `src-tauri/plughost/testplug/src/lib.rs` |

## Published formulas

| Source | Use | Our file |
| --- | --- | --- |
| Robert Bristow-Johnson, "Cookbook formulae for audio EQ biquad filter coefficients" (published freely for use) | Coefficient formulas for the low-pass, high-pass, band-pass, notch, peak and shelf filters behind Parametric EQ, Hum Cut and De-Esser. Formulas only; the filter code is our own. | `src-tauri/src/dsp/biquad.rs` |
| Sophocles J. Orfanidis, "Lecture Notes on Elliptic Filter Design" (Rutgers University, freely published) | The textbook elliptic design method: Landen's transformation for the Jacobi elliptic functions, the degree equation, and the pole and zero formulas. Method only; the code is our own. | `src-tauri/src/dsp/elliptic.rs` |
| MP4 file format (ISO/IEC 14496-12 box layout, as publicly documented) | The box layout for writing M4A tags (`moov/udta/meta/ilst`). Our own writer. | `src-tauri/src/export/m4a_tags.rs` |

## Bundled assets

| Asset | Source | License | Use |
| --- | --- | --- | --- |
| Source Sans 3 (regular, semibold; Latin) | npm package `@fontsource/source-sans-3` | SIL Open Font License 1.1 | Interface font, bundled into the app build. The license travels with the package. |
| `src/assets/themes/meadow-photo.webp` | "BlueSkyWhiteCloudsGrassland.jpg" by Raysonho @ Open Grid Scheduler / Grid Engine, [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:BlueSkyWhiteCloudsGrassland.jpg) | CC0 1.0 (public domain dedication) | Meadow Aero theme background; resized and color-adjusted (greener grass, deeper sky). |
| `src/assets/themes/moonlight.webp` | "Night sky with moon and stars over cotton field in Batesville, Texas" by the U.S. Department of Agriculture, [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:Night_sky_with_moon_and_stars_over_cotton_field_in_Batesville,_Texas.jpg) | Public domain (work of the U.S. federal government) | Moonlight Aero theme background; resized, color-graded to the theme's navy and teal, horizon lights removed. |
| `src/assets/themes/aqua.webp`, `prism.webp`, `droplets.png`, `orb.png`, `cursor.svg` | Made for Audiform | GPL-3.0-or-later, like the app | Aqua Aero scene, the Prism backdrop, water droplets, glass orb transport buttons and the glass pointer. |

## Libraries with notable licenses

Most dependencies are MIT or Apache-2.0 and are listed in `Cargo.lock` and `package-lock.json`. These are called out because of their license or what they do:

| Library | License | Use |
| --- | --- | --- |
| LAME 3.100 (via `mp3lame-sys` and `mp3lame-encoder`) | LGPL-2.0-or-later (LAME); MIT (bindings) | MP3 encoding. Built from source and linked into the app; LGPL is compatible with the app's GPL-3.0-or-later. |
| `flacenc` | Apache-2.0 | FLAC encoding. |
| `opus-decoder` | MIT or Apache-2.0 | Opus decoding. A patched copy lives in `src-tauri/vendor/opus-decoder` (faster forward FFT through `rustfft`); see the note there. |
| ASIO SDK 2.3 (Steinberg, via `cpal`'s `asio` feature, Windows release builds only) | GPL-3.0 (dual licensed by Steinberg; we use the GPLv3 option) | Talking to ASIO audio drivers. ASIO is a trademark of Steinberg Media Technologies GmbH. We do not use the ASIO logo. |
| `zip` | MIT | Reading zip downloads of recordings. |
| `vst3` (bindings generated from the VST 3 SDK headers) and `com-scrape-types` | MIT or Apache-2.0 | Hosting VST3 plug-ins, in a separate process (`src-tauri/plughost`). The VST 3 SDK itself is MIT licensed by Steinberg (since 2025). VST is a trademark of Steinberg Media Technologies GmbH; we do not use the VST logo. VST2 is not supported (its SDK is no longer licensed). |
| `libloading` | ISC | Loading a plug-in's library inside the plug-in host process. |
| `windows-sys` (Windows only, plug-in host) | MIT or Apache-2.0 | The window a plug-in draws its own controls in. |
| Symphonia | MPL-2.0 | Decoding WAV, AIFF, FLAC, MP3, AAC, ALAC, Ogg Vorbis and ADPCM. |
| `windows` (Windows only) | MIT or Apache-2.0 | Calls the AAC encoder built into Windows (Media Foundation) for M4A export. No AAC encoder is bundled; the operating system provides it. |

