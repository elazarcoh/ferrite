use ferrite_egui::sm_storage::SmStorage;
use ferrite_egui::gallery::{GalleryEntry, SheetLoader};
use ferrite_core::sprite::sheet::SpriteSheet;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct WebSmStorage {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

impl WebSmStorage {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }
}

impl SmStorage for WebSmStorage {
    fn list_names(&self) -> Vec<String> {
        self.inner.lock().unwrap().keys().cloned().collect()
    }
    fn load(&self, name: &str) -> Option<String> {
        self.inner.lock().unwrap().get(name).cloned()
    }
    fn save(&self, name: &str, source: &str) -> Result<(), String> {
        self.inner.lock().unwrap().insert(name.to_string(), source.to_string());
        Ok(())
    }
    fn delete(&self, name: &str) -> Result<(), String> {
        self.inner.lock().unwrap().remove(name);
        Ok(())
    }
}

pub struct WebSheetLoader {
    sheets: Arc<Mutex<HashMap<String, (Vec<u8>, Vec<u8>)>>>,
}

impl WebSheetLoader {
    pub fn new() -> Self {
        Self { sheets: Arc::new(Mutex::new(HashMap::new())) }
    }

    #[allow(dead_code)]
    pub fn register(&self, path: String, json: Vec<u8>, png: Vec<u8>) {
        self.sheets.lock().unwrap().insert(path, (json, png));
    }
}

/// Newtype so we can implement the foreign `SheetLoader` trait for an `Arc<WebSheetLoader>`.
pub struct SharedWebSheetLoader(pub Arc<WebSheetLoader>);

impl SheetLoader for SharedWebSheetLoader {
    fn load_sheet(&self, path: &str) -> anyhow::Result<SpriteSheet> {
        self.0.load_sheet(path)
    }
}

impl SheetLoader for WebSheetLoader {
    fn load_sheet(&self, path: &str) -> anyhow::Result<SpriteSheet> {
        if let Some(stem) = path.strip_prefix("embedded://") {
            return load_embedded_sheet(stem);
        }
        let map = self.sheets.lock().unwrap();
        let (json, png) = map.get(path)
            .ok_or_else(|| anyhow::anyhow!("sheet not found: {path}"))?;
        let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)?.into_rgba8();
        SpriteSheet::from_json_and_image(json, image)
    }
}

fn load_embedded_sheet(stem: &str) -> anyhow::Result<SpriteSheet> {
    match stem {
        "esheep" => {
            let json: &[u8] = include_bytes!("../assets/esheep.json");
            let png: &[u8] = include_bytes!("../assets/esheep.png");
            let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)?.into_rgba8();
            ferrite_core::sprite::sheet::SpriteSheet::from_json_and_image(json, image)
        }
        _ => anyhow::bail!("unknown embedded sheet: {stem}"),
    }
}

pub fn load_sheet_for_config(path: &str) -> anyhow::Result<SpriteSheet> {
    WebSheetLoader::new().load_sheet(path)
}

pub fn build_gallery() -> Vec<GalleryEntry> {
    vec![
        GalleryEntry { key: "embedded://esheep".to_string(), display_name: "eSheep".to_string() },
    ]
}
