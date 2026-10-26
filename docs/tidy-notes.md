# Tidy-up pass (branch `tidy/cleanup`)

A clean-up pass, kept on its own branch so nothing here reaches `main` until the project lead merges it. Every check passes on the branch (type check, frontend tests, `cargo fmt`, `cargo clippy -D warnings`, all Rust tests). Nothing here changes how the app sounds or looks.

## What changed

1. **Dead code removed**
   - Frontend wrappers nothing called: `loadWorkspaceLayout`, `saveWorkspaceLayout`, `clearWorkspaceLayout`, `waveGet`.
   - Their core commands: `workspace_load`, `workspace_save`, `workspace_clear`, `wave_get`. The single-layout file is still read once to move old layouts into named workspaces; that code stays.
   - Unused constants: `TARGETS` (loudness), `AAC_RATES`, `DEFAULT_BATCH` (a stale copy of the defaults `sanitizeBatch` builds), `rotationToward` (3D Space).
   - The four core commands came from Altered Audio Editor. If staying easy to merge with it matters more than the tidy, drop that part of the first commit.
2. **One frame engine for spectral effects**
   - The overlap-add machinery (window, transform, the per-frame step, transform back, overlap-add) existed three times: the shared noise-reduction helper, Phase Curve and the Center Channel Extractor. It is now one `Frames` type in `dsp/stft.rs`, which each effect gives a per-frame step. Their tests pass unchanged.
3. **Loose ends**
   - The response-graph list no longer names the Parametric EQ and Notch Filter, which have their own graph windows now.
   - The EQ and Phase Curve windows kept their view choices in browser storage. They now keep them in the app's preferences, like every other setting (CLAUDE.md: settings live in the app config folder). Those two view choices start fresh once after this change.

## Looked at and left alone

- **Exports only used by their own file or by tests** (about 180, mostly types). Harmless, and changing them is churn.
- **`App.svelte` is very large** (over 4,000 lines). Splitting it into modules (actions per menu, dialogs) would help but touches almost everything; worth doing deliberately, not as a side pass.
- **The plain response graph** (Hum Cut, Precision Filter, Curve EQ, Band EQ) still uses its own small ruler rather than the shared frequency ruler the newer windows use. That is a visual change, so it is left for a design pass.
- **Retired effects** (Phase Rotate, Center Remover) stay in the core so old sessions keep playing.
