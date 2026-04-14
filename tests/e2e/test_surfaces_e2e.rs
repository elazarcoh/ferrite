/// E2E: surface detection via find_floor with real Win32 windows.
///
/// SURFACE_LOCK serialises all tests in this module — Win32 Z-order and
/// WindowFromPoint are global and would produce flaky results if tests
/// create windows concurrently.
use ferrite::window::surfaces::find_floor;
use ferrite_core::geometry::PetGeom;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

static SURFACE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// ─── Test window class (registered once) ─────────────────────────────────────

const TEST_CLASS: &str = "SurfaceTestWindow\0";

unsafe extern "system" fn test_wnd_proc(
    hwnd: HWND, msg: u32, wp: usize, lp: isize,
) -> isize {
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn ensure_class_registered() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        let name: Vec<u16> = TEST_CLASS.encode_utf16().collect();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(test_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc); // returns 0 if already registered — that's fine
    });
}

/// Create a WS_EX_TOPMOST WS_POPUP window and show it immediately.
/// TOPMOST ensures WindowFromPoint at its rect returns this window, not a user app.
unsafe fn make_test_window(x: i32, y: i32, w: i32, h: i32) -> HWND {
    ensure_class_registered();
    let hinstance = GetModuleHandleW(std::ptr::null());
    let name: Vec<u16> = TEST_CLASS.encode_utf16().collect();
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST,
        name.as_ptr(),
        std::ptr::null(),
        WS_POPUP | WS_VISIBLE,
        x, y, w, h,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        hinstance,
        std::ptr::null(),
    );
    // Pump once so the WS_VISIBLE style is committed and the window is placed
    // in the TOPMOST Z-tier before we call WindowFromPoint on it.
    if !hwnd.is_null() {
        let mut msg: MSG = std::mem::zeroed();
        PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE);
    }
    hwnd
}

fn screen_dims() -> (i32, i32) {
    (
        unsafe { GetSystemMetrics(SM_CXSCREEN) },
        unsafe { GetSystemMetrics(SM_CYSCREEN) },
    )
}

fn get_rect(hwnd: HWND) -> RECT {
    let mut rc: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(hwnd, &mut rc) };
    rc
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    SURFACE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

// ─── detect window below pet ─────────────────────────────────────────────────

#[test]
fn find_floor_detects_window_below_pet() {
    let _g = lock();
    let (screen_w, screen_h) = screen_dims();

    let win_x = screen_w / 4;
    let win_y = screen_h / 2;
    let win_w = screen_w / 2;

    let hwnd = unsafe { make_test_window(win_x, win_y, win_w, 200) };
    assert!(!hwnd.is_null(), "CreateWindowExW failed");
    let rc = get_rect(hwnd);

    let pet_w = 32; let pet_h = 32;
    let pet_x = win_x + win_w / 4;
    let pet_y = rc.top - 100;

    let mut cache = ferrite::window::surfaces::SurfaceCache::default();
    let geom = PetGeom { x: pet_x, y: pet_y, w: pet_w, h: pet_h, baseline_offset: 0 };
    let floor = find_floor(&geom, screen_w, screen_h, &mut cache);
    unsafe { DestroyWindow(hwnd) };

    assert_eq!(
        floor, rc.top - pet_h,
        "floor should be win_top({}) - pet_h({}); got {}", rc.top, pet_h, floor
    );
}

// ─── pet stays on surface when standing (>= not >) ───────────────────────────

#[test]
fn find_floor_keeps_pet_on_surface_when_standing() {
    let _g = lock();
    let (screen_w, screen_h) = screen_dims();

    let win_x = screen_w / 4;
    let win_y = screen_h / 2;
    let win_w = screen_w / 2;

    let hwnd = unsafe { make_test_window(win_x, win_y, win_w, 200) };
    assert!(!hwnd.is_null());
    let rc = get_rect(hwnd);

    let pet_w = 32; let pet_h = 32;
    let pet_x = win_x + win_w / 4;
    let pet_y = rc.top - pet_h; // pet_bottom == win_top exactly

    let mut cache = ferrite::window::surfaces::SurfaceCache::default();
    let geom = PetGeom { x: pet_x, y: pet_y, w: pet_w, h: pet_h, baseline_offset: 0 };
    let floor = find_floor(&geom, screen_w, screen_h, &mut cache);
    unsafe { DestroyWindow(hwnd) };

    assert_eq!(
        floor, pet_y,
        "standing pet floor should equal pet_y ({}); got {}", pet_y, floor
    );
}

// ─── non-overlapping window is ignored (differential) ────────────────────────

#[test]
fn find_floor_ignores_non_overlapping_window() {
    let _g = lock();
    let (screen_w, screen_h) = screen_dims();

    // Pet on the far right; test window on the far left — no horizontal overlap.
    let pet_x = screen_w - 100;
    let pet_y = 0;
    let pet_w = 32; let pet_h = 32;

    let mut cache = ferrite::window::surfaces::SurfaceCache::default();
    let geom = PetGeom { x: pet_x, y: pet_y, w: pet_w, h: pet_h, baseline_offset: 0 };
    let floor_before = find_floor(&geom, screen_w, screen_h, &mut cache);

    let hwnd = unsafe { make_test_window(0, screen_h / 2, 50, 100) };
    assert!(!hwnd.is_null());

    let mut cache2 = ferrite::window::surfaces::SurfaceCache::default();
    let floor_after = find_floor(&geom, screen_w, screen_h, &mut cache2);
    unsafe { DestroyWindow(hwnd) };

    assert_eq!(
        floor_after, floor_before,
        "non-overlapping window must not change floor (before={floor_before}, after={floor_after})"
    );
}

// ─── occluded surface is skipped ─────────────────────────────────────────────

#[test]
fn find_floor_ignores_occluded_surface() {
    let _g = lock();
    let (screen_w, screen_h) = screen_dims();

    let pet_w = 32i32; let pet_h = 32i32;

    // Base window: the surface the pet would land on.
    let base_x = screen_w / 4;
    let base_y = screen_h / 2;
    let base_w = screen_w / 2;
    let base_hwnd = unsafe { make_test_window(base_x, base_y, base_w, 200) };
    assert!(!base_hwnd.is_null());
    let base_rc = get_rect(base_hwnd);

    let pet_x = base_x + base_w / 4;
    let pet_y = base_rc.top - 100;

    // Without cover: base surface must be visible and detected.
    let mut cache = ferrite::window::surfaces::SurfaceCache::default();
    let geom = PetGeom { x: pet_x, y: pet_y, w: pet_w, h: pet_h, baseline_offset: 0 };
    let floor_uncovered = find_floor(&geom, screen_w, screen_h, &mut cache);
    assert_eq!(
        floor_uncovered, base_rc.top - pet_h,
        "uncovered surface must be detected; expected {} got {}",
        base_rc.top - pet_h, floor_uncovered
    );

    // Cover window: sits over base's top edge. Both are TOPMOST; created after
    // base so it's higher in the TOPMOST Z-tier. WindowFromPoint at base_rc.top
    // will return cover, causing base to be skipped as occluded.
    let cover_y = base_rc.top - 10;
    let cover_hwnd = unsafe { make_test_window(base_x, cover_y, base_w, 30) };
    assert!(!cover_hwnd.is_null());

    let mut cache2 = ferrite::window::surfaces::SurfaceCache::default();
    let floor_covered = find_floor(&geom, screen_w, screen_h, &mut cache2);

    unsafe { DestroyWindow(cover_hwnd) };
    unsafe { DestroyWindow(base_hwnd) };

    assert_ne!(
        floor_covered, base_rc.top - pet_h,
        "covered surface must be skipped (floor={floor_covered}, base_top-pet_h={})",
        base_rc.top - pet_h
    );
}

// ─── window too close to screen top is not used as surface ───────────────────

#[test]
fn find_floor_ignores_window_too_close_to_screen_top() {
    let _g = lock();
    let (screen_w, screen_h) = screen_dims();

    let pet_w = 32i32; let pet_h = 32i32;
    // Window at y=4: floor_y would be 4-32=-28 (off-screen). Must be excluded.
    let win_x = screen_w / 4;
    let win_w = screen_w / 2;
    let hwnd = unsafe { make_test_window(win_x, 4, win_w, 200) };
    assert!(!hwnd.is_null());
    let rc = get_rect(hwnd);

    let pet_x = win_x + win_w / 4;
    let pet_y = -100;
    let mut cache = ferrite::window::surfaces::SurfaceCache::default();
    let geom = PetGeom { x: pet_x, y: pet_y, w: pet_w, h: pet_h, baseline_offset: 0 };
    let floor = find_floor(&geom, screen_w, screen_h, &mut cache);
    unsafe { DestroyWindow(hwnd) };

    assert!(
        floor >= 0,
        "window at y={} must not produce off-screen floor; got {}",
        rc.top, floor
    );
}
