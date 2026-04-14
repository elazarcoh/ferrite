/// Geometry of a single pet window for floor/surface calculations.
///
/// `baseline_offset` is the number of pixels between the window bottom
/// and the pet's visual contact point (legs/feet). Already scaled to
/// screen pixels — callers are responsible for `source_px * scale`.
#[derive(Debug, Clone, Copy)]
pub struct PetGeom {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub baseline_offset: i32,
}

impl PetGeom {
    /// y-coordinate of the pet's visual contact point with the surface.
    /// Surfaces must have their top edge at or below this value to be
    /// considered "below" the pet.
    pub fn effective_bottom(&self) -> i32 {
        self.y + self.h - self.baseline_offset
    }

    /// Minimum surface `top` y-coordinate for a surface to count as
    /// below the pet. Surfaces at `rect.top < min_surface_threshold()`
    /// are above the pet's contact zone and must be filtered out.
    pub fn min_surface_threshold(&self) -> i32 {
        self.effective_bottom().max(self.h)
    }

    /// The window top-y at which the pet should sit when standing on a
    /// surface whose top edge is at `surface_top`.
    pub fn floor_landing_y(&self, surface_top: i32) -> i32 {
        surface_top - self.h + self.baseline_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(y: i32, h: i32, baseline_offset: i32) -> PetGeom {
        PetGeom { x: 0, y, w: h, h, baseline_offset }
    }

    #[test]
    fn effective_bottom_subtracts_baseline() {
        let g = geom(100, 64, 16);
        assert_eq!(g.effective_bottom(), 100 + 64 - 16); // 148
    }

    #[test]
    fn effective_bottom_zero_baseline_equals_raw_bottom() {
        let g = geom(200, 32, 0);
        assert_eq!(g.effective_bottom(), 200 + 32);
    }

    #[test]
    fn floor_landing_y_places_contact_at_surface_top() {
        // Round-trip: a pet placed at floor_landing_y(surface_top) must have
        // effective_bottom() == surface_top.
        let surface_top = 1040i32;
        let template = geom(0, 137, 29);
        let landing_y = template.floor_landing_y(surface_top);
        let at_landing = PetGeom { y: landing_y, ..template };
        assert_eq!(
            at_landing.effective_bottom(), surface_top,
            "effective_bottom must equal surface_top at landing position"
        );
    }

    #[test]
    fn min_surface_threshold_at_landing_does_not_exclude_surface() {
        // Regression for the baseline_offset surface-filter bug:
        // when a pet is at its landing position, min_surface_threshold()
        // must be <= surface_top so the surface is NOT filtered out.
        let surface_top = 1040i32;
        let template = geom(0, 137, 29);
        let landing_y = template.floor_landing_y(surface_top);
        let at_landing = PetGeom { y: landing_y, ..template };
        assert!(
            at_landing.min_surface_threshold() <= surface_top,
            "min_surface_threshold ({}) must be <= surface_top ({}) at landing",
            at_landing.min_surface_threshold(), surface_top
        );
    }

    #[test]
    fn floor_landing_y_zero_baseline_matches_old_formula() {
        // baseline_offset=0 must be identical to the pre-PetGeom formula:
        // floor_y = surface_top - pet_h
        let g = geom(0, 64, 0);
        assert_eq!(g.floor_landing_y(500), 500 - 64);
    }

    #[test]
    fn floor_landing_y_nonzero_baseline_raises_window() {
        // With baseline_offset=16, the window top shifts up by 16 so the
        // visual feet (pet_h - 16 from top) align with the surface.
        let g = geom(0, 64, 16);
        assert_eq!(g.floor_landing_y(500), 500 - 64 + 16); // 452
    }
}
