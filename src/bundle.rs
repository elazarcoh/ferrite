use std::io::{Read, Write, Cursor};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct BundleMeta {
    name: String,
    author: Option<String>,
    version: String,
    recommended_sm: Option<String>,
}

pub struct BundleContents {
    pub bundle_name: String,
    pub sprite_json: String,
    pub sprite_png: Vec<u8>,
    pub sm_source: Option<String>,       // .petstate text
    pub recommended_sm: Option<String>,  // SM name from bundle.toml
}

/// Import a .petbundle file from bytes.
pub fn import(data: &[u8]) -> Result<BundleContents, String> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let meta: BundleMeta = {
        let mut f = archive.by_name("bundle.toml").map_err(|_| "missing bundle.toml")?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        toml::from_str(&s).map_err(|e| e.to_string())?
    };

    let sprite_json = read_text(&mut archive, "sprite.json")?;
    let sprite_png  = read_bytes(&mut archive, "sprite.png")?;
    let sm_source   = read_text(&mut archive, "behavior.petstate").ok();

    Ok(BundleContents {
        bundle_name: meta.name,
        sprite_json,
        sprite_png,
        sm_source,
        recommended_sm: meta.recommended_sm,
    })
}

/// Export a .petbundle to bytes.
pub fn export(
    bundle_name: &str,
    author: Option<&str>,
    sprite_json: &str,
    sprite_png: &[u8],
    sm_source: Option<&str>,
    recommended_sm: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default();

        // bundle.toml
        let meta = BundleMeta {
            name: bundle_name.to_string(),
            author: author.map(|s| s.to_string()),
            version: "1.0".to_string(),
            recommended_sm: recommended_sm.map(|s| s.to_string()),
        };
        let meta_toml = toml::to_string(&meta).map_err(|e| e.to_string())?;
        zip.start_file("bundle.toml", options).map_err(|e| e.to_string())?;
        zip.write_all(meta_toml.as_bytes()).map_err(|e| e.to_string())?;

        // sprite.json
        zip.start_file("sprite.json", options).map_err(|e| e.to_string())?;
        zip.write_all(sprite_json.as_bytes()).map_err(|e| e.to_string())?;

        // sprite.png
        zip.start_file("sprite.png", options).map_err(|e| e.to_string())?;
        zip.write_all(sprite_png).map_err(|e| e.to_string())?;

        // behavior.petstate (optional)
        if let Some(sm) = sm_source {
            zip.start_file("behavior.petstate", options).map_err(|e| e.to_string())?;
            zip.write_all(sm.as_bytes()).map_err(|e| e.to_string())?;
        }

        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf)
}

fn read_text(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String, String> {
    let mut f = archive.by_name(name).map_err(|_| format!("missing {}", name))?;
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

fn read_bytes(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>, String> {
    let mut f = archive.by_name(name).map_err(|_| format!("missing {}", name))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_sprite_only() {
        let json = r#"{"frames": [], "meta": {"app": "test", "version": "1.0"}}"#;
        let png = vec![137u8, 80, 78, 71, 13, 10, 26, 10]; // PNG header

        let data = export("Test Bundle", None, json, &png, None, None).unwrap();
        let contents = import(&data).unwrap();

        assert_eq!(contents.bundle_name, "Test Bundle");
        assert_eq!(contents.sprite_json, json);
        assert_eq!(contents.sprite_png, png);
        assert!(contents.sm_source.is_none());
    }

    #[test]
    fn round_trip_with_sm() {
        let json = r#"{"frames": [], "meta": {"app": "test", "version": "1.0"}}"#;
        let png = vec![137u8, 80, 78, 71, 13, 10, 26, 10];
        let sm = "[meta]\nname = \"Test\"";

        let data = export("Bundle", Some("author"), json, &png, Some(sm), Some("Test")).unwrap();
        let contents = import(&data).unwrap();

        assert_eq!(contents.sm_source.as_deref(), Some(sm));
        assert_eq!(contents.recommended_sm.as_deref(), Some("Test"));
    }

    #[test]
    fn import_invalid_data_returns_err() {
        let result = import(b"not a zip file");
        assert!(result.is_err());
    }
}
