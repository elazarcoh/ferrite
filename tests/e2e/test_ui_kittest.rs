// UI tests using egui_kittest — tests render functions directly; no GPU/HWND needed.
#[allow(unused_imports)]
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use egui_kittest::Harness;
use ferrite::{
    config::schema::{Config, PetConfig},
    event::AppEvent,
    tray::{
        app_window::{AppTab, AppWindowState, render_app_tab_bar},
        config_window::{render_config_panel, ConfigWindowState},
        sm_editor::{render_sm_panel, SmEditorViewport},
    },
    window::sprite_gallery::SpriteGallery,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_config_state() -> (ConfigWindowState, crossbeam_channel::Receiver<AppEvent>) {
    let (tx, rx) = crossbeam_channel::unbounded::<AppEvent>();
    let config = Config { pets: vec![PetConfig::default()] };
    let gallery = SpriteGallery::load();
    (ConfigWindowState::new(config, tx, gallery), rx)
}

fn make_sm_state() -> SmEditorViewport {
    let arc = SmEditorViewport::new(true, PathBuf::from("."));
    std::sync::Arc::try_unwrap(arc)
        .unwrap_or_else(|_| panic!("Arc has multiple owners"))
        .into_inner()
        .unwrap()
}

fn make_app_window_state() -> AppWindowState {
    let (tx, _rx) = crossbeam_channel::unbounded::<AppEvent>();
    let config = Config { pets: vec![PetConfig::default()] };
    let gallery = SpriteGallery::load();
    let arc = AppWindowState::new(config, tx, true, PathBuf::from("."), gallery);
    std::sync::Arc::try_unwrap(arc)
        .unwrap_or_else(|_| panic!("Arc has multiple owners"))
        .into_inner()
        .unwrap()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn tab_click_switches_to_sprites() {
    use egui_kittest::kittest::Queryable;
    let state = Rc::new(RefCell::new(make_app_window_state()));
    let state_c = Rc::clone(&state);
    let mut harness = Harness::new(move |ctx| {
        render_app_tab_bar(ctx, &mut state_c.borrow_mut());
    });
    harness.run();
    harness.get_by_label("🖼 Sprites").click();
    harness.run();
    assert_eq!(state.borrow().selected_tab, AppTab::Sprites);
}

#[test]
fn add_pet_increases_count() {
    use egui_kittest::kittest::Queryable;
    let (cs, _rx) = make_config_state();
    let before = cs.config.pets.len();
    let state = Rc::new(RefCell::new(cs));
    let state_c = Rc::clone(&state);
    let mut harness = Harness::new(move |ctx| {
        render_config_panel(ctx, &mut state_c.borrow_mut());
    });
    harness.run();
    harness.get_by_label("Add Pet").click();
    harness.run();
    assert_eq!(state.borrow().config.pets.len(), before + 1);
}

#[test]
fn remove_pet_decreases_count() {
    use egui_kittest::kittest::Queryable;
    let (tx, _rx) = crossbeam_channel::unbounded::<AppEvent>();
    let config = Config {
        pets: vec![
            PetConfig { id: "a".into(), ..PetConfig::default() },
            PetConfig { id: "b".into(), ..PetConfig::default() },
        ],
    };
    let mut cs = ConfigWindowState::new(config, tx, SpriteGallery::load());
    cs.selected_pet_idx = Some(0);
    let state = Rc::new(RefCell::new(cs));
    let state_c = Rc::clone(&state);
    let mut harness = Harness::new(move |ctx| {
        render_config_panel(ctx, &mut state_c.borrow_mut());
    });
    harness.run();
    harness.get_by_label("Remove").click();
    harness.run();
    assert_eq!(state.borrow().config.pets.len(), 1);
}
