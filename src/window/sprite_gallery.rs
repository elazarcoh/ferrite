//! Gallery discovery, thumbnail loading, and custom sprite install.
//!
//! `load_thumbnail` and `destroy_thumbnails` use Win32 GDI and must be called
//! from the Win32 thread. All other methods are pure Rust.

use crate::assets::{self, Assets};
use crate::sprite::sheet::load_embedded;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

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

#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP,
    SRCCOPY, BI_RGB,
};

// ─── Public types ─────────────────────────────────────────────────────────────

/// Whether a sprite is bundled with the app or user-installed.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceKind {
    BuiltIn,
    Custom,
}

#[derive(Clone)]
pub struct GalleryEntry {
    pub key: SpriteKey,
    pub display_name: String,
    pub source: SourceKind,
    /// 28×28 HBITMAP thumbnail; `None` until `load_thumbnail` is called.
    /// Cloning copies the handle value — only safe when thumbnail is None
    /// (e.g., freshly installed entries). Never clone an entry with a live thumbnail.
    #[cfg(target_os = "windows")]
    pub thumbnail: Option<HBITMAP>,
    #[cfg(not(target_os = "windows"))]
    pub thumbnail: Option<()>,
}

pub struct SpriteGallery {
    /// Real sprite entries only. The Browse sentinel is rendered separately by the dialog.
    pub entries: Vec<GalleryEntry>,
}

/// Sentinel last entry — not a real sprite.
pub struct BrowseEntry;

// ─── Embedded asset stems ────────────────────────────────────────────────────

/// Collect stems of embedded sprites that have BOTH a .json and a .png.
fn embedded_stems() -> Vec<String> {
    let mut jsons: std::collections::HashSet<String> = Default::default();
    let mut pngs: std::collections::HashSet<String> = Default::default();
    for path in Assets::iter() {
        let path = path.as_ref();
        if let Some(stem) = path.strip_suffix(".json") {
            jsons.insert(stem.to_string());
        } else if let Some(stem) = path.strip_suffix(".png") {
            pngs.insert(stem.to_string());
        }
    }
    let mut stems: Vec<String> = jsons
        .intersection(&pngs)
        .cloned()
        .collect();
    stems.sort();
    stems
}

// ─── SpriteGallery ───────────────────────────────────────────────────────────

impl SpriteGallery {
    /// Discover all available sprites (embedded + installed).
    /// Does NOT load thumbnails — call `load_thumbnail` per entry before painting.
    pub fn load() -> Self {
        let mut entries: Vec<GalleryEntry> = Vec::new();

        // Embedded sprites
        for stem in embedded_stems() {
            let display_name = if stem == "test_pet" {
                "arrows".to_string()
            } else {
                stem.clone()
            };
            entries.push(GalleryEntry {
                key: SpriteKey::Embedded(stem.clone()),
                display_name,
                source: SourceKind::BuiltIn,
                thumbnail: None,
            });
        }

        // Installed custom sprites
        let dir = Self::appdata_sprites_dir();
        if dir.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                let mut custom: Vec<PathBuf> = rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
                    .collect();
                custom.sort();
                for json_path in custom {
                    let stem = json_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    entries.push(GalleryEntry {
                        display_name: stem.clone(),
                        key: SpriteKey::Installed(json_path),
                        source: SourceKind::Custom,
                        thumbnail: None,
                    });
                }
            }
        }

        SpriteGallery { entries }
    }

    /// Returns the directory where custom sprites are stored.
    /// In tests, overridable via `MY_PET_SPRITES_DIR` environment variable.
    pub fn appdata_sprites_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("MY_PET_SPRITES_DIR") {
            return PathBuf::from(dir);
        }
        let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join("my-pet").join("sprites")
    }

    /// Validate and copy a `.json` + adjacent `.png` into the sprites directory.
    /// Returns a new `GalleryEntry` with `thumbnail = None`; caller must call
    /// `load_thumbnail` before painting.
    pub fn install(json_path: &Path) -> Result<GalleryEntry> {
        let json_bytes = std::fs::read(json_path)
            .with_context(|| format!("read {}", json_path.display()))?;
        let stem = json_path
            .file_stem()
            .ok_or_else(|| anyhow!("json_path has no stem"))?
            .to_string_lossy()
            .into_owned();
        let png_path = json_path.with_extension("png");
        if !png_path.exists() {
            return Err(anyhow!("missing PNG adjacent to JSON: {}", png_path.display()));
        }
        let png_bytes = std::fs::read(&png_path)
            .with_context(|| format!("read {}", png_path.display()))?;
        // Validate JSON is a real SpriteSheet
        load_embedded(&json_bytes, &png_bytes)
            .with_context(|| format!("invalid sprite at {}", json_path.display()))?;

        // Copy to sprites directory
        let dest_dir = Self::appdata_sprites_dir();
        std::fs::create_dir_all(&dest_dir).context("create sprites dir")?;
        let dest_json = dest_dir.join(format!("{stem}.json"));
        let dest_png = dest_dir.join(format!("{stem}.png"));
        std::fs::copy(json_path, &dest_json)
            .with_context(|| format!("copy JSON to {}", dest_json.display()))?;
        std::fs::copy(&png_path, &dest_png)
            .with_context(|| format!("copy PNG to {}", dest_png.display()))?;

        Ok(GalleryEntry {
            key: SpriteKey::Installed(dest_json),
            display_name: stem,
            source: SourceKind::Custom,
            thumbnail: None,
        })
    }

    /// Load a 28×28 thumbnail HBITMAP for the given entry.
    /// Stores the result in `entry.thumbnail`. No-op if already loaded.
    /// Must be called from the Win32 thread.
    #[cfg(target_os = "windows")]
    pub fn load_thumbnail(entry: &mut GalleryEntry) {
        if entry.thumbnail.is_some() {
            return;
        }
        let sheet = match &entry.key {
            SpriteKey::Embedded(stem) => {
                let Some((json, png)) = assets::embedded_sheet(stem) else { return };
                match load_embedded(&json, &png) {
                    Ok(s) => s,
                    Err(_) => return,
                }
            }
            SpriteKey::Installed(path) => {
                let Ok(json) = std::fs::read(path) else { return };
                let Ok(png) = std::fs::read(path.with_extension("png")) else { return };
                match load_embedded(&json, &png) {
                    Ok(s) => s,
                    Err(_) => return,
                }
            }
        };

        // Find idle tag frame index; fall back to frame 0
        let frame_idx = sheet
            .tags
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case("idle"))
            .map(|t| t.from)
            .unwrap_or(0);
        let Some(frame) = sheet.frames.get(frame_idx) else { return };

        // Convert RGBA → BGRA (Win32 DIB order)
        let img_w = sheet.image.width() as i32;
        let img_h = sheet.image.height() as i32;
        let bgra: Vec<u8> = sheet
            .image
            .pixels()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();

        unsafe {
            // Create a 28×28 top-down DIBSection as the render target
            let hdc_screen = GetDC(std::ptr::null_mut());
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = 28;
            bmi.bmiHeader.biHeight = -28; // top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB as u32;

            let mut bits = std::ptr::null_mut();
            let hbmp = CreateDIBSection(
                hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0,
            );
            if hbmp.is_null() {
                DeleteDC(hdc_mem);
                ReleaseDC(std::ptr::null_mut(), hdc_screen);
                return;
            }
            let old_bmp = SelectObject(hdc_mem, hbmp as *mut _);

            // Source BITMAPINFO for StretchDIBits (full spritesheet image)
            let mut src_bmi: BITMAPINFO = std::mem::zeroed();
            src_bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            src_bmi.bmiHeader.biWidth = img_w;
            src_bmi.bmiHeader.biHeight = -img_h; // top-down
            src_bmi.bmiHeader.biPlanes = 1;
            src_bmi.bmiHeader.biBitCount = 32;
            src_bmi.bmiHeader.biCompression = BI_RGB as u32;

            StretchDIBits(
                hdc_mem,
                0, 0, 28, 28,                                 // dest rect
                frame.x as i32, frame.y as i32,
                frame.w as i32, frame.h as i32,               // src rect (frame)
                bgra.as_ptr() as *const _,
                &src_bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );

            // Deselect before deleting — deleting a selected object is a GDI no-op (leak).
            SelectObject(hdc_mem, old_bmp as *mut _);
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            DeleteDC(hdc_mem);

            entry.thumbnail = Some(hbmp);
        }
    }

    /// Delete all GDI HBITMAP handles. Must be called from `WM_DESTROY`.
    #[cfg(target_os = "windows")]
    pub fn destroy_thumbnails(&mut self) {
        for entry in &mut self.entries {
            if let Some(hbmp) = entry.thumbnail.take() {
                unsafe {
                    DeleteObject(hbmp as *mut _);
                }
            }
        }
    }
}
