// UI tests using egui_kittest — tests render functions directly; no GPU/HWND needed.
#[allow(unused_imports)]
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use egui_kittest::Harness;
use ferrite::{
    config::schema::{Config, PetConfig},
    event::AppEvent,
    tray::{
        app_window::{new_app_window_state, AppTab, AppWindowState, render_app_tab_bar},
        config_window::{render_config_panel, ConfigWindowState},
        sm_editor::{render_sm_panel, SmEditorViewport},
    },
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_config_state() -> ConfigWindowState {
    use ferrite::tray::config_window::{DesktopSheetLoader, gallery_entries_from_desktop};
    let config = Config { pets: vec![PetConfig::default()] };
    let gallery = gallery_entries_from_desktop();
    ConfigWindowState::new(config, gallery, Box::new(DesktopSheetLoader))
}

fn make_sm_state() -> SmEditorViewport {
    ferrite::tray::sm_editor::new_desktop_sm_editor(true, std::env::temp_dir())
}

fn make_app_window_state() -> AppWindowState {
    let (tx, _rx) = crossbeam_channel::unbounded::<AppEvent>();
    let config = Config { pets: vec![PetConfig::default()] };
    let arc = new_app_window_state(config, tx, true, PathBuf::from("."));
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
    let cs = make_config_state();
    let before = cs.config.pets.len();
    let state = Rc::new(RefCell::new(cs));
    let state_c = Rc::clone(&state);
    let mut harness = Harness::new(move |ctx| {
        render_config_panel(ctx, &mut state_c.borrow_mut(), &mut false);
    });
    harness.run();
    harness.get_by_label("Add Pet").click();
    harness.run();
    assert_eq!(state.borrow().config.pets.len(), before + 1);
}

#[test]
fn remove_pet_decreases_count() {
    use egui_kittest::kittest::Queryable;
    use ferrite::tray::config_window::{DesktopSheetLoader, gallery_entries_from_desktop};
    let config = Config {
        pets: vec![
            PetConfig { id: "a".into(), ..PetConfig::default() },
            PetConfig { id: "b".into(), ..PetConfig::default() },
        ],
    };
    let mut cs = ConfigWindowState::new(config, gallery_entries_from_desktop(), Box::new(DesktopSheetLoader));
    cs.selected_pet_idx = Some(0);
    let state = Rc::new(RefCell::new(cs));
    let state_c = Rc::clone(&state);
    let mut harness = Harness::new(move |ctx| {
        render_config_panel(ctx, &mut state_c.borrow_mut(), &mut false);
    });
    harness.run();
    harness.get_by_label("Remove").click();
    harness.run();
    assert_eq!(state.borrow().config.pets.len(), 1);
}

#[test]
fn save_button_label_clean_when_not_dirty() {
    use egui_kittest::kittest::Queryable;
    let mut vp = make_sm_state();
    vp.is_dirty = false;
    let vp_rc = Rc::new(RefCell::new(vp));
    let vp_c = Rc::clone(&vp_rc);
    let mut harness = Harness::new(move |ctx| {
        render_sm_panel(ctx, &mut vp_c.borrow_mut());
    });
    harness.run();
    // Should find "💾 Save" (not dirty variant)
    harness.get_by_label("💾 Save"); // panics if not found
}

#[test]
fn save_button_label_dirty_when_dirty() {
    use egui_kittest::kittest::Queryable;
    let mut vp = make_sm_state();
    vp.is_dirty = true;
    let vp_rc = Rc::new(RefCell::new(vp));
    let vp_c = Rc::clone(&vp_rc);
    let mut harness = Harness::new(move |ctx| {
        render_sm_panel(ctx, &mut vp_c.borrow_mut());
    });
    harness.run();
    harness.get_by_label("💾 Save*"); // panics if not found
}

#[test]
fn new_sm_button_loads_template() {
    use egui_kittest::kittest::Queryable;
    let vp = make_sm_state();
    let vp_rc = Rc::new(RefCell::new(vp));
    let vp_c = Rc::clone(&vp_rc);
    let mut harness = Harness::new(move |ctx| {
        render_sm_panel(ctx, &mut vp_c.borrow_mut());
    });
    harness.run();
    harness.get_by_label("📄 New SM").click();
    harness.run();
    let borrowed = vp_rc.borrow();
    assert!(
        borrowed.editor_text.contains("[states.idle]"),
        "template must contain [states.idle], got: {:?}",
        &borrowed.editor_text[..borrowed.editor_text.len().min(80)]
    );
    assert!(borrowed.is_dirty, "New SM must set is_dirty=true");
    assert!(borrowed.selected_sm.is_none(), "New SM must clear selected_sm");
}

#[test]
fn dark_light_toggle_flips_mode() {
    use egui_kittest::kittest::Queryable;
    let mut s = make_app_window_state();
    s.dark_mode = true;
    let state = Rc::new(RefCell::new(s));
    let state_c = Rc::clone(&state);
    let mut harness = Harness::new(move |ctx| {
        render_app_tab_bar(ctx, &mut state_c.borrow_mut());
    });
    harness.run();
    // When dark_mode=true, the icon shown is "☀" (click to switch to light)
    harness.get_by_label("☀").click();
    harness.run();
    assert!(
        !state.borrow().dark_mode,
        "toggle from dark must set dark_mode=false"
    );
}

#[test]
fn sm_coverage_panel_renders_with_editable_rows() {
    use egui_kittest::kittest::Queryable;
    use ferrite::{
        sprite::{
            editor_state::{EditorTag, SpriteEditorState},
            sheet::TagDirection,
        },
        tray::sprite_editor::{render_sprite_editor_panel, SpriteEditorViewport},
    };
    use std::path::PathBuf;

    // Build a minimal SpriteEditorState with one tag named "idle"
    let mut state = SpriteEditorState::new(
        PathBuf::from("test.png"),
        image::RgbaImage::new(16, 16),
    );
    state.tags.push(EditorTag {
        name: "idle".to_string(),
        from: 0,
        to: 0,
        direction: TagDirection::Forward,
        flip_h: false,
        color: 0,
    });

    let viewport = Rc::new(RefCell::new(SpriteEditorViewport::new(state)));
    let viewport_c = Rc::clone(&viewport);

    // The panel must render without panicking — basic smoke test.
    // Use run_steps(1) because render_sprite_editor_panel always calls
    // ctx.request_repaint_after(), which would cause Harness::run to exceed max_steps.
    let mut harness = Harness::new(move |ctx| {
        render_sprite_editor_panel(ctx, &mut viewport_c.borrow_mut());
    });
    harness.run_steps(1);
    // Verify the SM selector label is present (panel rendered); panics if not found
    harness.get_by_label("SM:");
}
