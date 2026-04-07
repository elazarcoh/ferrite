use ferrite_core::sprite::sheet::SpriteSheet;

/// A platform-agnostic sprite gallery entry.
/// On desktop, produced from `SpriteGallery`; on web, built from the in-memory asset store.
#[derive(Clone, Debug, PartialEq)]
pub struct GalleryEntry {
    /// Canonical sheet path: `"embedded://esheep"` or an absolute filesystem path.
    pub key: String,
    /// Display name shown in the UI.
    pub display_name: String,
}

/// Loads a `SpriteSheet` from a sheet path.
/// Desktop: reads from disk/embedded assets.
/// Web: reads from the in-memory asset store.
pub trait SheetLoader: Send + Sync {
    fn load_sheet(&self, path: &str) -> anyhow::Result<SpriteSheet>;
}
