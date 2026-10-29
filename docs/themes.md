# Themes

A theme is one file with the extension `.audtheme`. It holds a style sheet and the pictures the style sheet uses, so a theme is a single file you can copy, share and drop in a folder. Audiform ships nine of them (Meadow Aero, Aqua Aero, Moonlight Aero, Prism, Phosphor, Analog, Grey Desktop '95 and the two Mall Pastel '95 themes). You can add your own.

## Install a theme

1. Open Edit > Settings > Appearance.
2. Choose Open Themes Folder. Your file manager opens the Themes folder, which lives in the Audiform config folder. It is made if it is not there yet.
3. Copy the `.audtheme` file into that folder.
4. Back in Settings, choose Look Again. The theme shows up under Your themes. Pick it and choose Apply.

A file that cannot be used is listed in the same place with the reason, and the rest keep working. Themes never touch the network. Remove a theme by deleting its file; if it was in use, Audiform goes back to the Dark look.

## The file

The file is JSON. `schemas/theme.schema.json` describes it exactly. A small example is in `docs/examples/sunset.audtheme`.

| Field | Needed | What it is |
| --- | --- | --- |
| `format` | yes | Always `"audiform-theme"`. |
| `version` | yes | The format version. Today it is `1`. A file with a higher number is refused with a note to update Audiform. |
| `id` | yes | A short unique id: lowercase letters, digits and single dashes, like `sunset`. The settings remember a theme by this id. `dark` is taken by the built-in look. If two files use the same id, the first one wins and the other is reported. |
| `name` | yes | The name in the list. |
| `base` | yes | `"dark"` or `"light"`: which built-in colors the theme starts from (see tokens below). A light theme also starts with light form controls and scroll bars. |
| `description` | no | One or two sentences shown under the list. |
| `author` | no | Who made it. |
| `secret` | no | `true` hides the theme from the list until its id is in the `unlockedThemes` setting. A secret theme cannot be used before that either. |
| `css` | yes | The style sheet, as one string. |
| `assets` | no | The pictures and fonts, by name (see below). |
| `pointers` | no | Mouse pointer pictures (see below). |
| `canvasLook` | no | Flags for views that draw themselves (see below). |

### Tokens and the style sheet

The built-in look is made of CSS variables (colors, fonts, sizes) defined in `src/styles/tokens.css`. A theme changes them, and adds any other rules it likes. Write every rule against your own id, so it only applies when your theme is on:

```css
:root[data-theme="sunset"] {
  --surface-1: #2a1a3a;
  --accent: #ff8a4a;
}
:root[data-theme="sunset"] .menubar {
  border-bottom: 1px solid var(--accent);
}
```

When the theme is on, Audiform puts the style sheet in a single `<style id="audiform-theme">` and sets `data-theme` on the page. Your colors in Settings > Appearance still apply on top of the theme.

For safety the style sheet may not load anything from outside the file: `@import`, `image-set()` and any address in `url()` other than `url(asset:name)` or `url(#name)` (a reference inside the page) are refused. The style sheet may be up to 512 KB.

### Assets

`assets` maps a name to a base64 `data:` address of a picture (PNG, JPEG, WebP, GIF, AVIF or SVG) or a font (WOFF2, WOFF, TTF or OTF). Names use letters, digits, dots, dashes and underscores. In the style sheet, refer to one as `url(asset:name)`:

```css
:root[data-theme="sunset"] body {
  background: #1d1228 url(asset:glow) 0 0 / 64px 64px;
}
```

Every name the style sheet or a pointer uses must be in `assets`, or the file is refused. There are at most 200 assets of up to 8 MB each, and the whole file may be up to 16 MB. Large photos are fine but make the file large, so resize them (a window background rarely needs more than 1920 pixels wide, in WebP).

### Pointers

`pointers` replaces the standard mouse pointers while the theme is on. The kinds are `text`, `ew`, `ns`, `nwse`, `nesw`, `move`, `grab`, `grabbing` and `busy`. Each one names an asset and the hot spot, the point in the picture (in pixels from its top left) that does the pointing:

```json
"pointers": {
  "text": { "asset": "cursor-text", "hot": [8, 8] },
  "ew":   { "asset": "cursor-ew",   "hot": [8, 8] }
}
```

Pointers are drawn at 16 by 16 pixels, with the hot spot in the middle except for an arrow's tip. A kind that is left out keeps the standard pointer. (Pointers a style sheet sets itself with `cursor: url(asset:name) x y, default` work too.)

### Canvas look

Some views draw themselves and cannot be styled with CSS. `canvasLook` holds true or false flags for them, so no code has to know a theme by its name:

| Flag | Effect |
| --- | --- |
| `pixel` | Views that can draw blocky (the Phase Wheel) do. |
| `markCursor` | Anything whose own style asks for the hand or the plain arrow gets a `data-aero-cursor` mark, so the style sheet can give it the theme's arrow. |
| `glass`, `marble` | Reserved for views that choose a backing. Nothing reads them yet. |

Unknown flags are ignored.

## Secret themes

A theme with `"secret": true` is left out of the list, and cannot be chosen, until its id is in the `unlockedThemes` list in the app settings. Audiform may ship some secret themes as surprises. This is a courtesy, not a lock: the file is readable by anyone.

## Where this lives in the code

- `src/themes/*.audtheme`: the built-in themes. They were made from the old style sheets by `scripts/pack-themes.mjs`, which reads them from git history and is kept as a record.
- `src/themes/validate.ts`: the check every file passes. `src/themes/loader.ts`: lists, loads and applies themes.
- `src/app/theme-cursor.ts`: draws the active theme's pointers.
- `src-tauri/src/commands/themes.rs`: lists and reads the files in the Themes folder (size limited, no network).
