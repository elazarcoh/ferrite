use crate::config::schema::Config;

// EnableWindow lives in Win32_UI_Input_KeyboardAndMouse which is not in our feature set.
// Keep the manual FFI declaration from the original file:
#[link(name = "user32")]
unsafe extern "system" {
    fn EnableWindow(hwnd: HWND, enable: i32) -> i32;
}

// Re-export for backward compatibility with existing tests that import these from tray::config_window.
pub use crate::config::dialog_state::{ConfigDialogState, DialogResult};
use crate::config::dialog_state::SpriteKey;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::HBRUSH,
    System::LibraryLoader::GetModuleHandleW,
    UI::Controls::Dialogs::{GetOpenFileNameW, OPENFILENAMEW},
    UI::WindowsAndMessaging::*,
};

// SS_LEFT = 0 (default left-aligned static text; not re-exported from WindowsAndMessaging)
const SS_LEFT: u32 = 0;
// COLOR_BTNFACE = 15; +1 converts color index to pseudo-brush handle
const DIALOG_BG_BRUSH: usize = 16;

// ─── Control IDs ──────────────────────────────────────────────────────────────

const ID_LIST: i32 = 101;
const ID_BTN_ADD: i32 = 102;
const ID_BTN_REMOVE: i32 = 103;
const ID_EDIT_SCALE: i32 = 106;
const ID_EDIT_X: i32 = 108;
const ID_EDIT_Y: i32 = 109;
const ID_EDIT_SPEED: i32 = 110;
const DLG_OK: i32 = 1;     // IDOK
const DLG_CANCEL: i32 = 2; // IDCANCEL

// ─── Win32 dialog ─────────────────────────────────────────────────────────────

const CLASS_NAME: &str = "MyPetConfigDlg";

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

static CLASS_ONCE: std::sync::Once = std::sync::Once::new();

fn register_config_class() {
    CLASS_ONCE.call_once(|| unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        let cls = wide(CLASS_NAME);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(config_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: DIALOG_BG_BRUSH as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
    });
}

/// Show the config dialog modally.
/// Returns the updated `Config` if the user pressed OK, or `None` for Cancel.
pub fn show_config_dialog(parent: HWND, config: &Config) -> Option<Config> {
    register_config_class();
    unsafe {
        let mut state = Box::new(ConfigDialogState::new(config.clone()));
        let state_ptr: *mut ConfigDialogState = &mut *state;

        let cls = wide(CLASS_NAME);
        let title = wide("My Pet — Configure");
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPSIBLINGS | WS_CLIPCHILDREN | WS_VISIBLE;

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            cls.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            510,
            390,
            parent,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return None;
        }

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        populate_controls(hwnd, &state);

        if !parent.is_null() {
            EnableWindow(parent, 0);
        }
        center_window(hwnd);

        // Nested modal message loop — runs until dialog window is destroyed.
        loop {
            let mut msg: MSG = std::mem::zeroed();
            let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if ret == 0 {
                // WM_QUIT: re-post for the outer loop, then stop.
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            if ret == -1 {
                break;
            }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if IsWindow(hwnd) == 0 {
                break;
            }
        }

        if !parent.is_null() {
            EnableWindow(parent, 1);
        }

        if state.result == DialogResult::Ok { Some(state.config) } else { None }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

unsafe fn center_window(hwnd: HWND) {
    let mut rc: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rc);
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;
    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    SetWindowPos(
        hwnd,
        std::ptr::null_mut(),
        (sw - w) / 2,
        (sh - h) / 2,
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER,
    );
}

unsafe fn populate_controls(hwnd: HWND, state: &ConfigDialogState) {
    refresh_list(hwnd, state);
    refresh_fields(hwnd, state);
}

unsafe fn refresh_list(hwnd: HWND, state: &ConfigDialogState) {
    let listbox = GetDlgItem(hwnd, ID_LIST);
    SendMessageW(listbox, LB_RESETCONTENT, 0, 0);
    for pet in &state.config.pets {
        let text = format!("{} \u{2014} {}", pet.id, pet.sheet_path);
        let w = wide(&text);
        SendMessageW(listbox, LB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    if !state.config.pets.is_empty() {
        SendMessageW(listbox, LB_SETCURSEL, state.selected, 0);
    }
}

unsafe fn refresh_fields(hwnd: HWND, state: &ConfigDialogState) {
    if let Some(pet) = state.selected_pet() {
        set_ctrl_text(hwnd, ID_EDIT_SCALE, &pet.scale.to_string());
        set_ctrl_text(hwnd, ID_EDIT_X, &pet.x.to_string());
        set_ctrl_text(hwnd, ID_EDIT_Y, &pet.y.to_string());
        set_ctrl_text(hwnd, ID_EDIT_SPEED, &pet.walk_speed.to_string());
    }
}

unsafe fn set_ctrl_text(parent: HWND, id: i32, text: &str) {
    let w = wide(text);
    SetWindowTextW(GetDlgItem(parent, id), w.as_ptr());
}

unsafe fn read_fields(hwnd: HWND, state: &mut ConfigDialogState) {
    let mut buf = [0u16; 512];
    macro_rules! get_text {
        ($id:expr) => {{
            let n = GetWindowTextW(GetDlgItem(hwnd, $id), buf.as_mut_ptr(), buf.len() as i32);
            String::from_utf16_lossy(&buf[..n.max(0) as usize])
        }};
    }
    let scale = get_text!(ID_EDIT_SCALE);
    state.update_scale(&scale);
    let x = get_text!(ID_EDIT_X);
    state.update_x(&x);
    let y = get_text!(ID_EDIT_Y);
    state.update_y(&y);
    let speed = get_text!(ID_EDIT_SPEED);
    state.update_walk_speed(&speed);
}

unsafe fn create_controls(hwnd: HWND) {
    let hi = GetModuleHandleW(std::ptr::null());
    let tab = WS_TABSTOP;

    macro_rules! label {
        ($text:expr, $x:expr, $y:expr, $w:expr) => {
            CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | SS_LEFT,
                $x, $y, $w, 20,
                hwnd, std::ptr::null_mut(), hi, std::ptr::null(),
            )
        };
    }
    macro_rules! btn {
        ($text:expr, $x:expr, $y:expr, $w:expr, $h:expr, $id:expr, $sty:expr) => {
            CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | ($sty as u32),
                $x, $y, $w, $h,
                hwnd, $id as usize as HMENU, hi, std::ptr::null(),
            )
        };
    }

    // Pets list
    label!("Pets:", 10, 10, 300);
    CreateWindowExW(
        WS_EX_CLIENTEDGE,
        wide("LISTBOX").as_ptr(),
        wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE | tab | LBS_NOTIFY as u32 | WS_VSCROLL,
        10, 30, 300, 80,
        hwnd, ID_LIST as usize as HMENU, hi, std::ptr::null(),
    );
    btn!("Add",    320, 30, 80, 24, ID_BTN_ADD,    BS_PUSHBUTTON);
    btn!("Remove", 320, 60, 80, 24, ID_BTN_REMOVE, BS_PUSHBUTTON);
}

unsafe fn handle_command(hwnd: HWND, id: i32, notify: u16, state: &mut ConfigDialogState) {
    match id {
        DLG_OK => {
            read_fields(hwnd, state);
            state.accept();
            DestroyWindow(hwnd);
        }
        DLG_CANCEL => {
            state.cancel();
            DestroyWindow(hwnd);
        }
        ID_BTN_ADD => {
            read_fields(hwnd, state);
            state.add_pet();
            refresh_list(hwnd, state);
            refresh_fields(hwnd, state);
        }
        ID_BTN_REMOVE => {
            state.remove_selected();
            refresh_list(hwnd, state);
            refresh_fields(hwnd, state);
        }
        ID_LIST => {
            if notify == LBN_SELCHANGE as u16 {
                let sel = SendMessageW(GetDlgItem(hwnd, ID_LIST), LB_GETCURSEL, 0, 0);
                if sel >= 0 {
                    read_fields(hwnd, state);
                    state.select(sel as usize);
                    refresh_fields(hwnd, state);
                }
            }
        }
        _ => {}
    }
}

unsafe extern "system" fn config_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            create_controls(hwnd);
            0
        }
        WM_CLOSE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ConfigDialogState;
            if !ptr.is_null() {
                (*ptr).cancel();
            }
            DestroyWindow(hwnd);
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as i32;
            let notify = ((wparam >> 16) & 0xFFFF) as u16;
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ConfigDialogState;
            if !ptr.is_null() {
                handle_command(hwnd, id, notify, &mut *ptr);
            }
            0
        }
        WM_DESTROY => 0,
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
