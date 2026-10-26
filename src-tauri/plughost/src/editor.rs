// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! The plug-in's own window. The host process opens a plain top-level
//! window and lets the plug-in draw into it, so a plug-in window that
//! hangs never freezes the editor. Windows only for now; elsewhere the
//! plug-in's controls show in the editor's own effect window.

#[cfg(not(windows))]
use std::sync::{Arc, Mutex};

#[cfg(not(windows))]
use crate::com::Edit;
#[cfg(not(windows))]
use vst3::ComPtr;
#[cfg(not(windows))]
use vst3::Steinberg::Vst::IEditController;

pub trait EditorWindow {
    /// Handles window messages; false once the user has closed it.
    fn pump(&mut self) -> bool;
    /// The plug-in asked for a new size.
    fn resize(&mut self, w: i32, h: i32);
    fn raise(&mut self);
}

/// Runs the window messages waiting for this thread, window or not.
/// Many plug-ins finish loading through them (timers, sounds loaded in
/// the background, work handed to the message thread), so an instrument
/// with its window closed would otherwise stay silent until it was
/// opened. Only Windows has such a queue here.
#[cfg(windows)]
pub fn pump_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(not(windows))]
pub fn pump_messages() {}

#[cfg(not(windows))]
pub fn open(
    _controller: &ComPtr<IEditController>,
    _title: &str,
    _edits: Arc<Mutex<Vec<Edit>>>,
    _owner: u64,
) -> Result<Box<dyn EditorWindow>, String> {
    Err(
        "Plug-in windows open on Windows for now. The plug-in's controls are in its effect window."
            .into(),
    )
}

#[cfg(windows)]
pub use win::open;

#[cfg(windows)]
mod win {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::{Arc, Mutex};

    use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait, ViewType};
    use vst3::Steinberg::{
        kPlatformTypeHWND, kResultTrue, IPlugFrame, IPlugView, IPlugViewTrait, ViewRect,
    };
    use vst3::{ComPtr, ComWrapper};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetClientRect, LoadCursorW, PeekMessageW, RegisterClassW, SetForegroundWindow,
        SetWindowPos, ShowWindow, TranslateMessage, CW_USEDEFAULT, IDC_ARROW, MSG, PM_REMOVE,
        SWP_NOMOVE, SWP_NOZORDER, SW_SHOW, WM_CLOSE, WM_SIZE, WNDCLASSW, WS_CAPTION,
        WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_THICKFRAME,
    };

    use super::EditorWindow;
    use crate::com::{Edit, Frame};

    thread_local! {
        static CLOSED: Cell<bool> = const { Cell::new(false) };
        static SIZED: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_CLOSE => {
                CLOSED.with(|c| c.set(true));
                0
            }
            WM_SIZE => {
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                SIZED.with(|s| s.set(Some((r.right - r.left, r.bottom - r.top))));
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    struct Win {
        hwnd: HWND,
        view: Option<ComPtr<IPlugView>>,
        _frame: ComWrapper<Frame>,
        style: u32,
        resizable: bool,
    }

    pub fn open(
        controller: &ComPtr<IEditController>,
        title: &str,
        edits: Arc<Mutex<Vec<Edit>>>,
        owner: u64,
    ) -> Result<Box<dyn EditorWindow>, String> {
        unsafe {
            let view = ComPtr::<IPlugView>::from_raw(controller.createView(ViewType::kEditor))
                .ok_or("This plug-in has no window of its own.")?;
            if view.isPlatformTypeSupported(kPlatformTypeHWND) != kResultTrue {
                return Err("This plug-in's window does not support Windows.".into());
            }
            let hinst = GetModuleHandleW(ptr::null());
            let class = wide("AlteredAudioPluginWindow");
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(proc_),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: ptr::null_mut(),
                hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
                hbrBackground: ptr::null_mut(),
                lpszMenuName: ptr::null(),
                lpszClassName: class.as_ptr(),
            };
            // Fails harmlessly when the class is already registered.
            RegisterClassW(&wc);
            let resizable = view.canResize() == kResultTrue;
            let mut style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
            if resizable {
                style |= WS_THICKFRAME;
            }
            let mut size: ViewRect = std::mem::zeroed();
            view.getSize(&mut size);
            let (w, h) = (
                (size.right - size.left).max(100),
                (size.bottom - size.top).max(60),
            );
            let mut r = RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            };
            AdjustWindowRectEx(&mut r, style, 0, 0);
            let name = wide(title);
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                name.as_ptr(),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                r.right - r.left,
                r.bottom - r.top,
                // Owned by the editor's main window: it stays above it, and
                // clicking in the main window never buries it.
                if owner == 0 {
                    ptr::null_mut()
                } else {
                    owner as usize as _
                },
                ptr::null_mut(),
                hinst,
                ptr::null(),
            );
            if hwnd.is_null() {
                return Err("Could not open a window for the plug-in.".into());
            }
            let frame = ComWrapper::new(Frame { edits });
            if let Some(f) = frame.as_com_ref::<IPlugFrame>() {
                view.setFrame(f.as_ptr());
            }
            if view.attached(hwnd as *mut c_void, kPlatformTypeHWND) != kResultTrue {
                DestroyWindow(hwnd);
                return Err("The plug-in could not open its window.".into());
            }
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            CLOSED.with(|c| c.set(false));
            Ok(Box::new(Win {
                hwnd,
                view: Some(view),
                _frame: frame,
                style,
                resizable,
            }))
        }
    }

    impl EditorWindow for Win {
        fn pump(&mut self) -> bool {
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                if let (true, Some((w, h)), Some(v)) =
                    (self.resizable, SIZED.with(|s| s.take()), &self.view)
                {
                    let mut r = ViewRect {
                        left: 0,
                        top: 0,
                        right: w,
                        bottom: h,
                    };
                    v.onSize(&mut r);
                }
            }
            !CLOSED.with(|c| c.get())
        }

        fn resize(&mut self, w: i32, h: i32) {
            unsafe {
                let mut r = RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                };
                AdjustWindowRectEx(&mut r, self.style, 0, 0);
                SetWindowPos(
                    self.hwnd,
                    ptr::null_mut(),
                    0,
                    0,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOMOVE | SWP_NOZORDER,
                );
            }
        }

        fn raise(&mut self) {
            unsafe {
                ShowWindow(self.hwnd, SW_SHOW);
                SetForegroundWindow(self.hwnd);
            }
        }
    }

    impl Drop for Win {
        fn drop(&mut self) {
            unsafe {
                if let Some(v) = self.view.take() {
                    v.setFrame(ptr::null_mut());
                    v.removed();
                }
                DestroyWindow(self.hwnd);
            }
        }
    }
}
