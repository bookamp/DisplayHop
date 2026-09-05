use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsZoomed, SetForegroundWindow,
    SetWindowPos, ShowWindow, GWL_STYLE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_MAXIMIZE,
    SW_RESTORE, WS_CAPTION, WS_POPUP,
};
use crate::monitor::{get_all_monitors, MonitorInfo};

pub fn move_window_to_monitor(hwnd: HWND, target_monitor: &MonitorInfo) -> Result<(), String> {
    crate::log_debug(&format!("[MOVE] Initiating window move for HWND 0x{:X} to '{}'", hwnd.0 as usize, target_monitor.display_label));
    unsafe {
        if hwnd.0.is_null() {
            crate::log_debug("[MOVE] Error: Invalid window handle null");
            return Err("Invalid window handle".to_string());
        }

        let mut class_buf = [0u16; 128];
        let class_len = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);

        let mut title_buf = [0u16; 256];
        let title_len = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut title_buf);
        let title = String::from_utf16_lossy(&title_buf[..title_len as usize]);

        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let proc_name = crate::taskbar::get_process_name(pid).unwrap_or_default();

        let root = windows::Win32::UI::WindowsAndMessaging::GetAncestor(hwnd, windows::Win32::UI::WindowsAndMessaging::GA_ROOT);

        crate::log_debug(&format!(
            "[MOVE] Target details: HWND=0x{:X}, Root=0x{:X}, PID={}, Process='{}', Class='{}', Title='{}'",
            hwnd.0 as usize, root.0 as usize, pid, proc_name, class_name, title
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

        // Check window style for borderless fullscreen game
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let is_borderless = (style & WS_POPUP.0 != 0) && (style & WS_CAPTION.0 == 0);

        if was_maximized {
            // Restore window before moving, then re-maximize on target monitor
            let _ = ShowWindow(hwnd, SW_RESTORE);
            std::thread::sleep(std::time::Duration::from_millis(30));

            let target_x = target_monitor.work_area.left + 50;
            let target_y = target_monitor.work_area.top + 50;
            let target_w = (target_monitor.work_area.right - target_monitor.work_area.left - 100).max(400);
            let target_h = (target_monitor.work_area.bottom - target_monitor.work_area.top - 100).max(300);

            let ret = SetWindowPos(
                hwnd,
                None,
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
        } else if is_borderless && current_monitor.map_or(false, |m| (current_w - m.width).abs() < 10 && (current_h - m.height).abs() < 10) {
            // Borderless fullscreen game/app: span target monitor fully
            let ret = SetWindowPos(
                hwnd,
                None,
                target_monitor.rect.left,
                target_monitor.rect.top,
                target_monitor.width,
                target_monitor.height,
                SWP_NOZORDER | SWP_SHOWWINDOW,
            );
            crate::log_debug(&format!("[MOVE] Borderless move SetWindowPos: {:?}", ret));
            if let Err(ref e) = ret {
                if e.code() == windows::Win32::Foundation::E_ACCESSDENIED {
                    crate::tray::show_tray_notification(
                        "Elevation Required",
                        "Cannot move Administrator window. Right-click tray icon and select 'Restart as Administrator'.",
                    );
                }
                return Err(format!("SetWindowPos failed: {:?}", e));
            }
        } else {
            // Regular window: calculate proportional placement within target monitor work area
            let (target_x, target_y, target_w, target_h) = calculate_target_window_bounds(
                window_rect,
                current_monitor.map(|m| m.work_area),
                target_monitor.work_area,
            );

            let ret = SetWindowPos(
                hwnd,
                None,
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

        // Set focus to the moved window and bring it forward
        let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        crate::log_debug("[MOVE] Move completed successfully. Window brought to foreground.");
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
