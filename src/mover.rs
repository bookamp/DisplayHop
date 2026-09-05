use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, BringWindowToTop, GetAncestor, GetClassNameW, GetForegroundWindow,
    GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsZoomed, LockSetForegroundWindow, PostMessageW, SetForegroundWindow, SetWindowLongW,
    SetWindowPos, ShowWindow, ASFW_ANY, GA_ROOT, GWL_STYLE, HWND_TOP, LSFW_UNLOCK, SWP_FRAMECHANGED,
    SWP_NOZORDER, SWP_SHOWWINDOW, SW_MAXIMIZE, SW_RESTORE, SW_SHOW, WA_ACTIVE, WM_ACTIVATE,
    WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_THICKFRAME,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, SetActiveWindow, SetFocus, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_MENU,
};

#[link(name = "user32")]
extern "system" {
    fn AttachThreadInput(
        idattach: u32,
        idattachto: u32,
        fattach: windows::Win32::Foundation::BOOL,
    ) -> windows::Win32::Foundation::BOOL;
}
use crate::monitor::{get_all_monitors, MonitorInfo};

pub fn is_game(
    proc_path: &str,
    proc_name: &str,
    class_name: &str,
    _title: &str,
    style: u32,
    window_rect: RECT,
    current_monitor: Option<&MonitorInfo>,
) -> bool {
    // 1. Engine & Game Window Classes
    let class_lower = class_name.to_lowercase();
    if class_lower.contains("unrealwindow")
        || class_lower.contains("unitywndclass")
        || class_lower.starts_with("sdl_app")
        || class_lower.starts_with("glfw")
        || class_lower.contains("valve001")
        || class_lower.contains("cryengine")
        || class_lower.contains("godot")
        || class_lower.contains("sfml_window")
        || class_lower.contains("yygame")
        || class_lower.contains("aceapp")
        || class_lower.contains("launchunrealuwindowsclient")
    {
        return true;
    }

    // 2. Common Game Launchers & Installation Folders
    let path_lower = proc_path.to_lowercase();
    if path_lower.contains("\\steamapps\\common\\")
        || path_lower.contains("\\epic games\\")
        || path_lower.contains("\\gog galaxy\\games\\")
        || path_lower.contains("\\gog games\\")
        || path_lower.contains("\\riot games\\")
        || path_lower.contains("\\ubisoft\\")
        || path_lower.contains("\\ea games\\")
        || path_lower.contains("\\origin games\\")
        || path_lower.contains("\\xboxgames\\")
        || path_lower.contains("\\battlenet\\")
        || path_lower.contains("\\battle.net\\")
        || path_lower.contains("\\games\\")
        || path_lower.contains("/games/")
    {
        return true;
    }

    // 3. Executable Naming Patterns (Unreal Engine Shipping binaries, Emulators, Standalone Games)
    let proc_lower = proc_name.to_lowercase();
    if proc_lower.ends_with("-shipping")
        || proc_lower.ends_with("-win64-shipping")
        || proc_lower.ends_with("-win32-shipping")
        || proc_lower.contains("retroarch")
        || proc_lower.contains("yuzu")
        || proc_lower.contains("ryujinx")
        || proc_lower.contains("rpcs3")
        || proc_lower.contains("pcsx2")
        || proc_lower.contains("dolphin")
        || proc_lower.contains("cemu")
    {
        return true;
    }

    // 4. Borderless Fullscreen Presentation Detection
    let is_borderless_popup = (style & WS_POPUP.0 != 0) && (style & WS_CAPTION.0 == 0);
    let win_w = window_rect.right - window_rect.left;
    let win_h = window_rect.bottom - window_rect.top;

    if let Some(mon) = current_monitor {
        // Window already fills monitor or is borderless and occupies substantial screen real estate
        if (win_w - mon.width).abs() <= 20 && (win_h - mon.height).abs() <= 20 {
            return true;
        }
        if is_borderless_popup && win_w >= mon.width * 4 / 5 && win_h >= mon.height * 4 / 5 {
            return true;
        }
    } else if is_borderless_popup && win_w >= 1280 && win_h >= 720 {
        return true;
    }

    false
}

pub unsafe fn force_foreground_window(hwnd: HWND) {
    if hwnd.0.is_null() || !IsWindow(hwnd).as_bool() {
        return;
    }

    let foreground_hwnd = GetForegroundWindow();
    let current_thread_id = GetCurrentThreadId();
    let mut target_thread_id = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut target_thread_id));

    let mut fg_thread_id = 0;
    if !foreground_hwnd.0.is_null() {
        GetWindowThreadProcessId(foreground_hwnd, Some(&mut fg_thread_id));
    }

    // 1. Attach thread input to share input queue
    let attached_current = if current_thread_id != target_thread_id && target_thread_id != 0 {
        AttachThreadInput(current_thread_id, target_thread_id, true.into()).as_bool()
    } else {
        false
    };

    let attached_fg = if fg_thread_id != 0 && fg_thread_id != target_thread_id {
        AttachThreadInput(fg_thread_id, target_thread_id, true.into()).as_bool()
    } else {
        false
    };

    // 2. Unlock and allow foreground setting
    let _ = AllowSetForegroundWindow(ASFW_ANY);
    let _ = LockSetForegroundWindow(LSFW_UNLOCK);

    // 3. Restore if minimized, otherwise show
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    } else {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }

    // 4. Bring to top of Z-order
    let _ = BringWindowToTop(hwnd);

    // 5. Try SetForegroundWindow with simulated Alt key fallback if Windows blocks it
    let success = SetForegroundWindow(hwnd).as_bool();
    if !success {
        keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_EXTENDEDKEY, 0);
        keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP, 0);
        let _ = SetForegroundWindow(hwnd);
    }

    // 6. Explicitly activate and set keyboard focus
    let _ = SetActiveWindow(hwnd);
    let _ = SetFocus(hwnd);

    // 7. Detach thread input
    if attached_fg {
        let _ = AttachThreadInput(fg_thread_id, target_thread_id, false.into());
    }
    if attached_current {
        let _ = AttachThreadInput(current_thread_id, target_thread_id, false.into());
    }

    // 8. Wake up window's message queue directly
    let _ = PostMessageW(hwnd, WM_ACTIVATE, WPARAM(WA_ACTIVE as usize), LPARAM(0));
}

pub fn move_window_to_monitor(hwnd: HWND, target_monitor: &MonitorInfo) -> Result<(), String> {
    crate::log_debug(&format!("[MOVE] Initiating window move for HWND 0x{:X} to '{}'", hwnd.0 as usize, target_monitor.display_label));
    unsafe {
        if hwnd.0.is_null() {
            crate::log_debug("[MOVE] Error: Invalid window handle null");
            return Err("Invalid window handle".to_string());
        }

        let mut class_buf = [0u16; 128];
        let class_len = GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);

        let mut title_buf = [0u16; 256];
        let title_len = GetWindowTextW(hwnd, &mut title_buf);
        let title = String::from_utf16_lossy(&title_buf[..title_len as usize]);

        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let proc_name = crate::taskbar::get_process_name(pid).unwrap_or_default();
        let proc_path = crate::taskbar::get_process_path(pid).unwrap_or_default();

        let root = GetAncestor(hwnd, GA_ROOT);

        crate::log_debug(&format!(
            "[MOVE] Target details: HWND=0x{:X}, Root=0x{:X}, PID={}, Process='{}', Path='{}', Class='{}', Title='{}'",
            hwnd.0 as usize, root.0 as usize, pid, proc_name, proc_path, class_name, title
        ));

        let hwnd = if !root.0.is_null() && root != hwnd {
            crate::log_debug(&format!("[MOVE] Using GA_ROOT HWND 0x{:X} instead of child 0x{:X}", root.0 as usize, hwnd.0 as usize));
            root
        } else {
            hwnd
        };

        // If window is minimized, restore it first
        if IsIconic(hwnd).as_bool() {
            crate::log_debug("[MOVE] Window is iconic (minimized), calling SW_RESTORE");
            let _ = ShowWindow(hwnd, SW_RESTORE);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let was_maximized = IsZoomed(hwnd).as_bool();

        // Get current window bounds
        let mut window_rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut window_rect).is_err() {
            crate::log_debug("[MOVE] Error: GetWindowRect failed on hwnd");
            return Err("Failed to get window rect".to_string());
        }

        if window_rect.left <= -30000 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = GetWindowRect(hwnd, &mut window_rect);
        }

        let current_w = window_rect.right - window_rect.left;
        let current_h = window_rect.bottom - window_rect.top;
        crate::log_debug(&format!("[MOVE] Current window bounds: ({}, {}, {}, {}), size={}x{}, maximized={}", window_rect.left, window_rect.top, window_rect.right, window_rect.bottom, current_w, current_h, was_maximized));

        // Find current monitor for this window
        let all_monitors = get_all_monitors();
        let current_monitor = all_monitors.iter().find(|m| {
            let center_x = window_rect.left + current_w / 2;
            let center_y = window_rect.top + current_h / 2;
            center_x >= m.rect.left && center_x < m.rect.right && center_y >= m.rect.top && center_y < m.rect.bottom
        }).or_else(|| all_monitors.first());

        crate::log_debug(&format!("[MOVE] Current monitor detected: '{}'", current_monitor.map_or("none", |m| &m.display_label)));

        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let is_game_window = is_game(
            &proc_path,
            &proc_name,
            &class_name,
            &title,
            style,
            window_rect,
            current_monitor,
        );

        if is_game_window {
            // GAME: Always opens in fullscreen mode on the target display
            crate::log_debug("[MOVE] Target identified as GAME. Enforcing fullscreen mode on target monitor.");

            if was_maximized || IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                std::thread::sleep(std::time::Duration::from_millis(30));
            }

            // If window has standard borders or title bar, remove them for true borderless fullscreen
            if style & WS_CAPTION.0 != 0 {
                let borderless_style = (style & !(WS_CAPTION.0 | WS_THICKFRAME.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0)) | WS_POPUP.0;
                let _ = SetWindowLongW(hwnd, GWL_STYLE, borderless_style as i32);
            }

            // Span ENTIRE target monitor resolution (including over the taskbar)
            let ret = SetWindowPos(
                hwnd,
                HWND_TOP,
                target_monitor.rect.left,
                target_monitor.rect.top,
                target_monitor.width,
                target_monitor.height,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
            crate::log_debug(&format!("[MOVE] Fullscreen game SetWindowPos: {:?}", ret));
            if let Err(ref e) = ret {
                if e.code() == windows::Win32::Foundation::E_ACCESSDENIED {
                    crate::tray::show_tray_notification(
                        "Elevation Required",
                        "Cannot move Administrator game. Right-click tray icon and select 'Restart as Administrator'.",
                    );
                }
                return Err(format!("SetWindowPos failed: {:?}", e));
            }
        } else if was_maximized {
            // Restore window before moving, then re-maximize on target monitor
            let _ = ShowWindow(hwnd, SW_RESTORE);
            std::thread::sleep(std::time::Duration::from_millis(30));

            let target_x = target_monitor.work_area.left + 50;
            let target_y = target_monitor.work_area.top + 50;
            let target_w = (target_monitor.work_area.right - target_monitor.work_area.left - 100).max(400);
            let target_h = (target_monitor.work_area.bottom - target_monitor.work_area.top - 100).max(300);

            let ret = SetWindowPos(
                hwnd,
                HWND_TOP,
                target_x,
                target_y,
                target_w,
                target_h,
                SWP_NOZORDER | SWP_SHOWWINDOW,
            );
            crate::log_debug(&format!("[MOVE] Maximized move SetWindowPos: {:?}, target=({}, {}, {}, {})", ret, target_x, target_y, target_w, target_h));
            if let Err(ref e) = ret {
                if e.code() == windows::Win32::Foundation::E_ACCESSDENIED {
                    crate::tray::show_tray_notification(
                        "Elevation Required",
                        "Cannot move Administrator window. Right-click tray icon and select 'Restart as Administrator'.",
                    );
                }
                return Err(format!("SetWindowPos failed: {:?}", e));
            }
            std::thread::sleep(std::time::Duration::from_millis(30));

            let _ = ShowWindow(hwnd, SW_MAXIMIZE);
        } else {
            // Regular window: calculate proportional placement within target monitor work area
            let (target_x, target_y, target_w, target_h) = calculate_target_window_bounds(
                window_rect,
                current_monitor.map(|m| m.work_area),
                target_monitor.work_area,
            );

            let ret = SetWindowPos(
                hwnd,
                HWND_TOP,
                target_x,
                target_y,
                target_w,
                target_h,
                SWP_NOZORDER | SWP_SHOWWINDOW,
            );
            crate::log_debug(&format!("[MOVE] Regular move SetWindowPos: {:?}, target=({}, {}, {}, {})", ret, target_x, target_y, target_w, target_h));
            if let Err(ref e) = ret {
                if e.code() == windows::Win32::Foundation::E_ACCESSDENIED {
                    crate::tray::show_tray_notification(
                        "Elevation Required",
                        "Cannot move Administrator window. Right-click tray icon and select 'Restart as Administrator'.",
                    );
                }
                return Err(format!("SetWindowPos failed: {:?}", e));
            }
        }

        // Always activate, bring to front, and guarantee focus to the moved window
        force_foreground_window(hwnd);

        // Background pulse to re-assert focus after game swapchains or DWM finishes resetting
        let hwnd_raw = hwnd.0 as isize;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(80));
            let h = HWND(hwnd_raw as *mut _);
            if IsWindow(h).as_bool() {
                force_foreground_window(h);
            }
        });

        crate::log_debug("[MOVE] Move completed successfully. Window brought to foreground with focus.");
    }

    Ok(())
}

pub fn calculate_target_window_bounds(
    window_rect: RECT,
    current_work_area: Option<RECT>,
    target_work_area: RECT,
) -> (i32, i32, i32, i32) {
    let current_w = window_rect.right - window_rect.left;
    let current_h = window_rect.bottom - window_rect.top;

    if let Some(cur_m) = current_work_area {
        let cur_work_w = (cur_m.right - cur_m.left).max(1);
        let cur_work_h = (cur_m.bottom - cur_m.top).max(1);

        let rel_x = (window_rect.left - cur_m.left) as f32 / cur_work_w as f32;
        let rel_y = (window_rect.top - cur_m.top) as f32 / cur_work_h as f32;

        let target_work_w = target_work_area.right - target_work_area.left;
        let target_work_h = target_work_area.bottom - target_work_area.top;

        // Scale window if it exceeds target monitor
        let new_w = current_w.min(target_work_w - 40);
        let new_h = current_h.min(target_work_h - 40);

        let mut new_x = target_work_area.left + (rel_x * target_work_w as f32) as i32;
        let mut new_y = target_work_area.top + (rel_y * target_work_h as f32) as i32;

        // Clamp within target monitor working area
        if new_x + new_w > target_work_area.right {
            new_x = target_work_area.right - new_w;
        }
        if new_x < target_work_area.left {
            new_x = target_work_area.left;
        }
        if new_y + new_h > target_work_area.bottom {
            new_y = target_work_area.bottom - new_h;
        }
        if new_y < target_work_area.top {
            new_y = target_work_area.top;
        }

        (new_x, new_y, new_w, new_h)
    } else {
        (
            target_work_area.left + 50,
            target_work_area.top + 50,
            current_w.min(target_work_area.right - target_work_area.left - 100),
            current_h.min(target_work_area.bottom - target_work_area.top - 100),
        )
    }
}
