/// E2E: config load → pet creation → hot-reload cycle.
use ferrite::{
    app::PetInstance,
    config::{load, save, schema::{Config, PetConfig}},
    sprite::sheet::load_embedded,
};
use tempfile::tempdir;

fn test_sheet() -> ferrite::sprite::sheet::SpriteSheet {
    load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap()
}

#[test]
fn pet_instance_uses_config_position() {
    // x comes from config; y is overridden to spawn above the screen (negative).
    let cfg = PetConfig { x: 300, y: 400, ..PetConfig::default() };
    let pet = PetInstance::new(cfg.clone(), test_sheet()).unwrap();
    assert_eq!(pet.x, cfg.x);
    assert!(pet.y < 0, "pet should spawn above screen (y < 0), got {}", pet.y);
}

#[test]
fn pet_instance_uses_config_scale() {
    // scale=1 → window 32×32; scale=2 (default) → 64×64
    let cfg_s1 = PetConfig { scale: 1.0, ..PetConfig::default() };
    let pet1 = PetInstance::new(cfg_s1, test_sheet()).unwrap();
    assert_eq!(pet1.window_width(), 32);

    let cfg_s2 = PetConfig { scale: 2.0, ..PetConfig::default() };
    let pet2 = PetInstance::new(cfg_s2, test_sheet()).unwrap();
    assert_eq!(pet2.window_width(), 64);
}

#[test]
fn config_roundtrip_preserves_all_pet_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = Config {
        pets: vec![
            PetConfig { id: "a".into(), x: 10, y: 20, scale: 3.0, walk_speed: 42.0, ..PetConfig::default() },
            PetConfig { id: "b".into(), x: 100, y: 200, ..PetConfig::default() },
        ],
    };
    save(&path, &original).unwrap();
    let loaded = load(&path).unwrap();
    assert_eq!(loaded, original);
}

#[test]
fn loading_missing_config_returns_single_default_pet() {
    let cfg = load(std::path::Path::new("C:/no/such/path.toml")).unwrap();
    assert_eq!(cfg.pets.len(), 1);
    assert_eq!(cfg.pets[0].sheet_path, "embedded://esheep");
}
