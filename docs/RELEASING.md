# Releasing

Every update that goes out gets its own version, so a bug report can name the build.

1. Bump the version. One command changes all five files that carry it (`package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`):

   ```
   npm run bump              # 0.1.1 -> 0.1.2, for each release
   npm run bump -- minor     # 0.1.9 -> 0.2.0, when a phase lands
   ```

   Stay on 0.x until the editor is stable for daily podcast work; 1.0.0 marks that.
2. For a major update (a minor or major version bump), check whether a better **public** separation model has come out than the ones Audio Separation offers, per type (vocals and instrumental for now). Compare published SDR on the same test set (the Multisong set on MVSep and in ZFTurbo's pretrained list), keep only models whose weights anyone can download from their author, and note what the author says about their use. To add or replace one, update `catalog()` in `src-tauri/src/separation/mod.rs` with its download, size, SHA-256 and SDR, and record the change in `docs/decisions.md`.
3. Commit and push, then make sure CI is green on `main`.
4. Tag the same version and push the tag:

   ```
   git tag v0.1.2
   git push origin v0.1.2
   ```

   Or, without the command line: on GitHub, open **Actions > Release > Run workflow**, type the version (like `v0.1.2`) and click **Run workflow**. The tag is made when you publish the draft.

5. The Release workflow builds the installer and a portable zip, then makes a **draft** release. Attach the separation engine to the draft too: `audiform-separate-windows-x64.zip` and `audiform-separate-linux-x64.zip`, each with its `.sha256` file, from the engine's own private build. Audio Separation downloads them from the release matching its version.
6. Install the installer on a clean Windows machine and run it. Release immutability means a published release can't be changed, so test first.
7. Edit the notes, then publish the draft. It becomes the Latest release (0.x versions too). Don't make a release by hand on GitHub: publishing one creates the tag, and the workflow's draft step then fails because the release already exists.

Local build: `npm run tauri build`. The installer lands in `src-tauri/target/release/bundle/nsis/`.

Files are unsigned for now, so Windows SmartScreen shows a warning. `SHA256SUMS.txt` is attached to each release so people can check their download.
