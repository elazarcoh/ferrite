//! Surface detection: enumerate visible window top-edges as walking platforms.

use crate::window::wndproc;
use ferrite_core::geometry::{PetGeom, PlatformBounds};
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, IsChild, IsIconic, IsWindowVisible, WindowFromPoint,
    },
};

/// Result of `find_floor_info` — floor y-coordinate plus surface metadata.
/// `surface_w == 0.0` and `surface_label == ""` when pet rests on the virtual screen ground.
pub struct SurfaceHit {
    pub floor_y: i32,
    pub surface_w: f32,
    pub surface_label: String,
}

/// One entry in the surface cache. Stores the raw rect of a qualifying window
/// plus a classification label computed at fill time.
#[derive(Clone)]
pub struct SurfaceRect {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    label: &'static str, // "taskbar" or "window"
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
    expires_at: Instant,
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

unsafe extern "system" fn fill_cb(hwnd: HWND, lparam: LPARAM) -> i32 { unsafe {
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
    let label = if is_taskbar_hwnd(hwnd) { "taskbar" } else { "window" };
    s.entries.push(SurfaceRect {
        left: rc.left,
        right: rc.right,
        top: rc.top,
        bottom: rc.bottom,
        label,
    });
    1
}}

fn is_taskbar_hwnd(hwnd: HWND) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        return false;
    }
    let class = String::from_utf16_lossy(&buf[..len as usize]);
    class == "Shell_TrayWnd"
}

/// Returns the y-coordinate the pet top should be at when it lands on the
/// nearest surface below it, along with surface metadata.
/// Falls back to the virtual screen ground (`surface_w: 0.0, surface_label: ""`).
///
/// `cache` is filled via `EnumWindows` on the first call (or after TTL expiry)
/// including full occlusion checks. Cache hits re-apply per-call overlap and
/// min_surface filters only; occlusion is skipped (acceptable 250 ms TTL trade-off).
pub fn find_floor_info(
    geom: &PetGeom,
    bounds: &PlatformBounds,
    cache: &mut SurfaceCache,
) -> SurfaceHit {
    // Refresh cache if expired.
    if cache.is_expired() {
        let mut fill = FillState { screen_w: bounds.screen_w, entries: Vec::new() };
        unsafe {
            EnumWindows(Some(fill_cb), &mut fill as *mut _ as LPARAM);
        }
        cache.entries = fill.entries;
        cache.expires_at = Instant::now() + Duration::from_millis(250);
    }

    let pet_left = geom.x;
    let pet_right = geom.x + geom.w;
    let min_surface = geom.min_surface_threshold();
    let virtual_ground_top = bounds.virtual_ground_y();
    let mut best = virtual_ground_top;
    let mut best_rect: Option<&SurfaceRect> = None;

    for rect in &cache.entries {
        // Re-apply per-call horizontal overlap filter.
        if pet_right <= rect.left || pet_left >= rect.right { continue; }
        // Re-apply min_surface filter.
        if rect.top < min_surface || rect.top >= best { continue; }
        // Occlusion already verified at fill time — skip WindowFromPoint here.
        best = rect.top;
        best_rect = Some(rect);
    }

    let floor_y = geom.floor_landing_y(best);
    match best_rect {
        Some(rect) => SurfaceHit {
            floor_y,
            surface_w: (rect.right - rect.left) as f32,
            surface_label: rect.label.to_string(),
        },
        None => SurfaceHit { floor_y, surface_w: 0.0, surface_label: String::new() },
    }
}

/// Returns the y-coordinate the pet top should be at when it lands on the
/// nearest surface below it. Falls back to the virtual screen ground.
///
/// This is a thin wrapper around [`find_floor_info`] for callers that do not
/// need surface metadata.
pub fn find_floor(
    geom: &PetGeom,
    bounds: &PlatformBounds,
    cache: &mut SurfaceCache,
) -> i32 {
    find_floor_info(geom, bounds, cache).floor_y
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::geometry::{PetGeom, PlatformBounds};

    #[test]
    fn surface_cache_default_is_expired() {
        let cache = SurfaceCache::default();
        assert!(cache.is_expired(), "default cache must be expired so first call always re-fetches");
    }

    #[test]
    fn baseline_offset_does_not_filter_landing_surface() {
        let surface_top = 1040i32;
        let template = PetGeom { x: 500, y: 0, w: 137, h: 137, baseline_offset: 29 };
        let at_landing = PetGeom { y: template.floor_landing_y(surface_top), ..template };
        assert!(
            at_landing.min_surface_threshold() <= surface_top,
            "surface_top ({surface_top}) must pass min_surface filter at landing (threshold={})",
            at_landing.min_surface_threshold()
        );
    }

    #[test]
    fn surface_cache_find_floor_returns_plausible_value() {
        let mut cache = SurfaceCache::default();
        let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
        let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
        let bounds = PlatformBounds { screen_w, screen_h };
        let geom = PetGeom { x: 0, y: 0, w: 32, h: 32, baseline_offset: 0 };
        let floor = find_floor(&geom, &bounds, &mut cache);
        assert!(floor >= 0, "floor y must be non-negative, got {floor}");
        assert!(floor < bounds.screen_h, "floor y must be above screen bottom, got {floor}");
    }

    #[test]
    fn surface_cache_warm_returns_same_result() {
        let mut cache = SurfaceCache::default();
        let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
        let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
        let bounds = PlatformBounds { screen_w, screen_h };
        let geom = PetGeom { x: 100, y: 0, w: 32, h: 32, baseline_offset: 0 };
        let floor1 = find_floor(&geom, &bounds, &mut cache);
        assert!(!cache.is_expired(), "cache must be warm after first call");
        let floor2 = find_floor(&geom, &bounds, &mut cache);
        assert_eq!(floor1, floor2, "warm cache must return same floor as cold call");
    }
}
