# Audiform separation engine: client for Audiform 2

This crate is the part of Audio Separation that lives in Audiform 2. Copy it into Audiform 2's source (or add it as a path dependency). It holds nothing private. The engine itself stays a separate, privately licensed program that Audiform downloads the first time someone uses Audio Separation.

Licence: MIT OR Apache-2.0, so it fits Audiform 2's own licence.

## What changed from Audiform 1

| | Audiform 1 (engine 0.2) | Audiform 2 (engine 1.0) |
| --- | --- | --- |
| Running | One engine process per file: start, load the model, separate, exit | One engine for the session: the model is loaded once and stays loaded for every file until another model is loaded |
| Talking | A job file, then text lines (`progress 0.42`, `done`, `error ...`) | JSON lines both ways, protocol 2 (below) |
| Cancelling | Killing the process | `cancel`: the engine stops between chunks and stays ready |
| Download | Two requests per update (the `.sha256`, then the zip) and a guess at the asset name | One small manifest (`audiform2-separate.json`) naming the version, the protocol and each build's file, size and SHA-256 |
| Update check | Compare the installed checksum with the published one | Compare `engine --version` with the manifest's `engine` |
| Who can use it | Any program named Audiform | Only Audiform 2 or later: the engine needs the protocol 2 handshake with a host version of 2 or more, and refuses the job-file form. Audiform 1 and Altered Audio Editor keep their own engine (`engine-v0.2.0`) and never pick this one up, since its files have new names |
| Install folder | `separation/engine` | `separation/engine2`, so both can sit side by side during the move |

## Using it

```rust
use audiform_separate_client::{install, Engine, Manifest, Output};

// 1. Download. Audiform fetches; the client only reads and checks.
let manifest = Manifest::parse(&fetch(RELEASE_URL.to_owned() + "/" + audiform_separate_client::MANIFEST_FILE)?)?;
let asset = manifest.asset()?; // this computer's build
let zip = download_to_temp(&format!("{RELEASE_URL}/{}", asset.file), asset.bytes)?;
install::verify(&zip, &asset.sha256)?;
let exe = install::unpack(&zip, &engine_dir(&separation_dir))?;

// 2. Start once (on first use) and keep it.
let mut engine = Engine::start(&exe, env!("CARGO_PKG_VERSION"))?;

// 3. Load once, run many.
engine.load(model.id, &model.spec, &checkpoint, use_gpu)?;
for file in files {
    // `input` is a WAV at the model's sample rate (Audiform decodes and resamples).
    engine.run(&input, &[Output::stems(&vocals, &[0]), Output::without(&instrumental, &[0])],
               None, |p| progress(p), &cancel_flag)?;
}
```

Errors: `Error::Gpu` means the graphics card could not be used. Offer the processor and call `load` again with `gpu = false`. `Error::Stopped` means the engine process went away. Start a new one.

Suggested panel behavior:

- Start the engine on the first Separate, not when the panel opens.
- Keep it running while the panel is used. Call `unload` after a few idle minutes to free graphics memory, and drop it when the app closes. If Audiform crashes, the engine stops by itself because its input closes.
- Batches (several files, the clip right-click on many clips, Measure Scores) reuse one load, which is where most of the time used to go.

## Protocol 2

The engine is started as `audiform-separate serve`. Each line is one JSON object with a `type`. Requests go on the engine's standard input and replies come on its standard output.

| From Audiform | Reply |
| --- | --- |
| `{"type":"hello","protocol":2,"host":"Audiform","host_version":"2.0.0"}` (first, always) | `{"type":"ready","engine":"1.0.0","protocol":2}`, or `error` and the engine exits |
| `{"type":"load","model":"id","spec":{...},"checkpoint":"path","device":"gpu"}` | `{"type":"loaded","model":"id","device":"gpu","stems":2,"channels":2,"sample_rate":44100,"seconds":3.1}` |
| `{"type":"run","job":7,"input":"in.wav","outputs":[{"path":"v.wav","stems":[0]},{"path":"i.wav","stems":[0],"rest":true}],"overlap":2}` | `{"type":"progress","job":7,"done":0.42}` lines, then `{"type":"done","job":7,"seconds":12.5}` |
| `{"type":"cancel","job":7}` | `{"type":"cancelled","job":7}` instead of `done` |
| `{"type":"unload"}` | `{"type":"unloaded"}` |
| `{"type":"quit"}`, or closing the input | the engine exits |

Any failure answers `{"type":"error","job":7,"gpu":false,"message":"..."}`. `job` is null outside a run, and `gpu: true` means the graphics card failed. After a failure on the graphics card, the model is unloaded. Outputs are 32-bit float WAVs at the input's rate and channel count. `overlap` (1 to 8) replaces the model's own. `spec` is the model's settings exactly as Audiform 1's model list kept them (`arch`, `freqs_per_bands` or `num_bands`, `sample_rate`, `n_fft`, `hop`, `win`, `heads`, `dim_head`, `chunk_size`, `num_overlap`).

## Tests

`cargo test` runs the unit tests. To test against a real engine, build the engine with `cargo build --features any-host` in its folder, then run `AUDIFORM_SEPARATE_TEST_ENGINE=<engine folder>/target/debug/audiform-separate cargo test`.
