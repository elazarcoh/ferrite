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
    /// Mirror sprite horizontally when this tag plays (e.g. walk faces right but pet moves left).
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

/// Load a spritesheet and, if present, the `myPetTagMap` behavior mapping.
/// Returns `(sheet, None)` if `myPetTagMap` is absent or has missing/empty
/// required fields (`idle` or `walk`). Optional fields that are non-strings
/// are silently ignored.
pub fn load_with_tag_map(
    json_bytes: &[u8],
    png_bytes: &[u8],
) -> Result<(SpriteSheet, Option<crate::sprite::behavior::AnimTagMap>)> {
    let sheet = load_embedded(json_bytes, png_bytes)?;
    let root: Value = serde_json::from_slice(json_bytes)
        .context("re-parse JSON for myPetTagMap")?;
    let tag_map = parse_my_pet_tag_map(&root);
    Ok((sheet, tag_map))
}

fn parse_my_pet_tag_map(
    root: &Value,
) -> Option<crate::sprite::behavior::AnimTagMap> {
    let map = root.pointer("/meta/myPetTagMap")?.as_object()?;
    let idle = map.get("idle")?.as_str().filter(|s| !s.is_empty())?.to_string();
    let walk = map.get("walk")?.as_str().filter(|s| !s.is_empty())?.to_string();
    let opt = |key: &str| map.get(key).and_then(|v| v.as_str()).map(str::to_string);
    Some(crate::sprite::behavior::AnimTagMap {
        idle,
        walk,
        run:     opt("run"),
        sit:     opt("sit"),
        sleep:   opt("sleep"),
        wake:    opt("wake"),
        grabbed: opt("grabbed"),
        petted:  opt("petted"),
        react:   opt("react"),
        fall:    opt("fall"),
        thrown:  opt("thrown"),
    })
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

    use crate::sprite::behavior::AnimTagMap;

    #[test]
    fn load_with_tag_map_absent_returns_none() {
        // test_pet.json has no myPetTagMap field
        let (_, tag_map) = load_with_tag_map(test_json(), test_png()).unwrap();
        assert!(tag_map.is_none(), "no myPetTagMap → None");
    }

    #[test]
    fn load_with_tag_map_round_trip() {
        let json = r#"{
            "frames": [
                {"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100},
                {"frame":{"x":32,"y":0,"w":32,"h":32},"duration":100}
            ],
            "meta": {
                "frameTags": [{"name":"idle","from":0,"to":1,"direction":"forward"}],
                "myPetTagMap": {"idle":"idle_loop","walk":"walk_cycle","run":"run_fast"}
            }
        }"#;
        let (sheet, tag_map) = load_with_tag_map(json.as_bytes(), test_png()).unwrap();
        assert_eq!(sheet.frames.len(), 2);
        let tm = tag_map.expect("should have tag map");
        assert_eq!(tm.idle, "idle_loop");
        assert_eq!(tm.walk, "walk_cycle");
        assert_eq!(tm.run, Some("run_fast".into()));
        assert_eq!(tm.sit, None);
    }

    #[test]
    fn load_with_tag_map_missing_required_drops_map() {
        let json = r#"{
            "frames": [{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],
            "meta": {"frameTags": [], "myPetTagMap": {"idle":"idle"}}
        }"#;
        let (_, tag_map) = load_with_tag_map(json.as_bytes(), test_png()).unwrap();
        assert!(tag_map.is_none(), "missing walk → None");
    }

    #[test]
    fn load_with_tag_map_empty_required_drops_map() {
        let json = r#"{
            "frames": [{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],
            "meta": {"frameTags": [], "myPetTagMap": {"idle":"","walk":"walk"}}
        }"#;
        let (_, tag_map) = load_with_tag_map(json.as_bytes(), test_png()).unwrap();
        assert!(tag_map.is_none(), "empty idle → None");
    }

    #[test]
    fn load_with_tag_map_bad_optional_ignored() {
        let json = r#"{
            "frames": [{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],
            "meta": {"frameTags": [], "myPetTagMap": {"idle":"idle","walk":"walk","run":42}}
        }"#;
        let (_, tag_map) = load_with_tag_map(json.as_bytes(), test_png()).unwrap();
        let tm = tag_map.expect("map returned despite bad optional");
        assert_eq!(tm.idle, "idle");
        assert_eq!(tm.walk, "walk");
        assert_eq!(tm.run, None, "non-string run silently ignored");
    }
}
