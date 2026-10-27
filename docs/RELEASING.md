# Releasing

Every update that goes out gets its own version, so a bug report can name the build.

1. Bump the version. One command changes all five files that carry it (`package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`):

   ```
   npm run bump              # 0.1.1 -> 0.1.2, for each release
   npm run bump -- minor     # 0.1.9 -> 0.2.0, when a phase lands
   ```

   Stay on 0.x until the editor is stable for daily podcast work; 1.0.0 marks that.
2. Commit and push, then make sure CI is green on `main`.
3. Tag the same version and push the tag:

   ```
   git tag v0.1.2
   git push origin v0.1.2
   ```

4. The Release workflow builds the installer and a portable zip, then makes a **draft** release.
5. Install the installer on a clean Windows machine and run it. Release immutability means a published release can't be changed, so test first.
6. Edit the notes, then publish the draft. It becomes the Latest release (0.x versions too). Don't make a release by hand on GitHub: publishing one creates the tag, and the workflow's draft step then fails because the release already exists.

Local build: `npm run tauri build`. The installer lands in `src-tauri/target/release/bundle/nsis/`.

Files are unsigned for now, so Windows SmartScreen shows a warning. `SHA256SUMS.txt` is attached to each release so people can check their download.
