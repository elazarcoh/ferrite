//! Surface detection: enumerate visible window top-edges as walking platforms.

use crate::window::wndproc;
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, IsChild, IsIconic, IsWindowVisible, WindowFromPoint,
    },
};

/// One entry in the surface cache. Stores the raw rect of a qualifying window
/// plus the HWND so the occlusion check can be performed at fill time.
/// `hwnd` is not public — it's an implementation detail of the fill pass.
#[derive(Clone)]
pub struct SurfaceRect {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    hwnd: HWND,
}

/// 250 ms TTL cache for walkable surface rects.
/// Rects are filtered for visibility and occlusion at fill time (`EnumWindows`).
/// Cache-hit path re-applies per-call overlap and `min_surface` checks only —
/// occlusion is intentionally skipped on hits (acceptable TTL trade-off).
///
/// `Default` produces an already-expired cache so the first `find_floor` call
/// always triggers a fresh `EnumWindows`.
pub struct SurfaceCache {
    entries: Vec<SurfaceRect>,
    pub expires_at: Instant,
}

impl Default for SurfaceCache {
    fn default() -> Self {
        SurfaceCache {
            entries: Vec::new(),
            expires_at: Instant::now() - Duration::from_secs(1), // already expired
        }
    }
}

impl SurfaceCache {
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

struct FillState {
    screen_w: i32,
    entries: Vec<SurfaceRect>,
}

unsafe extern "system" fn fill_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    if wndproc::is_pet_hwnd(hwnd) { return 1; }
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 { return 1; }
    let s = &mut *(lparam as *mut FillState);
    let mut rc: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rc);
    // Skip full-screen / maximised windows.
    if rc.right - rc.left >= s.screen_w - 10 && rc.top <= 0 { return 1; }
    // Occlusion check at fill time: sample the window at the horizontal midpoint
    // of this rect's top edge. If something else is in front, skip this rect.
    let check_x = (rc.left + rc.right) / 2;
    let top_at_pt = WindowFromPoint(POINT { x: check_x, y: rc.top });
    let visible = top_at_pt.is_null()
        || top_at_pt == hwnd
        || IsChild(hwnd, top_at_pt) != 0
        || wndproc::is_pet_hwnd(top_at_pt);
    if !visible { return 1; }
    s.entries.push(SurfaceRect {
        left: rc.left,
        right: rc.right,
        top: rc.top,
        bottom: rc.bottom,
        hwnd,
    });
    1
}

/// Returns the y-coordinate the pet top should be at when it lands on the
/// nearest surface below it. Falls back to the virtual screen ground.
///
/// `cache` is filled via `EnumWindows` on the first call (or after TTL expiry)
/// including full occlusion checks. Cache hits re-apply per-call overlap and
/// min_surface filters only; occlusion is skipped (acceptable 250 ms TTL trade-off).
pub fn find_floor(
    pet_x: i32,
    pet_y: i32,
    pet_w: i32,
    pet_h: i32,
    screen_w: i32,
    screen_h: i32,
    cache: &mut SurfaceCache,
) -> i32 {
    // Refresh cache if expired.
    if cache.is_expired() {
        let mut fill = FillState { screen_w, entries: Vec::new() };
        unsafe {
            EnumWindows(Some(fill_cb), &mut fill as *mut _ as LPARAM);
        }
        cache.entries = fill.entries;
        cache.expires_at = Instant::now() + Duration::from_millis(250);
    }

    let pet_left = pet_x;
    let pet_right = pet_x + pet_w;
    let pet_bottom = pet_y + pet_h;
    let min_surface = pet_bottom.max(pet_h);
    let virtual_ground_top = screen_h - 4;
    let mut best = virtual_ground_top;

    for rect in &cache.entries {
        // Re-apply per-call horizontal overlap filter.
        if pet_right <= rect.left || pet_left >= rect.right { continue; }
        // Re-apply min_surface filter.
        if rect.top < min_surface || rect.top >= best { continue; }
        // Occlusion already verified at fill time — skip WindowFromPoint here.
        best = rect.top;
    }

    best - pet_h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_cache_default_is_expired() {
        let cache = SurfaceCache::default();
        assert!(cache.is_expired(), "default cache must be expired so first call always re-fetches");
    }

    #[test]
    fn surface_cache_find_floor_returns_plausible_value() {
        let mut cache = SurfaceCache::default();
        let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) }; // SM_CXSCREEN
        let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) }; // SM_CYSCREEN
        // Pet at top of screen, 32x32
        let floor = find_floor(0, 0, 32, 32, screen_w, screen_h, &mut cache);
        // Floor must be above the screen bottom and >= 0
        assert!(floor >= 0, "floor y must be non-negative, got {floor}");
        assert!(floor < screen_h, "floor y must be above screen bottom, got {floor}");
    }

    #[test]
    fn surface_cache_warm_returns_same_result() {
        let mut cache = SurfaceCache::default();
        let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
        let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
        // First call: cold (fills cache)
        let floor1 = find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache);
        assert!(!cache.is_expired(), "cache must be warm after first call");
        // Second call: warm (must return same value as long as pet position unchanged)
        let floor2 = find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache);
        assert_eq!(floor1, floor2, "warm cache must return same floor as cold call");
    }
}
