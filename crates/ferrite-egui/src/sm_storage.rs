/// Platform-agnostic interface for reading and writing state machine source files.
/// Desktop: wraps `SmGallery` + filesystem operations.
/// Web: in-memory `HashMap<String, String>`.
pub trait SmStorage: Send + Sync {
    /// Returns all valid SM names (excluding drafts).
    fn list_names(&self) -> Vec<String>;
    /// Returns the TOML source for the named SM, or `None` if not found.
    fn load(&self, name: &str) -> Option<String>;
    /// Saves source under `name`. Returns an error string on failure.
    fn save(&self, name: &str, source: &str) -> Result<(), String>;
    /// Deletes the SM named `name`. Returns an error string on failure.
    fn delete(&self, name: &str) -> Result<(), String>;
}
