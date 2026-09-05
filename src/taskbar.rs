use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::{BOOL, CloseHandle, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EnumWindows, FindWindowW, GetClassNameW, GetForegroundWindow,
    GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsWindow, IsWindowVisible, SetWindowsHookExW, UnhookWindowsHookEx, GWL_EXSTYLE, HHOOK,
    MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_RBUTTONUP, WS_EX_TOOLWINDOW,
};

static MOUSE_HOOK: AtomicIsize = AtomicIsize::new(0);
static LAST_CLICKED_HWND: AtomicIsize = AtomicIsize::new(0);

pub fn init_taskbar_hooks() {
    unsafe {
        // Initialize COM for the thread
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        // Get module handle for global hook (required on Windows)
        let instance = HINSTANCE(GetModuleHandleW(None).unwrap().0);

        // Install Low-Level Mouse Hook
        let hook = SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_hook_proc),
            instance,
            0,
        );
        match hook {
            Ok(h) => {
                MOUSE_HOOK.store(h.0 as isize, Ordering::SeqCst);
            }
            Err(e) => {
                eprintln!("Failed to install mouse hook: {:?}", e);
            }
        }
    }
}

pub fn cleanup_taskbar_hooks() {
    let hook_val = MOUSE_HOOK.swap(0, Ordering::SeqCst);
    if hook_val != 0 {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(hook_val as *mut _));
        }
    }
}

unsafe extern "system" fn mouse_hook_proc(
    n_code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let msg = wparam.0 as u32;
        let ms = *(lparam.0 as *const MSLLHOOKSTRUCT);
        let pt = ms.pt;

        if msg == WM_LBUTTONDOWN {
            if crate::companion_ui::handle_mouse_down(pt) {
                return LRESULT(1);
            }
        } else if msg == WM_RBUTTONDOWN || msg == WM_MBUTTONDOWN {
            crate::companion_ui::check_click_outside(pt);
        }

        if msg == WM_RBUTTONUP {
            if is_point_over_taskbar(pt) {
                crate::log_debug(&format!("[HOOK] Right-click on taskbar at ({}, {})", pt.x, pt.y));
                // Find the window corresponding to the taskbar item under cursor
                if let Some(target_hwnd) = find_target_window_from_point(pt) {
                    crate::log_debug(&format!("[TARGET] Latched target app HWND: 0x{:X}", target_hwnd.0 as usize));
                    LAST_CLICKED_HWND.store(target_hwnd.0 as isize, Ordering::SeqCst);
                    crate::companion_ui::track_and_show(target_hwnd, pt);
                } else {
                    crate::log_debug("[TARGET] No application window matched taskbar button under cursor.");
                }
            }
        }
    }

    CallNextHookEx(None, n_code, wparam, lparam)
}

pub fn is_point_over_taskbar(pt: POINT) -> bool {
    unsafe {
        let taskbar_classes = ["Shell_TrayWnd\0", "Shell_SecondaryTrayWnd\0"];
        for class in &taskbar_classes {
            let utf16: Vec<u16> = class.encode_utf16().collect();
            let hwnd = FindWindowW(windows::core::PCWSTR(utf16.as_ptr()), None).unwrap_or(HWND(std::ptr::null_mut()));
            if !hwnd.0.is_null() {
                let mut rect: RECT = std::mem::zeroed();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    if pt.x >= rect.left && pt.x < rect.right && pt.y >= rect.top && pt.y < rect.bottom {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub fn is_shell_or_taskbar_hwnd(hwnd: HWND) -> bool {
    unsafe {
        let mut class_buf = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class_buf);
        if len > 0 {
            let class_str = String::from_utf16_lossy(&class_buf[..len as usize]);
            if class_str == "Shell_TrayWnd"
                || class_str == "Shell_SecondaryTrayWnd"
                || class_str == "Windows.UI.Composition.DesktopWindowTarget"
                || class_str == "DesktopWindowXamlSource"
                || class_str == "Progman"
                || class_str == "WorkerW"
                || class_str == "WindowsDisplaySwapperCompanion"
                || class_str == "WindowsDisplaySwapperSubmenu"
            {
                return true;
            }
        }
    }
    false
}

pub unsafe fn get_process_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 512];
    let mut size = 512u32;
    let ok = QueryFullProcessImageNameW(
        proc,
        PROCESS_NAME_FORMAT(0),
        windows::core::PWSTR(buf.as_mut_ptr()),
        &mut size,
    );
    let _ = CloseHandle(proc);
    if ok.is_ok() && size > 0 {
        let full_path = String::from_utf16_lossy(&buf[..size as usize]);
        let exe_name = full_path.rsplit('\\').next().unwrap_or(&full_path);
        let clean = exe_name.strip_suffix(".exe").unwrap_or(exe_name);
        Some(clean.to_lowercase())
    } else {
        None
    }
}

pub unsafe fn get_process_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 512];
    let mut size = 512u32;
    let ok = QueryFullProcessImageNameW(
        proc,
        PROCESS_NAME_FORMAT(0),
        windows::core::PWSTR(buf.as_mut_ptr()),
        &mut size,
    );
    let _ = CloseHandle(proc);
    if ok.is_ok() && size > 0 {
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    } else {
        None
    }
}


pub fn build_search_terms(raw_name: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let lower = raw_name.to_lowercase();

    // Strip " - N running window(s)" or " - running window"
    let cleaned = if let Some(idx) = lower.find(" running window") {
        let before = &lower[..idx];
        if let Some(dash_idx) = before.rfind(" - ") {
            before[..dash_idx].trim()
        } else {
            before.trim()
        }
    } else {
        lower.trim()
    };

    let cleaned = cleaned.trim_matches(|c: char| c.is_whitespace() || c == '-').trim();

    if !cleaned.is_empty() {
        terms.push(cleaned.to_string());

        // Also add individual words if >= 3 letters, excluding noise words
        for word in cleaned.split(|c: char| !c.is_alphanumeric()) {
            let w = word.trim();
            if w.len() >= 3 && w != "window" && w != "windows" && w != "the" && w != "and" && w != "app" {
                if !terms.contains(&w.to_string()) {
                    terms.push(w.to_string());
                }
            }
        }
    }

    terms
}

fn find_target_window_from_point(pt: POINT) -> Option<HWND> {
    crate::log_debug(&format!("find_target_window_from_point at ({}, {})", pt.x, pt.y));
    unsafe {
        // 1. Try using UI Automation to inspect the element under cursor
        let automation: Result<IUIAutomation, _> = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER);
        if let Ok(uia) = automation {
            if let Ok(element) = uia.ElementFromPoint(pt) {
                let name = element.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                let auto_id = element.CurrentAutomationId().map(|b| b.to_string()).unwrap_or_default();
                let class_name = element.CurrentClassName().map(|b| b.to_string()).unwrap_or_default();
                let raw_h = element.CurrentNativeWindowHandle().map(|h| h.0 as isize).unwrap_or(0);
                crate::log_debug(&format!("UIA Element: Name='{}', AutoId='{}', Class='{}', NativeHWND=0x{:X}", name, auto_id, class_name, raw_h));

                // Check if element has a native window handle that is an actual external app (NOT explorer/shell)
                if raw_h != 0 {
                    let h = HWND(raw_h as *mut _);
                    let mut pid = 0;
                    GetWindowThreadProcessId(h, Some(&mut pid));
                    let proc = get_process_name(pid).unwrap_or_default();
                    if proc != "explorer" && !is_shell_or_taskbar_hwnd(h) && IsWindow(h).as_bool() && IsWindowVisible(h).as_bool() {
                        crate::log_debug(&format!("Direct non-explorer HWND match: 0x{:X} (proc='{}')", raw_h, proc));
                        return Some(h);
                    } else {
                        crate::log_debug(&format!("NativeHWND 0x{:X} belongs to proc='{}' (ignored as shell)", raw_h, proc));
                    }
                }

                // Query element Name (e.g. "Antigravity IDE - 1 running window", "Windows PowerShell", "Phone Link")
                if !name.is_empty() {
                    let terms = build_search_terms(&name);
                    crate::log_debug(&format!("Search terms from name: {:?}", terms));
                    if let Some(hwnd) = find_window_by_terms(&terms) {
                        crate::log_debug(&format!("Found window by name terms: 0x{:X}", hwnd.0 as usize));
                        return Some(hwnd);
                    }
                }

                // Check element AutomationId
                if !auto_id.is_empty() {
                    let terms = build_search_terms(&auto_id);
                    crate::log_debug(&format!("Search terms from auto_id: {:?}", terms));
                    if let Some(hwnd) = find_window_by_terms(&terms) {
                        crate::log_debug(&format!("Found window by auto_id terms: 0x{:X}", hwnd.0 as usize));
                        return Some(hwnd);
                    }
                }
            } else {
                crate::log_debug("uia.ElementFromPoint returned Err");
            }
        } else {
            crate::log_debug("CoCreateInstance(CUIAutomation) failed");
        }

        // 2. Fallback: Check the foreground window if it is an actual app (not taskbar/shell)
        let fg = GetForegroundWindow();
        if !fg.0.is_null() && !is_shell_or_taskbar_hwnd(fg) && IsWindowVisible(fg).as_bool() {
            let mut pid = 0;
            GetWindowThreadProcessId(fg, Some(&mut pid));
            let proc = get_process_name(pid).unwrap_or_default();
            if proc != "explorer" {
                crate::log_debug(&format!("Fallback to foreground app: 0x{:X} (proc='{}')", fg.0 as usize, proc));
                return Some(fg);
            }
        }
    }
    crate::log_debug("find_target_window_from_point returned NONE");
    None
}

struct WindowSearchContext {
    search_terms: Vec<String>,
    found_hwnd: Option<HWND>,
}

fn find_window_by_terms(terms: &[String]) -> Option<HWND> {
    let mut context = WindowSearchContext {
        search_terms: terms.to_vec(),
        found_hwnd: None,
    };

    unsafe {
        let lparam = LPARAM(&mut context as *mut _ as isize);
        let _ = EnumWindows(Some(enum_windows_proc), lparam);
    }

    context.found_hwnd
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut WindowSearchContext);

    if !IsWindow(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }

    if is_shell_or_taskbar_hwnd(hwnd) {
        return BOOL(1);
    }

    // Filter out tool windows or invisible shell windows
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
        return BOOL(1);
    }

    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let proc_name = get_process_name(pid).unwrap_or_default();

    let len = GetWindowTextLengthW(hwnd);
    let title = if len > 0 {
        let mut buffer = vec![0u16; (len + 1) as usize];
        let read = GetWindowTextW(hwnd, &mut buffer);
        if read > 0 {
            String::from_utf16_lossy(&buffer[..read as usize]).to_lowercase()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    if title.is_empty() && proc_name.is_empty() {
        return BOOL(1);
    }

    for term in &ctx.search_terms {
        // Match in window title
        if !title.is_empty() && (title.contains(term) || term.contains(&title)) {
            ctx.found_hwnd = Some(hwnd);
            return BOOL(0);
        }

        // Match in process executable name (e.g. "powershell", "phoneexperiencehost", "code")
        if !proc_name.is_empty() && (proc_name.contains(term) || term.contains(&proc_name)) {
            ctx.found_hwnd = Some(hwnd);
            return BOOL(0);
        }
    }

    BOOL(1)
}
