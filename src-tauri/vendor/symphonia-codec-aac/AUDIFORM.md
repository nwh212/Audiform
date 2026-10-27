# Patched copy of symphonia-codec-aac 0.6.1

Audiform uses this copy (through `[patch.crates-io]` in `src-tauri/Cargo.toml`)
so AAC files with more than two channels open: 3.0, 4.0, 5.0, 5.1 and 7.1
(channel configurations 1 to 7, or a layout given by a program config
element, as files encoded with a side-surround 5.1 layout have).

The upstream decoder already decodes every channel element, but refused any
layout over two channels and wrote channels in bitstream order. The changes:

- `src/aac/mod.rs`: `ELEMENT_ORDERS` and `element_order` map each channel in
  bitstream order to its plane in the output buffer, whose planes follow the
  speaker positions; the complexity check accepts every layout that mapping
  knows.
- `src/aac/cpe.rs`: `synth_audio` takes the planes to write to.
- A program config element in the stream is read past instead of refused. The
  layout it gives comes from the audio specific config, which the patched
  `symphonia-common` beside this folder now reads.

HE-AAC (SBR) is refused with a clear error, both when the config says so and
when SBR data first shows up inside the stream (upstream then decoded only
the core: half the bandwidth, at half the sample rate). Audiform then opens the file
through the decoder built into Windows.

Changed lines are marked "Audiform". The files stay under the Mozilla Public
License 2.0, as upstream.
