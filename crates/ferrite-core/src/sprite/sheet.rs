use anyhow::{anyhow, Context, Result};
use image::RgbaImage;
use serde::Deserialize;
use serde_json::Value;

// ─── Public types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Frame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub duration_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum TagDirection {
    #[default]
    Forward,
    Reverse,
    PingPong,
    PingPongReverse,
}

impl TagDirection {
    pub fn label(&self) -> &'static str {
        match self {
            TagDirection::Forward => "Forward",
            TagDirection::Reverse => "Reverse",
            TagDirection::PingPong => "PingPong",
            TagDirection::PingPongReverse => "PingPongReverse",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrameTag {
    pub name: String,
    pub from: usize,
    pub to: usize,
    pub direction: TagDirection,
    /// Sprite frames face LEFT in the sheet. Mirror when moving RIGHT so the pet
    /// faces its direction of travel. Leave false if the sprite faces right (standard).
    pub flip_h: bool,
}

/// Chromakey configuration: remove pixels of a specific background color.
/// Stored in JSON `meta.chromakey`. Derives serde so parse_chromakey uses from_value.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChromakeyConfig {
    pub enabled: bool,
    /// Key color as [r, g, b] in 0–255.
    pub color: [u8; 3],
    /// Euclidean tolerance (0–255). Pixels within this distance are keyed.
    /// 0 = exact match.
    pub tolerance: u8,
}

impl Default for ChromakeyConfig {
    fn default() -> Self {
        Self { enabled: false, color: [0, 255, 0], tolerance: 0 }
    }
}

/// Zero the alpha of every pixel whose Euclidean RGB distance from `cfg.color`
/// is <= cfg.tolerance. No-op when cfg.enabled is false.
pub fn apply_chromakey(image: &mut RgbaImage, cfg: &ChromakeyConfig) {
    if !cfg.enabled { return; }
    let [kr, kg, kb] = cfg.color.map(|c| c as i32);
    let t_sq = cfg.tolerance as i32 * cfg.tolerance as i32;
    for px in image.pixels_mut() {
        let [r, g, b, _] = px.0;
        let d = (r as i32 - kr).pow(2) + (g as i32 - kg).pow(2) + (b as i32 - kb).pow(2);
        if d <= t_sq {
            px.0[3] = 0;
        }
    }
}

#[derive(Debug)]
pub struct SpriteSheet {
    pub image: RgbaImage,
    pub frames: Vec<Frame>,
    pub tags: Vec<FrameTag>,
    pub sm_mappings: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    pub chromakey: ChromakeyConfig,
}

// ─── Aseprite JSON serde helpers ─────────────────────────────────────────────

#[derive(Deserialize)]
struct AseRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Deserialize)]
struct AseFrame {
    frame: AseRect,
    duration: u32,
}

#[derive(Deserialize)]
struct AseTag {
    name: String,
    from: usize,
    to: usize,
    direction: String,
    /// `true` = sprite faces LEFT in the sheet; mirror when walking RIGHT.
    #[serde(rename = "flipH", default)]
    flip_h: bool,
}

// ─── Parsing ─────────────────────────────────────────────────────────────────

impl SpriteSheet {
    /// Load from raw JSON bytes and the decoded PNG image.
    pub fn from_json_and_image(json: &[u8], image: RgbaImage) -> Result<Self> {
        let root: Value = serde_json::from_slice(json).context("parse spritesheet JSON")?;

        let frames = parse_frames(&root).context("parse frames")?;
        let tags = parse_tags(&root).context("parse tags")?;
        let sm_mappings = parse_sm_mappings(&root);
        let chromakey = parse_chromakey(&root);

        Ok(SpriteSheet { image, frames, tags, sm_mappings, chromakey })
    }

    /// Parse only frame/tag metadata; image is a 1×1 dummy.
    /// Use this in WASM where PNG decoding is done by the browser.
    pub fn from_json_bytes(json: &[u8]) -> Result<Self> {
        Self::from_json_and_image(json, image::RgbaImage::new(1, 1))
    }

    /// Find a tag by name (case-sensitive).
    pub fn tag(&self, name: &str) -> Option<&FrameTag> {
        self.tags.iter().find(|t| t.name == name)
    }

    /// Resolve SM state name to a sprite tag name.
    /// Resolution order: smMappings[sm_name][state_name] → auto-match by name → None
    pub fn resolve_tag<'a>(&'a self, sm_name: &str, state_name: &str) -> Option<&'a str> {
        // 1. Explicit alias
        if let Some(mapping) = self.sm_mappings.get(sm_name)
            && let Some(tag) = mapping.get(state_name) {
                return Some(tag.as_str());
            }
        // 2. Auto-match: tag with same name exists
        if let Some(t) = self.tags.iter().find(|t| t.name == state_name) {
            return Some(t.name.as_str());
        }
        None
    }
}

fn parse_frames(root: &Value) -> Result<Vec<Frame>> {
    let frames_val = root.get("frames").ok_or_else(|| anyhow!("missing 'frames' key"))?;

    if let Some(obj) = frames_val.as_object() {
        // Hash format: { "name 0.ase": { frame: ... }, ... }
        // Sort by the numeric suffix so frames are in order.
        let mut entries: Vec<(usize, AseFrame)> = obj
            .iter()
            .map(|(k, v)| {
                let idx = extract_frame_index(k);
                let f: AseFrame =
                    serde_json::from_value(v.clone()).with_context(|| format!("frame '{k}'"))?;
                Ok((idx, f))
            })
            .collect::<Result<Vec<_>>>()?;
        entries.sort_by_key(|(i, _)| *i);
        Ok(entries.into_iter().map(|(_, f)| ase_to_frame(f)).collect())
    } else if let Some(arr) = frames_val.as_array() {
        // Array format: [ { frame: ... }, ... ]
        arr.iter()
            .enumerate()
            .map(|(i, v)| {
                let f: AseFrame =
                    serde_json::from_value(v.clone()).with_context(|| format!("frame {i}"))?;
                Ok(ase_to_frame(f))
            })
            .collect()
    } else {
        Err(anyhow!("'frames' must be an object or array"))
    }
}

fn parse_tags(root: &Value) -> Result<Vec<FrameTag>> {
    let tags_val = root
        .pointer("/meta/frameTags")
        .or_else(|| root.pointer("/meta/frame_tags"));

    let Some(arr) = tags_val.and_then(|v| v.as_array()) else {
        return Ok(vec![]); // tags are optional
    };

    arr.iter()
        .enumerate()
        .map(|(i, v)| {
            let t: AseTag =
                serde_json::from_value(v.clone()).with_context(|| format!("tag {i}"))?;
            Ok(FrameTag {
                name: t.name,
                from: t.from,
                to: t.to,
                direction: parse_direction(&t.direction),
                flip_h: t.flip_h,
            })
        })
        .collect()
}

fn parse_sm_mappings(root: &Value) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
    let mut result = std::collections::HashMap::new();
    let Some(obj) = root.pointer("/meta/smMappings").and_then(|v| v.as_object()) else {
        return result;
    };
    for (sm_name, mappings_val) in obj {
        if let Some(mappings_obj) = mappings_val.as_object() {
            let mut inner = std::collections::HashMap::new();
            for (state, tag) in mappings_obj {
                if let Some(tag_str) = tag.as_str() {
                    inner.insert(state.clone(), tag_str.to_string());
                }
            }
            result.insert(sm_name.clone(), inner);
        }
    }
    result
}

fn parse_chromakey(root: &Value) -> ChromakeyConfig {
    root.pointer("/meta/chromakey")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn ase_to_frame(f: AseFrame) -> Frame {
    Frame { x: f.frame.x, y: f.frame.y, w: f.frame.w, h: f.frame.h, duration_ms: f.duration }
}

fn parse_direction(s: &str) -> TagDirection {
    match s {
        "reverse" => TagDirection::Reverse,
        "pingpong" => TagDirection::PingPong,
        "pingpong_reverse" => TagDirection::PingPongReverse,
        _ => TagDirection::Forward,
    }
}

/// Extract trailing integer from strings like "sprite 0.aseprite" → 0.
fn extract_frame_index(key: &str) -> usize {
    // Find the last run of digits before the extension (or end).
    let stem = key.rsplit_once('.').map(|(s, _)| s).unwrap_or(key);
    let digits: String = stem.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return 0;
    }
    digits.chars().rev().collect::<String>().parse().unwrap_or(0)
}

// ─── Convenience: load from embedded bytes ───────────────────────────────────

/// Load a spritesheet from the embedded `Assets` bundle.
/// `name` is the stem (e.g. `"test_pet"`).
pub fn load_embedded(json_bytes: &[u8], png_bytes: &[u8]) -> Result<SpriteSheet> {
    let image = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .context("decode embedded PNG")?
        .into_rgba8();
    SpriteSheet::from_json_and_image(json_bytes, image)
}


// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sheet_with_tags(names: &[&str]) -> SpriteSheet {
        SpriteSheet {
            image: image::RgbaImage::new(1, 1),
            frames: vec![],
            tags: names.iter().map(|n| FrameTag {
                name: n.to_string(),
                from: 0,
                to: 0,
                direction: TagDirection::Forward,
                flip_h: false,
            }).collect(),
            sm_mappings: HashMap::new(),
            chromakey: ChromakeyConfig::default(),
        }
    }

    #[test]
    fn resolve_tag_auto_match() {
        let sheet = sheet_with_tags(&["idle", "walk"]);
        assert_eq!(sheet.resolve_tag("Any SM", "idle"), Some("idle"));
    }

    #[test]
    fn resolve_tag_explicit_alias() {
        let mut sheet = sheet_with_tags(&["idle_cycle"]);
        sheet.sm_mappings.insert("MyPet".into(), {
            let mut m = HashMap::new();
            m.insert("idle".into(), "idle_cycle".into());
            m
        });
        assert_eq!(sheet.resolve_tag("MyPet", "idle"), Some("idle_cycle"));
    }

    #[test]
    fn resolve_tag_not_found() {
        let sheet = sheet_with_tags(&["idle"]);
        assert_eq!(sheet.resolve_tag("SM", "missing"), None);
    }

    fn test_json() -> &'static [u8] {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/test_pet.json"))
    }
    fn test_png() -> &'static [u8] {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/test_pet.png"))
    }

    fn load() -> SpriteSheet {
        load_embedded(test_json(), test_png()).expect("load test sheet")
    }

    #[test]
    fn hash_format_frame_count() {
        let sheet = load();
        assert_eq!(sheet.frames.len(), 8);
    }

    #[test]
    fn hash_format_tag() {
        let sheet = load();
        let tag = sheet.tag("idle").expect("idle tag");
        assert_eq!(tag.from, 0);
        assert_eq!(tag.to, 1);
        assert_eq!(tag.direction, TagDirection::PingPong);
    }

    #[test]
    fn array_format() {
        let json = r#"{
            "frames": [
                { "frame": {"x":0,"y":0,"w":32,"h":32}, "rotated":false,"trimmed":false,
                  "spriteSourceSize":{"x":0,"y":0,"w":32,"h":32},
                  "sourceSize":{"w":32,"h":32}, "duration":200 },
                { "frame": {"x":32,"y":0,"w":32,"h":32}, "rotated":false,"trimmed":false,
                  "spriteSourceSize":{"x":0,"y":0,"w":32,"h":32},
                  "sourceSize":{"w":32,"h":32}, "duration":150 }
            ],
            "meta": {
                "frameTags": [{"name":"run","from":0,"to":1,"direction":"pingpong"}]
            }
        }"#;
        let image = image::load_from_memory_with_format(test_png(), image::ImageFormat::Png)
            .unwrap()
            .into_rgba8();
        let sheet = SpriteSheet::from_json_and_image(json.as_bytes(), image).unwrap();
        assert_eq!(sheet.frames.len(), 2);
        assert_eq!(sheet.frames[0].duration_ms, 200);
        assert_eq!(sheet.frames[1].duration_ms, 150);
        let tag = sheet.tag("run").unwrap();
        assert_eq!(tag.direction, TagDirection::PingPong);
    }

    #[test]
    fn chromakey_exact_match_zeroes_alpha() {
        let mut img = image::RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([0, 255, 0, 255]));  // key color
        img.put_pixel(1, 0, image::Rgba([255, 0, 0, 255]));  // non-key
        apply_chromakey(&mut img, &ChromakeyConfig { enabled: true, color: [0, 255, 0], tolerance: 0 });
        assert_eq!(img.get_pixel(0, 0).0[3], 0, "key pixel must be transparent");
        assert_eq!(img.get_pixel(1, 0).0[3], 255, "non-key pixel must be opaque");
    }

    #[test]
    fn chromakey_disabled_is_noop() {
        let mut img = image::RgbaImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgba([0, 255, 0, 255]));
        apply_chromakey(&mut img, &ChromakeyConfig { enabled: false, color: [0, 255, 0], tolerance: 0 });
        assert_eq!(img.get_pixel(0, 0).0[3], 255, "disabled chromakey must not change alpha");
    }

    #[test]
    fn chromakey_tolerance_fuzzy_match() {
        let mut img = image::RgbaImage::new(1, 1);
        // dist² from [0,255,0]: (5-0)²+(250-255)²+(5-0)² = 25+25+25 = 75 <= 10²=100
        img.put_pixel(0, 0, image::Rgba([5, 250, 5, 255]));
        apply_chromakey(&mut img, &ChromakeyConfig { enabled: true, color: [0, 255, 0], tolerance: 10 });
        assert_eq!(img.get_pixel(0, 0).0[3], 0, "near-key pixel within tolerance must be transparent");
    }

    #[test]
    fn chromakey_tolerance_boundary_included() {
        // dist² from [0,255,0]: 10²+0+0 = 100 == 10²  → must be keyed
        let mut img = image::RgbaImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgba([10, 255, 0, 255]));
        apply_chromakey(&mut img, &ChromakeyConfig { enabled: true, color: [0, 255, 0], tolerance: 10 });
        assert_eq!(img.get_pixel(0, 0).0[3], 0, "pixel at exact tolerance distance must be transparent");
    }

    #[test]
    fn chromakey_tolerance_boundary_excluded() {
        // dist² from [0,255,0]: 11²+0+0 = 121 > 10² → must NOT be keyed
        let mut img = image::RgbaImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgba([11, 255, 0, 255]));
        apply_chromakey(&mut img, &ChromakeyConfig { enabled: true, color: [0, 255, 0], tolerance: 10 });
        assert_eq!(img.get_pixel(0, 0).0[3], 255, "pixel just outside tolerance must remain opaque");
    }

    #[test]
    fn chromakey_json_round_trip() {
        let json = r#"{"frames":[{"frame":{"x":0,"y":0,"w":1,"h":1},"duration":100}],"meta":{"frameTags":[],"chromakey":{"enabled":true,"color":[0,255,0],"tolerance":5}}}"#;
        let sheet = SpriteSheet::from_json_and_image(json.as_bytes(), image::RgbaImage::new(1, 1)).unwrap();
        assert!(sheet.chromakey.enabled);
        assert_eq!(sheet.chromakey.color, [0, 255, 0]);
        assert_eq!(sheet.chromakey.tolerance, 5);
    }

    #[test]
    fn chromakey_missing_in_json_gives_default() {
        let json = r#"{"frames":[],"meta":{"frameTags":[]}}"#;
        let sheet = SpriteSheet::from_json_and_image(json.as_bytes(), image::RgbaImage::new(1, 1)).unwrap();
        assert!(!sheet.chromakey.enabled);
    }

}
