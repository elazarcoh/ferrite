//! Pure-Rust sprite editor state. No Win32 dependency.

use anyhow::{anyhow, Context, Result};
use image::RgbaImage;
use std::path::{Path, PathBuf};

use crate::sprite::sheet::TagDirection;

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
}

// ─── impl SpriteEditorState ───────────────────────────────────────────────────

impl SpriteEditorState {
    /// Create a new editor state for the given PNG file and decoded image.
    /// `rows` and `cols` default to 1×1; set them before calling `frame_rect`
    /// or `build_json`.
    pub fn new(png_path: PathBuf, image: RgbaImage) -> Self {
        SpriteEditorState {
            png_path,
            image,
            rows: 1,
            cols: 1,
            tags: Vec::new(),
            selected_tag: None,
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
    pub fn frames_for_tag(&self, tag_idx: usize) -> Vec<usize> {
        match self.tags.get(tag_idx) {
            Some(t) => (t.from..=t.to).collect(),
            None => vec![],
        }
    }

    /// True iff the sheet has at least one tag defined.
    pub fn is_saveable(&self) -> bool {
        !self.tags.is_empty()
    }

    /// Iterator of `(tag_idx, &EditorTag)` — used by the canvas painter.
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
    pub fn to_clean_json(&self) -> Vec<u8> {
        self.to_json()
    }

    /// Write JSON + copy PNG to `dir`, overwriting any existing files.
    /// Copies the source PNG file; does not re-encode from the in-memory image.
    pub fn save_to_dir(&self, dir: &Path) -> Result<()> {
        let stem = self.png_path
            .file_stem()
            .ok_or_else(|| anyhow!("png_path has no stem"))?
            .to_string_lossy();
        let dest_json = dir.join(format!("{stem}.json"));
        let dest_png = dir.join(format!("{stem}.png"));
        std::fs::write(&dest_json, self.to_json())
            .with_context(|| format!("write {}", dest_json.display()))?;
        std::fs::copy(&self.png_path, &dest_png)
            .with_context(|| format!("copy PNG to {}", dest_png.display()))?;
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

        serde_json::json!({
            "frames": frames,
            "meta": {"frameTags": frame_tags},
        })
    }
}

// ─── Free helpers ─────────────────────────────────────────────────────────────

fn direction_to_str(d: &TagDirection) -> &'static str {
    match d {
        TagDirection::Forward         => "forward",
        TagDirection::Reverse         => "reverse",
        TagDirection::PingPong        => "pingpong",
        TagDirection::PingPongReverse => "pingpong_reverse",
    }
}

