# symphonia-common 0.6.1, patched

Upstream: https://crates.io/crates/symphonia-common (MPL-2.0). The changed
file stays under MPL-2.0.

Three changes.

In `src/xiph/audio/vorbis/mod.rs`: `vorbis_channels_to_channels`
returned nothing for Vorbis streams with more than 8 channels, so such files
(multitrack Ogg and MOGG stems, often 10 or more channels) were refused. The
Vorbis spec says those channels are in an order the application defines, so
they now come through as numbered (discrete) channels.
In `src/mpeg/audio/mod.rs`: an AAC audio specific config whose layout is given
by a program config element (channel configuration 0) was refused. The new
`read_program_config` reads that element and places each channel on a
speaker; `AudioSpecificConfig::element_positions` keeps the order, which the
patched AAC decoder in `../symphonia-codec-aac` uses.

Also there: channel configuration 7 reads as 7.1 with side and rear pairs
(front left, right and center, LFE, rear left and right, side left and right),
as 7.1 AAC is made and read today, instead of the older reading with a
second pair of front speakers.

Drop this folder and its `[patch.crates-io]` entry once upstream does the same.
