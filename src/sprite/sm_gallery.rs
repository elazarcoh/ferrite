use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::sprite::sm_compiler::{compile, CompiledSM, CompileError};
use crate::sprite::sm_format::SmFile;

/// A loaded SM entry — either valid (compiled) or a draft (has errors).
pub enum SmEntry {
    Valid { name: String, path: PathBuf, source: String, sm: Arc<CompiledSM> },
    Draft { name: String, path: PathBuf, source: String, errors: Vec<CompileError> },
}

impl SmEntry {
    pub fn name(&self) -> &str {
        match self {
            SmEntry::Valid { name, .. } => name,
            SmEntry::Draft { name, .. } => name,
        }
    }
}

pub struct SmGallery {
    dir: PathBuf,
    draft_dir: PathBuf,
    entries: Vec<SmEntry>,
}

impl SmGallery {
    /// Load all .petstate and .draft.petstate files from base_dir/state_machines/
    pub fn load(base_dir: &Path) -> Self {
        let dir = base_dir.join("state_machines");
        let draft_dir = dir.join("drafts");
        let mut gallery = SmGallery { dir: dir.clone(), draft_dir: draft_dir.clone(), entries: Vec::new() };

        // Load live .petstate files
        if dir.exists()
            && let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("petstate")
                        && let Ok(source) = std::fs::read_to_string(&path) {
                            gallery.load_entry(path, source, false);
                        }
                }
            }

        // Load draft .draft.petstate files
        if draft_dir.exists()
            && let Ok(entries) = std::fs::read_dir(&draft_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if ext.ends_with(".draft.petstate")
                        && let Ok(source) = std::fs::read_to_string(&path) {
                            gallery.load_entry(path, source, true);
                        }
                }
            }

        gallery
    }

    fn load_entry(&mut self, path: PathBuf, source: String, _is_draft: bool) {
        let name = extract_sm_name(&source)
            .unwrap_or_else(|| path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string());

        match toml::from_str::<SmFile>(&source) {
            Ok(file) => {
                match compile(&file) {
                    Ok(sm) => {
                        self.entries.push(SmEntry::Valid { name, path, source, sm });
                    }
                    Err(errors) => {
                        self.entries.push(SmEntry::Draft { name, path, source, errors });
                    }
                }
            }
            Err(e) => {
                self.entries.push(SmEntry::Draft {
                    name,
                    path,
                    source,
                    errors: vec![CompileError::ConditionParseError("(parse)".to_string(), e.to_string())],
                });
            }
        }
    }

    /// List of valid SM names (for selection UI).
    pub fn valid_names(&self) -> Vec<&str> {
        self.entries.iter()
            .filter_map(|e| if let SmEntry::Valid { name, .. } = e { Some(name.as_str()) } else { None })
            .collect()
    }

    /// List of draft names (for SM editor browser).
    pub fn draft_names(&self) -> Vec<&str> {
        self.entries.iter()
            .filter_map(|e| if let SmEntry::Draft { name, .. } = e { Some(name.as_str()) } else { None })
            .collect()
    }

    /// Get a compiled SM by name (valid SMs only).
    pub fn get(&self, name: &str) -> Option<Arc<CompiledSM>> {
        self.entries.iter().find_map(|e| {
            if let SmEntry::Valid { name: n, sm, .. } = e {
                if n == name { Some(sm.clone()) } else { None }
            } else { None }
        })
    }

    /// Get raw TOML source for a valid SM by name.
    pub fn source(&self, name: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| {
            if let SmEntry::Valid { name: n, source, .. } = e {
                if n == name { Some(source.as_str()) } else { None }
            } else { None }
        })
    }

    /// Get raw TOML source for a draft SM by name.
    pub fn draft_source(&self, name: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| {
            if let SmEntry::Draft { name: n, source, .. } = e {
                if n == name { Some(source.as_str()) } else { None }
            } else { None }
        })
    }

    /// Get compile errors for a draft SM by name.
    pub fn draft_errors(&self, name: &str) -> &[CompileError] {
        self.entries.iter().find_map(|e| {
            if let SmEntry::Draft { name: n, errors, .. } = e {
                if n == name { Some(errors.as_slice()) } else { None }
            } else { None }
        }).unwrap_or(&[])
    }

    /// Check for name collision with existing entries.
    pub fn name_exists(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.name() == name)
    }

    /// Save source as live SM or draft depending on validation.
    /// Returns Ok(true) if saved as live, Ok(false) if saved as draft.
    pub fn save(&mut self, name: &str, source: &str) -> Result<bool, std::io::Error> {
        std::fs::create_dir_all(&self.dir)?;
        std::fs::create_dir_all(&self.draft_dir)?;

        // Remove existing entry with same name
        self.entries.retain(|e| e.name() != name);

        // Try to compile
        let is_valid = match toml::from_str::<SmFile>(source) {
            Ok(file) => compile(&file).is_ok(),
            Err(_) => false,
        };

        if is_valid {
            let filename = format!("{}.petstate", sanitize_filename(name));
            let path = self.dir.join(&filename);
            std::fs::write(&path, source)?;
            // Remove any draft version
            let draft_filename = format!("{}.draft.petstate", sanitize_filename(name));
            let draft_path = self.draft_dir.join(&draft_filename);
            let _ = std::fs::remove_file(&draft_path);

            let file: SmFile = toml::from_str(source).unwrap();
            let sm = compile(&file).unwrap();
            self.entries.push(SmEntry::Valid { name: name.to_string(), path, source: source.to_string(), sm });
            Ok(true)
        } else {
            let filename = format!("{}.draft.petstate", sanitize_filename(name));
            let path = self.draft_dir.join(&filename);
            std::fs::write(&path, source)?;

            let errors = match toml::from_str::<SmFile>(source) {
                Ok(file) => compile(&file).unwrap_err(),
                Err(e) => vec![CompileError::ConditionParseError("(parse)".to_string(), e.to_string())],
            };
            self.entries.push(SmEntry::Draft { name: name.to_string(), path, source: source.to_string(), errors });
            Ok(false)
        }
    }

    /// Delete a SM (valid or draft) by name. Removes the file and the in-memory entry.
    pub fn delete(&mut self, name: &str) -> std::io::Result<()> {
        let entry_idx = self.entries.iter().position(|e| e.name() == name);
        if let Some(idx) = entry_idx {
            let path = match &self.entries[idx] {
                SmEntry::Valid { path, .. } => path.clone(),
                SmEntry::Draft { path, .. } => path.clone(),
            };
            let _ = std::fs::remove_file(&path); // ignore error if already missing
            self.entries.remove(idx);
        }
        Ok(())
    }

    /// Import a .petstate file. Returns Err(collision_name) if name already exists.
    pub fn import(&mut self, source: &str) -> Result<bool, String> {
        let name = extract_sm_name(source)
            .ok_or_else(|| "Cannot parse SM name from source".to_string())?;
        if self.name_exists(&name) {
            return Err(name);
        }
        self.save(&name, source).map_err(|e| e.to_string())
    }
}

fn extract_sm_name(source: &str) -> Option<String> {
    toml::from_str::<SmFile>(source).ok().map(|f| f.meta.name)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn valid_sm_toml() -> &'static str {
        r#"
[meta]
name = "Test SM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
transitions = []

[states.grabbed]
required = true
action = "grabbed"
transitions = []

[states.fall]
required = true
action = "fall"
transitions = []

[states.thrown]
required = true
action = "thrown"
transitions = []
"#
    }

    fn invalid_sm_toml() -> &'static str {
        r#"
[meta]
name = "Bad SM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
action = "idle"
"#  // missing required = true and engine primitives
    }

    #[test]
    fn save_valid_sm_appears_in_valid_list() {
        let dir = tempdir().unwrap();
        let mut gallery = SmGallery::load(dir.path());
        let result = gallery.save("Test SM", valid_sm_toml());
        assert_eq!(result.unwrap(), true);
        assert!(gallery.valid_names().contains(&"Test SM"));
    }

    #[test]
    fn save_invalid_sm_appears_in_draft_list() {
        let dir = tempdir().unwrap();
        let mut gallery = SmGallery::load(dir.path());
        let result = gallery.save("Bad SM", invalid_sm_toml());
        assert_eq!(result.unwrap(), false);
        assert!(gallery.draft_names().contains(&"Bad SM"));
    }

    #[test]
    fn import_collision_returns_err() {
        let dir = tempdir().unwrap();
        let mut gallery = SmGallery::load(dir.path());
        gallery.save("Test SM", valid_sm_toml()).unwrap();
        let result = gallery.import(valid_sm_toml());
        assert!(result.is_err());
    }
}
