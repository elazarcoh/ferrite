use ferrite::config::{config_path, load, save, schema::Config};
use tempfile::tempdir;

#[test]
fn default_config_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = Config::default();
    save(&path, &original).expect("save");
    let loaded = load(&path).expect("load");
    assert_eq!(original, loaded);
}

#[test]
fn config_path_inside_localappdata() {
    let path = config_path();
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    assert!(
        path.starts_with(&local),
        "config_path {:?} should start with LOCALAPPDATA {:?}",
        path,
        local
    );
    assert!(path.ends_with("config.toml"));
}

#[test]
fn load_missing_returns_default() {
    let path = std::path::Path::new("C:/nonexistent/path/config.toml");
    let cfg = load(path).expect("should return default");
    assert_eq!(cfg, Config::default());
}

#[test]
fn multi_pet_roundtrip() {
    use ferrite::config::schema::PetConfig;
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut cfg = Config::default();
    cfg.pets.push(PetConfig { id: "pet_1".into(), x: 200, y: 400, ..PetConfig::default() });
    save(&path, &cfg).unwrap();
    let loaded = load(&path).unwrap();
    assert_eq!(loaded.pets.len(), 2);
    assert_eq!(loaded.pets[1].x, 200);
}
