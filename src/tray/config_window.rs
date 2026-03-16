// EnableWindow lives in Win32_UI_Input_KeyboardAndMouse which is not in our feature set.
#[link(name = "user32")]
unsafe extern "system" {
    fn EnableWindow(hwnd: HWND, enable: i32) -> i32;
}

// Re-export for backward compatibility with existing tests that import these from tray::config_window.
pub use crate::config::dialog_state::{ConfigDialogState, DialogResult};
use crate::config::dialog_state::SpriteKey;
use crate::config::schema::Config;
use crate::sprite::animation::AnimationState;
use crate::sprite::sheet::load_embedded;
use crate::assets;
use crate::window::sprite_gallery::{GalleryEntry, SourceKind, SpriteGallery};

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateSolidBrush,
        DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, InvalidateRect,
        ReleaseDC, SelectObject, SetBkColor, SetBkMode, SetTextColor, StretchDIBits,
        UpdateWindow, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DT_CENTER,
        DT_LEFT, DT_SINGLELINE, DT_TOP, DT_VCENTER, HBITMAP, HBRUSH, PAINTSTRUCT,
        SRCCOPY, TRANSPARENT, BI_RGB, HDC,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED},
    UI::Controls::Dialogs::{GetOpenFileNameW, OPENFILENAMEW},
    UI::WindowsAndMessaging::*,
};

// ─── Control IDs ──────────────────────────────────────────────────────────────
const ID_LIST_GALLERY:   i32 = 101;
const ID_BTN_ADD_PET:    i32 = 102;
const ID_BTN_REMOVE_PET: i32 = 103;
const ID_EDIT_SCALE:     i32 = 106;
const ID_EDIT_X:         i32 = 108;
const ID_EDIT_Y:         i32 = 109;
const ID_EDIT_SPEED:     i32 = 110;
const DLG_OK:            i32 = 1;  // IDOK
const DLG_CANCEL:        i32 = 2;  // IDCANCEL
const TIMER_ANIM:        usize = 1001;

// ─── Colors (dark VS Code-ish theme) ─────────────────────────────────────────
// Win32 COLORREF format: 0x00BBGGRR  i.e. R | (G<<8) | (B<<16)
const fn clr_bg()       -> u32 { 0x1e | (0x1e << 8) | (0x1e << 16) } // #1e1e1e
const fn clr_bg_card()  -> u32 { 0x26 | (0x25 << 8) | (0x25 << 16) } // #252526
const fn clr_bg_ctrl()  -> u32 { 0x3c | (0x3c << 8) | (0x3c << 16) } // #3c3c3c
const fn clr_bg_sel()   -> u32 { 0x71 | (0x47 << 8) | (0x09 << 16) } // #094771
const fn clr_accent()   -> u32 { 0xcc | (0x7a << 8) | (0x00 << 16) } // #007acc
const fn clr_text()     -> u32 { 0xcc | (0xcc << 8) | (0xcc << 16) } // #cccccc
const fn clr_label()    -> u32 { 0x85 | (0x85 << 8) | (0x85 << 16) } // #858585
const fn clr_text_acc() -> u32 { 0xf7 | (0xc3 << 8) | (0x4f << 16) } // #4fc3f7 accent blue

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

unsafe fn center_window(hwnd: HWND) {
    let mut rc: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rc);
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;
    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    SetWindowPos(hwnd, std::ptr::null_mut(), (sw - w) / 2, (sh - h) / 2, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
}

unsafe fn set_ctrl_text(parent: HWND, id: i32, text: &str) {
    let w = wide(text);
    SetWindowTextW(GetDlgItem(parent, id), w.as_ptr());
}

// ─── DialogCtx ────────────────────────────────────────────────────────────────

/// Heap-allocated context stored in GWLP_USERDATA.
struct DialogCtx {
    state: ConfigDialogState,
    gallery: SpriteGallery,
    chip_hwnds: Vec<HWND>,
    preview_hwnd: HWND,
    preview_sheet: Option<crate::sprite::sheet::SpriteSheet>,
    preview_anim: AnimationState,
    dark_bg_brush: HBRUSH,
    ctrl_brush: HBRUSH,
    card_brush: HBRUSH,
}

impl DialogCtx {
    unsafe fn new(config: Config) -> Box<Self> {
        let state = ConfigDialogState::new(config);
        let gallery = SpriteGallery::load();
        Box::new(DialogCtx {
            state,
            gallery,
            chip_hwnds: Vec::new(),
            preview_hwnd: std::ptr::null_mut(),
            preview_sheet: None,
            preview_anim: AnimationState::new(""),
            dark_bg_brush: CreateSolidBrush(clr_bg()),
            ctrl_brush: CreateSolidBrush(clr_bg_ctrl()),
            card_brush: CreateSolidBrush(clr_bg()),
        })
    }

    unsafe fn destroy_brushes(&self) {
        DeleteObject(self.dark_bg_brush as *mut _);
        DeleteObject(self.ctrl_brush as *mut _);
        DeleteObject(self.card_brush as *mut _);
    }
}

// ─── Window classes ───────────────────────────────────────────────────────────

const DLG_CLASS:     &str = "MyPetConfigDlg";
const CHIP_CLASS:    &str = "PetChip";
const PREVIEW_CLASS: &str = "SpritePreview";

static CLASS_ONCE: std::sync::Once = std::sync::Once::new();

fn register_classes() {
    CLASS_ONCE.call_once(|| unsafe {
        let hi = GetModuleHandleW(std::ptr::null());

        // ── Main dialog ──
        let cls = wide(DLG_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(config_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hi,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);

        // ── PetChip ──
        let cls2 = wide(CHIP_CLASS);
        let wc2 = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(chip_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hi,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls2.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc2);

        // ── SpritePreview ──
        let cls3 = wide(PREVIEW_CLASS);
        let wc3 = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(preview_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hi,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls3.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc3);
    });
}

// ─── Entry point ──────────────────────────────────────────────────────────────

pub fn show_config_dialog(parent: HWND, config: &Config) -> Option<Config> {
    register_classes();
    unsafe {
        let mut ctx = DialogCtx::new(config.clone());
        let ctx_ptr: *mut DialogCtx = &mut *ctx;

        let cls = wide(DLG_CLASS);
        let title = wide("My Pet \u{2014} Configure");
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPSIBLINGS | WS_CLIPCHILDREN | WS_VISIBLE;

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            cls.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT, CW_USEDEFAULT,
            560, 440,
            parent,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return None;
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctx_ptr as isize);
        setup_dialog_controls(hwnd, &mut *ctx_ptr);

        if !parent.is_null() {
            EnableWindow(parent, 0);
        }
        center_window(hwnd);

        loop {
            let mut msg: MSG = std::mem::zeroed();
            let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if ret == 0 {
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            if ret == -1 { break; }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if IsWindow(hwnd) == 0 { break; }
        }

        if !parent.is_null() {
            EnableWindow(parent, 1);
        }

        if ctx.state.result == DialogResult::Ok { Some(ctx.state.config) } else { None }
    }
}

// ─── Controls setup ───────────────────────────────────────────────────────────

unsafe fn create_controls(hwnd: HWND, ctx: &mut DialogCtx) {
    let hi = GetModuleHandleW(std::ptr::null());
    let tab = WS_TABSTOP;

    macro_rules! static_text {
        ($text:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("STATIC").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | 0 /*SS_LEFT*/,
                $x, $y, $w, $h, hwnd, std::ptr::null_mut(), hi, std::ptr::null())
        };
    }
    macro_rules! edit_ctrl {
        ($id:expr, $x:expr, $y:expr, $w:expr) => {
            CreateWindowExW(WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(), wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | ES_AUTOHSCROLL as u32,
                $x, $y, $w, 22, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }
    macro_rules! push_btn {
        ($text:expr, $id:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("BUTTON").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | BS_OWNERDRAW as u32,
                $x, $y, $w, $h, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }

    static_text!("PETS", 14, 14, 60, 14);
    push_btn!("+ Add pet", ID_BTN_ADD_PET, 460, 11, 80, 24);
    static_text!("SPRITE", 14, 58, 80, 14);

    CreateWindowExW(
        0,
        wide("LISTBOX").as_ptr(),
        wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | tab
            | LBS_NOTIFY as u32
            | LBS_OWNERDRAWFIXED as u32
            | LBS_HASSTRINGS as u32,
        14, 76, 150, 224,
        hwnd,
        ID_LIST_GALLERY as usize as HMENU,
        hi,
        std::ptr::null(),
    );

    let preview = CreateWindowExW(
        0,
        wide(PREVIEW_CLASS).as_ptr(),
        wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE,
        174, 76, 368, 224,
        hwnd, std::ptr::null_mut(), hi, std::ptr::null(),
    );
    ctx.preview_hwnd = preview;

    static_text!("Scale",  14, 314, 40, 14);
    edit_ctrl!(ID_EDIT_SCALE, 14, 330, 40);
    static_text!("X",      64, 314, 16, 14);
    edit_ctrl!(ID_EDIT_X,  64, 330, 60);
    static_text!("Y",     134, 314, 16, 14);
    edit_ctrl!(ID_EDIT_Y, 134, 330, 60);
    static_text!("Speed",  204, 314, 40, 14);
    edit_ctrl!(ID_EDIT_SPEED, 204, 330, 56);

    push_btn!("Cancel", DLG_CANCEL, 358, 395, 80, 28);
    push_btn!("Save",   DLG_OK,    450, 395, 80, 28);
}

unsafe fn populate_gallery_listbox(hwnd: HWND, gallery: &SpriteGallery) {
    let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
    SendMessageW(lb, LB_RESETCONTENT, 0, 0);
    for entry in &gallery.entries {
        let w = wide(&entry.display_name);
        SendMessageW(lb, LB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    let browse = wide("Browse\u{2026}");
    SendMessageW(lb, LB_ADDSTRING, 0, browse.as_ptr() as LPARAM);
}

unsafe fn refresh_pet_chips(hwnd: HWND, ctx: &mut DialogCtx) {
    for chip in ctx.chip_hwnds.drain(..) {
        DestroyWindow(chip);
    }
    let hi = GetModuleHandleW(std::ptr::null());
    let mut x = 14i32;
    let chip_y = 32i32;
    let chip_h = 24i32;
    for (i, pet) in ctx.state.config.pets.iter().enumerate() {
        let label = wide(&format!("\u{1F436} {}", pet.id));
        let w = (label.len() as i32 * 7 + 30).max(60).min(140);
        let chip = CreateWindowExW(
            0, wide(CHIP_CLASS).as_ptr(), label.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            x, chip_y, w, chip_h,
            hwnd, std::ptr::null_mut(), hi, std::ptr::null(),
        );
        SetWindowLongPtrW(chip, GWLP_USERDATA, i as isize);
        ctx.chip_hwnds.push(chip);
        x += w + 6;
    }
}

unsafe fn load_preview_for_sprite(ctx: &mut DialogCtx) {
    let sheet = match &ctx.state.selected_sprite {
        SpriteKey::Embedded(stem) => {
            let Some((json, png)) = assets::embedded_sheet(stem) else { return };
            load_embedded(&json, &png).ok()
        }
        SpriteKey::Installed(path) => {
            let Ok(json) = std::fs::read(path) else { return };
            let Ok(png) = std::fs::read(path.with_extension("png")) else { return };
            load_embedded(&json, &png).ok()
        }
    };
    if let Some(s) = sheet {
        let tag_name = s.tags.iter()
            .find(|t| t.name.eq_ignore_ascii_case("idle"))
            .or_else(|| s.tags.first())
            .map(|t| t.name.clone())
            .unwrap_or_default();
        ctx.preview_anim = AnimationState::new(&tag_name);
        ctx.preview_sheet = Some(s);
    } else {
        ctx.preview_sheet = None;
    }
    if !ctx.preview_hwnd.is_null() {
        InvalidateRect(ctx.preview_hwnd, std::ptr::null(), 0);
    }
}

unsafe fn setup_dialog_controls(hwnd: HWND, ctx: &mut DialogCtx) {
    create_controls(hwnd, ctx);
    populate_gallery_listbox(hwnd, &ctx.gallery);
    refresh_pet_chips(hwnd, ctx);
    sync_gallery_selection(hwnd, ctx);
    refresh_fields(hwnd, &ctx.state);
    load_preview_for_sprite(ctx);
    SetTimer(hwnd, TIMER_ANIM, 100, None);
}

unsafe fn sync_gallery_selection(hwnd: HWND, ctx: &DialogCtx) {
    let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
    for (i, entry) in ctx.gallery.entries.iter().enumerate() {
        if entry.key == ctx.state.selected_sprite {
            SendMessageW(lb, LB_SETCURSEL, i, 0);
            return;
        }
    }
    SendMessageW(lb, LB_SETCURSEL, ctx.gallery.entries.len(), 0);
}

// ─── Field helpers ────────────────────────────────────────────────────────────

unsafe fn refresh_fields(hwnd: HWND, state: &ConfigDialogState) {
    if let Some(pet) = state.selected_pet() {
        set_ctrl_text(hwnd, ID_EDIT_SCALE, &pet.scale.to_string());
        set_ctrl_text(hwnd, ID_EDIT_X, &pet.x.to_string());
        set_ctrl_text(hwnd, ID_EDIT_Y, &pet.y.to_string());
        set_ctrl_text(hwnd, ID_EDIT_SPEED, &pet.walk_speed.to_string());
    }
}

unsafe fn read_fields(hwnd: HWND, state: &mut ConfigDialogState) {
    let mut buf = [0u16; 512];
    macro_rules! get_text {
        ($id:expr) => {{
            let n = GetWindowTextW(GetDlgItem(hwnd, $id), buf.as_mut_ptr(), buf.len() as i32);
            String::from_utf16_lossy(&buf[..n.max(0) as usize])
        }};
    }
    state.update_scale(&get_text!(ID_EDIT_SCALE));
    state.update_x(&get_text!(ID_EDIT_X));
    state.update_y(&get_text!(ID_EDIT_Y));
    state.update_walk_speed(&get_text!(ID_EDIT_SPEED));
}

// ─── Gallery card drawing stub ────────────────────────────────────────────────

/// Draw a gallery listbox card. Stub replaced in Chunk 4.
unsafe fn draw_gallery_card(_dis: &DRAWITEMSTRUCT) {
    // Full implementation in Task 14.
}

// ─── Browse and install ───────────────────────────────────────────────────────

unsafe fn browse_and_install(hwnd: HWND, ctx: &mut DialogCtx) -> Option<GalleryEntry> {
    let mut buf = [0u16; 512];
    let mut filter: Vec<u16> = Vec::new();
    for chunk in &["Aseprite JSON (*.json)", "*.json", "All Files (*.*)", "*.*"] {
        filter.extend(chunk.encode_utf16());
        filter.push(0);
    }
    filter.push(0);

    let mut ofn: OPENFILENAMEW = std::mem::zeroed();
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = buf.len() as u32;
    ofn.Flags = 0x00001000 | 0x00000800;

    if GetOpenFileNameW(&mut ofn) == 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path_str = String::from_utf16_lossy(&buf[..end]);
    let path = std::path::Path::new(&path_str);

    match SpriteGallery::install(path) {
        Ok(entry) => {
            ctx.gallery.entries.push(entry);
            ctx.gallery.entries.last().cloned()
        }
        Err(e) => {
            let msg = wide(&format!("Failed to install sprite:\n{e}"));
            let title = wide("Install Error");
            MessageBoxW(hwnd, msg.as_ptr(), title.as_ptr(), MB_ICONERROR | MB_OK);
            None
        }
    }
}

// ─── Command handler ──────────────────────────────────────────────────────────

unsafe fn handle_command(hwnd: HWND, id: i32, notify: u16, ctx: &mut DialogCtx) {
    match id {
        DLG_OK => {
            read_fields(hwnd, &mut ctx.state);
            ctx.state.accept();
            DestroyWindow(hwnd);
        }
        DLG_CANCEL => {
            ctx.state.cancel();
            DestroyWindow(hwnd);
        }
        ID_BTN_ADD_PET => {
            read_fields(hwnd, &mut ctx.state);
            ctx.state.add_pet();
            refresh_pet_chips(hwnd, ctx);
            refresh_fields(hwnd, &ctx.state);
        }
        ID_BTN_REMOVE_PET => {
            ctx.state.remove_selected();
            refresh_pet_chips(hwnd, ctx);
            refresh_fields(hwnd, &ctx.state);
        }
        ID_LIST_GALLERY => {
            if notify == LBN_SELCHANGE as u16 {
                let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
                let sel = SendMessageW(lb, LB_GETCURSEL, 0, 0) as usize;
                if sel < ctx.gallery.entries.len() {
                    let key = ctx.gallery.entries[sel].key.clone();
                    ctx.state.select_sprite(key);
                    load_preview_for_sprite(ctx);
                    refresh_fields(hwnd, &ctx.state);
                } else if sel == ctx.gallery.entries.len() {
                    if browse_and_install(hwnd, ctx).is_some() {
                        let new_idx = ctx.gallery.entries.len() - 1;
                        populate_gallery_listbox(hwnd, &ctx.gallery);
                        let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
                        SendMessageW(lb, LB_SETCURSEL, new_idx, 0);
                        let key = ctx.gallery.entries[new_idx].key.clone();
                        ctx.state.select_sprite(key);
                        load_preview_for_sprite(ctx);
                        refresh_fields(hwnd, &ctx.state);
                    }
                }
            }
        }
        id if id >= 2000 => {
            let pet_idx = (id - 2000) as usize;
            ctx.state.select(pet_idx);
            if let Some(pet) = ctx.state.selected_pet() {
                ctx.state.selected_sprite = SpriteKey::from_sheet_path(&pet.sheet_path.clone());
            }
            refresh_fields(hwnd, &ctx.state);
            sync_gallery_selection(hwnd, ctx);
            load_preview_for_sprite(ctx);
            for chip in &ctx.chip_hwnds {
                InvalidateRect(*chip, std::ptr::null(), 1);
            }
        }
        _ => {}
    }
}

// ─── get_ctx ──────────────────────────────────────────────────────────────────

unsafe fn get_ctx(hwnd: HWND) -> *mut DialogCtx {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogCtx
}

// ─── Chip window proc ─────────────────────────────────────────────────────────

unsafe extern "system" fn chip_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);

            let pet_idx = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize;
            let parent = GetParent(hwnd);
            let ctx = get_ctx(parent);
            let selected = !ctx.is_null() && (*ctx).state.selected == pet_idx;

            let (bg, _border) = if selected {
                (clr_bg_sel(), clr_accent())
            } else {
                (0x2d2d2d_u32, 0x444444_u32)
            };
            let hbr = CreateSolidBrush(bg);
            FillRect(hdc, &rc, hbr);
            DeleteObject(hbr as *mut _);

            let mut buf = [0u16; 128];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            let text_color = if selected { 0x00F7C34F_u32 } else { clr_text() };
            SetTextColor(hdc, text_color);
            SetBkMode(hdc, TRANSPARENT as i32);
            let mut text_rc = rc;
            text_rc.left += 8;
            text_rc.right -= 8;
            DrawTextW(hdc, buf.as_ptr(), n, &mut text_rc,
                DT_VCENTER | DT_SINGLELINE | DT_LEFT);

            EndPaint(hwnd, &ps);
            0
        }
        WM_LBUTTONDOWN => {
            let pet_idx = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize;
            let parent = GetParent(hwnd);
            PostMessageW(parent, WM_COMMAND, (2000 + pet_idx) as WPARAM, hwnd as LPARAM);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ─── Preview window proc stub (Chunk 4) ───────────────────────────────────────

unsafe extern "system" fn preview_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

// ─── Main dialog window proc ──────────────────────────────────────────────────

unsafe extern "system" fn config_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            // Context is set by show_config_dialog after CreateWindowExW returns.
            0
        }
        WM_ERASEBKGND => {
            let ctx = get_ctx(hwnd);
            if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            FillRect(hdc, &rc, (*ctx).dark_bg_brush);
            1
        }
        WM_CTLCOLORSTATIC => {
            let ctx = get_ctx(hwnd);
            if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
            SetTextColor(hdc, clr_label());
            SetBkMode(hdc, TRANSPARENT as i32);
            (*ctx).dark_bg_brush as LRESULT
        }
        WM_CTLCOLOREDIT => {
            let ctx = get_ctx(hwnd);
            if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
            SetTextColor(hdc, clr_text());
            SetBkColor(hdc, clr_bg_ctrl());
            (*ctx).ctrl_brush as LRESULT
        }
        WM_CTLCOLORLISTBOX => {
            let ctx = get_ctx(hwnd);
            if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
            SetBkColor(hdc, clr_bg());
            (*ctx).card_brush as LRESULT
        }
        WM_MEASUREITEM => {
            let mis = &mut *(lparam as *mut MEASUREITEMSTRUCT);
            if mis.CtlID == ID_LIST_GALLERY as u32 {
                mis.itemHeight = 44;
            }
            1
        }
        WM_DRAWITEM => {
            let dis = &*(lparam as *const DRAWITEMSTRUCT);
            let id = dis.CtlID as i32;
            match id {
                DLG_OK => {
                    let hbr = CreateSolidBrush(clr_accent());
                    FillRect(dis.hDC, &dis.rcItem, hbr);
                    DeleteObject(hbr as *mut _);
                    SetTextColor(dis.hDC, 0x00FFFFFF);
                    SetBkMode(dis.hDC, TRANSPARENT as i32);
                    let text = wide("Save");
                    let mut rc = dis.rcItem;
                    DrawTextW(dis.hDC, text.as_ptr(), -1, &mut rc,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                }
                DLG_CANCEL => {
                    let hbr = CreateSolidBrush(clr_bg_ctrl());
                    FillRect(dis.hDC, &dis.rcItem, hbr);
                    DeleteObject(hbr as *mut _);
                    SetTextColor(dis.hDC, clr_text());
                    SetBkMode(dis.hDC, TRANSPARENT as i32);
                    let text = wide("Cancel");
                    let mut rc = dis.rcItem;
                    DrawTextW(dis.hDC, text.as_ptr(), -1, &mut rc,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                }
                ID_LIST_GALLERY => {
                    draw_gallery_card(dis);
                }
                _ => {}
            }
            1
        }
        WM_COMMAND => {
            let id     = (wparam & 0xFFFF) as i32;
            let notify = ((wparam >> 16) & 0xFFFF) as u16;
            let ctx = get_ctx(hwnd);
            if !ctx.is_null() {
                handle_command(hwnd, id, notify, &mut *ctx);
            }
            0
        }
        WM_CLOSE => {
            let ctx = get_ctx(hwnd);
            if !ctx.is_null() {
                (*ctx).state.cancel();
            }
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let ctx = get_ctx(hwnd);
            if !ctx.is_null() {
                KillTimer(hwnd, TIMER_ANIM);
                (*ctx).gallery.destroy_thumbnails();
                (*ctx).destroy_brushes();
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
