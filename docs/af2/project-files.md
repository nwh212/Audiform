# Project files: handoff guide for Audiform 2

This guide covers the three kinds of project file Audiform 1 opens, imports or exports, and exactly what it carries over from each:

1. Audiform's own sessions: `.audfrm`, and the older `.altaudio` from Altered Audio Editor (the same JSON).
2. Adobe Audition multitrack sessions: `.sesx` (XML). Import and export.
3. Audacity projects: `.aup3` (an SQLite database). Import only.

It is written for the engineer building Audiform 2, a pure Rust rewrite. Everything here was read from the Audiform 1 code at commit `042577e1ddee7b9718d82aea7947f9aab6edd628` (tag "Version 1.0.0"), plus `docs/decisions.md`, `docs/backlog.md`, `docs/review-notes.md`, the release notes and the git history. Where the code and the docs disagree, the code wins and the guide says so. Where something looks wrong but was never reported, it is marked **From reading the code**. Where something cannot be settled from the code, it is marked **Unclear**.

Base URL for links: `https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/`. Commits are linked as `https://github.com/nwh212/Audiform/commit/<sha>`. (The source code did not change between `042577e` and the commit that added these guides; only `docs/af2/` did.)

**Licence.** Audiform 1 is GPL-3.0-or-later. `sesx/mod.rs`, `sesx/export.rs`, `session/*.rs` and most of `src/app/*.ts` carry `Copyright (c) The GeekCast Radio Network, LLC.`; `aup3/mod.rs`, `commands/linked.rs` and `session/ops/media.rs` (the last also with the GeekCast line) carry `Copyright (c) The Audiform contributors.` Code copied or closely ported into Audiform 2 keeps GPL-3.0-or-later and keeps every copyright line of the file it came from.

**Naming.** Audiform's rules forbid presenting other products' feature names as Audiform features. The code itself says "the other editor" and "the other application". In this guide the format names (Adobe Audition, Audacity, `.sesx`, `.aup3`) are used only to describe interoperability. Audiform's own names are used for its features: Voice Chain (the effects chain, shown as Audio FX), Pre-render, Source Channels, 3D Space, 3D Bus, Vox Strip, Phase Curve, Link Media, Insert Session as Clip.

---

## Contents

1. Overview in one page
2. How files come in and go out (menus, drops, commands)
3. Audiform's own session format (`.audfrm`, `.altaudio`)
4. `.sesx` import
5. `.sesx` export
6. `.aup3` import
7. Bugs hit and fixes
8. Tests and fixtures
9. Open issues spotted while reading
10. Recommended design for Audiform 2
11. Ordered checklist with tests
12. File map

---

## 1. Overview in one page

| | Own session | `.sesx` | `.aup3` |
| --- | --- | --- | --- |
| Direction | open, save, autosave, recover, template | import, export | import only |
| Encoding | pretty JSON, UTF-8, trailing newline | XML (UTF-8, optional byte order mark, `<!DOCTYPE sesx>`) | SQLite database; layout in a binary tree blob; audio in sample blocks |
| Crate | `serde_json`, `schemars` (schemas), `pathdiff` | `roxmltree` 0.21 (read), hand-written strings (write), `rayon` (parallel probing), `pathdiff`, `uuid` | `rusqlite` 0.41.1 with `bundled`, `hound` (WAV writing) |
| Rust entry | `session::file::{open, save, autosave}`, `session::document::{load, save}` | `sesx::import_with`, `sesx::export::export` | `aup3::import` |
| Tauri command | `session_open`, `session_save`, `session_autosave`, `session_recover` | `session_import_sesx`, `sesx_is_surround`, `session_export_sesx`, `rack_presets_import` | `session_import_sesx` (same command, picked by extension) |
| Result | the session, plus ids of missing media | a new, unsaved session plus an `ImportReport` | a new, unsaved session plus an `ImportReport`, and WAV files written beside the project |
| Changes the source file | yes (own file) | never | never (opened read-only) |
| Status at 1.0 | complete, schema version 1 | import broad, export partial and never confirmed in the other program | built and shipped in 0.5.1 (not parked any more; `docs/backlog.md` still says "parked", which is stale) |

Key facts to carry over:

- The own format is one versioned JSON document with a `format` header (`altaudio.session`) and `schema_version` (1, never bumped). Old files are promised to open forever. Frozen fixtures enforce it.
- Timeline positions are integer samples at the session rate. Clip source offsets are integer samples at the media file's own rate. Levels are dB. Pan is -100 to 100.
- Media paths are stored twice: absolute and relative to the session file. Relative is tried first on open.
- Both imports produce the same thing: an ordinary `Session` (the Audiform model) plus a report. There is no separate import model. `aup3::import` even reuses the `.sesx` module's `ImportReport` and `Imported` types.
- `.sesx` import converts five of the other program's built-in effects to Audiform's own (Parametric EQ, Limiter, Vox Strip, Phase Curve) and brings VST3 plug-ins with their saved state. Everything else in a rack is named in the import notice.
- `.sesx` export is a copy for interoperability, never a save format. It writes far less than the own format and counts most of what it leaves out.

---

## 2. How files come in and go out

### 2.1 Entry points in the interface

| Gesture | What happens | Code |
| --- | --- | --- |
| File > Open | One dialog for "Sessions and audio files". Filters: Audiform session (`audfrm`, `altaudio`), Multitrack session (`.sesx`), Multitrack project (`.aup3`), audio files. Several files can be picked. `planOpen` sorts them: the first own session opens; else the first `.sesx` or `.aup3` is imported; audio files go to the Files panel; anything else is "Not opened" with a notice. If an own session and a `.sesx` are both picked, the `.sesx` is skipped. | [commands/session.rs#L681-L705](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/session.rs#L681-L705), [session-files.ts#L417-L434](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/session-files.ts#L417-L434), [App.svelte#L392-L402](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/App.svelte#L392-L402) |
| File > Import Session (.sesx, .aup3)... | `files.importSesx()`: makes room (sets the session in front aside, or asks to save), picks a file, imports it as a new unsaved session, shows the summary notice. | [session-files.ts#L279-L290](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/session-files.ts#L279-L290), menu item `file.import-sesx` in [menus/model.ts#L101](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/menus/model.ts#L101) |
| Drop a file on the window | `isSessionFile` matches `/\.(audfrm|altaudio|sesx|aup3)$/i`. Dropped anywhere but the Multitrack lanes, it opens through the same `openPaths` as File > Open. | [filedrop.ts#L246-L249](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/filedrop.ts#L246-L249) |
| Drop a session file on the Multitrack lanes | Becomes one linked clip of that session's mix (Insert Session as Clip), at the drop point. | [EditorPanel.svelte#L425-L438](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/panels/editor/EditorPanel.svelte#L425-L438), [commands/linked.rs#L32-L47](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/linked.rs#L32-L47) |
| Open Recent, Files panel double-click, a linked clip's "open" | `openSession(p)`: an already open session comes to the front; a path ending `.sesx` is imported; anything else goes to `files.open`. | [App.svelte#L1513-L1519](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/App.svelte#L1513-L1519) |
| File > Export > Multitrack Session (.sesx) | Save dialog, then `sesx::export::export`. The open session is not changed and stays tied to its own file. | [commands/session.rs#L770-L823](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/session.rs#L770-L823) |
| Audio FX rack, preset row, Import... | Reads every effects rack in XML preset files with the same rack reader as a `.sesx` import. | [commands/effects.rs#L376](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/effects.rs#L376), [sesx/mod.rs#L429-L452](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L429-L452) |

### 2.2 The import command

`session_import_sesx(path, surrounds)` handles both foreign formats ([commands/session.rs#L725-L768](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/session.rs#L725-L768)):

1. On a blocking thread: if the extension is `aup3` (any case), `aup3::import(&path, &aup3::audio_folder(&path))`; else `sesx::import_with(&path, surrounds)`.
2. Stops the transport, clears the autosave of the session it replaces, puts the new session in front with `path = None` (unsaved).
3. Starts loading the session's plug-ins in the background (`plugins::instance::warm`), a few at a time, so playback is ready sooner.
4. Returns the session, `missing` (media ids not found) and the report.

Before calling it, the interface asks one question for 5.1 sessions. `sesx_is_surround(path)` reads only the first 8192 bytes and looks for `audioChannelType="fivePointOne"` inside the `<session` start tag ([sesx/mod.rs#L1209-L1221](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L1209-L1221)). If true, a dialog asks "Where were the surround speakers?" with "Beside me" (`sides`) and "Behind me" (`behind`, the default) ([App.svelte#L947-L965](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/App.svelte#L947-L965)). After a 5.1 import the 3D Space panel opens.

The frontend then marks the session as unsaved, remembers which file it was imported from (`markImported`), and uses that file's folder as the session's "home" until the first save, so mixdowns and renders of an imported session land beside the source file ([stores/session.ts#L740-L760](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/session.ts#L740-L760)).

### 2.3 The import notice

`sesxSummary(report)` builds one line for both formats ([session-files.ts#L107-L126](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/session-files.ts#L107-L126)):

- "Imported N tracks, B buses and M clips."
- "P plug-ins came across with their settings."
- "Not found on this computer (install or scan them in the Plug-in Manager): names."
- "C phase shifters came across as Phase Curve." (see the bug note in section 9: `converted` also counts EQ, Limiter and Vox Strip conversions)
- "S clips keep their stretch and pitch."
- "K built-in effects were left out (names); rebuild them in Audio FX."
- "F media files were not found."

The summary replaces the generic "media files were not found" notice used on open.

---

## 3. Audiform's own session format (`.audfrm`, `.altaudio`)

### 3.1 Names and extensions

- `EXTENSION = "audfrm"`, `OLD_EXTENSION = "altaudio"` ([session/file.rs#L26-L38](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/file.rs#L26-L38)). Both open. The contents are identical; only the extension changed on 2026-10-27 (decisions.md "Sessions save as .audfrm"). No migration was needed.
- Saving always writes `.audfrm` (`with_extension`, [commands/session.rs#L518-L533](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/session.rs#L518-L533)): a path ending `.altaudio` becomes the same name with `.audfrm` beside it, and the old file is left as it was; a path with any other extension gets `.audfrm` appended (`Show.wav` would become `Show.wav.audfrm`).
- The `format` header stays `altaudio.session` and effect ids stay `altaudio.*`, on purpose, so files move between Audiform and Altered Audio Editor (decisions.md line 1400). Altered Audio Editor can read `.audfrm` after a rename.
- Sibling documents use the same loader: `altaudio.template` (`.altemplate`), `altaudio.library` (`library.json`), `altaudio.workspace` (`.alworkspace`), `altaudio.host_profile` (`.alhost`), `altaudio.keymap` (`.alkeys`), `altaudio.preferences` (`.alprefs`). All are version 1.

### 3.2 A small real example

Trimmed from the frozen fixture [`src-tauri/tests/fixtures/v1/session.altaudio`](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/fixtures/v1/session.altaudio):

```json
{
  "format": "altaudio.session",
  "schema_version": 1,
  "id": "00000000-0000-0000-0000-000000000001",
  "name": "Show 412",
  "sample_rate": 48000,
  "bit_depth": 32,
  "created": "2026-10-11T12:00:00Z",
  "modified": "2026-10-11T12:00:00Z",
  "media": [
    { "id": "...0064", "path": "C:/GCRN/Show/412/host.wav", "relative_path": "host.wav",
      "sample_rate": 44100, "channels": 1, "length": 158760000 }
  ],
  "tracks": [
    { "id": "...00c8", "name": "Host", "role": "standard", "host_profile_id": "host",
      "channels": "mono", "color": "#3f73b8", "height": 120,
      "volume_db": -2.0, "pan": -15.0, "muted": false, "solo": true,
      "record_armed": true, "monitor_input": false, "input": "Mic 1",
      "output": { "type": "bus", "bus_id": "...0190" },
      "sends": [ { "id": "...02bc", "bus_id": "...0190", "level_db": -12.0, "pan": 0.0,
                   "pre_fader": true, "enabled": false } ],
      "effects": { "enabled": true, "input_gain_db": -1.5, "output_gain_db": 2.0, "mix": 0.85,
        "slots": [ { "id": "...0384", "effect": "altaudio.dynamics", "enabled": false,
                     "params": { "ratio": 4.0, "threshold_db": -20.0 }, "preset": "Voice leveler" } ] },
      "clips": [
        { "id": "...012c", "name": "Cold open", "media_id": "...0064",
          "start": 48000, "source_in": 44100, "source_out": 441000,
          "gain_db": -3.25, "muted": false, "locked": true, "color": "#3d8f6a",
          "fade_in":  { "length": 4800, "shape": "cosine", "curve": 0.25 },
          "fade_out": { "length": 9600, "shape": "logarithmic", "curve": -0.5 },
          "envelopes": [ { "id": "...0321", "target": { "type": "volume" }, "enabled": true,
                           "points": [ { "position": 0, "value": -3.0, "curve": "linear" } ] } ],
          "effects": { "enabled": true, "input_gain_db": 0.0, "output_gain_db": 0.0, "mix": 1.0, "slots": [] } }
      ],
      "envelopes": [] }
  ],
  "buses": [ ... ],
  "master": { "channels": "stereo", "height": 64, "volume_db": -0.5, "effects": { ... }, "envelopes": [] },
  "markers": [ { "id": "...01f4", "name": "Segment 2", "kind": "chapter", "position": 1440000,
                 "duration": 0, "description": "News roundup",
                 "chapter": { "url": "https://thegcrn.com", "image": "art/segment2.png" },
                 "contracted_duration": null } ],
  "view": { "samples_per_pixel": 512.5, "scroll": 96000, "scroll_y": 40, "playhead": 123456,
            "selection": { "start": 48000, "end": 96000 } },
  "modes": { "podcast": true, "voiceover": false },
  "libraries": [],
  "template_id": null,
  "metadata": { "show_id": "show", "title": "Episode 412", "show": "Show", "episode_number": 412,
                "season_number": null, "description": "This week on the show.", "artist": "The GCRN",
                "artwork": "art/412.png", "explicit": false }
}
```

Ids are UUID v4 strings (`new_id()`), shortened here. Times are RFC 3339 UTC.

### 3.3 The loader: header, `schema_version`, migrations

All versioned documents go through [session/document.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/document.rs). The `Document` trait has `FORMAT`, `CURRENT_VERSION`, `SCHEMA_FILE`, `migrate_step(value, from)` and `upgrade_nested(value)`. For `Session`: `FORMAT = "altaudio.session"`, `CURRENT_VERSION = 1`, `SCHEMA_FILE = "session.schema.json"` ([model.rs#L88-L92](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L88-L92)).

Load pipeline (`load` then `from_json` then `from_value`, [document.rs#L102-L181](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/document.rs#L102-L181)):

1. Read the whole file as a string; parse to `serde_json::Value`.
2. `read_header`: `format` missing gives `MissingHeader("format")`; a different format gives `WrongFormat { expected, found }`; `schema_version` missing gives `MissingHeader("schema_version")`; 0 gives `UnknownVersion`; above `CURRENT_VERSION` gives `TooNew` with the message "this altaudio.session file uses version N, which is newer than this app supports (up to 1). Update Audiform to open it." Newer files are refused, never half loaded.
3. `upgrade`: while the version is below current, call `migrate_step(value, version)` and write the new `schema_version` into the value. The default `migrate_step` returns a `Migration` error ("no migration step is defined"). No format has a migration step today, because every format is still at version 1.
4. `upgrade_nested`: a template upgrades the session inside it with the session migrations ([library/template.rs#L38-L50](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/library/template.rs#L38-L50)).
5. `serde_json::from_value` into the Rust type.

Unknown fields are ignored (no `deny_unknown_fields` anywhere), so a file from a newer build with only added fields still opens in an older build at the same version. That is the property that let every later feature ship without a version bump.

Save (`document::save`, [document.rs#L183-L211](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/document.rs#L183-L211)): pretty JSON plus `\n`, written to `<name>.tmp` in the same folder, `sync_all`, then `rename` over the target. A crash mid-save leaves the old file. A failed rename removes the temp file. `roundtrip.rs` checks that saving twice replaces the file on Windows and leaves no temp file behind.

### 3.4 Versioning rules and the backward compatibility promise

From the module comment and decisions.md "Versioning rules" (2026-10-11):

- "Never break an old file."
- Adding a field with a serde default is not a version bump.
- Renaming, removing, or changing the meaning of a field is. It needs a `migrate_step`, a new fixture folder `tests/fixtures/v<N>/`, and a passing fixture test.
- Files from a newer app are refused with a message to update.
- A template's nested session is upgraded with the session migrations.
- `format_ids_are_stable` guards the seven format ids against accidental renames.

In practice every field added after v1 uses `#[serde(default, skip_serializing_if = ...)]`, so it is left out of the file when it holds its default. Two effects: old files load (missing means default), and a session that uses no new feature writes exactly what an old build wrote, which is why the v1 fixture still equals the current sample session byte for byte in meaning (`session_fixtures_open` asserts `docs.last() == common::session()`).

Fields added this way since v1 (all optional in the file): `Session.tempo`, `space`, `playlist`; `MediaRef.linked`; `Track.kind`, `collapsed`, `show_automation`, `automation_mode`, `hidden`, `polarity_inverted`, `mono`, `prerender`, `space`, `monitor_dry`, `instrument`, `midi_clips`; `Bus.automation_mode`, `position`, `mono`, `space`, `room`; `Clip.group`, `looped`, `stretch`, `audio_type`, `channel_mix`, `reversed`, `inverted`, `invert_channel`; `ClipStretch.pitch`, `polyphonic`, `rendered`, `advanced`; `Tempo.beat_unit`, `changes`; `EffectSlot.sidechain`, `plugin_state`, `data`, `position`, `routing`; `Envelope.height`; `Marker.color`; `SpacePos.z`, `size`, `width` (default 1.0), `rotation`, `lfe`, `spread`, `spatialize`, `spatialize_mode`; `Note.channel`, `pan`; `MidiClip.muted`, `color`, `gain_db`, `controls`.

Fields that are required in the file (no serde default; an `Option` without `default` is still accepted when missing, because serde treats a missing `Option` as `None`): see `"required"` arrays in [schemas/session.schema.json](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/schemas/session.schema.json). At the top level: `format`, `schema_version`, `id`, `name`, `sample_rate`, `bit_depth`, `created`, `modified`, `media`, `tracks`, `buses`, `master`, `markers`, `view`, `modes`, `libraries`, `metadata`. A Bus requires `id`, `name`, `channels`, `height`, `volume_db`, `pan`, `muted`, `solo`, `output`, `sends`, `effects`, `envelopes`. Audiform 2 must accept a file that has exactly these and nothing more.

Some phase-scaffolding fields are stored and round-tripped but had little or no behavior at 1.0 (marked "Scaffolding (Pn)" in [model.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs)): `Track.role` (Guest), `host_profile_id`, `MarkerKind::AdSlot` and `contracted_duration`, `SocialClip`, `CdTrack`, `CartTimer`, `ChapterInfo`, `EpisodeMetadata`. Keep them when reading so a round trip through Audiform 2 does not lose them.

### 3.5 What is saved, object by object

Units, from the model comment: timeline positions and lengths in samples at the session rate; `source_in` and `source_out` in samples at the media file's own rate ("so a 44.1 kHz file in a 48 kHz session never loses precision"); levels in dB; pan -100 (left) to 100 (right). Pan is a balance law in the engine: centred is 0 dB on both sides and panning turns the far side down linearly (`balance`, [engine/graph/levels.rs#L135-L143](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph/levels.rs#L135-L143)).

| Object | Saved fields | Notes |
| --- | --- | --- |
| `Session` ([model.rs#L35-L86](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L35-L86)) | `id`, `name`, `sample_rate`, `bit_depth` (16, 24, 32 float), `created`, `modified`, `media`, `tracks`, `buses`, `master`, `tempo`, `space`, `markers`, `playlist` (range marker ids in play order), `view`, `modes`, `libraries` (folders), `template_id`, `metadata` | `name` is set from the file stem on Save As or the first save of "Untitled Session". `modified` is set on each save. |
| `MediaRef` ([#L441-L468](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L441-L468)) | `id`, `path` (absolute at last save), `relative_path`, `sample_rate`, `channels`, `length` (file frames), `linked { path, stamp }` | Only media a clip uses is saved (section 3.7). `linked` marks a file rendered from another session (section 3.10). |
| `Track` ([#L531-L620](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L531-L620)) | `id`, `name`, `kind` (audio or midi), `role`, `host_profile_id`, `channels` (mono, stereo), `color` (`#rrggbb`), `height` (px), `collapsed`, `show_automation`, `automation_mode` (read, write, latch, touch), `hidden`, `volume_db`, `pan`, `muted`, `solo`, `polarity_inverted`, `mono`, `prerender`, `space` (`SpacePos`), `record_armed`, `monitor_input`, `monitor_dry`, `input` (device input name), `output` (`{type: master}` or `{type: bus, bus_id}`), `sends`, `effects`, `clips`, `instrument` (an `EffectSlot`, VST3), `midi_clips`, `envelopes` | `speaker` is `#[serde(skip)]`: playback only, never saved. |
| `Send` | `id`, `bus_id`, `level_db`, `pan`, `pre_fader`, `enabled` | |
| MIDI (`MidiClip`, `Note`, `ControlEvent`, [#L366-L436](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L366-L436)) | clip: `id`, `name`, `start` and `length` in beats (f64), `muted`, `color`, `gain_db`, `notes`, `controls`. note: `pitch` 0 to 127, `velocity` 1 to 127, `start`, `length` (beats from clip start), `channel` 0 to 15, `pan` -1 to 1. control: `at` (beats), `controller` (0 to 127, 128 channel aftertouch, 129 pitch bend), `value` 0 to 1, `channel` | MIDI is placed in beats so it follows the tempo map. |
| `Tempo` | `bpm` (20 to 400), `beats_per_bar`, `beat_unit` (default 4), `changes [{id, bar, bpm, beats_per_bar, beat_unit}]` | Left out when default (120, 4/4, no changes). |
| `Bus` ([#L672-L712](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L672-L712)) | `id`, `name`, `channels`, `color`, `height`, `volume_db`, `pan`, `muted`, `solo`, `output`, `sends`, `effects`, `envelopes`, `automation_mode`, `position` (display: below this many tracks), `mono`, `space`, `room` (3D Bus) | |
| `MasterTrack` ([#L936-L944](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L936-L944)) | `channels`, `height`, `volume_db`, `effects`, `envelopes` | No pan, mute or solo on the master. |
| `Clip` ([#L958-L1014](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L958-L1014)) | `id`, `name`, `media_id`, `start`, `source_in`, `source_out`, `gain_db`, `muted`, `locked`, `color`, `fade_in`, `fade_out`, `envelopes`, `effects`, `group`, `looped {length, phase}`, `stretch`, `audio_type` (dialogue, music, sfx, ambience), `channel_mix` (Source Channels: `sends [{left, right}]` per file channel, `to_speakers`), `reversed`, `inverted`, `invert_channel` (left, right) | Clip order in the list sets stacking: a later clip is on top. Timeline length = `(source_out - source_in)` converted from file rate to session rate, times the stretch ratio, or `looped.length` (`timeline_length`, [#L1127-L1155](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L1127-L1155)). |
| `Fade` | `length` (session samples, 0 is none), `shape` (linear, logarithmic, cosine, exponential, equal_power), `curve` (bend -1 to 1) | Engine curves: linear `t`; logarithmic `1-(1-t)^2`; cosine `0.5-0.5cos(pi t)`; exponential `t^2`; equal power `sin(pi t/2)`; a bend first maps `t` to `t^(4^-bend)` ([engine/plan.rs#L360-L384](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/plan.rs#L360-L384)). |
| `ClipStretch` ([#L1062-L1114](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L1062-L1114)) | `ratio` (timeline over natural length, 0.25 to 4), `keep_pitch` (false is varispeed), `pitch` (semitones, -24 to 24), `polyphonic`, `rendered`, `advanced {window_ms, transient_sensitivity, preserve_formants, high_precision}` | `rendered` and most of `advanced` are kept "so files from other editors round-trip"; only `window_ms` changes the sound. |
| `Envelope` ([#L1263-L1275](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L1263-L1275)) | `id`, `target` (`volume`, `pan`, `effect_param {slot_id, param}`, `space {param}`), `enabled`, `points [{position, value, curve}]`, `height` | Point positions are session samples; for clip envelopes they count from the clip's left edge. Curves: linear, hold, spline. Volume values are dB. |
| `Marker` ([#L1305-L1323](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L1305-L1323)) | `id`, `name`, `kind` (cue, subclip, chapter, ad_slot, social_clip, cd_track, cart_timer), `position`, `duration` (0 is a point), `description`, `chapter {url, image}`, `contracted_duration`, `color` | |
| `EffectChain` (Voice Chain) | `enabled`, `input_gain_db`, `output_gain_db`, `mix` (0 dry to 1 wet), `slots` | On clips, tracks, buses and the master. |
| `EffectSlot` ([#L1350-L1387](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs#L1350-L1387)) | `id`, `effect` (`altaudio.<name>` or `vst3:<32 hex class id>`), `enabled`, `params` (name to f64, a BTreeMap so the order is stable), `preset`, `sidechain` (track or bus id), `plugin_state` (base64, VST3 only), `data` (JSON text, for example the Amp Model capture), `position` (numbered slot 0 to 15), `routing {wide, inputs, outputs}` | |
| `Space`, `SpacePos` | `layout` ("2.0", "5.1", "7.1", "5.1.2", "7.1.2", "5.1.4", "7.1.4"), `listen` (stereo, binaural, speakers); spot `x`, `y`, `z`, `size`, `width`, `rotation`, `lfe`, `spread`, `spatialize`, `spatialize_mode` | See the 3D Space guide. |
| `SessionView` | `samples_per_pixel`, `scroll`, `scroll_y`, `playhead`, `selection {start, end}` | Saved where the user left off. |
| `SessionModes` | `podcast`, `voiceover` | Panel toggles. |
| `EpisodeMetadata` | `show_id`, `title`, `show`, `episode_number`, `season_number`, `description`, `artist`, `artwork`, `explicit` | |

Plug-in state: a VST3 plug-in's settings live in the plug-in while it runs. The interface fetches them before a session is set aside (`beforePark: pluginSync.captureAll()`) and the slot's `plugin_state` holds the base64 state as saved by the plug-in host (32-bit component length, then the component and controller blobs). See the VST3 guide for the capture path.

### 3.6 Paths: relative, absolute, missing, Link Media

Save ([file.rs#L40-L72](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/file.rs#L40-L72)): for each media file, `relative_path = relative_to(path, session folder)`. It is set only when both paths are absolute and on the same drive (`pathdiff::diff_paths`; a different Windows drive gives an absolute result, which is rejected). Separators are stored as `/` so a file saved on Windows opens elsewhere.

Open ([file.rs#L82-L116](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/file.rs#L82-L116)): for each media entry, try `session folder / relative_path` first; if it is a file, `path` becomes that, cleaned of `..` and `.` by `normalize` (no disk access). Else keep `path` if it is a file. Else the id goes into `missing`. Missing files are not an error: the session opens, the interface says "N media files were not found. They show as errors in the Files panel." Media length, rate and channels come from the file, not re-probed here (the media pool probes each file afterwards).

Link Media (decisions.md 2026-11-06, [file-tools.svelte.ts#L180-L199](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/file-tools.svelte.ts#L180-L199)): on the Files panel's right-click menu, and at the top of a clip's menu when its file could not be read. The user picks the new file; every session media entry with the old path gets `path = new` and `relative_path = null` through the `set_media` op, and the Files panel entry is repathed, all as one undoable step; then the file is read again. A path already in the Files panel is refused. `set_media` also shortens clips whose `source_out` is past a shorter new file, and lengthens clips that ended at the old file's end ([session/ops/media.rs#L37-L80](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/ops/media.rs#L37-L80)).

Collect Media ([session/collect.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/collect.rs)): optional on save (`collectOnSave`, `copyMedia` settings) and as a command. Copies used files into a folder beside the session (default `Audio`; the name is cleaned of `/ \ : * ? " < > |`). Files already anywhere inside the session folder stay. Same name, size and content reuses the copy; a different file with the same name gets a numbered name.

### 3.7 Saving

`session_save(path, libraries, backups)` ([commands/session.rs#L404-L468](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/session.rs#L404-L468)):

1. Target is the given path or the current one, through `with_extension`.
2. Library folders from the interface are stored in `libraries`.
3. Save As, or the first save of "Untitled Session", renames the session to the file stem.
4. If backups are on (0 to `MAX_BACKUPS = 50`), the file about to be replaced is copied to `Backups/<stem> YYYY-MM-DD HHMMSS.audfrm` beside it and only the newest N matching names are kept ([#L470-L516](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/session.rs#L470-L516)). Backup failure is only logged.
5. `file::save` drops media no clip uses (Files panel files that are not in the session stay out, so they do not come back on open), fills `relative_path`, sets `modified`, writes atomically.
6. Edits made while the file was being written are kept; only `media`, `modified`, `name`, `libraries` are taken from the saved copy. The autosave is cleared.

Before the save, the interface runs `beforeSave`: it keeps Waveform editor edits into the folder first, then collects media if the settings say so ([App.svelte#L999-L1010](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/App.svelte#L999-L1010)).

New Session with a folder creates `<folder>/<name>/<name>.audfrm` at once, refusing an existing file, so merged clips and copies have a home from the start ([commands/session.rs#L340-L402](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/session.rs#L340-L402)).

### 3.8 Autosave and crash recovery

- The interface calls `session_autosave(media)` every `autosaveMinutes` (setting; the built-in default `AUTOSAVE_MS` is 2 minutes) while there are unsaved changes, and also when a dirty session is set aside ([session-files.ts#L152-L171](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/session-files.ts#L152-L171), [App.svelte#L1798-L1803](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/App.svelte#L1798-L1803)). The `media` argument is the whole Files panel, so recovery also brings back files that were open but not used.
- `file::autosave` ([file.rs#L165-L200](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/file.rs#L165-L200)) writes `<app data>/autosave/<session id>.audfrm` (same atomic save), a note `<session id>.json` (`RecoveryNote { session_id, name, original_path, saved_at, owner_pid }`), and touches `running-<pid>.lock`.
- A clean save, close, open of another file or import removes the autosave and note (`clear_autosave`).
- At startup `find_recoverable` lists notes whose owner is not running. "Running" means the pid is this process, or its lock file was touched less than `LOCK_FRESH_SECS = 150` seconds ago. This stops a second running copy of the app from offering a live window's autosave as a crash. Notes without `owner_pid` (older builds) are always offered. If no `.audfrm` sits beside a note, an `.altaudio` autosave from before the rename is used. Newest first.
- The interface offers each one; recovering opens the autosave and keeps `original_path` as the session's path, so Save writes to the real file. Declining discards it. Only the first accepted one is recovered per start; older copies of other sessions stay for the next start ([session-files.ts#L341-L353](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/session-files.ts#L341-L353)).

**From reading the code:** the comment in `autosave` says "Media keeps absolute paths; relative ones would point inside the autosave folder", but the code does not clear `relative_path`. On recovery `file::open` tries `autosave folder / relative_path` first, which normally does not exist, so it falls back to the absolute path. It works, but Audiform 2 should clear `relative_path` in autosaves (or resolve it against `original_path`).

### 3.9 Templates

`.altemplate` files in the app's config `templates` folder ([commands/template.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/template.rs), [library/template.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/library/template.rs)). A `Template` document is `{format: "altaudio.template", schema_version: 1, id, name, description, created, session}`; the nested `session` is a full session document with its own header, upgraded by the session migrations.

- Save as template (`save_in`): copies the open session, resets `view.playhead`, `view.scroll`, `view.selection`, clears `template_id`, writes to a unique file name.
- New session from template (`session_from`): loads it, gives the session a new `id`, the chosen name, `template_id = template.id`, fresh `created` and `modified`. The template's rate, depth and mix win over the New Session dialog.
- **From reading the code:** `save_in` does not drop unused media and does not refresh `relative_path` for the template's folder, and `session_from` loads with `document::load`, not `file::open`, so relative paths are never tried and missing media are not reported. Template media (an intro, an outro) work only while their absolute paths exist.

### 3.10 Linked sessions (Insert Session as Clip)

[commands/linked.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/linked.rs). Another session's whole mix is placed as one clip. `linked_render` renders it once to a 32-bit float stereo WAV in a folder, and the clip's media gets `linked = { path: <session file>, stamp }`, where `stamp` is `"<size>-<modified ms>"`. `linked_check` compares stamps; once the inner session is saved again, the clip is rendered afresh. A session cannot hold its own mix. `load` reads a `.sesx` with `sesx::import` (default surrounds Behind) and anything else with `file::open`. When the inner session is open (set aside), it is rendered as it is in the editor, unsaved edits included, found by id. Opening a session from its file also opens the Audiform sessions its clips stand in for, set aside behind it; `.sesx` stand-ins are left closed (decisions.md 2026-10-31 and 2026-11-01).

**From reading the code:** `linked.path` is stored absolute only (no relative path), so moving a show folder breaks the link check even though the rendered WAV is found through its own `relative_path`. And `load` has no `.aup3` branch, so dropping an `.aup3` on the lanes would fail with a JSON error (see section 9).

### 3.11 Validation

[session/validate.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/validate.rs) checks meaning: `sample_rate > 0`, `bit_depth` in 16, 24, 32; unique ids across media, tracks, clips, buses, markers; outputs and sends point at existing buses; clips point at existing media; `source_out > source_in`; no bus output loops; chapter data only on chapter markers; contracted duration only on ad slots. It returns a list of `Issue { location, message }` (for example `tracks[2].clips[0]`) and never refuses a file (decisions.md "Session validation reports, it does not refuse").

**From reading the code:** `validate` is called only from tests (`fixtures.rs`, `roundtrip.rs`, `sesx_samples.rs`, the sesx unit test). Nothing in the app calls it on open or import, so issues are never shown to the user.

### 3.12 Published schemas

JSON Schemas in [`/schemas`](https://github.com/nwh212/Audiform/tree/042577e1ddee7b9718d82aea7947f9aab6edd628/schemas) are generated from the Rust types with `schemars` (`document::schema_json`). Doc comments become descriptions. [tests/schemas.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/schemas.rs) fails when any of session, library, template, workspace, host profile, keymap or preferences schemas drift; `UPDATE_SCHEMAS=1 cargo test --test schemas` regenerates them (line endings are normalized so a Windows checkout passes). `theme.schema.json` is not in that test.

---

## 4. `.sesx` import

Source: [src-tauri/src/sesx/mod.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs) (2137 lines, about 570 of them tests).

**Stale comment:** the module comment still says "all automation [is] left out". The code now reads drawn volume lines on tracks, buses, clips and the master, and 5.1 panner lines. It still leaves out other automation.

### 4.1 File structure

A `.sesx` is XML. Real files start with a byte order mark, `<?xml ...?>` and `<!DOCTYPE sesx>`. The parts the importer reads, assembled from the test samples in the file (which mirror real files):

```xml
<sesx version="1.9">
  <session audioChannelType="stereo" bitDepth="24" duration="400000" sampleRate="44100">
    <tracks>
      <audioTrack id="10001" index="1" preRendered="true">
        <trackParameters trackHeight="117"><name>HOST</name></trackParameters>
        <trackAudioParameters audioChannelType="mono" recordArmed="false" solo="true">
          <trackOutput outputID="10013" type="trackID"/>
          <component componentID="Audition.Fader" id="trackFader" name="volume" powered="true">
            <parameter index="0" name="volume" parameterValue="1.5848932"/>
            <parameter index="1" name="static gain" parameterValue="1"/>
          </component>
          <component componentID="Audition.Mute" id="trackMute" name="Mute" powered="true">
            <parameter index="1" name="mute" parameterValue="1"/>
          </component>
          <component componentID="Audition.StereoPanner" id="trackPan" name="StereoPanner" powered="true">
            <parameter index="0" name="Pan" parameterValue="-0.5"/>
          </component>
        </trackAudioParameters>
        <effectsRack preFader="true">
          <component id="rackInput" powered="true"><parameter index="0" parameterValue="1"/></component>
          <component id="rackOutput" powered="true"><parameter index="0" parameterValue="0.4217"/></component>
          <component id="rackMix" powered="true"><parameter index="0" parameterValue="0"/></component>
          <slot powered="true" slotIndex="1">
            <component componentID="3:56535441 54383141 75746f2d 54756e65" name="Tuner">
              <presetChunkData><![CDATA[base64...]]></presetChunkData>
            </component>
          </slot>
        </effectsRack>
        <audioClip fileID="0" id="0" name="host take" zOrder="0" lockedInTime="true"
                   sourceInPoint="1000" sourceOutPoint="23050" startPoint="44100" endPoint="66150">
          <component componentID="Audition.Fader" id="clipGain" name="volume" powered="true">
            <parameter index="0" name="volume" parameterValue="1">
              <parameterKeyframes>
                <parameterKeyframe sampleOffset="0" type="linear" value="1"/>
                <parameterKeyframe sampleOffset="22050" type="linear" value="0.5"/>
              </parameterKeyframes>
            </parameter>
          </component>
          <clipStretch stretchRatio="1.029" stretchType="solo" stretchMode="realtime" pitchAdjustment="2"
                       adaptiveWindowSize="37" transientSensitivity="40" preserveFormants="true" stretchQuality="high"/>
          <channelMap><channel index="0" sourceIndex="1"/></channelMap>
          <fadeIn startPoint="0" endPoint="4410" type="log"/>
          <fadeOut startPoint="17640" endPoint="22050" type="linear"/>
        </audioClip>
      </audioTrack>
      <busTrack id="10013">
        <trackParameters><name>Bus A</name></trackParameters>
        <trackAudioParameters><trackOutput outputID="10000" type="trackID"/></trackAudioParameters>
      </busTrack>
      <masterTrack id="10000"><effectsRack>...</effectsRack></masterTrack>
    </tracks>
  </session>
  <files>
    <file id="0" absolutePath="Z:\nowhere\host.wav" relativePath="Raw/host.wav"
          importerPrivateSettings="...;Channels:0:2;..."/>
  </files>
</sesx>
```

Positions (`startPoint`, `sourceInPoint`, keyframe `sampleOffset`) are integer samples. Gains are linear factors. Booleans are `"true"`/`"false"` attributes or `0`/`1` parameter values.

### 4.2 How it is read

`import_with(path, surrounds)` ([#L1223-L1496](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L1223-L1496)):

1. `std::fs::read_to_string`, then strip a leading `\u{feff}`. (A file that is not UTF-8 fails here as an I/O error.)
2. `roxmltree::Document::parse_with_options` with `allow_dtd: true` (needed for `<!DOCTYPE sesx>`). A parse error is `SesxError::Xml` ("This is not a readable .sesx session: ..."). No `<session>` anywhere is `SesxError::NoSession` ("This file has no multitrack session in it.").
3. Session: `sampleRate` (default 48000, clamped 8000 to 384000); name is the file stem; `audioChannelType="fivePointOne"` makes it a 3D Space session in the 5.1 layout.
4. Files: every `<files>/<file>` with an `id` becomes a `FileEntry { absolute, relative, channels }`.
5. `probe_files`: all files are located and probed at the same time with `rayon::par_iter`, so "a session with dozens of files opens in about the time of its slowest file instead of the sum of them" (added in [2a34486](https://github.com/nwh212/Audiform/commit/2a34486)). Every listed file is probed, used or not.
6. Buses first (so tracks can route to them), then tracks in document order, then the master.

Performance notes: decisions.md (2026-10-24) measured a stand-in for a tester's session (80 files, 23 tracks, 259 clips, 27 plug-ins): parsing about 30 ms, building waveform caches 1 to 2 seconds. The slow part was the interface redrawing every lane per file; lanes now redraw only for their own files, caches build four at a time, and plug-ins load a few at a time (half the cores, two to six). roxmltree holds the whole document in memory, which is fine for session files.

Small helpers: `attr_u64` parses any number as f64, clamps below 0 to 0 and rounds; `child` finds the first child with a tag; `component_param(node, componentID, name)` finds `<component componentID=..><parameter name=.. parameterValue=..>`.

Version differences: a track's or bus's components sit either inside `<trackAudioParameters>` or straight inside the track, "depending on the version that saved the file", so the mixer reads look at both nodes (`[audio, Some(track)]`). Keyframes sit either directly under the `<parameter>` or inside `<parameterKeyframes>`; the reader uses `descendants()` to cover both.

### 4.3 Carry-over table

Legend: **Kept** (same meaning, maybe a unit change), **Converted** (mapped to something else), **Approx.** (estimated), **Dropped (told)** (named or counted in the notice), **Dropped (silent)**.

Session and files:

| Source | Audiform | Status |
| --- | --- | --- |
| `session@sampleRate` | `Session.sample_rate`, clamped 8 kHz to 384 kHz, default 48 kHz | Kept |
| `session@audioChannelType="fivePointOne"` | `Session.space = { layout: 5.1, listen: stereo }` | Converted (section 4.10) |
| `session@audioChannelType` mono or stereo | not read; master stays stereo | Dropped (silent) |
| `session@bitDepth` | not read; `bit_depth` stays 32 | Dropped (silent) |
| `session@duration`, `<sessionState>` (playhead, selection) | not read | Dropped (silent) |
| File name | `Session.name` = file stem | Kept |
| Markers | not read at all | Dropped (silent) |
| Tempo, metronome, video tracks, any other track type | not read | Dropped (silent) |
| `file@relativePath`, `@absolutePath` | search order for the media file (section 4.11) | Kept |
| `file@importerPrivateSettings` `Channels:a:b` | channel count for a missing file's placeholder (`noted_channels` takes the last number) | Kept |
| File not found | placeholder `MediaRef` so clips still show | Kept (told: count in notice, error rows in Files panel) |

Tracks (`audioTrack`):

| Source | Audiform | Status |
| --- | --- | --- |
| `trackParameters/name` text | `Track.name`, default "Track N" | Kept |
| `trackAudioParameters@audioChannelType` | `"mono"` gives Mono; anything else (stereo, a 5.1 track) gives Stereo | Kept / Converted |
| `trackParameters@trackHeight` | `Track.height`, clamped 40 to 600 px | Kept |
| `audioTrack@preRendered="true"` | `Track.prerender` (Pre-render) | Converted ([c044cb0](https://github.com/nwh212/Audiform/commit/c044cb0)) |
| `@solo="true"` | `Track.solo` | Kept |
| `@recordArmed="true"` | `Track.record_armed` | Kept |
| `Audition.Mute` `mute` >= 0.5 | `Track.muted` | Kept |
| `Audition.StereoPanner` `Pan` | `Track.pan`: if `|p| <= 1` then `p*100`, else `p`; clamp -100 to 100; round to an integer | Converted (number only, no pan-law conversion; Audiform's law is the balance law in 3.5) |
| `Audition.Fader` `volume` x `static gain` | `Track.volume_db` (section 4.4) | Converted |
| Drawn `volume` keyframes | volume `Envelope` on the track; static gain becomes the fixed level | Converted |
| Other automation lanes (pan lines, mute lines, effect parameter lines) | not read | Dropped (silent) |
| `trackOutput@type="trackID"` `@outputID` naming a bus | `Output::Bus`; anything else `Output::Master` | Kept |
| Sends | not read | Dropped (silent) |
| `trackInput`, monitoring, `automationMode`, hue (colour), `trackMinimized`, `index` | not read | Dropped (silent) |
| `effectsRack` | `Track.effects` (section 4.8) | Converted / Dropped (told) |
| 5.1 panner (`Audition.StereoTo51Panner`, `Audition.MonoTo51Panner`) | `Track.space` plus Space automation lanes, 5.1 sessions only | Converted |

Buses (`busTrack`):

| Source | Audiform | Status |
| --- | --- | --- |
| name, mono, fader, mute, pan, volume line | as for tracks; default name "Bus N" | Kept / Converted |
| effects rack | `Bus.effects` | Converted |
| `trackOutput` | `Bus.output`; a bus routed to itself becomes Master | Kept |
| place among tracks | `Bus.position` = number of `audioTrack` siblings before it | Kept (display only) |
| a 5.1 bus in a 5.1 session | `Bus.room = true` (3D Bus) | Converted |
| a stereo or mono bus in a 5.1 session with its own 5.1 panner | `Bus.space` plus Space lanes | Converted |
| solo, sends | not read | Dropped (silent) |

Master (`masterTrack`):

| Source | Audiform | Status |
| --- | --- | --- |
| fader and volume line | `master.volume_db`, `master.envelopes` | Converted |
| effects rack | `master.effects` | Converted |
| mute, pan | read by `mix_params` but thrown away (Audiform's master has neither) | Dropped (silent) |

Clips (`audioClip`), [#L1385-L1471](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L1385-L1471):

| Source | Audiform | Status |
| --- | --- | --- |
| Order by `@zOrder` ascending | list order, so a higher `zOrder` is on top | Kept |
| `@fileID` (required; a clip without it is skipped) | `media_id`, one `MediaRef` per file id, made on first use | Kept |
| `@name` | `Clip.name`, default "Clip" | Kept |
| `@startPoint` | `Clip.start` (session samples) | Kept |
| `@sourceInPoint`, `@sourceOutPoint` | `source_in`, `source_out`, divided by the stretch ratio and rounded (section 4.6). A clip with `out <= in` is skipped (not counted). | Converted |
| `@endPoint` | used only to size a clip volume line; the clip length is derived from the source range | Not used for length |
| `@lockedInTime="true"` | `Clip.locked` | Kept |
| `Audition.Mute` on the clip | `Clip.muted` | Kept |
| `Audition.Fader` (`clipGain`) volume x static gain | `Clip.gain_db` | Converted |
| Drawn clip volume line | clip volume `Envelope`, moved to clip time and cut to the clip | Converted ([4379016](https://github.com/nwh212/Audiform/commit/4379016)) |
| `fadeIn`, `fadeOut` | `Fade { length = endPoint - startPoint, shape, curve: 0 }` | Converted (section 4.5) |
| `clipStretch` | `ClipStretch` | Converted (section 4.6) |
| `channelMap` | `Clip.channel_mix` (Source Channels) | Converted (section 4.7) |
| clip `effectsRack` | `Clip.effects` | Converted |
| hue, `looped`, crossfade attributes (`clipAutoCrossfade`, `crossFadeHeadClipID`...), `offline`, `select` | not read | Dropped (silent) |

### 4.4 Levels and volume lines

`fader(nodes)` finds the first `component[componentID="Audition.Fader"]` among the nodes' children. `level(nodes, clip)` ([#L1056-L1129](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L1056-L1129)):

- The fader keeps two gains that multiply: `volume` (what the user sees and draws on) and `static gain` (fixed, default 1).
- No keyframes under `volume`: level = `gain_to_db(volume.parameterValue * static_gain)`.
- With keyframes: each `parameterKeyframe` gives `(sampleOffset, gain_to_db(value))`, sorted by position; the line becomes a `volume` envelope with linear points (values rounded to 0.01 dB), and the fixed level is `gain_to_db(static_gain)` only. The keyframe `type` attribute is ignored for volume lines; every point is linear.
- `gain_to_db(g)`: `-96` for `g <= 0.000001`, else `20*log10(g)` rounded to 0.1 dB ([#L1175-L1181](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L1175-L1181)). Example: 1.58489 gives +4.0 dB; 0.5 x 0.5 gives -12.0 dB.
- Track and bus line positions count from the session start. Clip line positions count from the start of the clip's file (in the same units as `sourceInPoint`). So for a clip, `clip_line(points, origin = raw sourceInPoint, length = endPoint - startPoint)` subtracts the origin, keeps points strictly inside `0..length`, and adds a point at 0 and at `length` with the level interpolated from the points either side (or held from the nearest end). That also cuts a line shared by the pieces of a split clip to each piece ([#L1131-L1165](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L1131-L1165)).

Rack levels (`rack_levels`, [#L920-L951](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L920-L951)), read from the first `effectsRack` only: components with `id="rackInput"` and `id="rackOutput"` have a gain factor as parameter `index="0"`; dB = `20*log10(max(g, 1e-6))` rounded to 0.1 (0.4217 gives -7.5 dB) into `input_gain_db` and `output_gain_db`. `id="rackMix"` parameter 0 is read as the dry share, so `mix = clamp(1 - v, 0, 1)` (0 means fully wet). A component with `powered="false"` counts as unchanged. The mix reading is "the one guess here" (decisions.md 2026-11-01): only fully wet racks were in the sample sessions.

### 4.5 Fades

`fade(node)` ([#L1183-L1201](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L1183-L1201)): `length = endPoint - startPoint` (both relative to the clip start, so a fade out at `17640..22050` is 4410 long and its position is implied by the clip end). Shape: `type="log"` to Logarithmic, `"cosine"` to Cosine, anything else Linear. `curve` (bend) is always 0. Any curve-amount attribute the source may carry is not read. **Unclear:** how closely Audiform's `1-(1-t)^2` "logarithmic" curve matches the source program's log fade; nobody compared them by ear or measurement in the decisions log.

### 4.6 Stretch and pitch

`stretch(node)` ([#L999-L1029](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L999-L1029)):

| `clipStretch` attribute | `ClipStretch` field | Mapping |
| --- | --- | --- |
| `stretchRatio` | `ratio` | clamp 0.25 to 4 (`STRETCH_MIN`, `STRETCH_MAX`); not finite or <= 0 gives 1 |
| `stretchType` | `keep_pitch`, `polyphonic` | `varispeed` or `resample` (any case): `keep_pitch = false`, `polyphonic = false`, `pitch = 0`. `solo`: keep pitch, `polyphonic = false`. Anything else (for example `normal`): keep pitch, polyphonic. |
| `pitchAdjustment` | `pitch` | semitones, clamp -24 to 24; ignored for varispeed |
| `stretchMode="rendered"` | `rendered` | kept for round trips; playback is the same either way |
| `adaptiveWindowSize` | `advanced.window_ms` | clamp 0 to 200 (default 0 when absent) |
| `transientSensitivity` | `advanced.transient_sensitivity` | clamp 0 to 100 (default 40) |
| `preserveFormants="true"` | `advanced.preserve_formants` | |
| `stretchQuality="high"` | `advanced.high_precision` | |

Time units: "A stretched clip's source points are in stretched time: divide by the ratio for points in the file." So `source_in = round(sourceInPoint / ratio)`, same for out. This was checked against a tester's session where the same take appears stretched and not (decisions.md 2026-10-24). Example: ratio 1.029, `10290..20580` becomes `10000..20000`.

Every clip with a `clipStretch` element counts in `report.stretched`, even at ratio 1 and pitch 0.

### 4.7 Source channels (single-channel sources)

`channel_map(clip, file_channels, stereo_track)` ([#L608-L666](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L608-L666), [a2c55f3](https://github.com/nwh212/Audiform/commit/a2c55f3)): reads `<channelMap><channel index=I sourceIndex=S/>`. Returns `None` (the file's usual mix) when the map is empty, the file's channel count is 0, any source index is out of range, or the map is usual (a mono file with every entry from 0, or `n.min(2)` entries each mapping I to I). Otherwise it builds `ChannelMix { sends: one {left, right} per file channel, to_speakers: false }`:

- One listed channel, or a mono track: that file channel plays on both sides (`left = right = 1`). This is how a clip that uses only the left or right of a stereo file comes across: as a mono source in the middle.
- Two listed channels on a stereo track: `index 0` sets that source's `left = 1`, `index 1` sets `right = 1`.

`file_channels` comes from the probed file, or for a missing file from `importerPrivateSettings`, or 2 for stereo tracks and 1 for mono.

### 4.8 Effects racks

`racks(node)` reads every `effectsRack` directly inside the node into one chain ([#L900-L918](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L900-L918)). If the merged chain has more than `MAX_SLOTS = 16`, the extras are cut and added to `skipped_effects` (without names).

`read_rack(rack)` ([#L454-L594](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L454-L594)): every `<slot>` with a `<component>`, sorted by `slotIndex` (missing sorts last). Empty slots are skipped. For each component, in this order:

| componentID | Becomes | Enabled when | Counted as |
| --- | --- | --- | --- |
| `7354704a-d87c-11e0-b05b-763c4824019b` (per-frequency phase shifter) | `altaudio.phase_curve` from `presetChunkData`; if that does not parse, falls through and is skipped | slot not `powered="false"` | `converted` |
| `e0b23f05-f1a7-4ef7-9b50-7ec3e3002058` (hard limiter) | `altaudio.limiter` | slot powered | `converted` |
| `8edcd326-33de-4a4d-985b-f1bf48ed1427` (Parametric Equalizer) | `altaudio.parametric_eq` | slot powered and its `Bypass` < 0.5 | `converted` |
| `3:abcdef01 9182faeb 58766f78 58766f78` (the xvox vocal strip plug-in, a VST3 class id) | `altaudio.vox_strip` (Vox Strip) from `presetChunkData`; if that does not parse, falls through to the VST3 branch and comes in as that plug-in with its state | the plug-in's own `globalEnable` >= 0.5 (or absent) and slot powered | `converted` |
| any other `3:` id (VST3) | `vst3:<32 hex>` with `plugin_state` converted from `presetChunkData` | slot powered | `plugins`; also `missing_plugins` (by name) when not in this computer's scanned list |
| anything else (the other program's other built-in effects, unknown ids, a component without an id) | nothing | | `skipped_effects` and `skipped_names` (the component's `name`, each once) |

Every created slot keeps `position = slotIndex` when it is below 16, so a chain with gaps keeps its numbering (decisions.md 2026-10-24). A conversion is only attempted while the chain has fewer than 16 slots.

VST3 class id ([`vst3_effect`, #L954-L982](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L954-L982)): `"3:"` then four groups of eight hex digits `a b c d`. Two byte layouts are built:

- plain: the four words big-endian, concatenated.
- COM (Windows GUID layout): `a` little-endian, then the high 16 bits of `b` little-endian, then the low 16 bits of `b` little-endian, then `c` and `d` big-endian.

Each is tried as `vst3:<HEX>` against the scanned plug-ins (`plugins::find`); the first found wins. If neither is scanned, the platform default is used (COM on Windows, plain elsewhere). The test expects `3:56535441 54383141 75746f2d 54756e65` to give `vst3:41545356385441317574...` on Windows and `vst3:56535441543831417574...` elsewhere.

VST3 state ([`chunk_to_state`, #L984-L997](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L984-L997)): `presetChunkData` (CDATA) is base64 of `u64 LE component length`, `u64 LE controller length`, component bytes, controller bytes. If `16 + comp + ctl != total`, the state is dropped (the plug-in still comes in, with defaults). Audiform's form is base64 of `u32 LE component length`, then both blobs. A plug-in not scanned on this computer still comes in with its state and is named in the notice, so an install or scan brings it back.

### 4.9 Built-in effect conversions, parameter by parameter

All the other program's built-in effects store `parameterValue` as 0 to 1 knob positions with a `name`. Values below are the code's formulas.

**Parametric Equalizer to Parametric EQ** (`parametric_eq_params`, [#L704-L778](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L704-L778), fixed in [3c82ff6](https://github.com/nwh212/Audiform/commit/3c82ff6) against the other program's own readout):

- `nyquist = (Original Sample Rate) * 192000 / 2`, default 22050 (the knob keeps the rate as a share of 192 kHz; 0.2297 is 44.1 kHz).
- Frequency: `hz(v) = round(20 + v * (nyquist - 20))`.
- Band and shelf gain: `db(v) = round((v - 0.5) * 2 * 48, 0.01)`, so 0.5 is 0 dB and the ends are +/-48 dB (`EQ_DB_HALF_SPAN = 48`).
- Overall gain: `round(v * 60 - 40, 0.1)`; 2/3 is 0 dB.
- Q: `round(v * 10000, 0.01)`; 0.0001 is Q 1.
- On switches: `v >= 0.5`.
- Pass slope: `db = 12 + round(v * 5) * 6` dB/oct; Audiform's choices are 12, 24, 48: below 18 gives index 0 (12), below 36 gives 1 (24), else 2 (48). 0.4 is 24 dB/oct.

| Source parameter name | Audiform key | Conversion |
| --- | --- | --- |
| `Gain` | `gain` | overall gain |
| `High-Pass Filter Enable`, `High-Pass Cutoff Frequency`, `High-Pass Slope` | `hp_on`, `hp_freq`, `hp_slope` | on, hz, slope |
| `Low Shelf Enable`, `Low Shelf Frequency`, `Low Shelf Gain` | `ls_on`, `ls_freq`, `ls_gain` | on, hz, db |
| `EQ Band N Enable`, `Center Frequency`, `Gain`, `Q` (N = 1 to 5) | `bN_on`, `bN_freq`, `bN_gain`, `bN_q` | on, hz, db, Q |
| `High Shelf Enable`, `High Shelf Frequency`, `High Shelf Gain` | `hs_on`, `hs_freq`, `hs_gain` | on, hz, db |
| `Low-Pass Filter Enable`, `Low-Pass Cutoff Frequency`, `Low-Pass Slope` | `lp_on`, `lp_freq`, `lp_slope` | on, hz, slope |
| `Bypass` | slot `enabled = false` when >= 0.5 | removed from params |

Band types: shelves map to Audiform's shelves (Audiform's shelf Q is fixed at 1.0; any source shelf slope is not read), bands to peaking bands, passes to Butterworth cascades (1, 2 or 4 sections). Only parameters present are written; missing ones keep Audiform's defaults. **From reading the code:** values are not clamped to Audiform's ranges (band gain -24 to 24 dB, Q 0.1 to 100, `hp_freq` 10 to 2000 Hz, `hs_freq` 1 to 20 kHz, `lp_freq` 1 to 22 kHz), so a +/-48 dB band or a 20 Hz high shelf is stored out of range. Whether the effect clamps on load was not checked here.

**Hard limiter to Limiter** (`hard_limiter_params`, [#L669-L702](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L669-L702), [3df4676](https://github.com/nwh212/Audiform/commit/3df4676), from Altered Audio Editor):

| Source | Audiform | Conversion | Confidence |
| --- | --- | --- | --- |
| `Maximum Amplitude` | `ceiling` (dB) | `round(clamp((v - 1) * 100, -60, 0), 0.1)`: 1 is 0 dB, each 0.01 below is 1 dB (its "Limit to -6dB" preset reads 0.94) | exact |
| `Input Boost` | `input_boost` (dB) | `round(clamp((v - 2/3) * 30, -24, 24), 0.1)` | estimate |
| `Link Channels` | `linked` | `v >= 0.5` | |
| look-ahead, release | Audiform defaults (5 ms, 100 ms) | not read "until their scales are known" | dropped |

**xvox vocal strip to Vox Strip** (`vox_strip_params`, [#L780-L840](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L780-L840), [a984b3a](https://github.com/nwh212/Audiform/commit/a984b3a)): the plug-in state is base64; inside it the code finds `<XVox>...</XVox>` and reads every `<PARAM id="..." value="..."/>` by string splitting (not an XML parser). Gains are linear, trims dB, tone knobs -1 to 1.

| `PARAM id` | Vox Strip key | Conversion |
| --- | --- | --- |
| `input_gain`, `output_gain` | `input`, `output` | `20*log10(max(g, 1e-4))` dB |
| `hpf` | `low_cut` | on if >= 0.5 |
| `dyn_enable` | `dyn_on` | on |
| `dyn_mode` | `dyn_style` | rounded index |
| `dyn_comp` | `comp` | x100, rounded |
| `deesser_amount` | `deess` | x100, rounded |
| `dyn_trim` | `dyn_trim` | as is (dB) |
| `tone_enable` | `tone_on` | on |
| `tone_mode` | `tone_style` | rounded index |
| `tone_low`, `tone_mid`, `tone_high` | `low`, `mid`, `high` | x12 (dB) |
| `tone_exciter_gain` | `air` | x100, rounded |
| `tone_trim` | `tone_trim` | as is |
| `sfx_enable` | `fx_on` | on |
| `sfx_mode` | `fx_style` | rounded index |
| `sfx_control` | `fx` | x100, rounded |
| `sfx_trim` | `fx_trim` | as is |
| `globalEnable` | slot enabled | < 0.5 disables |
| `space_*` (reverb and delay) | nothing | Dropped (silent): Vox Strip's Space module was removed on 2026-10-31 |

**Unclear:** whether the source's mode indexes (`dyn_mode`, `tone_mode`, `sfx_mode`) line up with Vox Strip's style lists one for one; the code assumes they do.

**Per-frequency phase shifter to Phase Curve** (`phase_curve_params`, [#L842-L898](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L842-L898), [b13f804](https://github.com/nwh212/Audiform/commit/b13f804), fixed in [5ba8efd](https://github.com/nwh212/Audiform/commit/5ba8efd)): `presetChunkData` is base64 of little-endian f32 values, read from testers' sessions:

| Index | Meaning | Use |
| --- | --- | --- |
| 0 | sample rate | must be 8000 to 768000, else no conversion |
| 1 | channel turned (0 left, 1 right) | `channels = 2` (Right only) if >= 0.5, else 1 (Left only) |
| 6, 7 | low and high ends of the frequency axis as shares of half the rate | `lo = clamp(f6 * nyq, 10, nyq)`, `hi = clamp(f7 * nyq, lo + 1, nyq)` |
| 10 | point count | must be >= 2 and fit the data |
| 11 + 2i, 12 + 2i | point i: x along the axis (0 to 1), phase y (0 to 1) | `freq = lo * (hi/lo)^x` (log axis), `phase = (y - 0.5) * 360` degrees |
| others (2 to 5, 8, 9) | not understood | ignored; FFT size keeps Audiform's default (8192) |

Output: `points` (count), `spline = 0` (straight lines, because a smooth curve overshoots past +/-180 where a drawn curve jumps from +180 to -180), `channels`, `p1_freq`/`p1_phase` up to `p16_*` (rounded to 0.1). More than `MAX_POINTS = 16` points are thinned evenly, keeping the first and last. The log axis and the +/-180 range are assumptions ("neither is confirmed"), and the channel position is "not confirmed against that editor's window".

### 4.10 5.1 sessions to 3D Space

Full detail is in the 3D Space guide, section 3.6. Summary of the import code ([#L113-L412](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L113-L412), [4023041](https://github.com/nwh212/Audiform/commit/4023041), [4c26033](https://github.com/nwh212/Audiform/commit/4c26033)):

- Panner component: `Audition.StereoTo51Panner` is looked for first, then `Audition.MonoTo51Panner` (the mono one forces a mono spot), in the track's audio parameters and the track node.
- Settings and units:

| Parameter | Fixed value | Drawn keyframe value | Default |
| --- | --- | --- | --- |
| `Pan: Angle` | radians, front at a quarter turn: `deg = to_degrees(r) - 90` | 0 to 1 for a whole turn from straight back: `deg = v*360 - 180 - 90` | 0 |
| `Pan: Radius` | 0 to 1 | 0 to 1 | 1 |
| `Pan: Stereo Spread` | radians to degrees | `v*360 - 180` | 0 |
| `Pan: LFE` | 0 to 1 | 0 to 1 | 0 |
| `Pan: Center` | not read (decision 2026-11-01: it limits the center speaker, which 3D Space cannot express; [2b5101b](https://github.com/nwh212/Audiform/commit/2b5101b) mapped it to size and [5dafcda](https://github.com/nwh212/Audiform/commit/5dafcda) reverted that) | | |

  Keyframes with `type="hold"` hold until the next point; others are linear.
- Direction bending (`room_angle`): the panner's speakers sit at 30 and 110 degrees; the room is square with the front speakers in its front corners. With `Surrounds::Behind`: 0 to 30 degrees maps linearly to 0 to 45, 30 to 110 maps to 45 to 135, 110 to 180 maps to 135 to 180. With `Sides`, 110 maps to 90 instead of 135. `room_point(deg, radius)` puts the spot on a square: `(r sin a / k, r cos a / k)` with `k = max(|sin a|, |cos a|)`.
- A stereo source's two sides each keep their own direction (`angle -/+ spread/2`); the spot is their midpoint, `width` half their distance, `rotation = atan2(-dy, dx)` in degrees rounded to 0.1. LFE carries over as is.
- Drawn lines are replayed: moments are each keyframe time plus extra steps so the direction never moves more than `STEP_DEG = 5` degrees between looks (capped near `MAX_POINTS = 20000`). Lanes written: X, Y, Width (stereo only), Rotation (stereo only), LFE, values rounded to 0.001, linear. Rotation steps across its +/-180 wrap with two extra points instead of sweeping back. Lanes that never change by more than 0.001 are left out.
- 5.1 bus: `room = true` (3D Bus). Stereo bus with its own panner: a spot.
- The session plays folded to stereo while working (`listen` default), and exports to 5.1 or stereo as usual.

### 4.11 Finding media

`find_file(entry, sesx folder)` ([#L1511-L1530](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L1511-L1530)) tries, in order, the first that is a file and probes:

1. `relativePath` (backslashes turned to `/`) joined to the `.sesx` folder.
2. `absolutePath` as stored.
3. The file name from `absolutePath` (split on `\` or `/`) beside the `.sesx`, so a Windows path read on another system still finds a file sitting next to the session.

Found: `MediaRef` from the probe (`codecs::probe`): real path, rate, channels, length, `relative_path: None` (filled on the first save). Not found: a placeholder with `path` = the stored absolute (or relative) string, `sample_rate` = session rate, `channels` = noted channels or 2/1 by track, `length` = the largest `source_out` of any clip using it, and its id in `missing`. The clips still show where they were; the Files panel lists the file as an error; Link Media fixes it.

### 4.12 What the user is told, and what is silent

`ImportReport` ([#L46-L70](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L46-L70)): `tracks`, `clips`, `skipped_effects`, `skipped_names`, `buses`, `plugins`, `missing_plugins`, `stretched`, `converted`, `missing_files`. Serialized camelCase.

Told: built-in effects left out (by name), plug-ins brought in, plug-ins not installed or not scanned (by name), stretched clips, conversions, missing files (count).

Silent: markers, sends, automation other than volume and 5.1 panners, clip colours, track colours, master mute and pan, bit depth, playhead, clips without a file id or with an empty source range, Vox Strip space settings, Parametric EQ values out of Audiform's range, limiter look-ahead and release, the panner's Center, slots cut beyond 16 (counted but not named).

### 4.13 Rack preset files

`read_rack_presets(text, report)` ([#L429-L452](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L429-L452), [1b195ab](https://github.com/nwh212/Audiform/commit/1b195ab)): parses any XML, and every element that has `<slot>` children is a rack. Its name is the nearest `name` or `presetName` attribute on it or an ancestor, else "Imported Rack N". Each rack goes through `read_rack`, so the same conversions apply. Racks with no slot left are dropped. The command adds them to the user's presets under Yours; a taken name gets a number.

---

## 5. `.sesx` export

Source: [src-tauri/src/sesx/export.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/export.rs). Decision (2026-10-19, again 2026-10-27): export, not save, "because a `.sesx` cannot hold Voice Chains, envelopes, loops, stretch or markers, and saving to it would quietly lose them" (stretch was added to export later). **Never confirmed in the other program:** "Not yet checked by opening it in the other editor; the project lead will try it." No later entry says it was tried.

### 5.1 What is written

`to_sesx(session, base)` builds the XML with `write!` into a `String` ([#L318-L477](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/export.rs#L318-L477)); `export` writes it to `<path>.sesx.tmp` and renames it over the target (no fsync, unlike `document::save`). Text is escaped by `esc` (`& < > " '`, and control characters below 0x20 except tab are dropped). Every `componentGuid` is a fresh UUID v4 per file.

```xml
<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<!DOCTYPE sesx>
<sesx version="1.9">
  <session audioChannelType="stereo" bitDepth="32" duration="336000" sampleRate="48000">
    <tracks>
      <audioTrack automationLaneOpenState="false" id="10001" index="1" select="false" visible="true">
        <trackParameters trackHeight="96" trackHue="-1" trackMinimized="false">
          <name>Host &lt;1&gt;</name>
        </trackParameters>
        <trackAudioParameters audioChannelType="mono" automationMode="1" monitoring="false" recordArmed="false" solo="false" soloSafe="false">
          <trackOutput outputID="10000" type="trackID"/>
          <trackInput inputID="-1"/>
          <component componentGuid="..." componentID="Audition.Fader" id="trackFader" name="volume" powered="true">
            <parameter index="0" name="volume" parameterValue="0.501187..."/>
            <parameter index="1" name="static gain" parameterValue="1"/>
          </component>
          <component componentGuid="..." componentID="Audition.Mute" id="trackMute" name="Mute" powered="true">
            <parameter index="0" parameterValue="0"/>
            <parameter index="1" name="mute" parameterValue="0"/>
          </component>
          <component componentGuid="..." componentID="Audition.StereoPanner" id="trackPan" name="StereoPanner" powered="true">
            <parameter index="0" name="Pan" parameterValue="-25"/>
          </component>
        </trackAudioParameters>
        <audioClip clipAutoCrossfade="true" crossFadeHeadClipID="-1" crossFadeTailClipID="-1" endPoint="288000" fileID="0" hue="-1" id="0" lockedInTime="true" looped="false" name="Take" offline="false" select="false" sourceInPoint="48000" sourceOutPoint="240000" startPoint="96000" zOrder="0">
          (clipGain fader and clipMute components)
          <fadeIn crossFadeLinkType="linkedAsymmetric" endPoint="4800" startPoint="0" type="cosine"/>
          <fadeOut crossFadeLinkType="linkedAsymmetric" endPoint="192000" startPoint="192000" type="linear"/>
        </audioClip>
      </audioTrack>
      <masterTrack automationLaneOpenState="false" id="10000" index="2" select="false" visible="true">
        ... <name>Mix</name> ... <trackOutput outputID="1" type="hardwareOutput"/> ...
      </masterTrack>
    </tracks>
    <sessionState ctiPosition="0" smpteStart="0">
      <selectionState selectionDuration="0" selectionStart="0"/>
    </sessionState>
  </session>
  <files>
    <file absolutePath="/show/Audio/host &amp; guest.wav" id="0" relativePath="Audio/host &amp; guest.wav"/>
  </files>
</sesx>
```

### 5.2 Mapping

| Audiform | Written as | Notes |
| --- | --- | --- |
| `sample_rate` | `session@sampleRate` | |
| `bit_depth` | `session@bitDepth` | 16, 24 or 32, else 32 |
| master channels | `session@audioChannelType` and master `audioChannelType`: `mono` or `stereo` | a 3D Space session exports as stereo |
| end of the last clip | `session@duration` (at least 1) | |
| `view.playhead` | `sessionState@ctiPosition`, `selectionStart` | selection length 0 |
| track (each, in order) | `audioTrack` `id = 10001 + index`, `index = i + 1` | MIDI tracks are written as empty audio tracks |
| `name`, `height`, `channels`, `solo`, `record_armed` | name, `trackHeight`, `audioChannelType`, `solo`, `recordArmed` | |
| `volume_db` | fader `volume = 10^(dB/20)` (0 at or below -96 dB), `static gain = 1` | |
| `muted` | `Audition.Mute` params 0 and 1 = 0 or 1 | |
| `pan` | `Audition.StereoPanner` `Pan` = Audiform pan clamped -100 to 100 | see 5.4 |
| output | always `trackOutput outputID="10000"` (the master) | buses are not written |
| track Voice Chain | `effectsRack preFader="true"` with VST3 slots only | section 5.3 |
| clip | `audioClip` with `id` and `zOrder` = a running clip number across the session | |
| `start` | `startPoint` | |
| timeline length | `endPoint = start + len`, `len = clip.timeline_length(file rate, session rate)` | clips of length 0 or with missing media are skipped |
| `source_in`, `source_out` | not stretched: `sourceInPoint = source_in`, `sourceOutPoint = source_in + min(source_out - source_in, max(len, 1))`. Stretched: `sourceInPoint = round(source_in * ratio)`, `sourceOutPoint = sourceInPoint + len` | see 5.4 |
| `locked` | `lockedInTime` | |
| `gain_db`, `muted` | `clipGain` fader and `clipMute` | |
| `stretch` (not looped) | `clipStretch`: `stretchRatio`, `pitchAdjustment` (0 for varispeed), `stretchType` = `varispeed` / `normal` (polyphonic) / `solo`, `stretchMode` = `rendered` / `realtime`, `adaptiveWindowSize`, `transientSensitivity`, `preserveFormants`, `stretchQuality` = `high` / `medium` | without `advanced`, writes window 37 ms, sensitivity 40, formants true, high |
| `fade_in` | `fadeIn startPoint=0 endPoint=min(len, fade)` | |
| `fade_out` | `fadeOut startPoint=len - min(len, fade) endPoint=len` | |
| fade shape | `log` (Logarithmic), `cosine` (Cosine), `linear` (Linear, Exponential, EqualPower) | `curve` bend dropped |
| looped clip | plain clip playing its region once (`min(src_len, loop length)`); stretch dropped | counted |
| master | `masterTrack id=10000`, name "Mix", fader from `master.volume_db`, mute 0, pan 0, `trackOutput type="hardwareOutput" outputID="1"`, master VST3 rack | |
| media | every entry in `session.media` (in-memory list, not only used files) as `<file id=index absolutePath relativePath?>`; relative to the exported file's folder via `pathdiff`, with `/` | |
| VST3 slot | `slot powered = slot.enabled && chain.enabled`, `slotIndex = max(position or next, next)`; component `componentID` from `vst3_component`, `name` from the scanned plug-in or "Plug-in", `presetName="(Custom)"`, `presetChunkData` CDATA from `state_to_chunk` | class id written in this platform's layout (COM on Windows, plain elsewhere); state back to `u64, u64, comp, ctl` |

### 5.3 What is lost

`SesxExportReport` ([#L26-L43](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/export.rs#L26-L43)) counts some losses:

| Lost | Reported as |
| --- | --- |
| Track, clip and master chains holding at least one enabled built-in effect (the plug-ins in them are still written) | `chains_left_out` |
| Clips with an enabled, non-empty envelope (volume, pan, effect, space) | `envelopes_left_out` |
| Looped clips written as plain clips | `clips_simplified` |
| Markers | `markers_left_out` |
| Buses, plus tracks routed to a bus, with sends, or with a sidechain key (every track goes straight to the mix) | `routing_left_out` |

Lost silently: bus effects and levels (only counted as buses); track, bus and master envelopes (only clip envelopes are counted); chain input level, output level and mix (the importer reads them, the exporter does not write them); disabled built-in effects; track and clip colours; `prerender` (the importer reads `preRendered`, the exporter does not write it); track `hidden`, `mono`, `polarity_inverted`, `collapsed`, input; clip `channel_mix` (Source Channels), `reversed`, `inverted`, `invert_channel`, `group`, `audio_type`; fade bend; MIDI clips and instruments; 3D Space positions and lanes; tempo; playlist; metadata; clips whose media is missing from `session.media`. The interface shows the counted ones after export (decisions.md 2026-10-19).

### 5.4 Known mismatches found while reading

**From reading the code:**

1. **Pan scale.** The exporter writes `Pan` as -100 to 100. The importer's own sample (mirroring real files) has `-0.5` for 50 percent left, and the importer treats `|p| <= 1` as a fraction. So the round-trip test passes (Audiform reads -25 back as -25), but the other program probably expects -1 to 1 and would read -25 as fully left, and an Audiform pan of 1 would come back as 100 on re-import. Write `pan / 100` in Audiform 2.
2. **Sample-rate mismatch.** For a clip whose file rate differs from the session rate, `sourceOutPoint - sourceInPoint` is `min(file-rate span, session-rate length)`. With a 48 kHz file in a 44.1 kHz session, the written source span is the shorter session-rate length, so a re-import shortens the clip by about 8 percent. More basically, the importer treats `sourceInPoint` as file-rate samples and the exporter mixes file-rate and session-rate units. **Unclear:** which unit the other program uses for source points when the file rate differs. Get a real mixed-rate `.sesx` and settle it before writing Audiform 2's mapping.
3. Clip `id` and `zOrder` are numbered across the whole session, not per track. Whether the other program cares is unknown.

### 5.5 Export tests

`round_trips_through_the_importer`, `round_trips_stretch_pitch_and_plug_ins`, `counts_what_is_left_out` ([#L541-L657](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/export.rs#L541-L657)). They prove Audiform reads its own export back (name escaping, mono, volume within 0.11 dB, pan, positions, lock, cosine fade, rate, relative path with `&amp;`, plug-in id and state, disabled slot, stretch ratio, pitch, polyphonic). They do not prove the other program reads it.

---

## 6. `.aup3` import

Source: [src-tauri/src/aup3/mod.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/aup3/mod.rs) (708 lines), added in [070e66c](https://github.com/nwh212/Audiform/commit/070e66c) on 2026-10-29, shipped in 0.5.1.

### 6.1 Status

Built and wired: File > Open, File > Import Session and dropping on the window all accept `.aup3` (decisions.md 2026-10-29; release notes 0.5.1). It was parked on 2026-10-18 and then built eleven days later; `docs/backlog.md` still lists it as parked (stale). Tested only through two small unit tests and an ignored test that needs a real project (`AUP3_TEST=/path cargo test real_project -- --ignored`). There is no fixture `.aup3` in the repo.

### 6.2 File structure as the code reads it

- SQLite. The importer opens it with `OpenFlags::SQLITE_OPEN_READ_ONLY` and never writes.
- Table `project`, columns `dict` and `doc` (blobs): `SELECT dict, doc FROM project LIMIT 1`. If that fails: "it has no saved project data (it may only hold an autosave)".
- Table `sampleblocks`: `SELECT sampleformat, samples FROM sampleblocks WHERE blockid = ?1`.

`dict` (`names`, [#L106-L117](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/aup3/mod.rs#L106-L117)): the first 2 bytes are skipped; then repeated entries of `1 byte tag, u16 LE id, u16 LE byte length, UTF-16LE name`. It maps id to name.

`doc` (`tree`, [#L119-L180](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/aup3/mod.rs#L119-L180)): a stream of tagged records building an element tree (like XML without text):

| Tag | Meaning | Bytes after tag | Value |
| --- | --- | --- | --- |
| `0x01` | start element | `u16` name id | push |
| `0x02` | end element | `u16` name id | pop; attach to parent or make root |
| `0x03` | string attribute | `u16` name id, `u32` byte length, UTF-16LE text | `Text` |
| `0x04`, `0x06` | integer attribute | `u16` id, `i32` | `Num` |
| `0x05` | bool attribute | `u16` id, 1 byte | `Num` |
| `0x07` | 64-bit integer | `u16` id, `i64` | `Num` |
| `0x08` | size | `u16` id, `u32` | `Num` |
| `0x09` | float | `u16` id, `f32`, then 4 more bytes (skipped) | `Num` |
| `0x0a` | double | `u16` id, `f64`, then 4 more bytes (skipped) | `Num` |
| `0x0c` | raw data | `u32` length, bytes | skipped |
| anything else | | | error "unknown value kind in the project data" |

The root must be named `project`. Elements read: `project@rate`; `wavetrack@name, channel, linked, mute, solo, rate, gain, pan`; `waveclip@name, offset, trimLeft, trimRight, clipStretchRatio, centShift`; the first `sequence@numsamples` and its `waveblock@start, blockid`; the first `envelope`'s `controlpoint@t, val`. Everything else (label tracks, note tracks, time tracks, realtime effects, tags, selection) is not read.

Sample formats (`Format::of`): `0x4000F` 32-bit float; `0x20001` 16-bit integer (`/32768`); `0x40001` 24-bit integer kept in 4 bytes (`/8388608`). Any other code fails the import ("an unsupported sample format"). A block id with no row (for example a silent block) reads as silence.

### 6.3 How it is read

`import(path, audio_dir)` ([#L458-L675](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/aup3/mod.rs#L458-L675)):

1. Parse the tree. `project_rate = rate` clamped 8000 to 384000 (default 44100). The session is named after the file stem.
2. `raw_tracks`: every `wavetrack`; clips sorted by `offset + trimLeft`.
3. Stereo pairing: a track with `channel == 0` and `linked`, followed by one with `channel == 1` and the same number of clips, becomes one stereo track; clips are paired by index. Anything else is a mono track (an unpaired right channel becomes its own mono track).
4. Shared audio: clips often share one recording ("a split keeps the same blocks"). One `Source` per distinct tuple of first block ids per channel; when a later clip's block list extends an earlier one's (`starts_with`), the longer list is kept, so the file is long enough for every clip using it.
5. Writes each source to `<audio_dir>/<stem> NN.wav` (NN from 01), 32-bit float, channels interleaved, read in chunks of 65536 frames per channel through a `Reader` that pulls blocks one SQL query at a time. `audio_dir` is `<project folder>/<stem> audio` (`audio_folder`). Existing files of the same name are overwritten.
6. Builds tracks and clips with the math below. `missing` is always empty.

### 6.4 Clip math

For each clip (with `sr` = project rate, `mrate` = the source's track rate):

- `ratio = clamp(clipStretchRatio, 0.25, 4)` (not finite or <= 0 gives 1).
- `natural = numsamples / mrate` seconds.
- `left = max(trimLeft, 0)`, `at = offset + left` (seconds). If `at < 0`, the head is cut: `left -= at`, `at = 0`.
- `seen = max(natural * ratio - left - max(trimRight, 0), 0)` (visible seconds).
- `source_in = round(left / ratio * mrate)`, `source_out = min(source_in + round(seen / ratio * mrate), numsamples, media length)`. Empty clips are skipped (this drops clips lying entirely before zero).
- `start = round(at * sr)`.
- Stretched when `|ratio - 1| > 1e-6` or `|centShift| > 0.5`: `ClipStretch { ratio, keep_pitch: true, pitch: clamp(centShift / 100, -24, 24), polyphonic: true, rendered: false }`.
- Clip envelope: control points `(t seconds, val gain)`; `cut_line(points, origin = left, length = seen)` moves them by `-left`, cuts to the clip and adds end points, the same way as the `.sesx` clip line. If any point differs from 1 by more than 0.001, a volume envelope is added with `position = round(s * sr)` and `value = to_db(val)`, linear.
- `to_db(g)`: -96 for `g <= 0.00001`, else `20*log10(g)` rounded to 0.1.

Track: `volume_db = max(to_db(gain), -60)`, `pan = clamp(pan * 100, -100, 100)` (source pan -1 to 1), `muted`, `solo`.

### 6.5 Carry-over table

| Source | Audiform | Status |
| --- | --- | --- |
| project rate | `sample_rate` | Kept |
| track name, mute, solo | same | Kept |
| track gain (linear) | `volume_db`, floor -60 dB | Converted |
| track pan (-1 to 1) | `pan` x100 | Converted (number only) |
| stereo pair (two linked channel tracks) | one stereo track | Converted |
| track rate | WAV file rate (`source_in`/`out` in that rate) | Kept |
| clip offset, trims | `start`, `source_in`, `source_out` | Converted (seconds to samples) |
| clip name | `Clip.name` (empty uses the track name) | Kept |
| clip stretch ratio | `ClipStretch.ratio`, pitch kept | Converted |
| clip pitch (cents) | `ClipStretch.pitch` = cents / 100 semitones | Converted |
| clip volume line | clip volume envelope in dB, linear | Converted (**Unclear** whether the source interpolates in gain or dB between points; Audiform interpolates in dB) |
| sample blocks | WAV files beside the project, one per distinct recording | Converted |
| clips entirely before 0 | skipped | Dropped (silent) |
| clip heads before 0 | cut | Approx. |
| effects (realtime), label tracks, note tracks, time track, metadata tags, track colour, clip colour | not read | Dropped (silent; decisions.md says "Left behind: effects, labels") |

The import notice for `.aup3` says only tracks, clips and stretched clips; nothing about what was left behind.

### 6.6 Risks found while reading

**From reading the code** (none of these were reported as bugs):

1. Text width. The first two bytes of `dict` are skipped without being read, and all text is decoded as UTF-16. If those bytes give the character width (the layout suggests a header), a project whose text uses another width would not decode, and the import would fail with "the project data does not start as expected". Test with projects saved on Windows, macOS and Linux.
2. Only the first `sequence` of a `waveclip` is read. If a project stores both channels of a stereo clip inside one clip (one `wavetrack` with two sequences) instead of as two linked tracks, only the left channel comes in, as a mono track. Not checked against real projects from different versions.
3. Unknown tags stop the whole import. Any record type other than the ones in 6.2 fails it.
4. Source sharing is keyed by the first block id only. Two clips that start on the same block but diverge later (and neither list is a prefix of the other) share the first one's WAV, so the second clip would play the wrong audio after the point where they diverge.
5. `NoTracks` is effectively only raised when there are no `wavetrack` elements at all (`all(...) && layout.is_empty()`); a project with tracks but no clips imports as empty tracks, which is fine.
6. Performance: one `query_row` per block without a cached statement, one `write_sample` call per sample, everything on one thread, no progress and no cancel. A long project blocks the import until every WAV is written.
7. Re-importing overwrites the WAV files of an earlier import of the same project. Moving the project without its `<stem> audio` folder leaves the session with missing files (stated in decisions.md).
8. The 5.1 question regex (`/\.sesx$/i`) skips `.aup3` correctly, but `openSession` (Open Recent, Files panel) and the linked-session loader have no `.aup3` branch (section 9).

---

## 7. Bugs hit and fixes

| Symptom | Cause | Fix | Commit |
| --- | --- | --- | --- |
| Every imported `.sesx` clip came in at 0 dB, and some tracks lost their fader level | Clip faders were never read; the track fader was looked for in only one place | Fader found in audio parameters or the track node; `volume` x `static gain`; drawn lines become volume envelopes on clip, track, bus and master; clip mute read | [e45c3e0](https://github.com/nwh212/Audiform/commit/e45c3e0) |
| Drawn clip volume lines missing from newer files, or misplaced | Newer files wrap keyframes in `<parameterKeyframes>`; clip keyframes count from the file start, not the clip start; a split clip shares one line | `descendants()` to find keyframes; move by the raw `sourceInPoint`; cut to each piece with interpolated end points (`clip_line`) | [4379016](https://github.com/nwh212/Audiform/commit/4379016) |
| Imported Parametric EQ settings wrong | Assumed a fixed 31980 Hz span and +/-38 dB, guessed from a 1/16 dB grid | Gain is +/-48 dB; frequency runs 20 Hz to half the "Original Sample Rate" (share of 192 kHz); pass slopes mapped; checked against the other program's readout | [3c82ff6](https://github.com/nwh212/Audiform/commit/3c82ff6) (feature [5db21c3](https://github.com/nwh212/Audiform/commit/5db21c3)) |
| Imported phase shifter's turns barely showed on the Phase Wheel; 13-point curve lost shape | Both channels were turned alike (left and right stay in step); Phase Curve held 8 points; spline overshot past 180 degrees at a jump | Read the channel at float index 1; Phase Curve holds 16 points; imported curves use straight lines | [5ba8efd](https://github.com/nwh212/Audiform/commit/5ba8efd) |
| A clip that used one side of a stereo file played both sides | `channelMap` ignored | `channel_map` to Source Channels | [a2c55f3](https://github.com/nwh212/Audiform/commit/a2c55f3) |
| Imported 5.1 panner Center mapped to the room spot's size | Misread meaning: Center limits the center speaker | Reverted; Center left out by the lead's decision | [2b5101b](https://github.com/nwh212/Audiform/commit/2b5101b), [5dafcda](https://github.com/nwh212/Audiform/commit/5dafcda) |
| Opening sessions with dozens of files was slow | Files probed one after another | `rayon` parallel probing | [2a34486](https://github.com/nwh212/Audiform/commit/2a34486) |
| Stretched clips imported at the wrong source range | Source points are in stretched time | Divide by the ratio (export multiplies) | [c489eb3](https://github.com/nwh212/Audiform/commit/c489eb3) |
| A `.sesx` played about 18 dB louder than in the program that wrote it | Not the importer: the plug-in host turned on every plug-in input and fed silence to compressor sidechain inputs, so they never compressed | Only the main input on at load; sidechain inputs turned on when a key is routed (decisions.md 2026-11-01). See the VST3 guide. | in the plug-in host, not `sesx/` |
| Svelte `each_key_duplicate` while opening a large `.sesx` beside other sessions | Two causes found: Files pool `merge` could list an id twice after a relink; Help > About keyed plug-in hosts by name and size | Fresh id for the other row; About keyed by place | decisions.md 2026-11-01 |
| Plug-ins restarting between 44.1 and 48 kHz after a `.sesx` import (session at 44.1 kHz) | The stopped-playhead pictures rendered the mix at the session rate outside playback's pool | Borrow-only mode at playback's rate | decisions.md 2026-11-01 |
| Files open only in the Files panel came back when a session was reopened | `save` wrote every media entry | `save` keeps only media clips use; autosave keeps the whole Files list | [d9ecf86](https://github.com/nwh212/Audiform/commit/d9ecf86), reverted with 0.7.1 in [f4fa57e](https://github.com/nwh212/Audiform/commit/f4fa57e), reapplied as [7254032](https://github.com/nwh212/Audiform/commit/7254032) |
| A second running copy of the app offered a live window's autosave as a crash | Recovery looked only at files | `owner_pid` in the note plus a `running-<pid>.lock` touched each autosave, fresh for 150 s | `session/file.rs` (in the initial history) |
| Sessions saved as `.altaudio` after the rename | Policy | `with_extension` writes `.audfrm` beside an `.altaudio`, leaving it | decisions.md 2026-10-27 |

Use `git log --oneline -- src-tauri/src/sesx/` and `git log --oneline -- src-tauri/src/aup3/` for the full list. History before [fc2ee08](https://github.com/nwh212/Audiform/commit/fc2ee08) is squashed, so the first `.sesx` import (task 11, 2026-10-14) and the save, autosave and recovery work (task 12) have no separate commits.

---

## 8. Tests and fixtures

| Test | File | What it proves |
| --- | --- | --- |
| `session_fixtures_open` and the other `*_fixtures_open` | [tests/fixtures.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/fixtures.rs) | every frozen file in `tests/fixtures/v1/` (`session.altaudio`, `template.altemplate`, `library.json`, `default.alworkspace`, `host.alhost`, `keymap.alkeys`, `preferences.alprefs`) opens, validates, and the newest equals the current sample (`tests/common/mod.rs`). `WRITE_FIXTURES=1` writes a new version folder, never overwrites. |
| `format_ids_are_stable` | fixtures.rs | the seven `format` ids never change |
| `session_roundtrips`, `parts_roundtrip`, `new_session_roundtrips`, others | [tests/roundtrip.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/roundtrip.rs) | JSON round trip in memory and on disk; save twice replaces; no temp file left |
| `rejects_wrong_format`, `rejects_newer_version`, `rejects_version_zero`, `rejects_missing_header`, `rejects_invalid_json`, `template_checks_its_nested_session`, `missing_file_reports_path`, `every_document_writes_its_header` | roundtrip.rs | header rules and error messages |
| `published_schemas_are_current` | [tests/schemas.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/schemas.rs) | `/schemas` match the model |
| `save_and_open_follow_a_moved_show_folder`, `open_reports_missing_media`, `save_leaves_out_media_no_clip_uses`, `autosave_round_trip_and_clear` | [session/file.rs#L247-L391](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/file.rs#L247-L391) | relative paths, missing media, used-media filter, recovery with pid and lock |
| `sessions_save_as_audfrm_and_old_ones_move_over` | commands/session.rs tests | `with_extension` |
| validate tests | session/validate.rs | missing media, unknown bus, loops, duplicate ids |
| `space_settings_round_trip_and_stay_out_of_plain_files`, tempo tests | [session/model/tests.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model/tests.rs) | optional fields left out of plain files; old tempo reads as 4/4 |
| 17 `.sesx` unit tests | [sesx/mod.rs#L1573-L2137](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs#L1573-L2137) | tracks, clips, byte order mark, relative media, placeholder for a missing file, fades, lock, plug-ins and state conversion, class id layouts, buses and routing, stretch and source division, phase shifter (tester's and lead's real chunks), EQ against readout values, limiter presets, Vox Strip, one-side channel map, render switch, levels and volume lines, clip line cutting, gain to dB, rack presets, rack levels, 5.1 room spots and automation, non-session files rejected |
| 3 export tests | [sesx/export.rs#L490-L657](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/export.rs#L490-L657) | round trip through the importer; counts |
| `import_real_samples` (ignored) | [tests/sesx_samples.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/sesx_samples.rs) | imports every `.sesx` in `samples/` (git-ignored) or `$SESX_SAMPLES`, prints a summary, asserts `validate` is clean. Run with `cargo test --test sesx_samples -- --ignored --nocapture`. |
| `line_is_cut_to_the_clip`, `gain_in_decibels`, `real_project` (ignored, `AUP3_TEST`) | [aup3/mod.rs#L677-L708](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/aup3/mod.rs#L677-L708) | envelope cutting, dB conversion; a real project only on demand |
| `session-files.test.ts` | [src/app/session-files.test.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/session-files.test.ts) | the interface's import, open plan, save and recovery flow with a fake backend |

Test media in `src-tauri/tests/media/` (`mono_44k_24.wav` is used by the `.sesx` import test). Real `.sesx` sessions (two production sessions, the tester's and the lead's) were never committed; neither were `.aup3` projects. Audiform 2 should get permission to keep anonymised copies as fixtures.

---

## 9. Open issues spotted while reading

**From reading the code** unless noted:

1. `validate` is never called outside tests, so broken references are never shown (3.11).
2. The import notice says "phase shifters came across as Phase Curve" for every conversion, but `converted` also counts Parametric EQ, Limiter and Vox Strip (`sesxSummary`, [session-files.ts#L115](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/session-files.ts#L115)). Report each kind separately.
3. Export writes pan in -100 to 100 where real files use -1 to 1 (5.4).
4. Mixed sample rates: import and export disagree about source point units (5.4). **Unclear** which is right.
5. `.sesx` sends are not imported and not mentioned in the notice.
6. `.sesx` markers are not imported and not mentioned.
7. `openSession` (Open Recent, Files panel) sends an `.aup3` path to `files.open`, which would fail parsing it as JSON ([App.svelte#L1516](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/App.svelte#L1516)). Whether an imported `.aup3` can appear in Open Recent was not traced; `importSesx` does not call `addRecent`, so probably not, but a Files panel row for it may.
8. Dropping an `.aup3` on the Multitrack lanes (or Insert Session as Clip) calls the linked loader, which has no `.aup3` branch.
9. `linked.path` is absolute only (3.10).
10. Autosave keeps stale `relative_path` values (3.8). Templates keep stale relative paths and skip media resolution (3.9).
11. `.sesx` export writes every in-memory media entry, not only those clips use.
12. `.sesx` export does not fsync before rename (own saves do).
13. The `sesx` module comment says automation is left out (stale).
14. `docs/backlog.md` says `.aup3` is parked (stale).
15. `docs/review-notes.md` line 27 mentions an "extractor from other editors' session files" that is "skipped with the usual notice". **Unclear** which effect this is; it reads like an Altered Audio Editor review note about a built-in effect the importer does not convert.
16. The `.aup3` risks in 6.6.
17. Parametric EQ values are not clamped to Audiform's ranges (4.9).

---

## 10. Recommended design for Audiform 2

### 10.1 Keep reading Audiform 1's files, forever

- Read `.audfrm` and `.altaudio` with the same rules: `format == "altaudio.session"`, `schema_version` 1. Treat the Audiform 1 model as a frozen "v1" schema. Copy the frozen fixtures (`tests/fixtures/v1/*`) into Audiform 2 unchanged and require every build to open them. Add more real sessions (with permission) covering MIDI, 3D Space, linked clips, stretch and every optional field.
- Decide early whether Audiform 2 writes the same document (v1 plus optional fields, so Audiform 1 can still open it) or a v2. Recommendation: keep writing v1 with optional additions as long as the meaning of existing fields does not change. That keeps files moving between Audiform 1, Altered Audio Editor and Audiform 2. When a real break is needed, bump to 2 with a `migrate_step(1 -> 2)` on `serde_json::Value`, a `fixtures/v2/` folder, and a clear "Update Audiform" refusal in older builds (Audiform 1 already refuses version 2 with that message).
- Keep the loader shape: parse to `Value`, check header, migrate step by step, then deserialize. Keep atomic saves with fsync. Keep "unknown fields are ignored" so newer optional fields do not break older readers. Consider keeping unknown fields in a `#[serde(flatten)] extra: Map` on the top-level types so a round trip through Audiform 2 does not drop fields added by a newer build or by Altered Audio Editor.
- Keep integer samples for positions, file-rate samples for source offsets, dB for levels, -100 to 100 pan, and the effect ids (`altaudio.*`, `vst3:<hex>`), plus each built-in effect's parameter keys. Changing any of these is a migration.
- Call validation on open and import and show the issues.

### 10.2 A format-neutral import layer

Audiform 1 converts straight from XML or SQLite into its `Session`, which works but buries format quirks in one long function. For Audiform 2:

```text
reader (per format)  ->  ForeignProject (neutral, lossless enough)  ->  mapper  ->  Session + ImportReport
```

- **Readers** only parse: `sesx::read(&str) -> SesxDoc` (typed structs for files, tracks, buses, master, clips, components, keyframes, racks); `aup3::read(&Path) -> Aup3Doc` (tree plus a block reader). No Audiform types inside them. Each reader is unit-tested on fixture files.
- **ForeignProject** is a small neutral model: rate, channel layout, files (paths, noted channels), tracks and buses (name, layout, level as linear gain, pan as -1 to 1 with the source's pan law named, mute, solo, output, sends, height, colour), clips (start, source range in a stated unit and rate, gain, fades with shape names, stretch, channel map, envelopes in source units), effects (source id, name, raw parameters, raw state), markers, and a list of "seen but not mapped" items.
- **Mapper** turns it into a `Session`, applying unit conversions in one place, and records every decision in a structured `ImportReport`: per-item entries `{ kind: Kept | Converted | Approximated | Dropped, what, where, why }`. The notice is built from that list, so nothing is dropped silently and the "phase shifters" wording bug cannot recur.
- **Effect converters** as a registry keyed by source component id: `fn convert(component) -> Option<EffectSlot>`, each with its own tests copied from Audiform 1 (EQ readout values, limiter presets, Vox Strip knobs, phase shifter chunks). Unknown ids fall through to VST3 if they start with `3:`, else "dropped".
- **Media resolution** shared by all formats: relative to the project, absolute, file name beside the project, then (new) a search of folders the user has linked before. Probe in parallel. Placeholders for missing files, never failure.
- **Export** goes the other way through the same neutral model, so the exporter and importer share unit conversions and cannot disagree (Audiform 1's pan and sample-rate mismatches came from two hand-written conversions).

### 10.3 Where to start

1. Port `session::document` and the `Session` model with serde exactly as in Audiform 1, and pass Audiform 1's fixture and round-trip tests. This is the base everything else needs.
2. Port `.sesx` import next: it has the most users and the most tests. Port the 17 unit tests first, then build the reader, neutral model and mapper until they pass.
3. Settle the two open `.sesx` questions with a real file before writing export: pan scale, and source point units for mixed rates.
4. Then `.sesx` export through the neutral model, and get one export opened in the other program.
5. `.aup3` last, after getting real projects from Windows, macOS and Linux, and from more than one version of the program, into the fixture set.

---

## 11. Ordered checklist with tests

1. **Model and loader.** Port `Session` and friends with identical serde attributes. Test: Audiform 1's `tests/fixtures/v1/session.altaudio` and `template.altemplate` load and equal the sample; `roundtrip.rs` cases pass; a file with only the required fields loads.
2. **Header rules.** Port `read_header`, `upgrade`, `TooNew` message. Tests: wrong format, version 0, version 99, missing header, invalid JSON, nested template version.
3. **Atomic save.** Temp file, fsync, rename; save twice on Windows. Test: no leftover temp file.
4. **Paths.** Relative and absolute on save; relative first on open; `normalize`; missing ids returned. Tests: moved show folder; missing file; different drive gives no relative path; `/` separators on Windows.
5. **Used-media filter and backups.** Tests: unused media dropped; backups keep the newest N with the timestamp pattern.
6. **Autosave and recovery.** Note with `owner_pid`, lock freshness 150 s, `.altaudio` fallback, clear on save. Clear `relative_path` in autosaves. Test: Audiform 1's `autosave_round_trip_and_clear`, plus a recovered session whose relative path would be wrong.
7. **Validation on open.** Show issues. Test: Audiform 1's validate tests, plus an open that reports.
8. **Schemas.** Generate and check in. Test: schema drift test.
9. **`.sesx` reader.** BOM, DTD, UTF-8 (and decide on UTF-16), both component locations, both keyframe layouts. Tests: Audiform 1's `SAMPLE`, `LEVELS` and render-switch XML strings; non-session files rejected.
10. **`.sesx` mapper: tracks, buses, master, clips, files.** Tests: `imports_tracks_clips_and_finds_media_by_relative_path`, `brings_in_plug_ins_buses_and_stretched_clips`, `imports_clip_and_track_levels_and_drawn_volume_lines`, `a_clip_line_counts_from_the_file_and_is_cut_to_the_clip`, `a_tracks_render_switch_comes_across_as_pre_render`, `a_clip_using_one_side_of_a_stereo_file_keeps_to_that_side`.
11. **Effect converters.** Tests: EQ readout values (59 Hz +4.42 dB, 134 Hz +6.32 dB Q 1, ...), limiter 0.94 to -6 dB and 0.97 to -3 dB, Vox Strip knobs, phase shifter chunks (6 points right channel; 13 points left channel with the +/-180 jumps), rack levels (0.4217 to -7.5 dB, mix 0.25 to 0.75), rack presets. Add clamping to Audiform 2's ranges and a test for it.
12. **VST3 ids and state.** Tests: both class id layouts; `u64,u64` to `u32` state and back; a bad length drops only the state.
13. **5.1 to 3D Space.** Tests: `five_one_panners_become_room_spots`, `five_one_panner_lines_become_room_automation`, both surround choices.
14. **Media search and placeholders, parallel probing.** Tests: relative, absolute, name beside; placeholder length covers all clips; noted channels used.
15. **Structured import report and notice.** Every dropped item listed (sends, markers, other automation, colours). Test: a fixture with each kind produces each entry.
16. **Resolve the two `.sesx` questions** with real files (pan scale; source units at mixed rates). Add fixtures.
17. **`.sesx` export** through the neutral model: pan as -1 to 1, source points in the settled unit, write rack levels and `preRendered`, only used media, fsync. Tests: Audiform 1's three export tests; a mixed-rate round trip keeps clip length; open one export in the other program and record the result.
18. **`.aup3` reader.** Read the dict header instead of skipping it; handle every tag kind seen in real projects; handle multi-sequence clips. Tests: fixtures from three platforms and two program versions; `line_is_cut_to_the_clip`, `gain_in_decibels`.
19. **`.aup3` audio extraction.** Prepared statements, chunked writes, progress and cancel, source sharing keyed by the whole block list (or by each clip's own range). Test: two clips sharing a first block but diverging later play their own audio.
20. **Open flows.** One extension table used by File > Open, drop, Open Recent, the Files panel and Insert Session as Clip, so `.aup3` works everywhere `.sesx` does. Test: each entry point with each extension.
21. **Linked sessions.** Store a relative path beside the absolute one. Test: moved show folder keeps the link.

---

## 12. File map

| Audiform 1 file | Lines | What it holds | Audiform 2 home (suggested) |
| --- | --- | --- | --- |
| [src-tauri/src/session/model.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model.rs) | 1464 | the session model, units, defaults, `timeline_length` | `model` crate, frozen v1 types |
| [src-tauri/src/session/model/tests.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/model/tests.rs) | 95 | model tests | same |
| [src-tauri/src/session/document.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/document.rs) | 219 | versioned documents, header, migrations, atomic save, schema generation | `formats::document` |
| [src-tauri/src/session/file.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/file.rs) | 391 | extensions, save (used media, relative paths), open (path resolution), autosave, recovery | `formats::session_file` |
| [src-tauri/src/session/validate.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/validate.rs) | 299 | meaning checks | `model::validate` (and call it) |
| [src-tauri/src/session/collect.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/collect.rs) | 192 | Collect Media | `formats::collect` |
| [src-tauri/src/session/ops/media.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/session/ops/media.rs) | | `set_media` (Link Media) | edit ops |
| [src-tauri/src/library/template.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/library/template.rs) | | `Template` document, nested upgrade | `formats::template` |
| [src-tauri/src/commands/template.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/template.rs) | | templates folder, save, new from template | app layer |
| [src-tauri/src/commands/session.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/session.rs) | | new, open, save, backups, `with_extension`, autosave, recover, file dialogs, import and export commands | app layer |
| [src-tauri/src/commands/linked.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/linked.rs) | | linked sessions: render, stamp, check, loader | app layer |
| [src-tauri/src/commands/effects.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/commands/effects.rs) (L352 to L420) | | rack preset import command | app layer |
| [src-tauri/src/sesx/mod.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/mod.rs) | 2137 | `.sesx` import, effect conversions, 5.1 panners, rack presets, media search, tests | `formats::sesx::{read, map}`, `formats::convert::*` |
| [src-tauri/src/sesx/export.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/sesx/export.rs) | 657 | `.sesx` export and report | `formats::sesx::write` |
| [src-tauri/src/aup3/mod.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/aup3/mod.rs) | 708 | `.aup3` import: tree parser, block reader, WAV writer, mapping | `formats::aup3::{read, map}` |
| [src-tauri/src/dsp/effects/eq.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/effects/eq.rs), [dynamics.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/effects/dynamics.rs), [vocal.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/effects/vocal.rs), [phase_curve.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/dsp/effects/phase_curve.rs) | | target effects' parameter keys and ranges (`altaudio.parametric_eq`, `altaudio.limiter`, `altaudio.vox_strip`, `altaudio.phase_curve`) | effect crates |
| [src-tauri/src/engine/plan.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/plan.rs) (L360 to L384), [engine/graph/levels.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/src/engine/graph/levels.rs) (L135 to L143) | | fade curves, pan law (what imported values mean when played) | engine |
| [schemas/*.schema.json](https://github.com/nwh212/Audiform/tree/042577e1ddee7b9718d82aea7947f9aab6edd628/schemas) | | published schemas | same |
| [src-tauri/tests/fixtures/v1/](https://github.com/nwh212/Audiform/tree/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/fixtures/v1) | | frozen v1 files | copy unchanged |
| [src-tauri/tests/fixtures.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/fixtures.rs), [roundtrip.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/roundtrip.rs), [schemas.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/schemas.rs), [sesx_samples.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/sesx_samples.rs), [common/mod.rs](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src-tauri/tests/common/mod.rs) | | format tests | same |
| [src/app/session-files.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/session-files.ts) | | open plan, import, notice text, save, autosave timer, recovery | app layer |
| [src/app/filedrop.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/filedrop.ts) | | `isSessionFile`, drop routing | app layer |
| [src/app/App.svelte](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/App.svelte) | | 5.1 question, `openPaths`, `openSession`, `beforeSave`, autosave interval | app layer |
| [src/app/file-tools.svelte.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/app/file-tools.svelte.ts) | | Link Media, Collect Media, Save Space actions | app layer |
| [src/stores/session.ts](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/src/stores/session.ts) | | `markImported`, `home` | app layer |
| [docs/decisions.md](https://github.com/nwh212/Audiform/blob/042577e1ddee7b9718d82aea7947f9aab6edd628/docs/decisions.md) | 2740 | entries worth reading: "File format ids and extensions", "Versioning rules", "Schemas generated from Rust", "Units in the session model", "Atomic saves", "Session validation reports, it does not refuse" (2026-10-11); ".sesx session import (task 11)" (2026-10-14); "0.1.5 ... .sesx export" (2026-10-19); ".sesx plug-ins, buses and stretch" (2026-10-24); "Phase Curve" (2026-10-25); "Sessions save as .audfrm" (2026-10-27); "`.aup3` project import" (2026-10-29); "5.1 sessions from .sesx", "Effects rack levels from .sesx", "Phase Curve from a .sesx" (2026-11-01); "the .sesx limiter", "Link Media" (2026-11-06) | |
