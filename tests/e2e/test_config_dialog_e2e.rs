/// Tests for ConfigDialogState — the pure-Rust model driving the config dialog.
/// No Win32 calls involved; runs on any machine.
use my_pet::{
    config::schema::{Config, PetConfig},
    tray::config_window::{ConfigDialogState, DialogResult},
};
use my_pet::config::dialog_state::SpriteKey;
use std::path::PathBuf;

fn two_pet_config() -> Config {
    Config {
        pets: vec![
            PetConfig { id: "a".into(), x: 10, y: 20, scale: 1, ..PetConfig::default() },
            PetConfig { id: "b".into(), x: 200, y: 300, scale: 2, ..PetConfig::default() },
        ],
    }
}

#[test]
fn new_state_result_is_none() {
    let state = ConfigDialogState::new(Config::default());
    assert_eq!(state.result, DialogResult::None);
}

#[test]
fn new_state_selects_first_pet() {
    let state = ConfigDialogState::new(two_pet_config());
    assert_eq!(state.selected, 0);
    assert_eq!(state.selected_pet().unwrap().id, "a");
}

#[test]
fn add_pet_increments_count() {
    let mut state = ConfigDialogState::new(Config::default());
    let before = state.config.pets.len();
    state.add_pet();
    assert_eq!(state.config.pets.len(), before + 1);
}

#[test]
fn add_pet_selects_new_pet() {
    let mut state = ConfigDialogState::new(two_pet_config());
    state.add_pet();
    assert_eq!(state.selected, 2);
}

#[test]
fn remove_selected_decrements_count() {
    let mut state = ConfigDialogState::new(two_pet_config());
    state.remove_selected();
    assert_eq!(state.config.pets.len(), 1);
}

#[test]
fn remove_selected_clamps_index_when_last() {
    let mut state = ConfigDialogState::new(two_pet_config());
    state.select(1);
    state.remove_selected();
    assert_eq!(state.selected, 0);
    assert_eq!(state.config.pets.len(), 1);
}

#[test]
fn remove_from_empty_is_noop() {
    let mut state = ConfigDialogState::new(Config { pets: vec![] });
    state.remove_selected(); // must not panic
    assert_eq!(state.config.pets.len(), 0);
}

#[test]
fn select_changes_index() {
    let mut state = ConfigDialogState::new(two_pet_config());
    state.select(1);
    assert_eq!(state.selected, 1);
    assert_eq!(state.selected_pet().unwrap().id, "b");
}

#[test]
fn select_out_of_bounds_is_noop() {
    let mut state = ConfigDialogState::new(two_pet_config());
    state.select(99);
    assert_eq!(state.selected, 0);
}

#[test]
fn update_sheet_path_changes_selected_pet() {
    let mut state = ConfigDialogState::new(two_pet_config());
    state.update_sheet_path("C:/my/sheet.json".into());
    assert_eq!(state.config.pets[0].sheet_path, "C:/my/sheet.json");
}

#[test]
fn update_scale_valid_range() {
    let mut state = ConfigDialogState::new(Config::default());
    assert!(state.update_scale("1"));
    assert_eq!(state.config.pets[0].scale, 1);
    assert!(state.update_scale("4"));
    assert_eq!(state.config.pets[0].scale, 4);
}

#[test]
fn update_scale_rejects_zero() {
    let mut state = ConfigDialogState::new(Config::default());
    let original = state.config.pets[0].scale;
    assert!(!state.update_scale("0"));
    assert_eq!(state.config.pets[0].scale, original);
}

#[test]
fn update_scale_rejects_five() {
    let mut state = ConfigDialogState::new(Config::default());
    let original = state.config.pets[0].scale;
    assert!(!state.update_scale("5"));
    assert_eq!(state.config.pets[0].scale, original);
}

#[test]
fn update_scale_rejects_non_numeric() {
    let mut state = ConfigDialogState::new(Config::default());
    assert!(!state.update_scale("abc"));
}

#[test]
fn parse_scale_boundary_values() {
    assert_eq!(ConfigDialogState::parse_scale("1"), Some(1));
    assert_eq!(ConfigDialogState::parse_scale("4"), Some(4));
    assert_eq!(ConfigDialogState::parse_scale("0"), None);
    assert_eq!(ConfigDialogState::parse_scale("5"), None);
}

#[test]
fn update_x_and_y() {
    let mut state = ConfigDialogState::new(Config::default());
    assert!(state.update_x("300"));
    assert_eq!(state.config.pets[0].x, 300);
    assert!(state.update_y("-50"));
    assert_eq!(state.config.pets[0].y, -50);
}

#[test]
fn update_x_rejects_non_numeric() {
    let mut state = ConfigDialogState::new(Config::default());
    assert!(!state.update_x("??"));
}

#[test]
fn accept_sets_result_ok() {
    let mut state = ConfigDialogState::new(Config::default());
    state.accept();
    assert_eq!(state.result, DialogResult::Ok);
}

#[test]
fn cancel_sets_result_cancel() {
    let mut state = ConfigDialogState::new(Config::default());
    state.cancel();
    assert_eq!(state.result, DialogResult::Cancel);
}

#[test]
fn selected_pet_none_when_empty() {
    let state = ConfigDialogState::new(Config { pets: vec![] });
    assert!(state.selected_pet().is_none());
}

// ── SpriteKey tests ──────────────────────────────────────────────────────────

#[test]
fn sprite_key_roundtrip_embedded() {
    let key = SpriteKey::Embedded("esheep".into());
    let path = key.to_sheet_path();
    assert_eq!(path, "embedded://esheep");
    assert_eq!(SpriteKey::from_sheet_path(&path), key);
}

#[test]
fn sprite_key_roundtrip_installed() {
    let key = SpriteKey::Installed(PathBuf::from("C:/sprites/my_cat.json"));
    let path = key.to_sheet_path();
    assert_eq!(SpriteKey::from_sheet_path(&path), key);
}

// ── select_sprite / selected_sprite tests ────────────────────────────────────

#[test]
fn dialog_state_select_sprite_updates_path() {
    let mut state = ConfigDialogState::new(Config::default());
    state.select_sprite(SpriteKey::Embedded("esheep".into()));
    assert_eq!(state.config.pets[0].sheet_path, "embedded://esheep");
    assert_eq!(state.selected_sprite, SpriteKey::Embedded("esheep".into()));
}

#[test]
fn dialog_state_new_derives_selected_sprite_from_sheet_path() {
    let cfg = Config {
        pets: vec![PetConfig { sheet_path: "embedded://esheep".into(), ..PetConfig::default() }],
    };
    let state = ConfigDialogState::new(cfg);
    assert_eq!(state.selected_sprite, SpriteKey::Embedded("esheep".into()));
}

// ── update_walk_speed tests ───────────────────────────────────────────────────

#[test]
fn dialog_state_update_walk_speed_valid() {
    let mut state = ConfigDialogState::new(Config::default());
    assert!(state.update_walk_speed("80"));
    assert!((state.config.pets[0].walk_speed - 80.0).abs() < 0.001);
    assert!(state.update_walk_speed("80.5"));
    assert!((state.config.pets[0].walk_speed - 80.5).abs() < 0.001);
    assert!(state.update_walk_speed("1"));
    assert!(state.update_walk_speed("500"));
}

#[test]
fn dialog_state_update_walk_speed_invalid() {
    let mut state = ConfigDialogState::new(Config::default());
    let original = state.config.pets[0].walk_speed;
    assert!(!state.update_walk_speed("0"));
    assert!(!state.update_walk_speed("-1"));
    assert!(!state.update_walk_speed("abc"));
    assert!(!state.update_walk_speed("501"));
    assert!(!state.update_walk_speed("0.5"));
    // State must not have been mutated
    assert!((state.config.pets[0].walk_speed - original).abs() < 0.001);
}
