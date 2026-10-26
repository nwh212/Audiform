# Windows setup

One-time setup to build and run Audiform on Windows 10 or 11. Takes about 20 to 30 minutes, mostly downloads.

Open **PowerShell** (Start menu, type "PowerShell"). Run each block, wait for it to finish, then move on.

## 1. Microsoft C++ build tools

Rust on Windows uses Microsoft's compiler and linker.

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

A Visual Studio installer window opens and closes on its own. This is the largest download.

## 2. WebView2

Windows 11 and up-to-date Windows 10 already have it. Nothing to do unless the app later says WebView2 is missing. If it does:

```powershell
winget install --id Microsoft.EdgeWebView2Runtime
```

## 3. Rust

```powershell
winget install --id Rustlang.Rustup
```

**Close PowerShell and open a new one** so it picks up the new commands. Then:

```powershell
rustup default stable-msvc
rustc --version
```

You should see a version number like `rustc 1.9x.x`.

## 4. Node.js

```powershell
winget install --id OpenJS.NodeJS.LTS
```

Close and reopen PowerShell again, then check:

```powershell
node --version
```

Any version 22 or newer is fine.

## 5. Git

Skip if `git --version` already works.

```powershell
winget install --id Git.Git
```

## 6. First run

```powershell
cd $HOME\audiform
npm install
npm run tauri dev
```

The first `tauri dev` compiles every Rust dependency. Expect 3 to 8 minutes. Later runs take seconds. A dark window titled "Audiform" opens, and the status bar at the bottom shows the core version and "Session format v1". That means the frontend and the Rust core are talking.

Close the window or press Ctrl+C in PowerShell to stop.

## 7. Run the checks

Same checks CI runs:

```powershell
npm run check
npm test
npm run build
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cd ..
```

Every line should end without errors.

## 8. Put it under Git

```powershell
git init -b main
git add .
git commit -m "chore: P1 scaffold and data model"
```

To push to GitHub, create an empty repo named `audiform` on github.com (no README, no license, both are already here), then:

```powershell
git remote add origin https://github.com/<your-account>/audiform.git
git push -u origin main
```

CI runs on every push after that. Check the Actions tab.

## Editor (optional)

VS Code with the recommended extensions (it offers them when you open the folder): Svelte, Tauri, and rust-analyzer. rust-analyzer shows Rust errors as you type.

## Troubleshooting

- **`link.exe not found`**: step 1 did not finish. Run it again, or open "Visual Studio Installer" and add "Desktop development with C++".
- **`cargo` or `node` not recognized**: close and reopen PowerShell.
- **Port 1420 in use**: another `tauri dev` is still running. Close it.
- **Schema test fails after changing the model**: that is expected. Run `$env:UPDATE_SCHEMAS=1; cargo test --test schemas` from `src-tauri`, then `Remove-Item Env:UPDATE_SCHEMAS`, and commit the changed files in `/schemas`.
