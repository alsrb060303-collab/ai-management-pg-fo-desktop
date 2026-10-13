//! Windows: follow the active terminal/editor window with WinEvent hooks.
//!
//! A dedicated thread owns the hooks and a message loop. Every hook callback and
//! refresh request funnels into `Tracker::sync`, which reads the real window state
//! and moves, shows/hides and re-stacks the sidebar.
//!
//! Z-order: while the target (or the sidebar itself) is the foreground window the
//! sidebar is topmost; when another app comes forward the sidebar drops right
//! behind it. We avoid making the target the sidebar's owner, because Windows
//! destroys owned windows together with their owner.

use std::cell::RefCell;
use std::sync::mpsc;

use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, DispatchMessageW, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_DESTROY,
    EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, EnumWindows, GA_ROOT, GWL_EXSTYLE,
    GetAncestor, GetClassNameW, GetForegroundWindow, GetMessageW, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindow, IsWindowVisible,
    KillTimer, MSG, OBJID_WINDOW, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, STYLESTRUCT,
    SW_HIDE, SW_SHOWNOACTIVATE, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, ShowWindowAsync,
    TranslateMessage, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP, WM_STYLECHANGING,
    WM_TIMER, WM_USER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};
use windows::core::{BOOL, PWSTR};

use super::layout::{self, Rect, Side};
use super::{
    Dock, DockMode, DockStatus, Magnet, SIDEBAR_MIN_HEIGHT, SIDEBAR_WIDTH, Settings, TargetApp,
};

const WM_DOCK_REFRESH: u32 = WM_APP + 1;
const REFRESH_USER_SHOW: usize = 1;
/// Delay before re-applying the rect after a monitor change, so it wins over the
/// resize Windows applies for the new DPI.
const DPI_SETTLE_MS: u32 = 150;
/// Windows smaller than this (physical pixels) are never docking targets.
const MIN_TARGET_WIDTH: i32 = 320;
const MIN_TARGET_HEIGHT: i32 = 240;

pub struct TrackerHandle {
    thread_id: u32,
}

impl TrackerHandle {
    pub fn refresh(&self, user_show: bool) {
        post_refresh(self.thread_id, user_show);
    }
}

/// Subclass procedure that keeps the sidebar a tool window. tao rewrites the
/// whole extended style whenever it shows or hides the window, which would put
/// the sidebar back in the taskbar and Alt+Tab.
unsafe extern "system" fn keep_tool_window(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_STYLECHANGING && wparam.0 as i32 == GWL_EXSTYLE.0 {
        // SAFETY: for WM_STYLECHANGING, lparam points to the STYLESTRUCT being applied.
        let style = unsafe { &mut *(lparam.0 as *mut STYLESTRUCT) };
        style.styleNew = (style.styleNew | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Must run on the thread that owns the sidebar window (the main thread).
pub fn start(app: AppHandle, sidebar: HWND) -> std::io::Result<TrackerHandle> {
    // Keep the sidebar out of the taskbar and Alt+Tab for good. `skipTaskbar`
    // alone is lost whenever the window is hidden and shown again. The style is
    // picked up on the next show, which the tracker's first sync does.
    unsafe {
        let ex_style = GetWindowLongPtrW(sidebar, GWL_EXSTYLE);
        let _ = ShowWindow(sidebar, SW_HIDE);
        let _ = SetWindowSubclass(sidebar, Some(keep_tool_window), 1, 0);
        SetWindowLongPtrW(
            sidebar,
            GWL_EXSTYLE,
            (ex_style | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize),
        );
    }
    // HWND is not Send; pass the raw value to the tracker thread.
    let sidebar = sidebar.0 as isize;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("dock-tracker".into())
        .spawn(move || {
            let mut msg = MSG::default();
            // Create the thread's message queue before anyone posts to it.
            unsafe {
                let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
            }
            let _ = tx.send(unsafe { GetCurrentThreadId() });
            run(app, HWND(sidebar as _));
        })?;
    let thread_id = rx
        .recv()
        .map_err(|_| std::io::Error::other("dock tracker thread exited early"))?;
    Ok(TrackerHandle { thread_id })
}

thread_local! {
    static TRACKER: RefCell<Option<Tracker>> = const { RefCell::new(None) };
}

fn run(app: AppHandle, sidebar: HWND) {
    TRACKER.with(|cell| *cell.borrow_mut() = Some(Tracker::new(app, sidebar)));

    let global_hooks = [
        hook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, 0),
        hook(EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND, 0),
    ];
    with_tracker(|t| t.refresh(false));

    let mut msg = MSG::default();
    // GetMessageW returns 0 for WM_QUIT and -1 on error.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.hwnd.0.is_null() {
            match msg.message {
                WM_DOCK_REFRESH => {
                    let user_show = msg.wParam.0 == REFRESH_USER_SHOW;
                    with_tracker(|t| t.refresh(user_show));
                    continue;
                }
                WM_TIMER => {
                    let id = msg.wParam.0;
                    with_tracker(|t| t.on_timer(id));
                    continue;
                }
                _ => {}
            }
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }

    for h in global_hooks.into_iter().flatten() {
        unsafe {
            let _ = UnhookWinEvent(h);
        }
    }
    TRACKER.with(|cell| {
        if let Some(mut t) = cell.borrow_mut().take() {
            t.unhook_target();
        }
    });
}

fn post_refresh(thread_id: u32, user_show: bool) {
    let wparam = WPARAM(if user_show { REFRESH_USER_SHOW } else { 0 });
    unsafe {
        let _ = PostThreadMessageW(thread_id, WM_DOCK_REFRESH, wparam, LPARAM(0));
    }
}

/// Runs `f` on the tracker. Hook callbacks can re-enter while a Win32 call pumps
/// messages; in that case the work is deferred to a refresh instead.
fn with_tracker(f: impl FnOnce(&mut Tracker)) {
    TRACKER.with(|cell| match cell.try_borrow_mut() {
        Ok(mut guard) => {
            if let Some(tracker) = guard.as_mut() {
                f(tracker);
            }
        }
        Err(_) => post_refresh(unsafe { GetCurrentThreadId() }, false),
    });
}

fn hook(event_min: u32, event_max: u32, process_id: u32) -> Option<HWINEVENTHOOK> {
    let h = unsafe {
        SetWinEventHook(
            event_min,
            event_max,
            None,
            Some(on_win_event),
            process_id,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    (!h.is_invalid()).then_some(h)
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.0.is_null() || id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 {
        return;
    }
    with_tracker(|t| t.on_event(event, hwnd));
}

struct Target {
    hwnd: HWND,
    app: TargetApp,
}

struct Tracker {
    app: AppHandle,
    sidebar: HWND,
    own_pid: u32,
    target: Option<Target>,
    target_hooks: Vec<HWINEVENTHOOK>,
    hooked_pid: Option<u32>,
    /// Whether we last made the sidebar topmost. `None` until first applied.
    topmost: Option<bool>,
    /// Visibility we last requested; `IsWindowVisible` lags behind queued
    /// `ShowWindowAsync` calls. `None` after a refresh to resynchronize.
    shown: Option<bool>,
    last_rect: Option<Rect>,
    settle_timer: usize,
}

impl Tracker {
    fn new(app: AppHandle, sidebar: HWND) -> Self {
        Self {
            app,
            sidebar,
            own_pid: unsafe { GetCurrentProcessId() },
            target: None,
            target_hooks: Vec::new(),
            hooked_pid: None,
            topmost: None,
            shown: None,
            last_rect: None,
            settle_timer: 0,
        }
    }

    fn settings(&self) -> Settings {
        self.app.state::<Dock>().settings()
    }

    fn on_event(&mut self, event: u32, hwnd: HWND) {
        if event == EVENT_SYSTEM_FOREGROUND {
            self.on_foreground(hwnd);
            return;
        }
        if self.target.as_ref().map(|t| t.hwnd) != Some(hwnd) {
            return;
        }
        if event == EVENT_OBJECT_DESTROY {
            self.detach();
        }
        // Minimize, restore, show/hide, cloak (virtual desktops) and moves all
        // resolve the same way: re-read the target's state.
        self.sync();
    }

    fn on_foreground(&mut self, hwnd: HWND) {
        // Clicking the sidebar itself keeps the current target and stacking.
        if self.is_own(hwnd) {
            return;
        }
        // Re-apply the rect even if the target did not move: the user may have
        // dragged the sidebar away.
        self.last_rect = None;
        // Auto follows every target that comes forward; locked only takes one
        // while it has none.
        let take = match self.settings().magnet {
            Magnet::Auto => true,
            Magnet::Locked => self.target.is_none(),
            Magnet::Off => false,
        };
        if take
            && let Some(app) = classify(hwnd)
            && self.target.as_ref().map(|t| t.hwnd) != Some(hwnd)
        {
            self.attach(hwnd, app);
        }
        self.sync();
    }

    fn on_timer(&mut self, id: usize) {
        if id == self.settle_timer {
            unsafe {
                let _ = KillTimer(None, id);
            }
            self.settle_timer = 0;
            self.last_rect = None;
            self.sync();
        }
    }

    fn refresh(&mut self, user_show: bool) {
        self.last_rect = None;
        self.shown = None;
        // The user asked to see the sidebar while its target is out of view:
        // let go of the target so the sidebar shows at the screen edge.
        if user_show && self.target.as_ref().is_some_and(|t| is_hidden(t.hwnd)) {
            self.detach();
        }
        if self.settings().magnet.on()
            && self.target.is_none()
            && let Some((hwnd, app)) = find_target()
        {
            self.attach(hwnd, app);
        }
        self.sync();
    }

    /// Applies placement, visibility and z-order from the current state.
    fn sync(&mut self) {
        let settings = self.settings();
        if self
            .target
            .as_ref()
            .is_some_and(|t| !unsafe { IsWindow(Some(t.hwnd)) }.as_bool())
        {
            self.detach();
        }

        if !settings.magnet.on() {
            self.detach();
            self.set_visible(settings.visible);
            self.set_topmost(settings.pinned);
            self.publish(&settings, DockMode::Floating, None);
            return;
        }

        let Some(target) = self.target.as_ref().map(|t| t.hwnd) else {
            self.place_at_edge();
            self.set_visible(settings.visible);
            self.set_topmost(settings.pinned);
            self.publish(&settings, DockMode::Edge, None);
            return;
        };

        if is_hidden(target) {
            // Follow the target out of view (minimized, hidden, other desktop).
            self.set_visible(false);
            self.publish(&settings, DockMode::Docked, None);
            return;
        }

        let side = self.place_beside(target);
        self.set_visible(settings.visible);
        let foreground = unsafe { GetForegroundWindow() };
        let keep_on_top = settings.pinned
            || foreground == target
            || (self.is_own(foreground) && self.topmost == Some(true));
        self.set_topmost(keep_on_top);
        self.publish(&settings, DockMode::Docked, side);
    }

    fn attach(&mut self, hwnd: HWND, app: TargetApp) {
        // Location, show/hide and destroy come from the target's process, so scope
        // those noisy events to it. Console windows report their client (cmd.exe)
        // as owner while conhost raises the events, so listen to all processes;
        // on_event filters by hwnd anyway.
        let pid = if app == TargetApp::Console {
            0
        } else {
            window_pid(hwnd)
        };
        if self.hooked_pid != Some(pid) {
            self.unhook_target();
            self.target_hooks.extend(
                [
                    hook(EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE, pid),
                    hook(EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED, pid),
                ]
                .into_iter()
                .flatten(),
            );
            self.hooked_pid = Some(pid);
        }
        self.target = Some(Target { hwnd, app });
        self.last_rect = None;
    }

    fn detach(&mut self) {
        if self.target.take().is_some() {
            self.unhook_target();
            self.last_rect = None;
        }
    }

    fn unhook_target(&mut self) {
        for h in self.target_hooks.drain(..) {
            unsafe {
                let _ = UnhookWinEvent(h);
            }
        }
        self.hooked_pid = None;
    }

    fn place_beside(&mut self, target: HWND) -> Option<Side> {
        let bounds = visible_bounds(target)?;
        let monitor = unsafe { MonitorFromWindow(target, MONITOR_DEFAULTTONEAREST) };
        let work = work_area(monitor)?;
        let scale = monitor_scale(monitor);
        let placement = layout::place_beside(
            bounds,
            work,
            layout::to_physical(SIDEBAR_WIDTH, scale),
            layout::to_physical(SIDEBAR_MIN_HEIGHT, scale),
        );
        self.move_to(placement.rect, monitor);
        Some(placement.side)
    }

    fn place_at_edge(&mut self) {
        let monitor = unsafe { MonitorFromWindow(self.sidebar, MONITOR_DEFAULTTONEAREST) };
        let Some(work) = work_area(monitor) else {
            return;
        };
        let width = layout::to_physical(SIDEBAR_WIDTH, monitor_scale(monitor));
        self.move_to(layout::place_at_edge(work, width), monitor);
    }

    fn move_to(&mut self, rect: Rect, monitor: HMONITOR) {
        if self.last_rect == Some(rect) {
            return;
        }
        self.last_rect = Some(rect);
        let current = unsafe { MonitorFromWindow(self.sidebar, MONITOR_DEFAULTTONEAREST) };
        unsafe {
            let _ = SetWindowPos(
                self.sidebar,
                None,
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
            );
        }

        if current != monitor {
            // Moving to a monitor with another DPI makes Windows resize the
            // sidebar; re-apply our rect once that has settled.
            unsafe {
                if self.settle_timer != 0 {
                    let _ = KillTimer(None, self.settle_timer);
                }
                self.settle_timer = SetTimer(None, 0, DPI_SETTLE_MS, None);
            }
        }
    }

    fn set_visible(&mut self, visible: bool) {
        let shown = self
            .shown
            .unwrap_or_else(|| unsafe { IsWindowVisible(self.sidebar) }.as_bool());
        if shown != visible {
            let cmd = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
            unsafe {
                let _ = ShowWindowAsync(self.sidebar, cmd);
            }
        }
        self.shown = Some(visible);
    }

    fn set_topmost(&mut self, on: bool) {
        if self.topmost == Some(on) {
            return;
        }
        let insert_after = if on {
            HWND_TOPMOST
        } else {
            // Dropping behind the (non-topmost) foreground window clears topmost
            // and keeps the sidebar from covering the app the user switched to.
            // The desktop sits at the bottom of the z-order, so never go behind it.
            let foreground = unsafe { GetForegroundWindow() };
            if !foreground.0.is_null()
                && !self.is_own(foreground)
                && !has_ex_style(foreground, WS_EX_TOPMOST.0)
                && !matches!(
                    class_name(foreground).as_str(),
                    "Progman" | "WorkerW" | "Shell_TrayWnd"
                )
            {
                foreground
            } else {
                HWND_NOTOPMOST
            }
        };
        unsafe {
            let _ = SetWindowPos(
                self.sidebar,
                Some(insert_after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
            );
        }
        self.topmost = Some(on);
    }

    fn publish(&self, settings: &Settings, mode: DockMode, side: Option<Side>) {
        let status = DockStatus {
            magnet: settings.magnet,
            pinned: settings.pinned,
            visible: settings.visible,
            mode,
            target: self.target.as_ref().map(|t| t.app),
            side,
        };
        self.app.state::<Dock>().publish(&self.app, status);
    }

    fn is_own(&self, hwnd: HWND) -> bool {
        !hwnd.0.is_null() && window_pid(hwnd) == self.own_pid
    }
}

/// Decides whether a window is one we dock to.
fn classify(hwnd: HWND) -> Option<TargetApp> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || GetAncestor(hwnd, GA_ROOT) != hwnd {
            return None;
        }
    }
    if has_ex_style(hwnd, WS_EX_TOOLWINDOW.0) {
        return None;
    }
    // Skip small popups such as quick-entry or tooltip-like windows.
    let bounds = visible_bounds(hwnd)?;
    if bounds.width() < MIN_TARGET_WIDTH || bounds.height() < MIN_TARGET_HEIGHT {
        return None;
    }
    let class = class_name(hwnd);
    match class.as_str() {
        // Windows Terminal; the class is an internal constant, so also accept its process.
        "CASCADIA_HOSTING_WINDOW_CLASS" => return Some(TargetApp::WindowsTerminal),
        "ConsoleWindowClass" => return Some(TargetApp::Console),
        _ => {}
    }
    let exe = process_name(hwnd)?;
    match exe.as_str() {
        "windowsterminal.exe" => Some(TargetApp::WindowsTerminal),
        // Electron apps; Chromium appends a counter to the class name.
        "code.exe" | "code - insiders.exe" if class.starts_with("Chrome_WidgetWin_") => {
            Some(TargetApp::VsCode)
        }
        "claude.exe" if class.starts_with("Chrome_WidgetWin_") => Some(TargetApp::ClaudeDesktop),
        _ => None,
    }
}

/// Topmost (first in z-order) visible target window, used when nothing is focused yet.
fn find_target() -> Option<(HWND, TargetApp)> {
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Option<(HWND, TargetApp)>) };
        if !is_hidden(hwnd)
            && let Some(app) = classify(hwnd)
        {
            *found = Some((hwnd, app));
            return BOOL(0);
        }
        BOOL(1)
    }

    let mut found: Option<(HWND, TargetApp)> = None;
    unsafe {
        // Returns an error when the callback stops early; `found` carries the result.
        let _ = EnumWindows(Some(visit), LPARAM(&mut found as *mut _ as isize));
    }
    found
}

fn is_hidden(hwnd: HWND) -> bool {
    unsafe { !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || is_cloaked(hwnd) }
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            size_of::<u32>() as u32,
        )
    }
    .is_ok()
        && cloaked != 0
}

/// Visible frame bounds, without the invisible resize borders of Windows 10/11.
fn visible_bounds(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut RECT as *mut _,
            size_of::<RECT>() as u32,
        )
    }
    .is_ok()
        || unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok();
    ok.then_some(to_rect(rect))
}

fn work_area(monitor: HMONITOR) -> Option<Rect> {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetMonitorInfoW(monitor, &mut info) }
        .as_bool()
        .then_some(to_rect(info.rcWork))
}

fn monitor_scale(monitor: HMONITOR) -> f64 {
    let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
    match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) } {
        Ok(()) if dpi_x > 0 => f64::from(dpi_x) / 96.0,
        _ => 1.0,
    }
}

fn has_ex_style(hwnd: HWND, style: u32) -> bool {
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    ex_style & style != 0
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

/// Lower-cased executable file name of the process that owns `hwnd`.
fn process_name(hwnd: HWND) -> Option<String> {
    let pid = window_pid(hwnd);
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    unsafe {
        let _ = CloseHandle(process);
    }
    result.ok()?;
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit('\\').next().map(str::to_lowercase)
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32)) };
    pid
}

fn to_rect(r: RECT) -> Rect {
    Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}
