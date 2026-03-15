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
    /// Absolute path to the `.json` spritesheet, or `"embedded://test_pet"`.
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
        PetConfig {
            id: "pet_0".into(),
            sheet_path: "embedded://test_pet".into(),
            x: 100,
            y: 800,
            scale: 2,
            walk_speed: 100.0,
            flip_walk_left: true,
            tag_map: AnimTagMap::default(),
        }
    }
}
