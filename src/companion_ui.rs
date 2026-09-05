use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    FillRect, FrameRect, InvalidateRect, SelectObject, SetBkMode,
    SetTextColor, DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER, FW_NORMAL, HBRUSH,
    PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, EnumWindows, GetAncestor, GetCursorPos, GetWindowRect,
    IsWindow, IsWindowVisible, KillTimer, RegisterClassW, SetTimer,
    SetWindowPos, ShowWindow, WindowFromPoint, CS_HREDRAW, CS_VREDRAW, GA_ROOT,
    HCURSOR, HWND_TOPMOST, IDC_ARROW, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const WM_MOUSELEAVE: u32 = 0x02A3;
use crate::monitor::{get_all_monitors, get_monitor_for_window, MonitorInfo};
use crate::mover::move_window_to_monitor;

static COMPANION_HWND: AtomicIsize = AtomicIsize::new(0);
static SUBMENU_HWND: AtomicIsize = AtomicIsize::new(0);
static TARGET_HWND: AtomicIsize = AtomicIsize::new(0);
static CURRENT_JUMPLIST_HWND: AtomicIsize = AtomicIsize::new(0);
static IS_HOVERED: AtomicBool = AtomicBool::new(false);
static HOVERED_MONITOR_IDX: AtomicIsize = AtomicIsize::new(-1);

static SESSION_ID: AtomicU64 = AtomicU64::new(0);
static IS_VISIBLE: AtomicBool = AtomicBool::new(false);

static MONITORS_CACHE: Mutex<Vec<MonitorInfo>> = Mutex::new(Vec::new());

const CLASS_COMPANION: PCWSTR = w!("WindowDisplaySwapperCompanion");
const CLASS_SUBMENU: PCWSTR = w!("WindowDisplaySwapperSubmenu");

const TIMER_DISMISS_CHECK: usize = 1001;

pub fn init_companion_window() {
    unsafe {
        let instance = HINSTANCE(windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap().0);

        // Register Companion Header Window Class
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(companion_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: crate::tray::get_app_icon(),
            hCursor: HCURSOR(windows::Win32::UI::WindowsAndMessaging::LoadCursorW(None, IDC_ARROW).unwrap().0),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: CLASS_COMPANION,
        };
        let _ = RegisterClassW(&wc);

        // Register Flyout Submenu Window Class
        let sub_wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(submenu_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: crate::tray::get_app_icon(),
            hCursor: HCURSOR(windows::Win32::UI::WindowsAndMessaging::LoadCursorW(None, IDC_ARROW).unwrap().0),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: CLASS_SUBMENU,
        };
        let _ = RegisterClassW(&sub_wc);

        // Create Companion Window (hidden initially)
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS_COMPANION,
            w!("Move to Monitor"),
            WS_POPUP,
            0,
            0,
            240,
            38,
            None,
            None,
            instance,
            None,
        ).unwrap();

        let sub_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS_SUBMENU,
            w!("Monitors Submenu"),
            WS_POPUP,
            0,
            0,
            290,
            120,
            None,
            None,
            instance,
            None,
        ).unwrap();

        // Enable Windows 11 rounded corners on both
        let corner_pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );
        let _ = DwmSetWindowAttribute(
            sub_hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        COMPANION_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        SUBMENU_HWND.store(sub_hwnd.0 as isize, Ordering::SeqCst);
    }
}

fn is_window_cloaked(hwnd: HWND) -> bool {
    unsafe {
        let mut cloaked: u32 = 0;
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
        hr.is_ok() && cloaked != 0
    }
}

fn get_window_visible_bounds(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut frame_rect: RECT = std::mem::zeroed();
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame_rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        );
        if hr.is_ok() && frame_rect.right > frame_rect.left {
            Some(frame_rect)
        } else {
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                Some(rect)
            } else {
                None
            }
        }
    }
}

fn find_jumplist_hwnd(click_pt: POINT) -> Option<HWND> {
    unsafe {
        let comp_val = COMPANION_HWND.load(Ordering::SeqCst);
        let sub_val = SUBMENU_HWND.load(Ordering::SeqCst);

        // Try hit-testing near the click point at multiple offsets
        for offset_y in [25, 45, 75, 120, 180, 260] {
            for offset_x in [0, 30, -30, 60, -60] {
                let test_pt = POINT { x: click_pt.x + offset_x, y: click_pt.y - offset_y };
                let win = WindowFromPoint(test_pt);
                if !win.0.is_null() {
                    let root = GetAncestor(win, GA_ROOT);
                    let target = if !root.0.is_null() { root } else { win };
                    if target.0 as isize != comp_val && target.0 as isize != sub_val {
                        if !is_window_cloaked(target) {
                            if let Some(rect) = get_window_visible_bounds(target) {
                                let w = rect.right - rect.left;
                                let h = rect.bottom - rect.top;
                                if w >= 120 && w <= 750 && h >= 60 && h <= 1200 && rect.top < click_pt.y - 30 {
                                    return Some(target);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: enumerate visible windows near click_pt
        struct Search {
            pt: POINT,
            comp_h: isize,
            sub_h: isize,
            found: Option<HWND>,
        }
        let mut search = Search {
            pt: click_pt,
            comp_h: comp_val,
            sub_h: sub_val,
            found: None,
        };

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam.0 as *mut Search);
            if hwnd.0 as isize == ctx.comp_h || hwnd.0 as isize == ctx.sub_h {
                return BOOL(1);
            }
            if !IsWindow(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() || is_window_cloaked(hwnd) {
                return BOOL(1);
            }
            if let Some(rect) = get_window_visible_bounds(hwnd) {
                let w = rect.right - rect.left;
                let h = rect.bottom - rect.top;
                if w >= 120 && w <= 750 && h >= 60 && h <= 1200 {
                    if ctx.pt.x >= rect.left - 80 && ctx.pt.x <= rect.right + 80 && rect.bottom >= ctx.pt.y - 120 && rect.top < ctx.pt.y - 30 {
                        ctx.found = Some(hwnd);
                        return BOOL(0);
                    }
                }
            }
            BOOL(1)
        }

        let lparam = LPARAM(&mut search as *mut _ as isize);
        let _ = EnumWindows(Some(enum_proc), lparam);
        search.found
    }
}

fn get_dpi_for_hwnd(hwnd: HWND) -> u32 {
    unsafe {
        let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd);
        if dpi == 0 { 96 } else { dpi }
    }
}

pub fn scale_dpi(val: i32, dpi: u32) -> i32 {
    (val * dpi as i32 + 48) / 96
}

pub fn font_size_for_dpi(point_size: i32, dpi: u32) -> i32 {
    -((point_size * dpi as i32 + 36) / 72)
}

pub fn calculate_submenu_position(
    comp_rect: RECT,
    sub_w: i32,
    mon_right: i32,
    dpi: u32,
) -> (i32, i32) {
    let mut sub_x = comp_rect.right + scale_dpi(4, dpi);
    let sub_y = comp_rect.top;

    if sub_x + sub_w > mon_right - scale_dpi(10, dpi) {
        sub_x = comp_rect.left - sub_w - scale_dpi(4, dpi);
    }

    (sub_x, sub_y)
}

pub fn track_and_show(target: HWND, click_pt: POINT) {
    let target_raw = target.0 as isize;
    let session = SESSION_ID.fetch_add(1, Ordering::SeqCst) + 1;
    IS_VISIBLE.store(true, Ordering::SeqCst);

    std::thread::spawn(move || {
        if SESSION_ID.load(Ordering::SeqCst) != session {
            return;
        }

        let comp_val = COMPANION_HWND.load(Ordering::SeqCst);
        if comp_val == 0 {
            return;
        }
        let companion_hwnd = HWND(comp_val as *mut _);

        TARGET_HWND.store(target_raw, Ordering::SeqCst);
        let monitors = get_all_monitors();
        {
            let mut lock = MONITORS_CACHE.lock().unwrap();
            *lock = monitors;
        }

        let mut tracked_hwnd: Option<HWND> = None;
        let mut last_top = 0;
        let mut last_left = 0;
        let mut last_width = 0;

        // Phase 1: Poll frequently to latch onto the Jump List window
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(20));

            if SESSION_ID.load(Ordering::SeqCst) != session {
                return;
            }

            let current_j_hwnd = tracked_hwnd.or_else(|| find_jumplist_hwnd(click_pt));
            if let Some(j_hwnd) = current_j_hwnd {
                tracked_hwnd = Some(j_hwnd);
                CURRENT_JUMPLIST_HWND.store(j_hwnd.0 as isize, Ordering::SeqCst);

                if !unsafe { IsWindow(j_hwnd).as_bool() && IsWindowVisible(j_hwnd).as_bool() } || is_window_cloaked(j_hwnd) {
                    hide_companion();
                    return;
                }

                if let Some(rect) = get_window_visible_bounds(j_hwnd) {
                    let dpi = get_dpi_for_hwnd(j_hwnd);
                    let bar_h = scale_dpi(44, dpi);
                    let shadow_inset_x = scale_dpi(12, dpi);
                    let shadow_inset_top = scale_dpi(8, dpi);
                    let gap = scale_dpi(6, dpi);
                    let min_w = scale_dpi(240, dpi);
                    let w = (rect.right - rect.left - (shadow_inset_x * 2)).max(min_w);
                    let x = rect.left + shadow_inset_x;
                    let y = (rect.top + shadow_inset_top - bar_h - gap).max(10);

                    // If position or height changed (e.g. recent files finished loading)
                    if rect.top != last_top || rect.left != last_left || w != last_width {
                        last_top = rect.top;
                        last_left = rect.left;
                        last_width = w;

                        unsafe {
                            let _ = SetWindowPos(
                                companion_hwnd,
                                HWND_TOPMOST,
                                x,
                                y,
                                w,
                                bar_h,
                                SWP_NOACTIVATE | SWP_SHOWWINDOW,
                            );
                            let _ = ShowWindow(companion_hwnd, SW_SHOW);
                            let _ = InvalidateRect(companion_hwnd, None, true);
                            let _ = SetTimer(companion_hwnd, TIMER_DISMISS_CHECK, 80, None);
                        }
                    }
                }
            }
        }

        // Phase 2: Maintain position and dismiss immediately when Jump List closes or cloaks
        if let Some(j_hwnd) = tracked_hwnd {
            while unsafe { IsWindow(j_hwnd).as_bool() && IsWindowVisible(j_hwnd).as_bool() } && !is_window_cloaked(j_hwnd) {
                std::thread::sleep(std::time::Duration::from_millis(30));

                if SESSION_ID.load(Ordering::SeqCst) != session {
                    return;
                }

                if let Some(rect) = get_window_visible_bounds(j_hwnd) {
                    if rect.top != last_top || rect.left != last_left {
                        last_top = rect.top;
                        last_left = rect.left;
                        let dpi = get_dpi_for_hwnd(j_hwnd);
                        let bar_h = scale_dpi(44, dpi);
                        let shadow_inset_x = scale_dpi(12, dpi);
                        let shadow_inset_top = scale_dpi(8, dpi);
                        let gap = scale_dpi(6, dpi);
                        let min_w = scale_dpi(240, dpi);
                        let w = (rect.right - rect.left - (shadow_inset_x * 2)).max(min_w);
                        let x = rect.left + shadow_inset_x;
                        let y = (rect.top + shadow_inset_top - bar_h - gap).max(10);

                        unsafe {
                            let _ = SetWindowPos(
                                companion_hwnd,
                                HWND_TOPMOST,
                                x,
                                y,
                                w,
                                bar_h,
                                SWP_NOACTIVATE | SWP_SHOWWINDOW,
                            );
                        }
                    }
                }
            }
            hide_companion();
        } else {
            // Jump List never appeared or closed before latching
            hide_companion();
        }
    });
}

pub fn handle_mouse_down(pt: POINT) -> bool {
    if !IS_VISIBLE.load(Ordering::SeqCst) {
        return false;
    }
    let comp_val = COMPANION_HWND.load(Ordering::SeqCst);
    let sub_val = SUBMENU_HWND.load(Ordering::SeqCst);
    if comp_val == 0 {
        return false;
    }

    unsafe {
        let comp_hwnd = HWND(comp_val as *mut _);
        if !IsWindowVisible(comp_hwnd).as_bool() {
            return false;
        }

        // 1. Check if click is inside the Submenu
        if sub_val != 0 {
            let sub_hwnd = HWND(sub_val as *mut _);
            if IsWindowVisible(sub_hwnd).as_bool() {
                let mut r2: RECT = std::mem::zeroed();
                let _ = GetWindowRect(sub_hwnd, &mut r2);
                if pt.x >= r2.left && pt.x <= r2.right && pt.y >= r2.top && pt.y <= r2.bottom {
                    crate::log_debug(&format!("[INPUT] Click inside SUBMENU at ({}, {}) (Submenu Bounds: {:?})", pt.x, pt.y, r2));
                    let dpi = get_dpi_for_hwnd(sub_hwnd);
                    let item_h = scale_dpi(42, dpi);
                    let pad_top = scale_dpi(6, dpi);
                    let y_rel = pt.y - r2.top;

                    let monitors = {
                        let lock = MONITORS_CACHE.lock().unwrap();
                        lock.clone()
                    };

                    let clicked_idx = if y_rel >= pad_top {
                        let raw = (y_rel - pad_top) / item_h;
                        if raw >= 0 && (raw as usize) < monitors.len() {
                            raw
                        } else {
                            HOVERED_MONITOR_IDX.load(Ordering::SeqCst) as i32
                        }
                    } else {
                        HOVERED_MONITOR_IDX.load(Ordering::SeqCst) as i32
                    };

                    crate::log_debug(&format!("[INPUT] Resolved monitor row index: {} (Total monitors: {})", clicked_idx, monitors.len()));

                    if clicked_idx >= 0 && (clicked_idx as usize) < monitors.len() {
                        if let Some(target_mon) = monitors.get(clicked_idx as usize) {
                            let target_val = TARGET_HWND.swap(0, Ordering::SeqCst);
                            crate::log_debug(&format!("[INPUT] User selected: '{}' -> Relocating target 0x{:X}", target_mon.display_label, target_val));
                            if target_val != 0 {
                                let target_hwnd = HWND(target_val as *mut _);
                                let res = move_window_to_monitor(target_hwnd, target_mon);
                                crate::log_debug(&format!("[MOVE] Execution result: {:?}", res));
                            } else {
                                crate::log_debug("[MOVE] Warning: TARGET_HWND was 0 or already moved by a concurrent event!");
                            }
                        }
                    }

                    dismiss_native_jumplist();
                    hide_companion();
                    return true;
                }
            }
        }

        // 2. Check if click is inside the Companion Header
        let mut r1: RECT = std::mem::zeroed();
        let _ = GetWindowRect(comp_hwnd, &mut r1);
        if pt.x >= r1.left && pt.x <= r1.right && pt.y >= r1.top && pt.y <= r1.bottom {
            crate::log_debug(&format!("[INPUT] Click inside COMPANION BAR at ({}, {}) -> Showing submenu", pt.x, pt.y));
            show_submenu();
            return true;
        }

        // 3. Outside both: dismiss
        crate::log_debug(&format!("[INPUT] Click OUTSIDE companion UI at ({}, {}) -> Dismissing", pt.x, pt.y));
        hide_companion();
    }
    false
}

pub fn dismiss_native_jumplist() {
    crate::log_debug("[UI] Dismissing native Windows Jump List / context menu");
    let j_val = CURRENT_JUMPLIST_HWND.swap(0, Ordering::SeqCst);
    if j_val != 0 {
        unsafe {
            let j_hwnd = HWND(j_val as *mut _);
            if IsWindow(j_hwnd).as_bool() {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    j_hwnd,
                    windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }

    // Send VK_ESCAPE to dismiss any active Shell context menu
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_ESCAPE};
        keybd_event(VK_ESCAPE.0 as u8, 0, Default::default(), 0);
        keybd_event(VK_ESCAPE.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

pub fn check_click_outside(pt: POINT) -> bool {
    if !IS_VISIBLE.load(Ordering::SeqCst) {
        return false;
    }
    let comp_val = COMPANION_HWND.load(Ordering::SeqCst);
    let sub_val = SUBMENU_HWND.load(Ordering::SeqCst);
    if comp_val == 0 {
        return false;
    }

    unsafe {
        let comp_hwnd = HWND(comp_val as *mut _);
        if !IsWindowVisible(comp_hwnd).as_bool() {
            return false;
        }

        let mut r1: RECT = std::mem::zeroed();
        let _ = GetWindowRect(comp_hwnd, &mut r1);
        let in_r1 = pt.x >= r1.left && pt.x <= r1.right && pt.y >= r1.top && pt.y <= r1.bottom;

        let in_r2 = if sub_val != 0 {
            let sub_hwnd = HWND(sub_val as *mut _);
            if IsWindowVisible(sub_hwnd).as_bool() {
                let mut r2: RECT = std::mem::zeroed();
                let _ = GetWindowRect(sub_hwnd, &mut r2);
                pt.x >= r2.left && pt.x <= r2.right && pt.y >= r2.top && pt.y <= r2.bottom
            } else {
                false
            }
        } else {
            false
        };

        if !in_r1 && !in_r2 {
            hide_companion();
            return true;
        }
    }
    false
}

pub fn hide_companion() {
    SESSION_ID.fetch_add(1, Ordering::SeqCst);
    IS_VISIBLE.store(false, Ordering::SeqCst);
    unsafe {
        let h1 = COMPANION_HWND.load(Ordering::SeqCst);
        if h1 != 0 {
            let hwnd = HWND(h1 as *mut _);
            let _ = KillTimer(hwnd, TIMER_DISMISS_CHECK);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        let h2 = SUBMENU_HWND.load(Ordering::SeqCst);
        if h2 != 0 {
            let sub_hwnd = HWND(h2 as *mut _);
            let _ = ShowWindow(sub_hwnd, SW_HIDE);
        }
    }
}

fn show_submenu() {
    let sub_val = SUBMENU_HWND.load(Ordering::SeqCst);
    let comp_val = COMPANION_HWND.load(Ordering::SeqCst);
    if sub_val == 0 || comp_val == 0 {
        return;
    }

    let sub_hwnd = HWND(sub_val as *mut _);
    let comp_hwnd = HWND(comp_val as *mut _);

    let monitors_count = {
        let lock = MONITORS_CACHE.lock().unwrap();
        lock.len()
    };

    let dpi = get_dpi_for_hwnd(comp_hwnd);
    let item_h = scale_dpi(42, dpi);
    let sub_w = scale_dpi(320, dpi);
    let pad_v = scale_dpi(12, dpi);
    let sub_h = (monitors_count as i32 * item_h) + pad_v;

    unsafe {
        let mut comp_rect: RECT = std::mem::zeroed();
        let _ = GetWindowRect(comp_hwnd, &mut comp_rect);

        let mut mon_right = 99999;
        {
            let lock = MONITORS_CACHE.lock().unwrap();
            for m in lock.iter() {
                if comp_rect.left >= m.rect.left && comp_rect.left < m.rect.right {
                    mon_right = m.rect.right;
                    break;
                }
            }
        }
        let (sub_x, sub_y) = calculate_submenu_position(comp_rect, sub_w, mon_right, dpi);

        let _ = SetWindowPos(
            sub_hwnd,
            HWND_TOPMOST,
            sub_x,
            sub_y,
            sub_w,
            sub_h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = ShowWindow(sub_hwnd, SW_SHOW);
        let _ = InvalidateRect(sub_hwnd, None, true);
    }
}

// Window procedure for Companion Header
unsafe extern "system" fn companion_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect: RECT = std::mem::zeroed();
            let _ = GetWindowRect(hwnd, &mut rect);
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;
            let client_rect = RECT { left: 0, top: 0, right: w, bottom: h };

            let hovered = IS_HOVERED.load(Ordering::SeqCst);

            // Windows 11 Dark theme colors
            let bg_color = if hovered { COLORREF(0x003A3A3A) } else { COLORREF(0x00242424) };
            let border_color = COLORREF(0x00454545);
            let text_color = COLORREF(0x00FFFFFF);

            let bg_brush = CreateSolidBrush(bg_color);
            let border_pen = CreatePen(PS_SOLID, 1, border_color);

            FillRect(hdc, &client_rect, bg_brush);

            let old_pen = SelectObject(hdc, border_pen);
            FrameRect(hdc, &client_rect, CreateSolidBrush(border_color));

            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, text_color);

            // Dynamic DPI-scaled 12pt ClearType Segoe UI Variable font matching native Windows 11 context menus
            let dpi = get_dpi_for_hwnd(hwnd);
            let font_h = font_size_for_dpi(12, dpi);
            let font = CreateFontW(
                font_h, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, 0, 0, 0, 5, 0, w!("Segoe UI Variable Text"),
            );
            let old_font = SelectObject(hdc, font);

            // Draw label
            let pad_left = scale_dpi(16, dpi);
            let pad_right = scale_dpi(32, dpi);
            let mut text_rect = RECT { left: pad_left, top: 0, right: w - pad_right, bottom: h };
            let mut text: Vec<u16> = "🖥️  Move to Monitor".encode_utf16().collect();
            DrawTextW(hdc, &mut text, &mut text_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // Draw arrow
            let mut arrow_rect = RECT { left: w - pad_right + scale_dpi(4, dpi), top: 0, right: w - scale_dpi(8, dpi), bottom: h };
            let mut arrow: Vec<u16> = "▶".encode_utf16().collect();
            DrawTextW(hdc, &mut arrow, &mut arrow_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

            SelectObject(hdc, old_font);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(font);
            let _ = DeleteObject(bg_brush);
            let _ = DeleteObject(border_pen);

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if !IS_HOVERED.swap(true, Ordering::SeqCst) {
                let _ = InvalidateRect(hwnd, None, true);
                show_submenu();

                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            IS_HOVERED.store(false, Ordering::SeqCst);
            let _ = InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            show_submenu();
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_DISMISS_CHECK {
                if !IS_VISIBLE.load(Ordering::SeqCst) {
                    let _ = KillTimer(hwnd, TIMER_DISMISS_CHECK);
                    return LRESULT(0);
                }

                let mut pt: POINT = std::mem::zeroed();
                let _ = GetCursorPos(&mut pt);

                let mut r1: RECT = std::mem::zeroed();
                let _ = GetWindowRect(hwnd, &mut r1);

                let mut r2: RECT = std::mem::zeroed();
                let sub_val = SUBMENU_HWND.load(Ordering::SeqCst);
                if sub_val != 0 {
                    let _ = GetWindowRect(HWND(sub_val as *mut _), &mut r2);
                }

                // If mouse is moved far outside the menu area, dismiss
                let far_x = pt.x < r1.left - 200 || pt.x > r1.right + 450;
                let far_y = pt.y < r1.top - 200 || pt.y > r1.bottom + 650;

                if far_x || far_y {
                    hide_companion();
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// Window procedure for Monitors Flyout Submenu
unsafe extern "system" fn submenu_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect: RECT = std::mem::zeroed();
            let _ = GetWindowRect(hwnd, &mut rect);
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;
            let client_rect = RECT { left: 0, top: 0, right: w, bottom: h };

            let bg_brush = CreateSolidBrush(COLORREF(0x00242424));
            let border_brush = CreateSolidBrush(COLORREF(0x00454545));
            FillRect(hdc, &client_rect, bg_brush);
            FrameRect(hdc, &client_rect, border_brush);

            SetBkMode(hdc, TRANSPARENT);

            // Dynamic DPI-scaled 12pt ClearType Segoe UI Variable font matching native Windows 11 context menus
            let dpi = get_dpi_for_hwnd(hwnd);
            let font_h = font_size_for_dpi(12, dpi);
            let font = CreateFontW(
                font_h, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, 0, 0, 0, 5, 0, w!("Segoe UI Variable Text"),
            );
            let old_font = SelectObject(hdc, font);

            let target_val = TARGET_HWND.load(Ordering::SeqCst);
            let current_mon = if target_val != 0 {
                get_monitor_for_window(HWND(target_val as *mut _))
            } else {
                None
            };

            let hovered_idx = HOVERED_MONITOR_IDX.load(Ordering::SeqCst);

            let monitors = {
                let lock = MONITORS_CACHE.lock().unwrap();
                lock.clone()
            };

            let item_h = scale_dpi(42, dpi);
            let pad_top = scale_dpi(6, dpi);
            let pad_h = scale_dpi(6, dpi);

            for (i, mon) in monitors.iter().enumerate() {
                let item_top = pad_top + (i as i32 * item_h);
                let item_rect = RECT {
                    left: pad_h,
                    top: item_top,
                    right: w - pad_h,
                    bottom: item_top + item_h - scale_dpi(2, dpi),
                };

                if hovered_idx == i as isize {
                    let h_brush = CreateSolidBrush(COLORREF(0x003A3A3A));
                    FillRect(hdc, &item_rect, h_brush);
                    let _ = DeleteObject(h_brush);
                }

                let is_current = current_mon.map_or(false, |m| m == mon.hmonitor);
                let check = if is_current { "✓  " } else { "    " };
                let label = format!("{}{}", check, mon.display_label);
                let mut wide_label: Vec<u16> = label.encode_utf16().collect();

                SetTextColor(hdc, if is_current { COLORREF(0x0080FF80) } else { COLORREF(0x00FFFFFF) });

                let mut text_rect = RECT {
                    left: item_rect.left + scale_dpi(8, dpi),
                    top: item_rect.top,
                    right: item_rect.right - scale_dpi(8, dpi),
                    bottom: item_rect.bottom,
                };
                DrawTextW(
                    hdc,
                    &mut wide_label,
                    &mut text_rect,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
            }

            SelectObject(hdc, old_font);
            let _ = DeleteObject(font);
            let _ = DeleteObject(bg_brush);
            let _ = DeleteObject(border_brush);

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let y = (lparam.0 >> 16) as i16 as i32;
            let dpi = get_dpi_for_hwnd(hwnd);
            let item_h = scale_dpi(42, dpi);
            let pad_top = scale_dpi(6, dpi);
            let idx = (y - pad_top) / item_h;

            let count = {
                let lock = MONITORS_CACHE.lock().unwrap();
                lock.len() as isize
            };

            let new_idx = if idx >= 0 && (idx as isize) < count { idx as isize } else { -1 };
            if HOVERED_MONITOR_IDX.swap(new_idx, Ordering::SeqCst) != new_idx {
                let _ = InvalidateRect(hwnd, None, true);
            }

            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);

            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            HOVERED_MONITOR_IDX.store(-1, Ordering::SeqCst);
            let _ = InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let y = (lparam.0 >> 16) as i16 as i32;
            let dpi = get_dpi_for_hwnd(hwnd);
            let item_h = scale_dpi(42, dpi);
            let pad_top = scale_dpi(6, dpi);
            let clicked_idx = (y - pad_top) / item_h;

            let monitors = {
                let lock = MONITORS_CACHE.lock().unwrap();
                lock.clone()
            };

            crate::log_debug(&format!("Submenu LBUTTONUP at y={}, clicked_idx={}, monitors_count={}", y, clicked_idx, monitors.len()));

            let selected_monitor = if clicked_idx >= 0 && (clicked_idx as usize) < monitors.len() {
                monitors.get(clicked_idx as usize).cloned()
            } else {
                let h_idx = HOVERED_MONITOR_IDX.load(Ordering::SeqCst);
                if h_idx >= 0 {
                    monitors.get(h_idx as usize).cloned()
                } else {
                    None
                }
            };

            if let Some(target_mon) = selected_monitor {
                let target_val = TARGET_HWND.swap(0, Ordering::SeqCst);
                crate::log_debug(&format!("Target monitor chosen: '{}', TARGET_HWND=0x{:X}", target_mon.display_label, target_val));
                if target_val != 0 {
                    let target_hwnd = HWND(target_val as *mut _);
                    let res = move_window_to_monitor(target_hwnd, &target_mon);
                    crate::log_debug(&format!("move_window_to_monitor result: {:?}", res));
                } else {
                    crate::log_debug("TARGET_HWND already moved or 0.");
                }
            } else {
                crate::log_debug("No monitor was selected.");
            }

            dismiss_native_jumplist();
            hide_companion();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
