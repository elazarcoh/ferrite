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
    /// Path to state machine TOML, or `"embedded://default"` for the built-in SM.
    pub state_machine: String,
    pub x: i32,
    pub y: i32,
    /// Pixel-art upscale factor (fractional values supported, e.g. 1.5).
    pub scale: f32,
    /// Pixels per second for walk.
    pub walk_speed: f32,
}

impl Default for PetConfig {
    fn default() -> Self {
        Self {
            id: "esheep".to_string(),
            sheet_path: "embedded://esheep".to_string(),
            state_machine: "embedded://default".to_string(),
            x: 100,
            y: 800,
            scale: 2.0,
            walk_speed: 80.0,
        }
    }
}
