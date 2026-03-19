/// E2E: drag and click via injected Win32 messages.
///
/// Uses SendMessage + SetCursorPos to simulate real mouse interaction.
/// The global EVENT_TX OnceCell is set up once via DRAG_CHANNEL; all drag
/// tests share the same receiver and drain it before each assertion.
use my_pet::{
    app::PetInstance,
    config::schema::PetConfig,
    event::AppEvent,
    sprite::{behavior::BehaviorState, sheet::load_embedded},
    window::wndproc,
};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::time::Duration;
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::*,
};

fn test_sheet() -> my_pet::sprite::sheet::SpriteSheet {
    load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap()
}

fn make_pet() -> PetInstance {
    PetInstance::new(PetConfig::default(), test_sheet()).unwrap()
}

// ─── Shared test event channel ────────────────────────────────────────────────

type DragChannel = (
    crossbeam_channel::Sender<AppEvent>,
    Mutex<crossbeam_channel::Receiver<AppEvent>>,
);

static DRAG_CHANNEL: Lazy<DragChannel> = Lazy::new(|| {
    let (tx, rx) = crossbeam_channel::bounded::<AppEvent>(128);
    wndproc::init_event_sender(tx.clone());
    (tx, Mutex::new(rx))
});

fn drain_events() {
    let rx = DRAG_CHANNEL.1.lock().unwrap();
    while rx.try_recv().is_ok() {}
}

fn recv_event(timeout_ms: u64) -> Option<AppEvent> {
    let rx = DRAG_CHANNEL.1.lock().unwrap();
    rx.recv_timeout(Duration::from_millis(timeout_ms)).ok()
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_lparam(x: i32, y: i32) -> LPARAM {
    ((y as u32 as LPARAM) << 16) | (x as u16 as LPARAM)
}

/// Move the real cursor to (x, y) and send a Win32 message to the window.
/// SetCursorPos ensures GetCursorPos() inside wndproc returns the right value.
unsafe fn send_mouse(hwnd: *mut std::ffi::c_void, msg: u32, wparam: WPARAM, screen_x: i32, screen_y: i32, win_x: i32, win_y: i32) {
    SetCursorPos(screen_x, screen_y);
    let lp = make_lparam(screen_x - win_x, screen_y - win_y);
    SendMessageW(hwnd, msg, wparam, lp);
}

// ─── NCHITTEST: opaque pixel → HTCLIENT ──────────────────────────────────────

#[test]
fn nchittest_opaque_pixel_returns_htclient() {
    let _init = &*DRAG_CHANNEL; // ensure event sender is registered

    // Land pet so it has a rendered frame.
    let mut pet = make_pet();
    let mut cache = my_pet::window::surfaces::SurfaceCache::default();
    for _ in 0..300 {
        if !matches!(pet.ai.state, BehaviorState::Fall { .. }) {
            break;
        }
        pet.tick(20, &mut cache).unwrap();
    }

    // Centre of the pet window — opaque green pixel.
    let cx = pet.x + (pet.window.width as i32) / 2;
    let cy = pet.y + (pet.window.height as i32) / 2;
    let lp = ((cy as u32 as LPARAM) << 16) | (cx as u16 as LPARAM);

    let result = unsafe { SendMessageW(pet.window.hwnd, WM_NCHITTEST, 0, lp) };
    assert_eq!(
        result, HTCLIENT as LRESULT,
        "opaque pixel must return HTCLIENT; got {result}"
    );
}

// ─── Click (no drag): PetClicked event ───────────────────────────────────────

#[test]
fn click_sends_pet_clicked_event() {
    let _init = &*DRAG_CHANNEL;
    drain_events();

    let mut pet = make_pet();
    // Land pet first.
    let mut cache = my_pet::window::surfaces::SurfaceCache::default();
    for _ in 0..300 {
        if !matches!(pet.ai.state, BehaviorState::Fall { .. }) {
            break;
        }
        pet.tick(20, &mut cache).unwrap();
    }

    let hwnd = pet.window.hwnd;
    let cx = pet.x + (pet.window.width as i32) / 2;
    let cy = pet.y + (pet.window.height as i32) / 2;

    // Simulate: button down then up without moving (click, not drag).
    unsafe {
        send_mouse(hwnd, WM_LBUTTONDOWN, 0, cx, cy, pet.x, pet.y);
        send_mouse(hwnd, WM_LBUTTONUP, 0, cx, cy, pet.x, pet.y);
    }

    let ev = recv_event(500).expect("expected PetClicked within 500 ms");
    assert!(
        matches!(ev, AppEvent::PetClicked { .. }),
        "expected PetClicked, got {ev:?}"
    );
}

// ─── Drag: PetDragStart → PetDragEnd ─────────────────────────────────────────

#[test]
fn drag_sends_drag_start_and_end_events() {
    let _init = &*DRAG_CHANNEL;
    drain_events();

    let mut pet = make_pet();
    // Land pet first.
    let mut cache = my_pet::window::surfaces::SurfaceCache::default();
    for _ in 0..300 {
        if !matches!(pet.ai.state, BehaviorState::Fall { .. }) {
            break;
        }
        pet.tick(20, &mut cache).unwrap();
    }

    let hwnd = pet.window.hwnd;
    let cx = pet.x + (pet.window.width as i32) / 2;
    let cy = pet.y + (pet.window.height as i32) / 2;

    unsafe {
        // Button down at pet center.
        send_mouse(hwnd, WM_LBUTTONDOWN, 0, cx, cy, pet.x, pet.y);

        // Move 20 px right — exceeds the 5 px drag threshold.
        const MK_LBUTTON: WPARAM = 0x0001;
        send_mouse(hwnd, WM_MOUSEMOVE, MK_LBUTTON, cx + 20, cy, pet.x, pet.y);

        // Move another 50 px for velocity.
        send_mouse(hwnd, WM_MOUSEMOVE, MK_LBUTTON, cx + 70, cy, pet.x, pet.y);

        // Release.
        send_mouse(hwnd, WM_LBUTTONUP, 0, cx + 70, cy, pet.x, pet.y);
    }

    let ev1 = recv_event(500).expect("expected PetDragStart within 500 ms");
    assert!(
        matches!(ev1, AppEvent::PetDragStart { .. }),
        "expected PetDragStart, got {ev1:?}"
    );

    let ev2 = recv_event(500).expect("expected PetDragEnd within 500 ms");
    assert!(
        matches!(ev2, AppEvent::PetDragEnd { .. }),
        "expected PetDragEnd, got {ev2:?}"
    );
}

// ─── Drag: ai state transitions via grab/release ──────────────────────────────

#[test]
fn grab_then_fast_release_makes_thrown_state() {
    let mut pet = make_pet();
    pet.ai.grab((5, 5));
    assert!(matches!(pet.ai.state, BehaviorState::Grabbed { .. }));
    pet.ai.release((600.0, -200.0));
    assert!(
        matches!(pet.ai.state, BehaviorState::Thrown { .. }),
        "fast release must produce Thrown state"
    );
}

#[test]
fn grab_then_slow_release_makes_fall_state() {
    let mut pet = make_pet();
    pet.ai.grab((0, 0));
    pet.ai.release((0.0, 0.0));
    assert!(
        matches!(pet.ai.state, BehaviorState::Fall { .. }),
        "slow release must produce Fall state"
    );
}

// ─── Alpha buffer correctness ─────────────────────────────────────────────────

#[test]
fn alpha_buf_center_pixel_is_opaque_after_render() {
    let mut pet = make_pet();
    // Land pet so render_current_frame has been called.
    let mut cache = my_pet::window::surfaces::SurfaceCache::default();
    for _ in 0..300 {
        if !matches!(pet.ai.state, BehaviorState::Fall { .. }) {
            break;
        }
        pet.tick(20, &mut cache).unwrap();
    }

    // The alpha buffer (1 byte/pixel) must have 255 at the center.
    let buf = pet.window_frame_buf();
    let w = pet.window_width();
    let h = pet.window.height;

    // Direct 1-byte-per-pixel read of alpha channel.
    let cx = w / 2;
    let cy = h / 2;
    // frame_buf is BGRA (4 bytes/pixel); alpha is at index 3.
    let idx = ((cy * w + cx) * 4 + 3) as usize;
    let a = buf.get(idx).copied().unwrap_or(0);
    assert_eq!(a, 255, "centre pixel of solid sprite must have alpha=255");
}
