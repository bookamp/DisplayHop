#![windows_subsystem = "windows"]

use window_display_swapper::{companion_ui, monitor, taskbar, tray, log_debug};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HINSTANCE, HWND, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, ReleaseMutex, TerminateProcess, WaitForSingleObject,
    PROCESS_TERMINATE, PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GetMessageW,
    GetWindowThreadProcessId, PostMessageW, PostQuitMessage, RegisterClassW, TranslateMessage,
    MSG, WNDCLASSW, WM_CLOSE, WM_DESTROY, WM_DISPLAYCHANGE,
};

const CLASS_MAIN_WINDOW: PCWSTR = w!("WindowDisplaySwapperMain");

fn main() {
    log_debug("=== WindowDisplaySwapper Started ===");
    unsafe {
        // Set DPI awareness for sharp text and accurate coordinate math across ultrawide/secondary screens
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // Find and cleanly replace any existing running instance
        let existing_wnd = FindWindowW(CLASS_MAIN_WINDOW, None).unwrap_or(HWND(std::ptr::null_mut()));
        if !existing_wnd.0.is_null() {
            log_debug("Existing background instance detected. Replacing it with new instance...");
            let mut old_pid = 0;
            GetWindowThreadProcessId(existing_wnd, Some(&mut old_pid));

            // Post WM_CLOSE to gracefully shutdown previous instance
            let _ = PostMessageW(existing_wnd, WM_CLOSE, WPARAM(0), LPARAM(0));

            // Wait up to 1000ms for old process to exit cleanly, or terminate if hung
            if old_pid != 0 {
                if let Ok(old_proc) = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, old_pid) {
                    let wait = WaitForSingleObject(old_proc, 1000);
                    if wait != WAIT_OBJECT_0 {
                        log_debug("Previous instance did not exit in 1000ms, terminating...");
                        let _ = TerminateProcess(old_proc, 0);
                    }
                    let _ = CloseHandle(old_proc);
                }
            }
        }

        // Single-instance mutex lock
        let mutex_name = w!("Global\\WindowDisplaySwapper_SingleInstanceMutex");
        let mutex = CreateMutexW(None, true, mutex_name);
        if windows::Win32::Foundation::GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS {
            if let Ok(m) = mutex {
                let _ = WaitForSingleObject(m, 1200);
            }
        }

        let instance = HINSTANCE(GetModuleHandleW(None).unwrap().0);

        // Register message-only window class
        let wc = WNDCLASSW {
            lpfnWndProc: Some(main_wnd_proc),
            hInstance: instance,
            hIcon: tray::get_app_icon(),
            lpszClassName: CLASS_MAIN_WINDOW,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        // Create message window for tray icon and display change events
        let hwnd = CreateWindowExW(
            Default::default(),
            CLASS_MAIN_WINDOW,
            w!("Window Display Swapper Daemon"),
            Default::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            None,
        ).unwrap();

        // Initialize Tray Icon
        tray::init_tray_icon(hwnd);

        // Initialize Companion Entry UI
        companion_ui::init_companion_window();

        // Install Taskbar Mouse & WinEvent hooks
        taskbar::init_taskbar_hooks();

        // Run Windows Message Loop
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup on exit
        taskbar::cleanup_taskbar_hooks();
        tray::remove_tray_icon();
        if let Ok(m) = mutex {
            let _ = ReleaseMutex(m);
        }
    }
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        tray::WM_TRAY_ICON => {
            tray::handle_tray_click(hwnd, lparam);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            // Monitor plugged/unplugged or resolution changed; refresh
            let _ = monitor::get_all_monitors();
            LRESULT(0)
        }
        WM_CLOSE => {
            log_debug("Daemon received WM_CLOSE, shutting down cleanly.");
            tray::remove_tray_icon();
            taskbar::cleanup_taskbar_hooks();
            let _ = DestroyWindow(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_DESTROY => {
            tray::remove_tray_icon();
            taskbar::cleanup_taskbar_hooks();
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
