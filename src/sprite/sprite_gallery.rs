use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Default)]
struct GalleryFile {
    #[serde(default)]
    sprites: Vec<SpriteEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SpriteEntry {
    pub id: String,
    pub json_path: String,
    pub png_path: String,
    pub recommended_sm: Option<String>,  // SM name (not path); omitted if none
}

pub struct SpriteGallery {
    dir: PathBuf,
    entries: Vec<SpriteEntry>,
}

impl SpriteGallery {
    pub fn load(base_dir: &Path) -> Self {
        let dir = base_dir.join("sprites");
        let gallery_path = dir.join("gallery.toml");

        let entries = if gallery_path.exists() {
            std::fs::read_to_string(&gallery_path)
                .ok()
                .and_then(|s| toml::from_str::<GalleryFile>(&s).ok())
                .map(|f| f.sprites)
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        SpriteGallery { dir, entries }
    }

    #[allow(dead_code)]
    pub fn all(&self) -> &[SpriteEntry] {
        &self.entries
    }

    #[allow(dead_code)]
    pub fn get_by_id(&self, id: &str) -> Option<&SpriteEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn add(&mut self, entry: SpriteEntry) -> Result<(), std::io::Error> {
        // Replace if id already exists, otherwise append
        if let Some(pos) = self.entries.iter().position(|e| e.id == entry.id) {
            self.entries[pos] = entry;
        } else {
            self.entries.push(entry);
        }
        self.save_gallery_file()
    }

    #[allow(dead_code)]
    pub fn set_recommended_sm(&mut self, sprite_id: &str, sm_name: Option<&str>) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == sprite_id) {
            entry.recommended_sm = sm_name.map(|s| s.to_string());
            let _ = self.save_gallery_file();
        }
    }

    fn save_gallery_file(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.dir)?;
        let file = GalleryFile { sprites: self.entries.clone() };
        let toml = toml::to_string_pretty(&file)
            .map_err(std::io::Error::other)?;
        std::fs::write(self.dir.join("gallery.toml"), toml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sprite_gallery_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_empty_gallery() {
        let dir = make_temp_dir();
        let gallery = SpriteGallery::load(&dir);
        assert_eq!(gallery.all().len(), 0);
    }

    #[test]
    fn add_and_retrieve_entry() {
        let dir = make_temp_dir();
        let mut gallery = SpriteGallery::load(&dir);
        let entry = SpriteEntry {
            id: "test-sprite".to_string(),
            json_path: "test.json".to_string(),
            png_path: "test.png".to_string(),
            recommended_sm: None,
        };
        gallery.add(entry.clone()).unwrap();
        assert!(gallery.get_by_id("test-sprite").is_some());
    }

    #[test]
    fn persists_across_load() {
        let dir = make_temp_dir();
        {
            let mut gallery = SpriteGallery::load(&dir);
            gallery.add(SpriteEntry {
                id: "cat".to_string(),
                json_path: "cat.json".to_string(),
                png_path: "cat.png".to_string(),
                recommended_sm: Some("Cat Behavior".to_string()),
            }).unwrap();
        }
        // Load again — should have the entry
        let gallery = SpriteGallery::load(&dir);
        let entry = gallery.get_by_id("cat").unwrap();
        assert_eq!(entry.recommended_sm.as_deref(), Some("Cat Behavior"));
    }
}
