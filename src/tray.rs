use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::Shell::{
    IsUserAnAdmin, Shell_NotifyIconW, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, NIF_ICON,
    NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_WARNING,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, LoadImageW,
    PostQuitMessage, SetForegroundWindow, TrackPopupMenuEx, HICON, IDI_APPLICATION,
    IMAGE_ICON, LR_LOADFROMFILE,
    MF_CHECKED, MF_DISABLED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, TPM_RETURNCMD, WM_RBUTTONUP, WM_USER,
};

pub const WM_TRAY_ICON: u32 = WM_USER + 101;
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);

const IDM_EXIT: u32 = 2001;
const IDM_STARTUP_TOGGLE: u32 = 2002;
const IDM_VIEW_LOG: u32 = 2003;
const IDM_RESTART_ADMIN: u32 = 2004;

const REG_KEY_RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APP_REG_NAME: PCWSTR = w!("WindowDisplaySwapper");

const APP_ICON_BYTES: &[u8] = include_bytes!("../app.ico");

pub fn get_app_icon() -> HICON {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    let cache_dir = std::path::Path::new(&local_app_data).join("WindowDisplaySwapper");
    let _ = std::fs::create_dir_all(&cache_dir);
    let cache_ico = cache_dir.join("app.ico");
    let _ = std::fs::write(&cache_ico, APP_ICON_BYTES);

    let wide: Vec<u16> = cache_ico
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = LoadImageW(
            None,
            PCWSTR(wide.as_ptr()),
            IMAGE_ICON,
            32,
            32,
            LR_LOADFROMFILE,
        );
        if let Ok(h) = handle {
            if !h.0.is_null() {
                return HICON(h.0);
            }
        }
        LoadIconW(None, IDI_APPLICATION).unwrap_or(HICON(std::ptr::null_mut()))
    }
}

pub fn init_tray_icon(hwnd: HWND) {
    TRAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_ICON;
        nid.hIcon = get_app_icon();

        let tip = "Window Display Swapper\0";
        for (i, c) in tip.encode_utf16().enumerate() {
            if i < nid.szTip.len() {
                nid.szTip[i] = c;
            }
        }

        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

pub fn show_tray_notification(title: &str, message: &str) {
    let hwnd_val = TRAY_HWND.load(Ordering::SeqCst);
    if hwnd_val == 0 {
        return;
    }
    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = HWND(hwnd_val as *mut _);
        nid.uID = 1;
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = NIIF_WARNING;

        for (i, c) in title.encode_utf16().enumerate() {
            if i < nid.szInfoTitle.len() - 1 {
                nid.szInfoTitle[i] = c;
            }
        }
        for (i, c) in message.encode_utf16().enumerate() {
            if i < nid.szInfo.len() - 1 {
                nid.szInfo[i] = c;
            }
        }

        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

pub fn remove_tray_icon() {
    let hwnd_val = TRAY_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = HWND(hwnd_val as *mut _);
            nid.uID = 1;
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
}

pub fn handle_tray_click(hwnd: HWND, lparam: LPARAM) {
    let msg = lparam.0 as u32;
    if msg == WM_RBUTTONUP {
        show_tray_menu(hwnd);
    }
}

fn show_tray_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu().unwrap();

        // Title
        let is_elevated = IsUserAnAdmin().as_bool();
        let title_text = if is_elevated {
            w!("Window Display Swapper (Admin)")
        } else {
            w!("Window Display Swapper (Active)")
        };

        let _ = AppendMenuW(
            menu,
            MF_STRING | MF_DISABLED | MF_GRAYED,
            0,
            title_text,
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        if !is_elevated {
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                IDM_RESTART_ADMIN as usize,
                w!("🛡️ Restart as Administrator"),
            );
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        }

        // Startup checkbox
        let is_startup = is_run_at_startup();
        let startup_flag = if is_startup { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(
            menu,
            MF_STRING | startup_flag,
            IDM_STARTUP_TOGGLE as usize,
            w!("Start with Windows"),
        );

        let _ = AppendMenuW(
            menu,
            MF_STRING,
            IDM_VIEW_LOG as usize,
            w!("Open Debug Log"),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, IDM_EXIT as usize, w!("Exit"));

        let mut pt: POINT = std::mem::zeroed();
        let _ = GetCursorPos(&mut pt);

        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenuEx(
            menu,
            TPM_RETURNCMD.0 | TPM_BOTTOMALIGN.0 | TPM_LEFTALIGN.0,
            pt.x,
            pt.y,
            hwnd,
            None,
        );

        if cmd.0 as u32 == IDM_EXIT {
            remove_tray_icon();
            PostQuitMessage(0);
        } else if cmd.0 as u32 == IDM_STARTUP_TOGGLE {
            toggle_run_at_startup(!is_startup);
        } else if cmd.0 as u32 == IDM_VIEW_LOG {
            let _ = std::process::Command::new("notepad.exe")
                .arg("e:\\Projects\\Window-Display-Swapper\\debug.log")
                .spawn();
        } else if cmd.0 as u32 == IDM_RESTART_ADMIN {
            if let Ok(exe_path) = std::env::current_exe() {
                let path_str = exe_path.to_string_lossy().to_string();
                let _ = std::process::Command::new("powershell")
                    .args(["-NoProfile", "-Command", &format!("Start-Process '{}' -Verb RunAs", path_str)])
                    .spawn();
                remove_tray_icon();
                PostQuitMessage(0);
            }
        }

        let _ = DestroyMenu(menu);
    }
}

fn is_run_at_startup() -> bool {
    unsafe {
        let mut hkey = HKEY(std::ptr::null_mut());
        if RegOpenKeyExW(HKEY_CURRENT_USER, REG_KEY_RUN, 0, KEY_READ, &mut hkey).is_err() {
            return false;
        }

        let mut buf_size = 0;
        let res = RegQueryValueExW(
            hkey,
            APP_REG_NAME,
            None,
            None,
            None,
            Some(&mut buf_size),
        );

        let _ = RegCloseKey(hkey);
        res.is_ok() && buf_size > 0
    }
}

fn toggle_run_at_startup(enable: bool) {
    unsafe {
        let mut hkey = HKEY(std::ptr::null_mut());
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            REG_KEY_RUN,
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        ).is_ok() {
            if enable {
                if let Ok(exe_path) = std::env::current_exe() {
                    let path_str = exe_path.to_string_lossy().to_string();
                    let wide: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
                    let bytes = std::slice::from_raw_parts(
                        wide.as_ptr() as *const u8,
                        wide.len() * 2,
                    );
                    let _ = RegSetValueExW(
                        hkey,
                        APP_REG_NAME,
                        0,
                        REG_SZ,
                        Some(bytes),
                    );
                }
            } else {
                let _ = RegDeleteValueW(hkey, APP_REG_NAME);
            }
            let _ = RegCloseKey(hkey);
        }
    }
}
