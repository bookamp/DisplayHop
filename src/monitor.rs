use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow,
    HDC, HMONITOR, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
};

const MONITORINFOF_PRIMARY: u32 = 1;

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub hmonitor: isize,
    #[allow(dead_code)]
    pub device_name: String,
    pub display_label: String,
    pub rect: RECT,
    pub work_area: RECT,
    pub is_primary: bool,
    pub width: i32,
    pub height: i32,
}

impl MonitorInfo {
    pub fn contains_point(&self, pt: POINT) -> bool {
        pt.x >= self.rect.left
            && pt.x < self.rect.right
            && pt.y >= self.rect.top
            && pt.y < self.rect.bottom
    }
}

pub fn get_all_monitors() -> Vec<MonitorInfo> {
    let mut monitors = Vec::new();
    let lparam = LPARAM(&mut monitors as *mut Vec<MonitorInfo> as isize);

    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum_proc),
            lparam,
        );
    }

    // Sort: Primary first, then by X position
    monitors.sort_by(|a: &MonitorInfo, b: &MonitorInfo| {
        if a.is_primary != b.is_primary {
            b.is_primary.cmp(&a.is_primary)
        } else {
            a.rect.left.cmp(&b.rect.left)
        }
    });

    monitors
}

unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let list = &mut *(lparam.0 as *mut Vec<MonitorInfo>);

    let mut info: MONITORINFOEXW = std::mem::zeroed();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _).as_bool() {
        let is_primary = (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0;
        let rect = info.monitorInfo.rcMonitor;
        let work_area = info.monitorInfo.rcWork;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let dev_name = String::from_utf16_lossy(
            &info.szDevice[..info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len())]
        );

        let display_label = if is_primary {
            format!("Display {} ({}x{}) - Primary", list.len() + 1, width, height)
        } else {
            format!("Display {} ({}x{})", list.len() + 1, width, height)
        };

        list.push(MonitorInfo {
            hmonitor: hmonitor.0 as isize,
            device_name: dev_name,
            display_label,
            rect,
            work_area,
            is_primary,
            width,
            height,
        });
    }

    BOOL(1)
}

pub fn get_monitor_for_window(hwnd: HWND) -> Option<isize> {
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if hmon.0.is_null() {
            None
        } else {
            Some(hmon.0 as isize)
        }
    }
}

#[allow(dead_code)]
pub fn get_monitor_at_point(pt: POINT) -> Option<isize> {
    unsafe {
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if hmon.0.is_null() {
            None
        } else {
            Some(hmon.0 as isize)
        }
    }
}
