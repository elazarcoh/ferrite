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

#[derive(Debug)]
pub struct SpriteSheet {
    pub image: RgbaImage,
    pub frames: Vec<Frame>,
    pub tags: Vec<FrameTag>,
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

        Ok(SpriteSheet { image, frames, tags })
    }

    /// Find a tag by name (case-sensitive).
    pub fn tag(&self, name: &str) -> Option<&FrameTag> {
        self.tags.iter().find(|t| t.name == name)
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

    fn test_json() -> &'static [u8] {
        include_bytes!("../../assets/test_pet.json")
    }
    fn test_png() -> &'static [u8] {
        include_bytes!("../../assets/test_pet.png")
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

}
