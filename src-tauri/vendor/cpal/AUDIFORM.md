# Patched copy of cpal 0.18.2

Audiform uses this copy (through `[patch.crates-io]` in `src-tauri/Cargo.toml`)
for one addition on Windows builds with the `asio` feature.

cpal lists ASIO devices by loading every installed driver in turn, in name
order, and stops at the first that is already loaded. A computer with many
drivers (virtual ones, wrappers, several from one interface) gets every one
of them loaded and unloaded on the way to the one wanted, which is slow, can
pop up driver windows, and can crash the app when one of them misbehaves.

The addition:

- `src/host/asio/device.rs`: `Devices::named` lists one driver by name.
- `src/host/asio/mod.rs`: `Host::device_named` builds the device for that
  one driver, loading nothing else.
- `src/platform/mod.rs`: `Host::asio_device_named` on the platform host
  (Windows, `asio` feature), None on other hosts.

Nothing else is changed. The examples are left out. Upstream:
https://github.com/RustAudio/cpal (Apache-2.0).
