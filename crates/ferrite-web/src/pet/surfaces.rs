/// Configurable rules for discovering walkable DOM surfaces.
pub struct SurfaceConfig {
    /// CSS selector used to discover walkable elements.
    pub selector: &'static str,
    /// Minimum element width in CSS pixels. 0 = no minimum.
    pub min_width: i32,
    /// Minimum element height in CSS pixels. 0 = no minimum.
    pub min_height: i32,
    /// Maximum element height as a fraction of viewport height. 0.0 = no maximum.
    pub max_height_ratio: f32,
}

pub const DEFAULT_CONFIG: SurfaceConfig = SurfaceConfig {
    selector: concat!(
        "button, [role=button], a[href], ",
        "input:not([type=hidden]), select, textarea, ",
        "nav, header, ",
        "[class*=card], [class*=box], [class*=panel]"
    ),
    min_width: 0,
    min_height: 0,
    max_height_ratio: 0.0,
};

/// Viewport-space bounding rect of one walkable element (top edge only).
pub struct WebSurface {
    pub left:  i32,
    pub right: i32,
    pub top:   i32,
}

/// TTL-based cache of discovered surfaces.
pub struct SurfaceCache {
    pub entries:    Vec<WebSurface>,
    pub expires_at: f64,  // performance.now() ms
}

impl Default for SurfaceCache {
    fn default() -> Self {
        Self { entries: Vec::new(), expires_at: 0.0 }
    }
}

/// Query the DOM for matching elements, filter by visibility and config thresholds,
/// and rebuild the cache. No-ops if the cache is still fresh.
#[cfg(target_arch = "wasm32")]
pub fn refresh_if_expired(
    cache:  &mut SurfaceCache,
    doc:    &web_sys::Document,
    win_h:  i32,
    config: &SurfaceConfig,
) {
    use wasm_bindgen::JsCast;
    let now = web_sys::window().unwrap().performance().unwrap().now();
    if now < cache.expires_at { return; }

    cache.entries.clear();
    let Ok(nodes) = doc.query_selector_all(config.selector) else { return };

    for i in 0..nodes.length() {
        let Some(el) = nodes.item(i) else { continue };
        let Ok(el) = el.dyn_into::<web_sys::Element>() else { continue };
        let r = el.get_bounding_client_rect();

        let top    = r.top()    as i32;
        let left   = r.left()   as i32;
        let right  = r.right()  as i32;
        let width  = r.width()  as i32;
        let height = r.height() as i32;

        // Viewport visibility: top edge must be inside the visible viewport
        if top < 0 || top >= win_h { continue; }
        // Non-zero area
        if width <= 0 || height <= 0 { continue; }
        // User-configured size thresholds
        if config.min_width  > 0 && width  < config.min_width  { continue; }
        if config.min_height > 0 && height < config.min_height { continue; }
        if config.max_height_ratio > 0.0
            && height as f32 > win_h as f32 * config.max_height_ratio { continue; }

        cache.entries.push(WebSurface { left, right, top });
    }

    cache.expires_at = now + 250.0;
}

/// Returns the sprite-top y-coordinate when resting on the nearest surface below
/// the pet, or on the viewport bottom if no surface qualifies.
///
/// Mirrors the Windows `find_floor()` convention: returns `best_surface_top - pet_h`.
pub fn find_floor(
    pet_x:  i32,
    pet_y:  i32,
    pet_w:  i32,
    pet_h:  i32,
    win_h:  i32,
    cache:  &SurfaceCache,
) -> i32 {
    let pet_left   = pet_x;
    let pet_right  = pet_x + pet_w;
    let pet_bottom = pet_y + pet_h;

    let mut best = win_h;

    for s in &cache.entries {
        if pet_right <= s.left || pet_left >= s.right { continue; }
        if s.top < pet_bottom { continue; }
        if s.top < best { best = s.top; }
    }

    best - pet_h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_floor_falls_back_to_viewport_when_no_surfaces() {
        let cache = SurfaceCache::default();
        assert_eq!(find_floor(100, 900, 64, 80, 1000, &cache), 920);
    }

    #[test]
    fn find_floor_returns_nearest_surface_below_pet() {
        let mut cache = SurfaceCache::default();
        cache.entries = vec![
            WebSurface { left: 0, right: 800, top: 600 },
            WebSurface { left: 0, right: 800, top: 400 },
        ];
        assert_eq!(find_floor(100, 100, 64, 80, 1000, &cache), 320);
    }

    #[test]
    fn find_floor_ignores_surfaces_above_pet_bottom() {
        let mut cache = SurfaceCache::default();
        cache.entries = vec![
            WebSurface { left: 0, right: 800, top: 50 },
        ];
        assert_eq!(find_floor(100, 100, 64, 80, 1000, &cache), 920);
    }

    #[test]
    fn find_floor_ignores_non_overlapping_surfaces() {
        let mut cache = SurfaceCache::default();
        cache.entries = vec![
            WebSurface { left: 500, right: 800, top: 400 },
        ];
        assert_eq!(find_floor(100, 100, 64, 80, 1000, &cache), 920);
    }
}
