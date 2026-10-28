# Privacy

Audiform, like Altered Audio Editor it is based on, has no accounts, no sign-in, no telemetry, no analytics, no tracking and no ads. It works fully offline, and nobody has to do anything for you to use every feature.

## What the app does not do

- It never contacts the internet on its own. There is no update check, crash reporter, usage counter or license check. The one download it can make is the one you ask for in Audio Separation (below).
- It does not collect, send or sell anything about you, your computer, your audio or how you use the app.
- It needs no account and never asks you to sign in.

## What stays on your computer

Everything the app saves stays in folders on your own computer:

- Sessions (`.audfrm`, or `.altaudio` from older versions) and libraries wherever you save them.
- Settings, keyboard shortcuts, workspaces, autosaves and the waveform cache in the app's own config, data and cache folders.

## The only time the app touches the web

The Help menu has links (Audiform's GitHub page and problem reports, and, credited to the original project, the Altered Audio Editor page, The GCRN website, Steve Phillips's book and a PayPal page to support him). They open in your own web browser only when you pick them. The app itself only opens https links on a short allow list, kept in `src-tauri/src/commands/links.rs`.

## Audio Separation downloads, only when you ask

Audio Separation needs a model to separate audio with, and a small separation engine to run it. Neither comes with Audiform. When you pick a model and click Download (or Separate, which asks first), Audiform downloads:

- the model, from its author's own page, and
- the separation engine, the first time only, from Audiform's own releases.

It only pulls the necessary model for demixing. Audio separation is handled locally on your hardware, and speed and results depend on your hardware. Nothing about you or your audio is sent; these are plain file downloads. Each file is checked against a fingerprint (SHA-256) kept in Audiform before it is used, and kept in the app's data folder for next time. The Audio Separation panel lists what is downloaded and removes it.

The downloads use the `curl` program that comes with Windows, macOS and Linux, from one place in the code (`src-tauri/src/separation/download.rs`), only over HTTPS and only from `github.com` and `huggingface.co`. If you never use Audio Separation, nothing is ever downloaded.

## For contributors

Changes that add network access, telemetry, accounts or tracking of any kind will not be accepted. If a future feature truly needs the network (for example, an optional sync with a podcast host you choose), it must be off by default, clearly labeled, and never required to use the app.
