use crate::event::AppEvent;
use crossbeam_channel::Sender;
use once_cell::sync::OnceCell;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    UI::WindowsAndMessaging::*,
};

// ─── Extra user32 imports not re-exported by windows-sys glob ────────────────

#[link(name = "user32")]
unsafe extern "system" {
    fn SetCapture(hwnd: HWND) -> HWND;
    fn ReleaseCapture() -> i32;
    fn GetCursorPos(pt: *mut POINT) -> i32;
}

// ─── Global event sender ─────────────────────────────────────────────────────

static EVENT_TX: OnceCell<Sender<AppEvent>> = OnceCell::new();

pub fn init_event_sender(tx: Sender<AppEvent>) {
    EVENT_TX.set(tx).ok();
}

pub fn send_event(ev: AppEvent) {
    if let Some(tx) = EVENT_TX.get() {
        let _ = tx.send(ev);
    }
}

// ─── Per-HWND registry ───────────────────────────────────────────────────────

struct HwndData {
    pet_id: String,
    /// Alpha-only buffer (one byte per pixel, row-major).
    alpha_buf: Vec<u8>,
    buf_width: u32,
    // ── Drag state ────────────────────────────────────────────────────────────
    /// Left button is held down.
    mouse_down: bool,
    /// Screen position of the initial mousedown.
    cursor_down_screen: (i32, i32),
    /// Window top-left at the time of mousedown.
    win_down_pos: (i32, i32),
    /// Movement threshold (5 px) exceeded — dragging is active.
    drag_active: bool,
    /// PetDragStart event has been sent.
    drag_start_sent: bool,
    /// Two most-recent cursor screen positions + timestamps for velocity.
    vel_prev: Option<((i32, i32), Instant)>,
    vel_cur: Option<((i32, i32), Instant)>,
}

static HWND_REGISTRY: Lazy<Mutex<HashMap<isize, HwndData>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register a new pet window. Called from `PetInstance::new`.
pub fn register_hwnd(hwnd: HWND, pet_id: String) {
    let mut reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    reg.insert(
        hwnd as isize,
        HwndData {
            pet_id,
            alpha_buf: Vec::new(),
            buf_width: 0,
            mouse_down: false,
            cursor_down_screen: (0, 0),
            win_down_pos: (0, 0),
            drag_active: false,
            drag_start_sent: false,
            vel_prev: None,
            vel_cur: None,
        },
    );
}

/// Update the alpha buffer after a new frame is rendered.
/// Called from `PetWindow::render_frame`.
pub fn update_alpha_buf(hwnd: HWND, frame_buf: &[u8], width: u32) {
    let alpha: Vec<u8> = frame_buf.chunks(4).map(|px| px[3]).collect();
    let mut reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(data) = reg.get_mut(&(hwnd as isize)) {
        data.alpha_buf = alpha;
        data.buf_width = width;
    }
}

/// Unregister on window destruction. Called from `PetInstance::drop`.
pub fn unregister_hwnd(hwnd: HWND) {
    let mut reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    reg.remove(&(hwnd as isize));
}

/// Returns true if the user currently holds the mouse button on this window.
/// Used by app.rs to suppress the ground clamp before PetDragStart is processed.
pub fn is_mouse_down(hwnd: HWND) -> bool {
    let reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    reg.get(&(hwnd as isize)).map(|d| d.mouse_down).unwrap_or(false)
}

/// Returns true if `hwnd` is one of our own pet windows.
pub fn is_pet_hwnd(hwnd: HWND) -> bool {
    let reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    reg.contains_key(&(hwnd as isize))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Retrieve alpha buffer and pet_id for a window without holding the lock.
fn get_hwnd_info(hwnd: HWND) -> Option<(Vec<u8>, u32, String)> {
    let reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    reg.get(&(hwnd as isize)).map(|d| (d.alpha_buf.clone(), d.buf_width, d.pet_id.clone()))
}

/// Unpack cursor screen coordinates from a WM_NCHITTEST lParam.
fn cursor_pos(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam as i32) as i16 as i32;
    let y = ((lparam >> 16) as i32) as i16 as i32;
    (x, y)
}

// ─── WNDPROC ─────────────────────────────────────────────────────────────────

/// Win32 window procedure for all pet windows.
///
/// # Safety
/// Called by Win32. All parameters are valid for the call duration.
pub unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT { unsafe {
    match msg {
        // ─── Per-pixel hit testing ────────────────────────────────────────────
        WM_NCHITTEST => {
            // Release the registry lock BEFORE calling any Win32 functions to
            // avoid re-entrancy deadlock.
            let info = get_hwnd_info(hwnd);
            if let Some((alpha_buf, buf_width, _)) = info
                && !alpha_buf.is_empty() && buf_width > 0 {
                    let (cx, cy) = cursor_pos(lparam);
                    let mut rc: RECT = std::mem::zeroed();
                    GetWindowRect(hwnd, &mut rc);
                    let lx = cx - rc.left;
                    let ly = cy - rc.top;
                    if lx >= 0 && ly >= 0 {
                        // alpha_buf is 1 byte per pixel (alpha channel only).
                        let idx = (ly as u32 * buf_width + lx as u32) as usize;
                        let a = alpha_buf.get(idx).copied().unwrap_or(0);
                        if a < 32 {
                            return HTTRANSPARENT as LRESULT;
                        }
                        log::trace!("NCHITTEST opaque pixel alpha={a} local=({lx},{ly}) → HTCLIENT");
                    }
                }
            // Opaque pixel → HTCLIENT so WM_LBUTTONDOWN is delivered and we
            // can implement custom drag (HTCAPTION would cause snap-back when
            // the animation frame changes and app.rs calls move_to).
            HTCLIENT as LRESULT
        }

        // ─── Left button down — start tracking; no drag yet ──────────────────
        WM_LBUTTONDOWN => {
            let mut cursor_screen = POINT { x: 0, y: 0 };
            GetCursorPos(&mut cursor_screen);
            let mut rc: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut rc);

            log::debug!(
                "WM_LBUTTONDOWN hwnd={:p} cursor=({},{}) win=({},{})",
                hwnd, cursor_screen.x, cursor_screen.y, rc.left, rc.top
            );

            {
                let mut reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(data) = reg.get_mut(&(hwnd as isize)) {
                    data.mouse_down = true;
                    data.cursor_down_screen = (cursor_screen.x, cursor_screen.y);
                    data.win_down_pos = (rc.left, rc.top);
                    data.drag_active = false;
                    data.drag_start_sent = false;
                    data.vel_prev = None;
                    data.vel_cur =
                        Some(((cursor_screen.x, cursor_screen.y), Instant::now()));
                }
            }

            SetCapture(hwnd);
            0
        }

        // ─── Mouse move — if button held and threshold exceeded, drag ─────────
        WM_MOUSEMOVE => {
            const MK_LBUTTON: WPARAM = 0x0001;
            if wparam & MK_LBUTTON == 0 {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }

            let mut cursor_screen = POINT { x: 0, y: 0 };
            GetCursorPos(&mut cursor_screen);

            // Read state under lock; release before Win32 calls.
            let drag_info = {
                let reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
                reg.get(&(hwnd as isize)).map(|d| {
                    (d.mouse_down, d.cursor_down_screen, d.win_down_pos, d.drag_active, d.drag_start_sent, d.pet_id.clone())
                })
            };

            let Some((mouse_down, cursor_down, win_down, drag_active, start_sent, pet_id)) =
                drag_info
            else {
                return 0;
            };

            if !mouse_down {
                return 0;
            }

            let dx = cursor_screen.x - cursor_down.0;
            let dy = cursor_screen.y - cursor_down.1;

            // Require at least 5 px movement before starting drag.
            if !drag_active && dx * dx + dy * dy < 25 {
                return 0;
            }

            let new_wx = win_down.0 + dx;
            let new_wy = win_down.1 + dy;

            // Update velocity history and activate drag under lock.
            let now = Instant::now();
            {
                let mut reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(data) = reg.get_mut(&(hwnd as isize)) {
                    data.drag_active = true;
                    data.vel_prev = data.vel_cur.take();
                    data.vel_cur = Some(((cursor_screen.x, cursor_screen.y), now));
                }
            }

            // Move the window directly — immediate drag response.
            log::debug!(
                "WM_MOUSEMOVE drag cursor=({},{}) → win=({},{})",
                cursor_screen.x, cursor_screen.y, new_wx, new_wy
            );
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                new_wx,
                new_wy,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );

            // Send PetDragStart exactly once.
            if !start_sent {
                {
                    let mut reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(data) = reg.get_mut(&(hwnd as isize)) {
                        data.drag_start_sent = true;
                    }
                }
                send_event(AppEvent::PetDragStart {
                    pet_id,
                    cursor_x: cursor_screen.x,
                    cursor_y: cursor_screen.y,
                });
            }

            0
        }

        // ─── Left button up — end drag or register click ──────────────────────
        WM_LBUTTONUP => {
            ReleaseCapture();

            let outcome = {
                let mut reg = HWND_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
                reg.get_mut(&(hwnd as isize)).map(|data| {
                    let was_drag = data.drag_active;
                    let pet_id = data.pet_id.clone();
                    let velocity =
                        if let (Some((p0, t0)), Some((p1, t1))) = (&data.vel_prev, &data.vel_cur) {
                            let dt = t1.duration_since(*t0).as_secs_f32().max(0.001);
                            ((p1.0 - p0.0) as f32 / dt, (p1.1 - p0.1) as f32 / dt)
                        } else {
                            (0.0, 0.0)
                        };
                    data.mouse_down = false;
                    data.drag_active = false;
                    data.drag_start_sent = false;
                    data.vel_prev = None;
                    data.vel_cur = None;
                    (was_drag, pet_id, velocity)
                })
            };

            if let Some((was_drag, pet_id, velocity)) = outcome {
                log::debug!("WM_LBUTTONUP was_drag={was_drag} vel=({:.0},{:.0})", velocity.0, velocity.1);
                if was_drag {
                    send_event(AppEvent::PetDragEnd { pet_id, velocity });
                } else {
                    send_event(AppEvent::PetClicked { pet_id });
                }
            }
            0
        }

        // ─── Double-click → react ─────────────────────────────────────────────
        WM_LBUTTONDBLCLK => {
            if let Some((_, _, pet_id)) = get_hwnd_info(hwnd) {
                send_event(AppEvent::PetClicked { pet_id });
            }
            0
        }

        WM_DESTROY => 0,

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}}
