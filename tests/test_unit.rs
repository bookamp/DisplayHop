use window_display_swapper::companion_ui::{
    calculate_submenu_position, font_size_for_dpi, scale_dpi,
};
use window_display_swapper::monitor::MonitorInfo;
use window_display_swapper::mover::calculate_target_window_bounds;
use window_display_swapper::taskbar::build_search_terms;
use windows::Win32::Foundation::{POINT, RECT};

// ============================================================================
// 1. Taskbar Search Term Parsing & Tokenization Tests
// ============================================================================

#[test]
fn test_build_search_terms_stripping_single_window_suffix() {
    let raw = "Visual Studio Code - 1 running window";
    let terms = build_search_terms(raw);

    assert!(
        terms.contains(&"visual studio code".to_string()),
        "Must contain stripped full name without running window suffix"
    );
    assert!(
        terms.contains(&"code".to_string()),
        "Must contain significant token 'code'"
    );
    assert!(
        terms.contains(&"visual".to_string()),
        "Must contain significant token 'visual'"
    );
    assert!(
        terms.contains(&"studio".to_string()),
        "Must contain significant token 'studio'"
    );
}

#[test]
fn test_build_search_terms_stripping_multiple_windows_suffix() {
    let raw = "Google Chrome - 5 running windows";
    let terms = build_search_terms(raw);

    assert_eq!(terms[0], "google chrome");
    assert!(terms.contains(&"google".to_string()));
    assert!(terms.contains(&"chrome".to_string()));
}

#[test]
fn test_build_search_terms_filtering_noise_words() {
    let raw = "The Window App and Browser - 2 running windows";
    let terms = build_search_terms(raw);

    // Full cleaned string without suffix
    assert_eq!(terms[0], "the window app and browser");
    // Noise words should NOT be separate tokens: "the", "window", "app", "and"
    assert!(
        !terms[1..].contains(&"the".to_string()),
        "Noise word 'the' should be excluded from token list"
    );
    assert!(
        !terms[1..].contains(&"window".to_string()),
        "Noise word 'window' should be excluded from token list"
    );
    assert!(
        !terms[1..].contains(&"app".to_string()),
        "Noise word 'app' should be excluded from token list"
    );
    assert!(
        !terms[1..].contains(&"and".to_string()),
        "Noise word 'and' should be excluded from token list"
    );
    // Meaningful token must exist
    assert!(terms.contains(&"browser".to_string()));
}

#[test]
fn test_build_search_terms_short_words_ignored() {
    let raw = "UI Go Rust - 1 running window";
    let terms = build_search_terms(raw);

    assert_eq!(terms[0], "ui go rust");
    // "ui" (2 chars) and "go" (2 chars) are < 3 characters
    assert!(!terms[1..].contains(&"ui".to_string()));
    assert!(!terms[1..].contains(&"go".to_string()));
    assert!(terms.contains(&"rust".to_string()));
}

#[test]
fn test_build_search_terms_empty_and_whitespace() {
    assert!(build_search_terms("").is_empty());
    assert!(build_search_terms("   ").is_empty());
    assert!(build_search_terms(" - running window").is_empty());
}

#[test]
fn test_build_search_terms_no_suffix() {
    let raw = "Windows PowerShell";
    let terms = build_search_terms(raw);

    assert_eq!(terms[0], "windows powershell");
    assert!(terms.contains(&"powershell".to_string()));
    // "windows" is filtered as noise
    assert!(!terms[1..].contains(&"windows".to_string()));
}

#[test]
fn test_build_search_terms_deduplication() {
    let raw = "Code Code Code - 1 running window";
    let terms = build_search_terms(raw);

    let count = terms.iter().filter(|&t| t == "code").count();
    assert_eq!(count, 1, "Duplicate tokens should be deduplicated");
}

// ============================================================================
// 2. Monitor Point Inclusion Tests (contains_point)
// ============================================================================

#[test]
fn test_monitor_contains_point_standard_monitor() {
    let monitor = MonitorInfo {
        hmonitor: 1,
        device_name: "\\\\.\\DISPLAY1".to_string(),
        display_label: "Display 1 (1920x1080) - Primary".to_string(),
        rect: RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        work_area: RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        },
        is_primary: true,
        width: 1920,
        height: 1080,
    };

    // Center
    assert!(monitor.contains_point(POINT { x: 960, y: 540 }));
    // Top-left corner (inclusive)
    assert!(monitor.contains_point(POINT { x: 0, y: 0 }));
    // Just inside bottom-right
    assert!(monitor.contains_point(POINT { x: 1919, y: 1079 }));
    // Bottom-right edge (exclusive)
    assert!(!monitor.contains_point(POINT { x: 1920, y: 1080 }));
    // Outside left
    assert!(!monitor.contains_point(POINT { x: -1, y: 500 }));
    // Outside top
    assert!(!monitor.contains_point(POINT { x: 500, y: -1 }));
    // Outside right
    assert!(!monitor.contains_point(POINT { x: 1921, y: 500 }));
    // Outside bottom
    assert!(!monitor.contains_point(POINT { x: 500, y: 1081 }));
}

#[test]
fn test_monitor_contains_point_negative_coordinates() {
    // Secondary monitor positioned to the left of the primary screen
    let left_monitor = MonitorInfo {
        hmonitor: 2,
        device_name: "\\\\.\\DISPLAY2".to_string(),
        display_label: "Display 2 (1920x1080)".to_string(),
        rect: RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        },
        work_area: RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        },
        is_primary: false,
        width: 1920,
        height: 1080,
    };

    // Inside left monitor
    assert!(left_monitor.contains_point(POINT { x: -1000, y: 500 }));
    // Top-left corner of left monitor
    assert!(left_monitor.contains_point(POINT { x: -1920, y: 0 }));
    // Just inside right edge of left monitor
    assert!(left_monitor.contains_point(POINT { x: -1, y: 500 }));
    // Boundary between monitors: x=0 belongs to primary, NOT left monitor
    assert!(!left_monitor.contains_point(POINT { x: 0, y: 500 }));
    // Past left boundary
    assert!(!left_monitor.contains_point(POINT { x: -1921, y: 500 }));

    // Secondary monitor positioned above primary screen
    let top_monitor = MonitorInfo {
        hmonitor: 3,
        device_name: "\\\\.\\DISPLAY3".to_string(),
        display_label: "Display 3 (2560x1440)".to_string(),
        rect: RECT {
            left: 0,
            top: -1440,
            right: 2560,
            bottom: 0,
        },
        work_area: RECT {
            left: 0,
            top: -1440,
            right: 2560,
            bottom: 0,
        },
        is_primary: false,
        width: 2560,
        height: 1440,
    };

    assert!(top_monitor.contains_point(POINT { x: 1280, y: -720 }));
    assert!(top_monitor.contains_point(POINT { x: 0, y: -1440 }));
    assert!(top_monitor.contains_point(POINT { x: 2559, y: -1 }));
    assert!(!top_monitor.contains_point(POINT { x: 1280, y: 0 }));
    assert!(!top_monitor.contains_point(POINT { x: 1280, y: -1441 }));
}

// ============================================================================
// 3. DPI Scaling & Font Metrics Tests
// ============================================================================

#[test]
fn test_scale_dpi_standard_scaling() {
    // 96 DPI = 100% scaling
    assert_eq!(scale_dpi(100, 96), 100);
    assert_eq!(scale_dpi(4, 96), 4);
    assert_eq!(scale_dpi(240, 96), 240);
    assert_eq!(scale_dpi(0, 96), 0);
}

#[test]
fn test_scale_dpi_high_dpi() {
    // 120 DPI = 125% scaling (100 * 1.25 = 125)
    assert_eq!(scale_dpi(100, 120), 125);
    assert_eq!(scale_dpi(240, 120), 300);

    // 144 DPI = 150% scaling (100 * 1.50 = 150)
    assert_eq!(scale_dpi(100, 144), 150);
    assert_eq!(scale_dpi(240, 144), 360);

    // 192 DPI = 200% scaling (100 * 2.0 = 200)
    assert_eq!(scale_dpi(100, 192), 200);
    assert_eq!(scale_dpi(240, 192), 480);
}

#[test]
fn test_font_size_for_dpi() {
    // Formula: -((point_size * dpi + 36) / 72)
    // 9pt font at 96 DPI: -((9 * 96 + 36) / 72) = -(900 / 72) = -12
    assert_eq!(font_size_for_dpi(9, 96), -12);

    // 9pt font at 144 DPI (150%): -((9 * 144 + 36) / 72) = -(1332 / 72) = -18
    assert_eq!(font_size_for_dpi(9, 144), -18);

    // 10pt font at 96 DPI: -((10 * 96 + 36) / 72) = -(996 / 72) = -13
    assert_eq!(font_size_for_dpi(10, 96), -13);

    // 10pt font at 192 DPI (200%): -((10 * 192 + 36) / 72) = -(1956 / 72) = -27
    assert_eq!(font_size_for_dpi(10, 192), -27);
}

// ============================================================================
// 4. Submenu Placement & Boundary Clamping Tests
// ============================================================================

#[test]
fn test_calculate_submenu_position_normal_placement() {
    // Companion bar on left side of 1080p screen: plenty of space to the right
    let comp_rect = RECT {
        left: 300,
        top: 600,
        right: 540,
        bottom: 638,
    };
    let sub_w = 290;
    let mon_right = 1920;
    let dpi = 96;

    let (sub_x, sub_y) = calculate_submenu_position(comp_rect, sub_w, mon_right, dpi);

    // sub_x should be to the right of the companion bar: right + scale_dpi(4, 96) = 540 + 4 = 544
    assert_eq!(sub_x, 544);
    // sub_y should align with companion bar top
    assert_eq!(sub_y, 600);
}

#[test]
fn test_calculate_submenu_position_flips_left_on_overflow() {
    // Companion bar right near the right screen edge
    let comp_rect = RECT {
        left: 1650,
        top: 600,
        right: 1890,
        bottom: 638,
    };
    let sub_w = 290;
    let mon_right = 1920;
    let dpi = 96;

    let (sub_x, sub_y) = calculate_submenu_position(comp_rect, sub_w, mon_right, dpi);

    // If placed on the right: 1890 + 4 + 290 = 2184 > 1920 - 10 (1910).
    // Must flip to left: left - sub_w - scale_dpi(4, 96) = 1650 - 290 - 4 = 1356
    assert_eq!(sub_x, 1356);
    assert_eq!(sub_y, 600);
}

#[test]
fn test_calculate_submenu_position_high_dpi_flip() {
    let comp_rect = RECT {
        left: 3300,
        top: 800,
        right: 3660,
        bottom: 857,
    };
    let sub_w = 435; // 290 scaled to 150%
    let mon_right = 3840;
    let dpi = 144; // 150%

    let (sub_x, sub_y) = calculate_submenu_position(comp_rect, sub_w, mon_right, dpi);

    // scale_dpi(4, 144) = 6
    // sub_x right = 3660 + 6 = 3666 + 435 = 4101 > 3840 - 15 = 3825 -> Flips!
    // sub_x flipped = 3300 - 435 - 6 = 2859
    assert_eq!(sub_x, 2859);
    assert_eq!(sub_y, 800);
}

// ============================================================================
// 5. Window Mover Target Coordinate Math Tests
// ============================================================================

#[test]
fn test_calculate_target_window_bounds_proportional_move() {
    // Monitor 1: 1920x1040 work area
    let current_work = Some(RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    });
    // Target Monitor: 1920x1040 work area starting at x=1920 (Monitor 2)
    let target_work = RECT {
        left: 1920,
        top: 0,
        right: 3840,
        bottom: 1040,
    };

    // Window located at (200, 150), size 800x600
    let window_rect = RECT {
        left: 200,
        top: 150,
        right: 1000,
        bottom: 750,
    };

    let (target_x, target_y, target_w, target_h) =
        calculate_target_window_bounds(window_rect, current_work, target_work);

    // Size should be preserved since it comfortably fits target monitor
    assert_eq!(target_w, 800);
    assert_eq!(target_h, 600);
    // Position should be shifted onto target monitor
    assert_eq!(target_x, 1920 + 200);
    assert_eq!(target_y, 150);
}

#[test]
fn test_calculate_target_window_bounds_downscaling_for_smaller_monitor() {
    // Source: 4K monitor (3840x2120 work area)
    let current_work = Some(RECT {
        left: 0,
        top: 0,
        right: 3840,
        bottom: 2120,
    });
    // Target: 1080p monitor (1920x1040 work area)
    let target_work = RECT {
        left: 3840,
        top: 0,
        right: 5760,
        bottom: 1040,
    };

    // Huge 4K window (3000x1800)
    let window_rect = RECT {
        left: 200,
        top: 100,
        right: 3200,
        bottom: 1900,
    };

    let (target_x, target_y, target_w, target_h) =
        calculate_target_window_bounds(window_rect, current_work, target_work);

    // Window size must be clamped to target_work_w - 40 = 1920 - 40 = 1880
    assert_eq!(target_w, 1880);
    // Window height must be clamped to target_work_h - 40 = 1040 - 40 = 1000
    assert_eq!(target_h, 1000);

    // Window must stay strictly within target monitor work area bounds
    assert!(target_x >= target_work.left);
    assert!(target_x + target_w <= target_work.right);
    assert!(target_y >= target_work.top);
    assert!(target_y + target_h <= target_work.bottom);
}

#[test]
fn test_calculate_target_window_bounds_edge_clamping() {
    // Window partially off-screen or right at the bottom-right corner of source monitor
    let current_work = Some(RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    });
    let target_work = RECT {
        left: 1920,
        top: 0,
        right: 3840,
        bottom: 1040,
    };

    let window_rect = RECT {
        left: 1700,
        top: 900,
        right: 2500, // extends past source right
        bottom: 1500, // extends past source bottom
    };

    let (target_x, target_y, target_w, target_h) =
        calculate_target_window_bounds(window_rect, current_work, target_work);

    // Must be clamped so it doesn't overflow target right or bottom
    assert!(target_x + target_w <= target_work.right);
    assert!(target_y + target_h <= target_work.bottom);
    assert!(target_x >= target_work.left);
    assert!(target_y >= target_work.top);
}

#[test]
fn test_calculate_target_window_bounds_negative_coordinates() {
    let current_work = Some(RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    });
    // Target monitor positioned to the left (negative X coordinates)
    let target_work = RECT {
        left: -1920,
        top: 0,
        right: 0,
        bottom: 1040,
    };

    let window_rect = RECT {
        left: 100,
        top: 100,
        right: 900,
        bottom: 700,
    };

    let (target_x, target_y, target_w, target_h) =
        calculate_target_window_bounds(window_rect, current_work, target_work);

    assert!(target_x >= target_work.left);
    assert!(target_x + target_w <= target_work.right);
    assert!(target_y >= target_work.top);
    assert!(target_y + target_h <= target_work.bottom);
}

#[test]
fn test_calculate_target_window_bounds_fallback_no_current_work_area() {
    let current_work = None;
    let target_work = RECT {
        left: 1920,
        top: 0,
        right: 3840,
        bottom: 1040,
    };

    let window_rect = RECT {
        left: 0,
        top: 0,
        right: 800,
        bottom: 600,
    };

    let (target_x, target_y, target_w, target_h) =
        calculate_target_window_bounds(window_rect, current_work, target_work);

    // Fallback places at target.left + 50, target.top + 50
    assert_eq!(target_x, 1920 + 50);
    assert_eq!(target_y, 50);
    assert_eq!(target_w, 800);
    assert_eq!(target_h, 600);
}
