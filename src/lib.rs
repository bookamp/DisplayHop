pub mod companion_ui;
pub mod monitor;
pub mod mover;
pub mod taskbar;
pub mod tray;

pub fn log_debug(msg: &str) {
    use std::io::Write;
    use windows::Win32::System::SystemInformation::GetLocalTime;

    let timestamp = unsafe {
        let st = GetLocalTime();
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond, st.wMilliseconds
        )
    };

    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("e:\\Projects\\Window-Display-Swapper\\debug.log")
    {
        let _ = writeln!(f, "[{}] {}", timestamp, msg);
    }
}
