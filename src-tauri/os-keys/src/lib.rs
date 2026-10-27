// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

//! Modifier keys read straight from the system. While files are dragged
//! in from the file manager, the system owns the keyboard, so the window
//! never hears key presses; asking the system is the only way to know
//! whether Alt is down.
//!
//! This is its own small crate because the system call needs `unsafe`,
//! and the app crate forbids `unsafe` everywhere.

/// Whether Alt is held right now, or `None` where this is not known
/// (systems other than Windows: the window's own key events are used there).
pub fn alt_down() -> Option<bool> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
        // SAFETY: GetAsyncKeyState only reads the key state; any key code is valid.
        let state = unsafe { GetAsyncKeyState(i32::from(VK_MENU)) };
        // The high bit is set while the key is down.
        Some(state < 0)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn answers_on_every_system() {
        let a = super::alt_down();
        assert_eq!(a.is_some(), cfg!(windows));
    }
}
