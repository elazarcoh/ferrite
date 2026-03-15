//! Surface detection: enumerate visible window top-edges as walking platforms.

use crate::window::wndproc;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, IsChild, IsIconic, IsWindowVisible, WindowFromPoint,
    },
};

struct FindState {
    pet_left: i32,
    pet_right: i32,
    /// Current bottom of the pet (pet_y + pet_h).
    pet_bottom: i32,
    /// Pet height (pixels after scale). Used to ensure landing keeps pet on-screen.
    pet_h: i32,
    screen_w: i32,
    /// Minimum surface top found so far that is >= min_surface.
    best: i32,
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    // Skip our own pet windows.
    if wndproc::is_pet_hwnd(hwnd) {
        return 1;
    }
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
        return 1;
    }
    let s = &mut *(lparam as *mut FindState);
    let mut rc: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rc);

    // Skip full-screen / maximised windows (desktop, maximised apps):
    // their top edge is at or near y=0 and they span the full screen width.
    if rc.right - rc.left >= s.screen_w - 10 && rc.top <= 0 {
        return 1;
    }
    // Must have horizontal overlap with the pet footprint.
    if s.pet_right <= rc.left || s.pet_left >= rc.right {
        return 1;
    }
    // A surface is valid when:
    //   rc.top >= pet_bottom   — surface is at/below the pet's current feet
    //   rc.top >= pet_h        — landing (pet_y = rc.top − pet_h) keeps pet on-screen
    // Taking max of the two covers both the "pet above screen" case (pet_h wins)
    // and the normal "pet on-screen" case (pet_bottom wins).
    // This also implicitly excludes DWM invisible borders with negative rc.top.
    let min_surface = s.pet_bottom.max(s.pet_h);
    if rc.top >= min_surface && rc.top < s.best {
        // Visibility check: is this window's top edge actually visible (not occluded)?
        // Sample the topmost window at the pet's horizontal centre on this surface's top row.
        // If it is neither hwnd itself, a child of hwnd, nor one of our pet windows, then
        // another window is covering this surface — skip it.
        let check_x = (s.pet_left + s.pet_right) / 2;
        let top_at_pt = WindowFromPoint(POINT { x: check_x, y: rc.top });
        let visible = top_at_pt.is_null()
            || top_at_pt == hwnd
            || IsChild(hwnd, top_at_pt) != 0
            || wndproc::is_pet_hwnd(top_at_pt);
        if visible {
            s.best = rc.top;
        }
    }
    1
}

/// Returns the y-coordinate the pet top should be at when it lands on the
/// nearest surface below it.  Falls back to the virtual screen ground.
///
/// * `pet_x`, `pet_y` — pet window top-left
/// * `pet_w`, `pet_h` — scaled pet dimensions
/// * `screen_w`, `screen_h` — primary monitor size
pub fn find_floor(
    pet_x: i32,
    pet_y: i32,
    pet_w: i32,
    pet_h: i32,
    screen_w: i32,
    screen_h: i32,
) -> i32 {
    let pet_bottom = pet_y + pet_h;
    // Virtual screen ground: leave a small gap above the taskbar / screen edge.
    let virtual_ground_top = screen_h - 4;

    let mut state = FindState {
        pet_left: pet_x,
        pet_right: pet_x + pet_w,
        pet_bottom,
        pet_h,
        screen_w,
        best: virtual_ground_top,
    };

    unsafe {
        EnumWindows(Some(enum_cb), &mut state as *mut _ as LPARAM);
    }

    // pet_y when landed = surface_top - pet_h
    state.best - pet_h
}
