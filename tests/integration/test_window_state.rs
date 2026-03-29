// Tests that AppWindowState accepts a pre-loaded gallery and doesn't require
// double-loading at open time.
use ferrite::tray::app_window::AppWindowState;
use ferrite::config::schema::Config;
use ferrite::window::sprite_gallery::SpriteGallery;
use crossbeam_channel::unbounded;
use ferrite::event::AppEvent;

fn make_state() -> std::sync::Arc<std::sync::Mutex<AppWindowState>> {
    let (tx, _rx) = unbounded::<AppEvent>();
    let config = Config::default();
    let gallery = SpriteGallery::load();
    let config_dir = std::path::PathBuf::from(".");
    AppWindowState::new(config, tx, true, config_dir, gallery)
}

#[test]
fn app_window_state_should_close_starts_false() {
    let state = make_state();
    let s = state.lock().unwrap();
    assert!(!s.should_close, "should_close must start as false");
}

#[test]
fn app_window_state_can_toggle_should_close() {
    let state = make_state();
    {
        let mut s = state.lock().unwrap();
        s.should_close = true;
        assert!(s.should_close);
    }
    {
        let mut s = state.lock().unwrap();
        s.should_close = false;
        assert!(!s.should_close, "should_close can be reset to false");
    }
}

#[test]
fn app_window_state_dark_mode_default() {
    let state = make_state();
    let s = state.lock().unwrap();
    assert!(s.dark_mode, "dark_mode should default to true");
}

#[test]
fn app_window_state_accepts_gallery() {
    // Verify that the gallery is loaded and passed in (not empty for built-in assets)
    let state = make_state();
    let s = state.lock().unwrap();
    assert!(!s.sprite_gallery.entries.is_empty(), "gallery should have at least the built-in esheep entry");
}
