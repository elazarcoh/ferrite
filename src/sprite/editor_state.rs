//! Pure-Rust sprite editor state. No Win32 dependency.

use anyhow::{anyhow, Context, Result};
use image::RgbaImage;
use std::path::{Path, PathBuf};

use crate::sprite::sheet::{ChromakeyConfig, TagDirection};

// ─── Tag color palette ────────────────────────────────────────────────────────
// Stored as 0x00BBGGRR (Win32 COLORREF format) for historical reasons.
// In the egui sprite editor these are converted to egui::Color32 before display.
// Pending: tag-colored labels in the editor left panel.

pub const TAG_COLORS: &[u32] = &[
    0x0000ffff, // yellow
    0x00ffff00, // cyan
    0x00ff00ff, // magenta
    0x000080ff, // orange
    0x0000ff00, // lime
    0x000000ff, // red
    0x00ff0000, // blue
    0x008080ff, // pink
];

// ─── Public types ─────────────────────────────────────────────────────────────

pub struct EditorTag {
    pub name: String,
    pub from: usize,
    pub to: usize,
    pub direction: TagDirection,
    /// `true` = sprite faces LEFT in the sheet; mirror when walking RIGHT.
    pub flip_h: bool,
    /// COLORREF (0x00BBGGRR) from TAG_COLORS. Converted to egui::Color32 for rendering.
    pub color: u32,
}

pub struct SpriteEditorState {
    pub png_path: PathBuf,
    pub image: RgbaImage,
    pub rows: u32,
    pub cols: u32,
    pub tags: Vec<EditorTag>,
    pub selected_tag: Option<usize>,
    /// smMappings: sm_name → (state_name → tag_name). Written back to JSON on save.
    pub sm_mappings: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    /// Chromakey configuration. Written back to JSON on save when enabled.
    pub chromakey: ChromakeyConfig,
    /// User-editable name for this sprite; used as the file stem on save.
    pub sprite_name: String,
    /// Pixels from the bottom of a frame to the actual walking floor.
    /// 0 = bottom edge of the sprite grid is the floor (default, backwards compatible).
    pub baseline_offset: u32,
}

// ─── impl SpriteEditorState ───────────────────────────────────────────────────

impl SpriteEditorState {
    /// Create a new editor state for the given PNG file and decoded image.
    /// `rows` and `cols` default to 1×1; set them before calling `frame_rect`
    /// or `build_json`.
    pub fn new(png_path: PathBuf, image: RgbaImage) -> Self {
        let sprite_name = png_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "sprite".to_string());
        SpriteEditorState {
            png_path,
            image,
            rows: 1,
            cols: 1,
            tags: Vec::new(),
            selected_tag: None,
            sm_mappings: std::collections::HashMap::new(),
            chromakey: ChromakeyConfig::default(),
            sprite_name,
            baseline_offset: 0,
        }
    }

    /// Returns `(x, y, w, h)` for frame `i` in a uniform grid.
    pub fn frame_rect(&self, i: usize) -> (u32, u32, u32, u32) {
        let w = self.image.width() / self.cols;
        let h = self.image.height() / self.rows;
        let col = (i as u32) % self.cols;
        let row = (i as u32) / self.cols;
        (col * w, row * h, w, h)
    }

    /// Frame indices (inclusive range) covered by tag `tag_idx`.
    #[allow(dead_code)]
    pub fn frames_for_tag(&self, tag_idx: usize) -> Vec<usize> {
        match self.tags.get(tag_idx) {
            Some(t) => (t.from..=t.to).collect(),
            None => vec![],
        }
    }

    /// True iff the sheet has at least one tag defined.
    #[allow(dead_code)]
    pub fn is_saveable(&self) -> bool {
        !self.tags.is_empty()
    }

    /// Iterator of `(tag_idx, &EditorTag)` — used by the canvas painter.
    #[allow(dead_code)]
    pub fn state_tags_iter(&self) -> impl Iterator<Item = (usize, &EditorTag)> {
        self.tags.iter().enumerate()
    }

    /// COLORREF for tag at `idx` (cycles through TAG_COLORS palette).
    pub fn assign_color(idx: usize) -> u32 {
        TAG_COLORS[idx % TAG_COLORS.len()]
    }

    /// Serialise to Aseprite array-format JSON.
    pub fn to_json(&self) -> Vec<u8> {
        let json = self.build_json();
        serde_json::to_vec_pretty(&json).unwrap_or_else(|e| unreachable!("serde_json::Value serialize failed: {e}"))
    }

    /// Serialise to Aseprite array-format JSON (alias for export compatibility).
    #[allow(dead_code)]
    pub fn to_clean_json(&self) -> Vec<u8> {
        self.to_json()
    }

    /// Write JSON + copy PNG to `dir`, overwriting any existing files.
    /// Copies the source PNG file; does not re-encode from the in-memory image.
    pub fn save_to_dir(&self, dir: &Path) -> Result<()> {
        let stem = sanitize_name(&self.sprite_name);
        if stem.is_empty() {
            return Err(anyhow!("sprite_name is empty after sanitization"));
        }
        let dest_json = dir.join(format!("{stem}.json"));
        let dest_png = dir.join(format!("{stem}.png"));
        std::fs::write(&dest_json, self.to_json())
            .with_context(|| format!("write {}", dest_json.display()))?;
        if self.png_path != dest_png {
            std::fs::copy(&self.png_path, &dest_png)
                .with_context(|| format!("copy PNG to {}", dest_png.display()))?;
        }
        Ok(())
    }

    // ─── Private helpers ───────────────────────────────────────────────────

    fn build_json(&self) -> serde_json::Value {
        let total = (self.rows * self.cols) as usize;
        let frames: Vec<serde_json::Value> = (0..total)
            .map(|i| {
                let (x, y, w, h) = self.frame_rect(i);
                serde_json::json!({"frame": {"x": x, "y": y, "w": w, "h": h}, "duration": 100})
            })
            .collect();

        let frame_tags: Vec<serde_json::Value> = self.tags
            .iter()
            .map(|t| {
                let mut obj = serde_json::json!({
                    "name": t.name,
                    "from": t.from,
                    "to": t.to,
                    "direction": direction_to_str(&t.direction),
                });
                if t.flip_h {
                    obj["flipH"] = true.into();
                }
                obj
            })
            .collect();

        // Build smMappings object if any mappings exist
        let sm_mappings_json: serde_json::Value = if self.sm_mappings.is_empty() {
            serde_json::Value::Null
        } else {
            let mut obj = serde_json::Map::new();
            for (sm_name, mapping) in &self.sm_mappings {
                let mut inner = serde_json::Map::new();
                for (state, tag) in mapping {
                    inner.insert(state.clone(), serde_json::Value::String(tag.clone()));
                }
                obj.insert(sm_name.clone(), serde_json::Value::Object(inner));
            }
            serde_json::Value::Object(obj)
        };

        let mut meta = serde_json::json!({"frameTags": frame_tags});
        if !sm_mappings_json.is_null() {
            meta["smMappings"] = sm_mappings_json;
        }
        if self.chromakey.enabled {
            meta["chromakey"] = serde_json::to_value(&self.chromakey)
                .unwrap_or_else(|e| unreachable!("ChromakeyConfig serialize failed: {e}"));
        }
        if self.baseline_offset > 0 {
            meta["baseline_offset"] = self.baseline_offset.into();
        }

        serde_json::json!({
            "frames": frames,
            "meta": meta,
        })
    }
}

// ─── Free helpers ─────────────────────────────────────────────────────────────

/// Sanitize a sprite name to produce a safe file stem.
/// Allowed characters: alphanumeric, `-`, `_`. Everything else becomes `_`.
/// Leading/trailing whitespace is trimmed first.
pub fn sanitize_name(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn direction_to_str(d: &TagDirection) -> &'static str {
    match d {
        TagDirection::Forward         => "forward",
        TagDirection::Reverse         => "reverse",
        TagDirection::PingPong        => "pingpong",
        TagDirection::PingPongReverse => "pingpong_reverse",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn baseline_offset_round_trips_in_json() {
        let mut state = SpriteEditorState::new(
            std::path::PathBuf::from("test.png"),
            RgbaImage::new(32, 32),
        );
        state.rows = 1;
        state.cols = 1;
        state.baseline_offset = 8;
        let json = state.to_json();
        let parsed: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(parsed["meta"]["baseline_offset"], 8);
    }

    #[test]
    fn baseline_offset_zero_not_written_to_json() {
        let mut state = SpriteEditorState::new(
            std::path::PathBuf::from("test.png"),
            RgbaImage::new(32, 32),
        );
        state.rows = 1;
        state.cols = 1;
        // default is 0 — should not appear in JSON (or be 0)
        let json = state.to_json();
        let parsed: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert!(parsed["meta"].get("baseline_offset").is_none() || parsed["meta"]["baseline_offset"] == 0);
    }
}

