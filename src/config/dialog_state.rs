use crate::config::schema::{Config, PetConfig};
use std::path::PathBuf;

// ─── SpriteKey ────────────────────────────────────────────────────────────────

/// Identifies a sprite in the gallery.
#[derive(Debug, Clone, PartialEq)]
pub enum SpriteKey {
    /// A sprite bundled with the app, referenced by its asset stem (e.g. "esheep").
    Embedded(String),
    /// A user-installed sprite, referenced by absolute path to its .json file.
    Installed(PathBuf),
}

impl SpriteKey {
    /// Returns the `sheet_path` string stored in `PetConfig`.
    pub fn to_sheet_path(&self) -> String {
        match self {
            SpriteKey::Embedded(stem) => format!("embedded://{stem}"),
            SpriteKey::Installed(p) => p.to_string_lossy().into_owned(),
        }
    }

    /// Parses a `sheet_path` string back into a `SpriteKey`.
    pub fn from_sheet_path(s: &str) -> Self {
        if let Some(stem) = s.strip_prefix("embedded://") {
            SpriteKey::Embedded(stem.to_string())
        } else {
            SpriteKey::Installed(PathBuf::from(s))
        }
    }
}

// ─── DialogResult ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DialogResult {
    None,
    Ok,
    Cancel,
}

// ─── ConfigDialogState ───────────────────────────────────────────────────────

pub struct ConfigDialogState {
    pub config: Config,
    /// Index into `config.pets` for the currently selected pet chip.
    pub selected: usize,
    /// Currently highlighted gallery entry.
    pub selected_sprite: SpriteKey,
    pub result: DialogResult,
}

impl ConfigDialogState {
    pub fn new(config: Config) -> Self {
        let selected_sprite = config
            .pets
            .first()
            .map(|p| SpriteKey::from_sheet_path(&p.sheet_path))
            .unwrap_or_else(|| SpriteKey::Embedded("esheep".into()));
        ConfigDialogState { config, selected: 0, selected_sprite, result: DialogResult::None }
    }

    pub fn selected_pet(&self) -> Option<&PetConfig> {
        self.config.pets.get(self.selected)
    }

    fn selected_pet_mut(&mut self) -> Option<&mut PetConfig> {
        self.config.pets.get_mut(self.selected)
    }

    pub fn add_pet(&mut self) {
        let n = self.config.pets.len();
        self.config.pets.push(PetConfig { id: format!("pet_{n}"), ..PetConfig::default() });
        self.selected = self.config.pets.len() - 1;
    }

    pub fn remove_selected(&mut self) {
        if self.config.pets.is_empty() {
            return;
        }
        self.config.pets.remove(self.selected);
        if !self.config.pets.is_empty() && self.selected >= self.config.pets.len() {
            self.selected = self.config.pets.len() - 1;
        }
    }

    pub fn select(&mut self, index: usize) {
        if index < self.config.pets.len() {
            self.selected = index;
        }
    }

    /// Update the currently selected sprite and write its path to the selected pet's config.
    pub fn select_sprite(&mut self, key: SpriteKey) {
        let path = key.to_sheet_path();
        self.selected_sprite = key;
        self.update_sheet_path(path);
    }

    pub fn update_sheet_path(&mut self, path: String) {
        if let Some(p) = self.selected_pet_mut() {
            p.sheet_path = path;
        }
    }

    /// Returns `true` if scale was valid (1–4).
    pub fn update_scale(&mut self, s: &str) -> bool {
        match Self::parse_scale(s) {
            Some(v) => {
                if let Some(p) = self.selected_pet_mut() {
                    p.scale = v;
                }
                true
            }
            None => false,
        }
    }

    pub fn parse_scale(s: &str) -> Option<u32> {
        let v: u32 = s.trim().parse().ok()?;
        if (1..=4).contains(&v) { Some(v) } else { None }
    }

    pub fn update_x(&mut self, s: &str) -> bool {
        match s.trim().parse::<i32>() {
            Ok(v) => {
                if let Some(p) = self.selected_pet_mut() {
                    p.x = v;
                }
                true
            }
            Err(_) => false,
        }
    }

    pub fn update_y(&mut self, s: &str) -> bool {
        match s.trim().parse::<i32>() {
            Ok(v) => {
                if let Some(p) = self.selected_pet_mut() {
                    p.y = v;
                }
                true
            }
            Err(_) => false,
        }
    }

    /// Returns `true` if speed was valid (1.0–500.0 inclusive).
    /// Does not mutate state on invalid input.
    pub fn update_walk_speed(&mut self, s: &str) -> bool {
        let v: f32 = match s.trim().parse() {
            Ok(f) => f,
            Err(_) => return false,
        };
        if !v.is_finite() || v < 1.0 || v > 500.0 {
            return false;
        }
        if let Some(p) = self.selected_pet_mut() {
            p.walk_speed = v;
        }
        true
    }

    pub fn accept(&mut self) {
        self.result = DialogResult::Ok;
    }

    pub fn cancel(&mut self) {
        self.result = DialogResult::Cancel;
    }
}
