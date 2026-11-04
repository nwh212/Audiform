# Add-ons

An add-on gives Audiform a new window. It is a folder you put in the Add-ons folder by hand; Audiform ships none, downloads none and lists none from anywhere else. Each add-on brings its own page, shown in a window that docks, tabs and floats like any other, and, when it does work, its own tool program, which Audiform runs on request and whose output it relays to the page.

The design keeps the app small and safe: the page cannot reach the app, the network or the computer. It only talks to Audiform through the messages below, and the tool only runs when the page asks.

## Installing one

1. Window > Add-ons > Open Add-ons Folder. The folder is `addons` inside Audiform's settings folder.
2. Put the add-on's folder inside it. The folder's name is the add-on's id.
3. Window > Add-ons > Reload Add-ons (or start Audiform again). The add-on's window is listed in Window > Add-ons.

Only install add-ons from people you trust: a tool program can do anything a program you run can do.

## What is in an add-on folder

```
voice-tool/
  addon.json       the manifest
  panel.html       the page shown in the window
  run.py           the tool, if any (any program; this is an example)
  ...              pictures, styles, scripts the page uses
```

### addon.json

```json
{
  "id": "voice-tool",
  "name": "Voice Tool",
  "version": "1.0.0",
  "title": "Voice Tool",
  "entry": "panel.html",
  "command": ["python", "run.py"],
  "description": "What it does, in a line."
}
```

- `id`: lower-case letters, digits and dashes, up to 40, and the same as the folder's name. Add-ons whose id and folder differ are not listed.
- `name`: shown in notices. `title`: the window's tab title; the name when left out.
- `entry`: the page, relative to the folder. `panel.html` when left out.
- `command`: the tool program and its fixed arguments. A bare name (`python`) is found on the system path; a path (`bin/tool.exe`, `./run.sh`) is taken relative to the folder. Leave it out for a window with no tool.
- `version` and `description` are for people.

### The page

Any HTML page. It is served on Audiform's `addon` scheme from the add-on's folder, so relative addresses (`style.css`, `pictures/logo.png`, `app.js`) work. The page's policy allows scripts and styles from the folder or inline, pictures and sounds from the folder or data addresses, and nothing else: no network, no forms, no frames. The page has no access to Audiform's own code or to the computer.

The page talks to Audiform with `window.parent.postMessage(message, "*")` and hears back on `window.addEventListener("message", ...)`. Every message is a plain object with a `type`.

## Messages the page sends

| Message | What happens |
| --- | --- |
| `{ type: "ready" }` | Send this once the page has loaded. Audiform answers with `hello`. |
| `{ type: "run", run, args, input }` | Runs the tool with `args` (strings) after the manifest's fixed arguments and `input` (a string, usually JSON) on its standard input. `run` is any id the page picks; it comes back on every message about that run. One run at a time. |
| `{ type: "cancel" }` | Stops the running tool. |
| `{ type: "import", request, paths }` | Brings result files into the Files panel (and the Waveform editor's list). `request` is any id; it comes back in `imported`. |
| `{ type: "pick", request, folder, title }` | Opens a file dialog (`folder: true` for a folder) with `title`; answers with `picked`. |
| `{ type: "openFolder", path }` | Opens a folder inside the add-on's own folder (`"models"`, or `""` for the add-on folder itself) in the system file manager, making it first. Nothing outside the add-on can be opened. |
| `{ type: "notice", text }` | Shows `text` in Audiform's notice line. |
| `{ type: "save", data }` | Keeps `data` (any JSON) in Audiform's settings for this add-on; it comes back in `hello` next time. |

## Messages Audiform sends

| Message | When |
| --- | --- |
| `{ type: "hello", addon, selection, theme, data, running }` | After `ready`. `addon` is `{ id, name, version, hasTool }`; `data` is what the page last saved, or null; `running` is the id of a run still going, or null. |
| `{ type: "selection", selection }` | Whenever what is selected changes. |
| `{ type: "theme", theme }` | When the theme changes. |
| `{ type: "output", run, line, err }` | One line the tool printed; `err` is true for its error channel. |
| `{ type: "done", run, code, lines }` | The tool ended with exit `code` (null when stopped by a signal); `lines` are its last two hundred output lines. |
| `{ type: "failed", run, error }` | The tool could not start, was stopped, or another run was still going. |
| `{ type: "imported", request, ok, error? }` | The answer to `import`. |
| `{ type: "picked", request, path }` | The answer to `pick`; `path` is null when the dialog was cancelled. |

`selection` is `{ kind, label, files }`: `kind` is `"files"` (the file open in the Waveform editor, or the files selected in the Files panel), `"clips"` (the selected clips in the session; each source file once) or `"none"`; `label` is a short description such as "3 clips on 2 tracks"; `files` is a list of `{ path, name }`.

`theme` holds the theme's colors and type as strings, by token name: `surface-0` to `surface-3`, `border`, `border-strong`, `text`, `text-dim`, `text-faint`, `accent`, `value`, `danger`, `font-ui`, `font-mono`, `font-size`. A page that sets these as its own CSS variables matches the app.

## The tool

Any program. It starts with the add-on folder as its working folder, gets the manifest's fixed arguments and then the page's `args`, reads the page's `input` on standard input, and prints lines. Every line goes to the page as it is printed (buffer-free output, such as Python's `print(..., flush=True)`, keeps progress live). A good tool prints one JSON object per line so the page can parse progress and results; the convention is yours. Result files should be written next to their source or in a folder the user picked, then named in a line the page turns into an `import` request. The tool is killed when the page sends `cancel` or the window closes while it runs.

## A small example

`panel.html`:

```html
<!doctype html>
<meta charset="utf-8">
<button id="go">Count the selected files</button>
<pre id="out"></pre>
<script>
  let selection = { files: [] };
  const out = document.getElementById("out");
  window.addEventListener("message", (e) => {
    const m = e.data;
    if (m.type === "hello" || m.type === "selection") selection = m.selection;
    if (m.type === "output") out.textContent += m.line + "\n";
    if (m.type === "done") out.textContent += "exit " + m.code + "\n";
  });
  document.getElementById("go").onclick = () =>
    window.parent.postMessage({ type: "run", run: "r1", args: [], input: JSON.stringify(selection.files.map((f) => f.path)) }, "*");
  window.parent.postMessage({ type: "ready" }, "*");
</script>
```

`addon.json`: `{ "id": "count", "name": "Count", "command": ["python", "-I", "count.py"] }`

`count.py`:

```python
import json, sys
paths = json.load(sys.stdin)
for p in paths:
    print(f"file: {p}", flush=True)
print(f"{len(paths)} files", flush=True)
```

## Limits

- Up to 32 add-ons are listed, twelve of them in the Window menu.
- A page or picture over 32 MB is not served; a manifest over 64 KB is ignored.
- One tool run at a time across all add-ons.
- The last 200 output lines are kept for `done`; everything is streamed as `output` while it runs.
