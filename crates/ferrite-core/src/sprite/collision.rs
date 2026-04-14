use std::collections::HashSet;
use crate::sprite::sm_runner::CollideData;

/// The collision geometry for one pet in a given frame.
/// Build this from `SpriteSheet::tight_bbox` output plus the SM runner's
/// current velocity (`SMRunner::speed()`).
#[derive(Debug, Clone)]
pub struct Collidable {
    pub id: String,
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    /// Vertical midpoint of the bounding box.
    pub center_y: i32,
    pub vx: f32,
    pub vy: f32,
}

/// Both sides of a freshly-started collision.
pub struct CollisionPair {
    pub id_a: String,
    pub data_a: CollideData,
    pub id_b: String,
    pub data_b: CollideData,
}

/// Returns the sorted canonical key for a pair of pet IDs.
/// Used by callers to maintain the `previously_overlapping` set.
pub fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) }
}

/// Returns the set of all currently-overlapping ID pairs.
/// `collidables` must be sorted by `left` ascending (callers sort before passing).
pub fn overlapping_pairs(collidables: &[Collidable]) -> HashSet<(String, String)> {
    let mut result = HashSet::new();
    for i in 0..collidables.len() {
        for j in (i + 1)..collidables.len() {
            let a = &collidables[i];
            let b = &collidables[j];
            if b.left >= a.right { break; }
            if a.bottom <= b.top || b.bottom <= a.top { continue; }
            if a.left == a.right || b.left == b.right { continue; }
            result.insert(canonical_pair(&a.id, &b.id));
        }
    }
    result
}

/// Returns collision events for pairs that are overlapping *now* but were
/// NOT overlapping in the previous frame. Fires at most once per collision.
/// `collidables` must be sorted by `left` ascending.
pub fn detect_new_collisions(
    collidables: &[Collidable],
    previously_overlapping: &HashSet<(String, String)>,
) -> Vec<CollisionPair> {
    let mut result = Vec::new();
    for i in 0..collidables.len() {
        for j in (i + 1)..collidables.len() {
            let a = &collidables[i];
            let b = &collidables[j];
            if b.left >= a.right { break; }
            if a.bottom <= b.top || b.bottom <= a.top { continue; }
            if a.left == a.right || b.left == b.right { continue; }
            let key = canonical_pair(&a.id, &b.id);
            if previously_overlapping.contains(&key) { continue; }
            let (type_a, type_b) = classify(a, b);
            result.push(CollisionPair {
                id_a: a.id.clone(),
                data_a: make_data(a, b, type_a),
                id_b: b.id.clone(),
                data_b: make_data(b, a, type_b),
            });
        }
    }
    result
}

fn classify(a: &Collidable, b: &Collidable) -> (String, String) {
    let rel_vx = a.vx - b.vx;
    let rel_vy = a.vy - b.vy;
    if rel_vx.abs() >= rel_vy.abs() {
        let a_cx = (a.left + a.right) / 2;
        let b_cx = (b.left + b.right) / 2;
        let approaching = (a_cx < b_cx && a.vx > b.vx) || (a_cx > b_cx && a.vx < b.vx);
        let t = if approaching { "head_on" } else { "same_dir" };
        (t.to_string(), t.to_string())
    } else {
        let a_above = a.center_y < b.center_y;
        let a_moving_down = a.vy > b.vy;
        match (a_above, a_moving_down) {
            (true,  true)  => ("fell_on".to_string(),        "landed_on".to_string()),
            (true,  false) => ("landed_on".to_string(),      "fell_on".to_string()),
            (false, true)  => ("hit_from_below".to_string(), "hit_into_above".to_string()),
            (false, false) => ("hit_into_above".to_string(), "hit_from_below".to_string()),
        }
    }
}

fn make_data(me: &Collidable, other: &Collidable, collide_type: String) -> CollideData {
    let vx = me.vx - other.vx;
    let vy = me.vy - other.vy;
    CollideData { collide_type, vx, vy, v: (vx * vx + vy * vy).sqrt() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(id: &str, left: i32, right: i32, top: i32, bottom: i32, vx: f32, vy: f32) -> Collidable {
        Collidable {
            id: id.to_string(),
            left, right, top, bottom,
            center_y: (top + bottom) / 2,
            vx, vy,
        }
    }

    #[test]
    fn head_on_collision_detected() {
        // a moving right, b moving left, overlapping
        let a = col("a", 0,  50, 0, 50, 100.0, 0.0);
        let b = col("b", 25, 75, 0, 50, -100.0, 0.0);
        let pairs = detect_new_collisions(&[a, b], &HashSet::new());
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].data_a.collide_type, "head_on");
        assert_eq!(pairs[0].data_b.collide_type, "head_on");
    }

    #[test]
    fn continued_overlap_not_re_fired() {
        let a = col("a", 0, 50, 0, 50, 0.0, 0.0);
        let b = col("b", 25, 75, 0, 50, 0.0, 0.0);
        let prev: HashSet<_> = [("a".to_string(), "b".to_string())].into_iter().collect();
        let pairs = detect_new_collisions(&[a, b], &prev);
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn non_overlapping_produces_no_pair() {
        let a = col("a", 0,  50, 0, 50, 0.0, 0.0);
        let b = col("b", 60, 110, 0, 50, 0.0, 0.0);
        let pairs = detect_new_collisions(&[a, b], &HashSet::new());
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn fell_on_vertical_collision() {
        // a above b, a moving down faster
        let a = col("a", 0, 50, 0,  50, 0.0, 200.0);
        let b = col("b", 0, 50, 40, 90, 0.0,   0.0);
        let pairs = detect_new_collisions(&[a, b], &HashSet::new());
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].data_a.collide_type, "fell_on");
        assert_eq!(pairs[0].data_b.collide_type, "landed_on");
    }

    #[test]
    fn overlapping_pairs_tracks_all_current_overlaps() {
        let a = col("a", 0,  50, 0, 50, 0.0, 0.0);
        let b = col("b", 25, 75, 0, 50, 0.0, 0.0);
        let c = col("c", 80, 130, 0, 50, 0.0, 0.0);
        let pairs = overlapping_pairs(&[a, b, c]);
        assert!(pairs.contains(&("a".to_string(), "b".to_string())));
        assert!(!pairs.contains(&("a".to_string(), "c".to_string())));
    }
}
