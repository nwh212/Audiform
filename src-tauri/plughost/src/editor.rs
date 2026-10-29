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
    /// Shows whether the effect is bypassed on the window's Audiform strip.
    fn set_bypassed(&mut self, bypassed: bool);
    /// The bypass the user picked on the strip since the last look.
    fn take_bypass(&mut self) -> Option<bool>;
    /// Hides the window for a while (the editor's close prompt), or shows it again.
    fn hide(&mut self, hidden: bool);
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
    _bypassed: bool,
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
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::{Arc, Mutex};

    use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait, ViewType};
    use vst3::Steinberg::{
        kPlatformTypeHWND, kResultTrue, IPlugFrame, IPlugView, IPlugViewContentScaleSupport,
        IPlugViewContentScaleSupportTrait, IPlugViewTrait, ViewRect,
    };
    use vst3::{ComPtr, ComWrapper};
    use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        Arc as ArcTo, BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject,
        DrawTextW, EndPaint, FillRect, GetStockObject, InvalidateRect, LineTo, MonitorFromWindow,
        MoveToEx, SelectObject, SetBkMode, SetTextColor, DT_LEFT, DT_RIGHT, DT_SINGLELINE,
        DT_VCENTER, MONITOR_DEFAULTTONEAREST, NULL_BRUSH, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{
        AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForSystem, GetDpiForWindow,
        SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        MDT_EFFECTIVE_DPI,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetWindowLongPtrW, LoadCursorW, MoveWindow, PeekMessageW, RegisterClassW,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
        CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MSG, PM_REMOVE, SWP_NOMOVE, SWP_NOZORDER, SW_HIDE,
        SW_SHOW, SW_SHOWNOACTIVATE, WM_CLOSE, WM_DPICHANGED, WM_ERASEBKGND, WM_LBUTTONDOWN,
        WM_PAINT, WM_SIZE, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
        WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
    };

    use super::EditorWindow;
    use crate::com::{Edit, Frame};

    /// The Audiform strip under the plug-in, at 96 DPI.
    const STRIP: i32 = 22;

    /// One window's state, reached from its messages (several plug-in
    /// windows can share this thread when a maker's plug-ins run together).
    struct State {
        closed: bool,
        /// The client size after a resize.
        sized: Option<(i32, i32)>,
        /// A new monitor scale: the DPI and the window rectangle Windows
        /// suggests for it.
        dpi: Option<(u32, RECT)>,
        bypassed: bool,
        /// The bypass picked on the strip, not yet passed on.
        picked: Option<bool>,
        /// The strip's height in pixels at the window's scale.
        strip: i32,
    }

    unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State> {
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).as_mut()
    }

    fn strip_for(dpi: u32) -> i32 {
        (STRIP * dpi as i32 + 48) / 96
    }

    /// The scale of the monitor a window is on (1.0 is 96 DPI).
    unsafe fn monitor_dpi(hwnd: HWND) -> u32 {
        if hwnd.is_null() {
            return GetDpiForSystem();
        }
        let (mut x, mut y) = (0u32, 0u32);
        if GetDpiForMonitor(
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
            MDT_EFFECTIVE_DPI,
            &mut x,
            &mut y,
        ) == 0
            && x > 0
        {
            x
        } else {
            GetDpiForSystem()
        }
    }

    /// Tells a plug-in the display scale, when it wants to know.
    unsafe fn tell_scale(view: &ComPtr<IPlugView>, dpi: u32) {
        if let Some(cs) = view.cast::<IPlugViewContentScaleSupport>() {
            cs.setContentScaleFactor(dpi as f32 / 96.0);
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
        r as u32 | (g as u32) << 8 | (b as u32) << 16
    }

    unsafe fn client(hwnd: HWND) -> RECT {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        r
    }

    /// The power button and its label, from the strip's left edge.
    fn button_width(strip: i32) -> i32 {
        strip * 4
    }

    /// Draws the strip: a power button that bypasses the effect (the same
    /// switch as on its row in the effects rack), and the app's name.
    unsafe fn paint(hwnd: HWND, st: &State) {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        let r = client(hwnd);
        let s = st.strip;
        let top = r.bottom - s;
        let fill = |rect: RECT, c: COLORREF| {
            let b = CreateSolidBrush(c);
            FillRect(dc, &rect, b);
            DeleteObject(b);
        };
        fill(
            RECT {
                left: 0,
                top,
                right: r.right,
                bottom: r.bottom,
            },
            rgb(0x1d, 0x1e, 0x24),
        );
        fill(
            RECT {
                left: 0,
                top,
                right: r.right,
                bottom: top + 1,
            },
            rgb(0x3a, 0x3c, 0x46),
        );
        let k = s as f32 / STRIP as f32;
        let col = if st.bypassed {
            rgb(0x7a, 0x7c, 0x86)
        } else {
            rgb(0x5c, 0xd6, 0x8a)
        };
        let (cx, cy) = ((13.0 * k) as i32, top + s / 2);
        let rad = (5.5 * k) as i32;
        let pen = CreatePen(PS_SOLID, (1.6 * k).round().max(1.0) as i32, col);
        let old_pen = SelectObject(dc, pen);
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        // An open ring with its gap at the top, and the stroke through it.
        let gap = (rad as f32 * 0.6) as i32;
        ArcTo(
            dc,
            cx - rad,
            cy - rad,
            cx + rad + 1,
            cy + rad + 1,
            cx - gap,
            cy - rad,
            cx + gap,
            cy - rad,
        );
        MoveToEx(dc, cx, cy - rad - (1.0 * k) as i32, ptr::null_mut());
        LineTo(dc, cx, cy + 1);
        let face = wide("Segoe UI");
        let font = CreateFontW(
            -((11.0 * k) as i32),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            1,
            0,
            0,
            5,
            0,
            face.as_ptr(),
        );
        let old_font = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as _);
        SetTextColor(dc, col);
        let mut label = wide(if st.bypassed { "Bypassed" } else { "On" });
        let mut lr = RECT {
            left: cx + rad + (7.0 * k) as i32,
            top,
            right: button_width(s),
            bottom: r.bottom,
        };
        DrawTextW(
            dc,
            label.as_mut_ptr(),
            -1,
            &mut lr,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        SetTextColor(dc, rgb(0x8a, 0x8c, 0x96));
        let mut name = wide("Audiform");
        let mut nr = RECT {
            left: button_width(s),
            top,
            right: r.right - (8.0 * k) as i32,
            bottom: r.bottom,
        };
        DrawTextW(
            dc,
            name.as_mut_ptr(),
            -1,
            &mut nr,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, old_font);
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(font);
        DeleteObject(pen);
        EndPaint(hwnd, &ps);
    }

    unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        let Some(st) = state(hwnd) else {
            return DefWindowProcW(hwnd, msg, wp, lp);
        };
        match msg {
            WM_CLOSE => {
                st.closed = true;
                0
            }
            WM_DPICHANGED => {
                let r = *(lp as *const RECT);
                st.dpi = Some(((wp & 0xFFFF) as u32, r));
                0
            }
            WM_SIZE => {
                let r = client(hwnd);
                st.sized = Some((r.right - r.left, r.bottom - r.top));
                0
            }
            // The plug-in covers the rest; the strip paints itself whole.
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                paint(hwnd, st);
                0
            }
            WM_LBUTTONDOWN => {
                let x = (lp & 0xFFFF) as i16 as i32;
                let y = ((lp >> 16) & 0xFFFF) as i16 as i32;
                if y >= client(hwnd).bottom - st.strip && x < button_width(st.strip) {
                    st.bypassed = !st.bypassed;
                    st.picked = Some(st.bypassed);
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    struct Win {
        hwnd: HWND,
        /// The child window the plug-in draws in, above the strip.
        child: HWND,
        view: Option<ComPtr<IPlugView>>,
        _frame: ComWrapper<Frame>,
        style: u32,
        resizable: bool,
        state: Box<State>,
        opened: std::time::Instant,
        settled: bool,
    }

    impl Win {
        /// Reads the plug-in's size once and sizes the window to it. A
        /// plug-in that scales itself when it attaches says its real size
        /// only then, so the first size can be too small. Asked once or
        /// twice, never in a loop, so a window cannot keep growing.
        unsafe fn adopt_size(&mut self) {
            let Some(v) = &self.view else { return };
            let mut now: ViewRect = std::mem::zeroed();
            if v.getSize(&mut now) != vst3::Steinberg::kResultOk {
                return;
            }
            let (w, h) = (now.right - now.left, now.bottom - now.top);
            let r = client(self.hwnd);
            if w > 0 && h > 0 && (w, h) != (r.right, (r.bottom - self.state.strip).max(1)) {
                self.resize(w, h);
                if let Some(v) = &self.view {
                    let mut vr = ViewRect {
                        left: 0,
                        top: 0,
                        right: w,
                        bottom: h,
                    };
                    v.onSize(&mut vr);
                }
            }
        }
    }

    impl Win {
        /// Fits the plug-in's area to the window, above the strip.
        unsafe fn fit_child(&self) -> (i32, i32) {
            let r = client(self.hwnd);
            let h = (r.bottom - self.state.strip).max(1);
            MoveWindow(self.child, 0, 0, r.right, h, 1);
            InvalidateRect(self.hwnd, ptr::null(), 0);
            (r.right, h)
        }
    }

    pub fn open(
        controller: &ComPtr<IEditController>,
        title: &str,
        edits: Arc<Mutex<Vec<Edit>>>,
        owner: u64,
        bypassed: bool,
    ) -> Result<Box<dyn EditorWindow>, String> {
        unsafe {
            let view = ComPtr::<IPlugView>::from_raw(controller.createView(ViewType::kEditor))
                .ok_or("This plug-in has no window of its own.")?;
            if view.isPlatformTypeSupported(kPlatformTypeHWND) != kResultTrue {
                return Err("This plug-in's window does not support Windows.".into());
            }
            let hinst = GetModuleHandleW(ptr::null());
            let class = wide("AlteredAudioPluginWindow");
            let view_class = wide("AudiformPluginView");
            for (name, proc_fn) in [
                (&class, Some(proc_ as _)),
                (&view_class, Some(DefWindowProcW as _)),
            ] {
                let wc = WNDCLASSW {
                    style: 0,
                    lpfnWndProc: proc_fn,
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: hinst,
                    hIcon: ptr::null_mut(),
                    hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
                    hbrBackground: ptr::null_mut(),
                    lpszMenuName: ptr::null(),
                    lpszClassName: name.as_ptr(),
                };
                // Fails harmlessly when the class is already registered.
                RegisterClassW(&wc);
            }
            let resizable = view.canResize() == kResultTrue;
            let mut style =
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
            if resizable {
                style |= WS_THICKFRAME;
            }
            // Plug-in windows follow each monitor's scale (per-monitor DPI
            // aware), so a plug-in sizes and draws in real pixels. Plug-ins
            // that take the VST3 content scale are told it before they size
            // themselves; the rest read the window's DPI.
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let owner_hwnd: HWND = if owner == 0 {
                ptr::null_mut()
            } else {
                owner as usize as _
            };
            let dpi = monitor_dpi(owner_hwnd);
            let strip = strip_for(dpi);
            tell_scale(&view, dpi);
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
                bottom: h + strip,
            };
            AdjustWindowRectExForDpi(&mut r, style, 0, 0, dpi);
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
                owner_hwnd,
                ptr::null_mut(),
                hinst,
                ptr::null(),
            );
            if hwnd.is_null() {
                return Err("Could not open a window for the plug-in.".into());
            }
            let mut state = Box::new(State {
                closed: false,
                sized: None,
                dpi: None,
                bypassed,
                picked: None,
                strip,
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *state as *mut State as isize);
            let child = CreateWindowExW(
                0,
                view_class.as_ptr(),
                ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                0,
                0,
                w,
                h,
                hwnd,
                ptr::null_mut(),
                hinst,
                ptr::null(),
            );
            if child.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DestroyWindow(hwnd);
                return Err("Could not open a window for the plug-in.".into());
            }
            let frame = ComWrapper::new(Frame { edits });
            if let Some(f) = frame.as_com_ref::<IPlugFrame>() {
                view.setFrame(f.as_ptr());
            }
            if view.attached(child as *mut c_void, kPlatformTypeHWND) != kResultTrue {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DestroyWindow(hwnd);
                return Err("The plug-in could not open its window.".into());
            }
            let mut win = Win {
                hwnd,
                child,
                view: Some(view),
                _frame: frame,
                style,
                resizable,
                state,
                opened: std::time::Instant::now(),
                settled: false,
            };
            win.adopt_size();
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            Ok(Box::new(win))
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
                // Once more after the plug-in has had a moment to settle.
                if !self.settled && self.opened.elapsed() > std::time::Duration::from_millis(600) {
                    self.settled = true;
                    self.adopt_size();
                }
                // Moved to a monitor with another scale: tell the plug-in,
                // then size the window to what it asks for.
                if let (Some((dpi, suggested)), Some(v)) = (self.state.dpi.take(), &self.view) {
                    self.state.strip = strip_for(dpi);
                    tell_scale(v, dpi);
                    let mut size: ViewRect = std::mem::zeroed();
                    v.getSize(&mut size);
                    let mut r = RECT {
                        left: 0,
                        top: 0,
                        right: size.right - size.left,
                        bottom: size.bottom - size.top + self.state.strip,
                    };
                    AdjustWindowRectExForDpi(&mut r, self.style, 0, 0, dpi);
                    SetWindowPos(
                        self.hwnd,
                        ptr::null_mut(),
                        suggested.left,
                        suggested.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER,
                    );
                    self.fit_child();
                    let mut vr = size;
                    v.onSize(&mut vr);
                }
                if self.state.sized.take().is_some() {
                    let (w, h) = self.fit_child();
                    if let (true, Some(v)) = (self.resizable, &self.view) {
                        let mut r = ViewRect {
                            left: 0,
                            top: 0,
                            right: w,
                            bottom: h,
                        };
                        v.onSize(&mut r);
                    }
                }
            }
            !self.state.closed
        }

        fn resize(&mut self, w: i32, h: i32) {
            unsafe {
                let mut r = RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h + self.state.strip,
                };
                AdjustWindowRectExForDpi(&mut r, self.style, 0, 0, GetDpiForWindow(self.hwnd));
                SetWindowPos(
                    self.hwnd,
                    ptr::null_mut(),
                    0,
                    0,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOMOVE | SWP_NOZORDER,
                );
                self.fit_child();
            }
        }

        fn raise(&mut self) {
            unsafe {
                ShowWindow(self.hwnd, SW_SHOW);
                SetForegroundWindow(self.hwnd);
            }
        }

        fn set_bypassed(&mut self, bypassed: bool) {
            if self.state.bypassed != bypassed {
                self.state.bypassed = bypassed;
                unsafe { InvalidateRect(self.hwnd, ptr::null(), 0) };
            }
        }

        fn take_bypass(&mut self) -> Option<bool> {
            self.state.picked.take()
        }

        fn hide(&mut self, hidden: bool) {
            unsafe {
                ShowWindow(self.hwnd, if hidden { SW_HIDE } else { SW_SHOWNOACTIVATE });
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
                // Its messages stop reaching the state before it goes.
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                DestroyWindow(self.hwnd);
            }
        }
    }
}
