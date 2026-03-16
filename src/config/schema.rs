use crate::sprite::behavior::AnimTagMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub pets: Vec<PetConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config { pets: vec![PetConfig::default()] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PetConfig {
    pub id: String,
    /// Absolute path to the `.json` spritesheet, or `"embedded://<stem>"` for bundled sheets.
    pub sheet_path: String,
    pub x: i32,
    pub y: i32,
    /// Integer pixel-art upscale factor.
    pub scale: u32,
    /// Pixels per second for walk.
    pub walk_speed: f32,
    /// Mirror the sprite when walking left (so only one direction is needed).
    pub flip_walk_left: bool,
    pub tag_map: AnimTagMap,
}

impl Default for PetConfig {
    fn default() -> Self {
        PetConfig::esheep()
    }
}

impl PetConfig {
    /// Classic eSheep (40×40 sprites, all animations).
    pub fn esheep() -> Self {
        PetConfig {
            id: "esheep".into(),
            sheet_path: "embedded://esheep".into(),
            x: 100,
            y: 800,
            scale: 2,
            walk_speed: 80.0,
            flip_walk_left: true,
            tag_map: AnimTagMap {
                idle:    "idle".into(),
                walk:    "walk".into(),
                run:     Some("run".into()),
                sit:     Some("sit".into()),
                sleep:   Some("sleep".into()),
                wake:    Some("wake".into()),
                grabbed: Some("grabbed".into()),
                petted:  Some("petted".into()),
                react:   Some("react".into()),
                fall:    Some("fall".into()),
                thrown:  Some("thrown".into()),
            },
        }
    }
}
