// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

//! Opening a `.vst3` bundle and reading its factory.
//!
//! A bundle is a folder (`Name.vst3/Contents/<arch>-<os>/Name.<ext>`); on
//! Windows an older plug-in may also be a single `Name.vst3` file.

#[cfg(all(unix, not(target_os = "macos")))]
use std::ffi::c_void;
use std::path::{Path, PathBuf};

use libloading::Library;
use vst3::ComPtr;
use vst3::Steinberg::{
    IPluginFactory, IPluginFactory2, IPluginFactory2Trait, IPluginFactoryTrait, PClassInfo,
    PClassInfo2,
};

use crate::com::from_cstring;

/// The category VST3 effects and instruments are listed under.
pub const AUDIO_CLASS: &str = "Audio Module Class";

pub struct Module {
    factory: Option<ComPtr<IPluginFactory>>,
    lib: Option<Library>,
}

/// One class in a factory.
#[derive(Debug, Clone)]
pub struct FactoryClass {
    pub cid: [u8; 16],
    pub category: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub sub_categories: String,
}

/// The shared library inside a bundle, for this platform.
pub fn library_path(bundle: &Path) -> Result<PathBuf, String> {
    if bundle.is_file() {
        return Ok(bundle.to_path_buf());
    }
    let stem = bundle
        .file_stem()
        .ok_or("The bundle has no name.")?
        .to_string_lossy()
        .into_owned();
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    let contents = bundle.join("Contents");
    let p = if cfg!(windows) {
        contents
            .join(format!("{arch}-win"))
            .join(format!("{stem}.vst3"))
    } else if cfg!(target_os = "macos") {
        return Err("Plug-ins on macOS are not supported yet.".into());
    } else {
        contents
            .join(format!("{arch}-linux"))
            .join(format!("{stem}.so"))
    };
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!(
            "No {arch} build for this system inside the bundle (looked for {}).",
            p.display()
        ))
    }
}

impl Module {
    pub fn open(bundle: &Path) -> Result<Module, String> {
        let path = library_path(bundle)?;
        // SAFETY: loading a plug-in runs its code; that is why this whole
        // host runs in its own process.
        let lib = unsafe { Library::new(&path) }
            .map_err(|e| format!("Could not load the plug-in: {e}"))?;
        unsafe {
            #[cfg(windows)]
            if let Ok(init) = lib.get::<unsafe extern "system" fn() -> bool>(b"InitDll\0") {
                init();
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            if let Ok(entry) =
                lib.get::<unsafe extern "C" fn(*mut c_void) -> bool>(b"ModuleEntry\0")
            {
                // Plug-ins use the handle to find their own files; a null one is allowed.
                if !entry(std::ptr::null_mut()) {
                    return Err("The plug-in refused to start (ModuleEntry failed).".into());
                }
            }
            let get = lib
                .get::<unsafe extern "system" fn() -> *mut IPluginFactory>(b"GetPluginFactory\0")
                .map_err(|_| "This is not a VST3 plug-in (no GetPluginFactory).".to_string())?;
            let factory = ComPtr::from_raw(get()).ok_or("The plug-in has no factory.")?;
            Ok(Module {
                factory: Some(factory),
                lib: Some(lib),
            })
        }
    }

    pub fn factory(&self) -> &ComPtr<IPluginFactory> {
        self.factory.as_ref().expect("set until drop")
    }

    pub fn classes(&self) -> Vec<FactoryClass> {
        let mut out = Vec::new();
        // The maker the file declares, for classes that leave theirs blank.
        let maker = unsafe {
            let mut fi: vst3::Steinberg::PFactoryInfo = std::mem::zeroed();
            if self.factory().getFactoryInfo(&mut fi) == vst3::Steinberg::kResultOk {
                from_cstring(&fi.vendor)
            } else {
                String::new()
            }
        };
        let or_maker = |v: String| {
            if v.trim().is_empty() {
                maker.clone()
            } else {
                v
            }
        };
        unsafe {
            let f2 = self.factory().cast::<IPluginFactory2>();
            let n = self.factory().countClasses();
            for i in 0..n {
                if let Some(f2) = &f2 {
                    let mut info: PClassInfo2 = std::mem::zeroed();
                    if f2.getClassInfo2(i, &mut info) == vst3::Steinberg::kResultOk {
                        out.push(FactoryClass {
                            cid: info.cid.map(|b| b as u8),
                            category: from_cstring(&info.category),
                            name: from_cstring(&info.name),
                            vendor: or_maker(from_cstring(&info.vendor)),
                            version: from_cstring(&info.version),
                            sub_categories: from_cstring(&info.subCategories),
                        });
                        continue;
                    }
                }
                let mut info: PClassInfo = std::mem::zeroed();
                if self.factory().getClassInfo(i, &mut info) == vst3::Steinberg::kResultOk {
                    out.push(FactoryClass {
                        cid: info.cid.map(|b| b as u8),
                        category: from_cstring(&info.category),
                        name: from_cstring(&info.name),
                        vendor: maker.clone(),
                        version: String::new(),
                        sub_categories: String::new(),
                    });
                }
            }
        }
        out
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        // Let go of the factory before the module is told to shut down.
        drop(self.factory.take());
        let Some(lib) = self.lib.take() else { return };
        unsafe {
            #[cfg(windows)]
            if let Ok(exit) = lib.get::<unsafe extern "system" fn() -> bool>(b"ExitDll\0") {
                exit();
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            if let Ok(exit) = lib.get::<unsafe extern "C" fn() -> bool>(b"ModuleExit\0") {
                exit();
            }
        }
        // Many plug-ins leave threads running; unloading under them can
        // crash the host on the way out. The process is ending anyway.
        std::mem::forget(lib);
    }
}

pub fn cid_hex(cid: &[u8; 16]) -> String {
    cid.iter().map(|b| format!("{b:02X}")).collect()
}

pub fn parse_cid(hex: &str) -> Option<[u8; 16]> {
    if hex.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}
