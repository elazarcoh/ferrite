#![allow(unsafe_op_in_unsafe_fn)]
#![allow(unused_imports)]

use crate::sprite::editor_state::{EditorTag, SpriteEditorState};
use crate::sprite::sheet::{load_embedded, TagDirection};
use image::RgbaImage;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::ffi::c_void;

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreatePen,
        CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, FillRect, InvalidateRect,
        MoveToEx, LineTo, Rectangle, SelectObject,
        SetBkMode, SetTextColor, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBRUSH, HPEN, PAINTSTRUCT, PS_SOLID, SRCCOPY,
        TRANSPARENT, DrawTextW, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
    },
    System::LibraryLoader::GetModuleHandleW as SysGetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;

// ─── Single-instance guard ────────────────────────────────────────────────────

static EDITOR_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

// ─── Control IDs ─────────────────────────────────────────────────────────────

const ID_EDIT_ROWS:      i32 = 201;
const ID_EDIT_COLS:      i32 = 202;
const ID_LIST_TAGS:      i32 = 203;
const ID_BTN_ADD_TAG:    i32 = 204;
const ID_BTN_REMOVE_TAG: i32 = 205;
const ID_COMBO_BEHAVIOR: i32 = 206;
const ID_BTN_SAVE:       i32 = 207;
const ID_BTN_EXPORT:     i32 = 208;
const ID_STATIC_STATUS:  i32 = 209;
const ID_EDIT_TAG_NAME:  i32 = 210;
const ID_EDIT_TAG_FROM:  i32 = 211;
const ID_EDIT_TAG_TO:    i32 = 212;
const ID_COMBO_DIR:      i32 = 213;
const ID_BTN_TAG_OK:     i32 = 214;
const TIMER_PREVIEW:     usize = 2001;

// ─── Colors (same dark theme as config_window.rs) ────────────────────────────

const fn clr_bg()      -> u32 { 0x1e | (0x1e << 8) | (0x1e << 16) }
const fn clr_bg_ctrl() -> u32 { 0x3c | (0x3c << 8) | (0x3c << 16) }
const fn clr_text()    -> u32 { 0xcc | (0xcc << 8) | (0xcc << 16) }
const fn clr_label()   -> u32 { 0x85 | (0x85 << 8) | (0x85 << 16) }

// ─── Window class name ────────────────────────────────────────────────────────

const EDITOR_CLASS: &str = "MyPetSpriteEditor";

// ─── Context ──────────────────────────────────────────────────────────────────

struct SpriteEditorCtx {
    state: SpriteEditorState,
    /// BGRA cache of state.image for StretchDIBits (converted once on open).
    bgra_cache: Vec<u8>,
    /// Current preview frame index within the selected tag.
    preview_frame: usize,
    preview_elapsed_ms: u32,
    /// Whether the "add tag" inline form is visible.
    add_form_visible: bool,
    dark_bg_brush: HBRUSH,
    ctrl_brush: HBRUSH,
}

impl SpriteEditorCtx {
    unsafe fn new(state: SpriteEditorState) -> Box<Self> {
        let bgra_cache = rgba_to_bgra(&state.image);
        Box::new(SpriteEditorCtx {
            state,
            bgra_cache,
            preview_frame: 0,
            preview_elapsed_ms: 0,
            add_form_visible: false,
            dark_bg_brush: CreateSolidBrush(clr_bg()),
            ctrl_brush: CreateSolidBrush(clr_bg_ctrl()),
        })
    }

    unsafe fn destroy_brushes(&self) {
        DeleteObject(self.dark_bg_brush as *mut _);
        DeleteObject(self.ctrl_brush as *mut _);
    }
}

fn rgba_to_bgra(image: &RgbaImage) -> Vec<u8> {
    image.pixels()
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect()
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

unsafe fn get_ctx(hwnd: HWND) -> *mut SpriteEditorCtx {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SpriteEditorCtx
}

unsafe fn center_window(hwnd: HWND) {
    let mut rc: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rc);
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;
    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    SetWindowPos(hwnd, std::ptr::null_mut(),
        (sw - w) / 2, (sh - h) / 2, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
}

// ─── Window class registration ────────────────────────────────────────────────

fn register_editor_class() {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| unsafe {
        let hi = SysGetModuleHandleW(std::ptr::null());
        let cls_name = wide(EDITOR_CLASS);
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize        = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc   = Some(editor_wnd_proc);
        wc.hInstance     = hi;
        wc.hCursor       = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        wc.lpszClassName = cls_name.as_ptr();
        RegisterClassExW(&wc);
    });
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Open the sprite editor for `state`. Only one editor window is allowed at a
/// time — if one is already open it is brought to the foreground.
pub fn show_sprite_editor(parent: HWND, state: SpriteEditorState) {
    let stored = EDITOR_HWND.load(Ordering::Relaxed);
    if !stored.is_null() {
        unsafe {
            if IsWindow(stored as HWND) != 0 {
                SetForegroundWindow(stored as HWND);
                return;
            }
        }
    }

    register_editor_class();

    unsafe {
        let ctx = SpriteEditorCtx::new(state);
        let ctx_ptr = Box::into_raw(ctx);

        let cls   = wide(EDITOR_CLASS);
        let title = wide("My Pet \u{2014} Sprite Editor");
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPSIBLINGS | WS_CLIPCHILDREN | WS_VISIBLE;

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            cls.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT, CW_USEDEFAULT,
            780, 540,
            parent,
            std::ptr::null_mut(),
            SysGetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            drop(Box::from_raw(ctx_ptr));
            return;
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctx_ptr as isize);
        create_editor_controls(hwnd, &mut *ctx_ptr);
        refresh_tags_list(hwnd, &(*ctx_ptr).state);
        update_save_button(hwnd, &(*ctx_ptr).state);
        center_window(hwnd);
        EDITOR_HWND.store(hwnd as *mut c_void, Ordering::Relaxed);
        SetTimer(hwnd, TIMER_PREVIEW, 100, None);
    }
}

// ─── Controls ─────────────────────────────────────────────────────────────────

unsafe fn create_editor_controls(hwnd: HWND, ctx: &mut SpriteEditorCtx) {
    let hi = SysGetModuleHandleW(std::ptr::null());
    let tab = WS_TABSTOP;

    macro_rules! label {
        ($text:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("STATIC").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE, $x, $y, $w, $h,
                hwnd, std::ptr::null_mut(), hi, std::ptr::null())
        };
    }
    macro_rules! edit {
        ($id:expr, $text:expr, $x:expr, $y:expr, $w:expr) => {
            CreateWindowExW(WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | ES_AUTOHSCROLL as u32,
                $x, $y, $w, 22, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }
    macro_rules! btn {
        ($text:expr, $id:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("BUTTON").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | BS_PUSHBUTTON as u32,
                $x, $y, $w, $h, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }
    macro_rules! combo {
        ($id:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("COMBOBOX").as_ptr(), wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | CBS_DROPDOWNLIST as u32,
                $x, $y, $w, $h, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }

    // ── Below-canvas: Rows / Cols ────────────────────────────────────────────
    label!("Rows:", 10, 400, 40, 20);
    let rows_str = ctx.state.rows.to_string();
    edit!(ID_EDIT_ROWS, &rows_str, 54, 398, 36);
    label!("Cols:", 100, 400, 40, 20);
    let cols_str = ctx.state.cols.to_string();
    edit!(ID_EDIT_COLS, &cols_str, 144, 398, 36);

    // ── Right panel ──────────────────────────────────────────────────────────
    let rx = 394;

    label!("TAGS", rx, 10, 200, 14);
    // Tags listbox
    CreateWindowExW(0,
        wide("LISTBOX").as_ptr(), wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | tab | LBS_NOTIFY as u32,
        rx, 28, 340, 120,
        hwnd, ID_LIST_TAGS as usize as HMENU, hi, std::ptr::null());

    btn!("+ Add Tag",    ID_BTN_ADD_TAG,    rx,       154, 100, 24);
    btn!("Remove Tag",   ID_BTN_REMOVE_TAG, rx + 110, 154, 100, 24);

    // Inline "add tag" form (initially hidden)
    edit!(ID_EDIT_TAG_NAME, "tag_name", rx,       184, 120);
    label!("From:",        rx + 130, 186, 36, 18);
    edit!(ID_EDIT_TAG_FROM, "0",       rx + 168,  184,  36);
    label!("To:",          rx + 214,  186, 24, 18);
    edit!(ID_EDIT_TAG_TO,  "0",        rx + 240,  184,  36);
    // Direction combo
    let dir_combo = combo!(ID_COMBO_DIR, rx + 285, 183, 110, 120);
    for dir in &["forward", "reverse", "pingpong", "pingpong_reverse"] {
        let w = wide(dir);
        SendMessageW(dir_combo, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    SendMessageW(dir_combo, CB_SETCURSEL, 0, 0);
    btn!("OK", ID_BTN_TAG_OK, rx + 400, 183, 50, 24);

    // Hide add-form controls initially
    for id in &[ID_EDIT_TAG_NAME, ID_EDIT_TAG_FROM, ID_EDIT_TAG_TO,
                ID_COMBO_DIR, ID_BTN_TAG_OK] {
        ShowWindow(GetDlgItem(hwnd, *id), SW_HIDE);
    }

    // ── Behavior mapping ─────────────────────────────────────────────────────
    label!("Behavior for selected tag:", rx, 218, 200, 14);
    let beh_combo = combo!(ID_COMBO_BEHAVIOR, rx, 234, 200, 200);
    SendMessageW(beh_combo, CB_ADDSTRING, 0, wide("— not set —").as_ptr() as LPARAM);
    for state_name in &["idle","walk","run","sit","sleep","wake",
                        "grabbed","petted","react","fall","thrown"] {
        let w = wide(state_name);
        SendMessageW(beh_combo, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    SendMessageW(beh_combo, CB_SETCURSEL, 0, 0);

    // ── Preview area label ───────────────────────────────────────────────────
    label!("PREVIEW", rx, 270, 200, 14);
    // (preview drawn directly in WM_PAINT at rx, 288, 150, 150)

    // ── Save / Export ────────────────────────────────────────────────────────
    btn!("Save",     ID_BTN_SAVE,   rx,       440, 80, 28);
    btn!("Export…",  ID_BTN_EXPORT, rx + 90,  440, 80, 28);

    // Status label
    CreateWindowExW(0, wide("STATIC").as_ptr(),
        wide("Assign idle and walk to enable Save").as_ptr(),
        WS_CHILD | WS_VISIBLE,
        rx, 472, 340, 18, hwnd,
        ID_STATIC_STATUS as usize as HMENU, hi, std::ptr::null());
}

/// Populate the tags listbox from `state.tags`.
unsafe fn refresh_tags_list(hwnd: HWND, state: &SpriteEditorState) {
    let lb = GetDlgItem(hwnd, ID_LIST_TAGS);
    SendMessageW(lb, LB_RESETCONTENT, 0, 0);
    for (_i, tag) in state.tags.iter().enumerate() {
        let behavior = behavior_for_tag(state, &tag.name);
        let entry = format!("{} [{}-{}] → {}", tag.name, tag.from, tag.to,
            behavior.unwrap_or("— not set —"));
        let w = wide(&entry);
        SendMessageW(lb, LB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    if let Some(sel) = state.selected_tag {
        SendMessageW(lb, LB_SETCURSEL, sel, 0);
    }
}

/// Enable/disable Save button based on `is_saveable`.
unsafe fn update_save_button(hwnd: HWND, state: &SpriteEditorState) {
    let saveable = state.is_saveable();
    EnableWindow(GetDlgItem(hwnd, ID_BTN_SAVE), if saveable { 1 } else { 0 });
    let status = GetDlgItem(hwnd, ID_STATIC_STATUS);
    if saveable {
        let w = wide("Ready to save");
        SetWindowTextW(status, w.as_ptr());
    } else {
        let w = wide("Assign idle and walk to enable Save");
        SetWindowTextW(status, w.as_ptr());
    }
}

/// Find which behavior state maps to `tag_name` (reverse lookup of tag_map).
fn behavior_for_tag<'a>(state: &'a SpriteEditorState, tag_name: &str) -> Option<&'a str> {
    let tm = &state.tag_map;
    if tm.idle == tag_name { return Some("idle"); }
    if tm.walk == tag_name { return Some("walk"); }
    if tm.run.as_deref()     == Some(tag_name) { return Some("run"); }
    if tm.sit.as_deref()     == Some(tag_name) { return Some("sit"); }
    if tm.sleep.as_deref()   == Some(tag_name) { return Some("sleep"); }
    if tm.wake.as_deref()    == Some(tag_name) { return Some("wake"); }
    if tm.grabbed.as_deref() == Some(tag_name) { return Some("grabbed"); }
    if tm.petted.as_deref()  == Some(tag_name) { return Some("petted"); }
    if tm.react.as_deref()   == Some(tag_name) { return Some("react"); }
    if tm.fall.as_deref()    == Some(tag_name) { return Some("fall"); }
    if tm.thrown.as_deref()  == Some(tag_name) { return Some("thrown"); }
    None
}

// ─── Canvas painting ──────────────────────────────────────────────────────────

/// Paint the spritesheet with grid lines and tag highlights into the canvas
/// rectangle (left=10, top=10, width=370, height=380) of the window DC.
unsafe fn paint_canvas(hdc: windows_sys::Win32::Graphics::Gdi::HDC,
                       ctx: &SpriteEditorCtx) {
    let cx = 10i32;
    let cy = 10i32;
    let cw = 370i32;
    let ch = 380i32;

    // Background
    let canvas_rc = RECT { left: cx, top: cy, right: cx + cw, bottom: cy + ch };
    FillRect(hdc, &canvas_rc, ctx.dark_bg_brush);

    let img_w = ctx.state.image.width() as i32;
    let img_h = ctx.state.image.height() as i32;
    if img_w == 0 || img_h == 0 { return; }

    // Scale to fit, preserving aspect ratio
    let (sw, sh) = if (cw as i64) * (img_h as i64) <= (ch as i64) * (img_w as i64) {
        // scale_x is smaller
        let sw = cw;
        let sh = img_h * cw / img_w;
        (sw, sh)
    } else {
        let sh = ch;
        let sw = img_w * ch / img_h;
        (sw, sh)
    };

    // Center in canvas
    let ox = cx + (cw - sw) / 2;
    let oy = cy + (ch - sh) / 2;

    // Draw spritesheet via StretchDIBits
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize        = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth       = img_w;
    bmi.bmiHeader.biHeight      = -img_h; // top-down
    bmi.bmiHeader.biPlanes      = 1;
    bmi.bmiHeader.biBitCount    = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;

    StretchDIBits(
        hdc,
        ox, oy, sw, sh,
        0, 0, img_w, img_h,
        ctx.bgra_cache.as_ptr() as *const _,
        &bmi, DIB_RGB_COLORS, SRCCOPY,
    );

    // Cell dimensions in screen coords
    let cell_w = if ctx.state.cols > 0 { sw / ctx.state.cols as i32 } else { sw };
    let cell_h = if ctx.state.rows > 0 { sh / ctx.state.rows as i32 } else { sh };

    // Draw tag frame highlights (colored rectangle outlines)
    for (tag_idx, tag) in ctx.state.state_tags_iter() {
        let is_selected = ctx.state.selected_tag == Some(tag_idx);
        let pen_width = if is_selected { 3 } else { 1 };
        let pen = CreatePen(PS_SOLID as i32, pen_width, tag.color);
        let old_pen = SelectObject(hdc, pen as *mut _);
        let old_brush = SelectObject(hdc,
            windows_sys::Win32::Graphics::Gdi::GetStockObject(
                windows_sys::Win32::Graphics::Gdi::NULL_BRUSH as i32) as *mut _);

        for frame_idx in tag.from..=tag.to {
            let col = (frame_idx % ctx.state.cols as usize) as i32;
            let row = (frame_idx / ctx.state.cols as usize) as i32;
            let fx = ox + col * cell_w;
            let fy = oy + row * cell_h;
            Rectangle(hdc, fx, fy, fx + cell_w, fy + cell_h);
        }

        SelectObject(hdc, old_pen as *mut _);
        SelectObject(hdc, old_brush as *mut _);
        DeleteObject(pen as *mut _);
    }

    // Draw gray grid lines
    let gray_pen = CreatePen(PS_SOLID as i32, 1, 0x00555555);
    let old = SelectObject(hdc, gray_pen as *mut _);
    for col in 1..ctx.state.cols as i32 {
        let x = ox + col * cell_w;
        MoveToEx(hdc, x, oy, std::ptr::null_mut());
        LineTo(hdc, x, oy + sh);
    }
    for row in 1..ctx.state.rows as i32 {
        let y = oy + row * cell_h;
        MoveToEx(hdc, ox, y, std::ptr::null_mut());
        LineTo(hdc, ox + sw, y);
    }
    SelectObject(hdc, old as *mut _);
    DeleteObject(gray_pen as *mut _);
}

/// Paint the preview of the selected tag's current frame at (rx, 288, 150, 150).
unsafe fn paint_preview(hdc: windows_sys::Win32::Graphics::Gdi::HDC,
                        ctx: &SpriteEditorCtx) {
    let px = 394i32;
    let py = 288i32;
    let pw = 150i32;
    let ph = 150i32;

    let rc = RECT { left: px, top: py, right: px + pw, bottom: py + ph };
    FillRect(hdc, &rc, ctx.dark_bg_brush);

    let sel = match ctx.state.selected_tag {
        Some(idx) => idx,
        None => return,
    };
    let tag = match ctx.state.tags.get(sel) {
        Some(t) => t,
        None => return,
    };

    let total_tag_frames = tag.to.saturating_sub(tag.from) + 1;
    if total_tag_frames == 0 { return; }
    let frame_in_tag = ctx.preview_frame % total_tag_frames;
    let frame_idx = tag.from + frame_in_tag;

    let (fx, fy, fw, fh) = ctx.state.frame_rect(frame_idx);
    if fw == 0 || fh == 0 { return; }

    let img_w = ctx.state.image.width() as i32;
    let img_h = ctx.state.image.height() as i32;

    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize        = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth       = img_w;
    bmi.bmiHeader.biHeight      = -img_h;
    bmi.bmiHeader.biPlanes      = 1;
    bmi.bmiHeader.biBitCount    = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;

    StretchDIBits(
        hdc,
        px, py, pw, ph,
        fx as i32, fy as i32, fw as i32, fh as i32,
        ctx.bgra_cache.as_ptr() as *const _,
        &bmi, DIB_RGB_COLORS, SRCCOPY,
    );
}

// ─── Window procedure ─────────────────────────────────────────────────────────

unsafe extern "system" fn editor_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let ctx = get_ctx(hwnd);
            if !ctx.is_null() {
                // Fill entire background
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, (*ctx).dark_bg_brush);
                paint_canvas(hdc, &*ctx);
                paint_preview(hdc, &*ctx);
            }
            EndPaint(hwnd, &ps);
            0
        }

        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => {
            let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, clr_text());
            let ctx = get_ctx(hwnd);
            if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            (*ctx).dark_bg_brush as LRESULT
        }

        WM_TIMER => {
            if wparam == TIMER_PREVIEW {
                let ctx = get_ctx(hwnd);
                if !ctx.is_null() && (*ctx).state.selected_tag.is_some() {
                    (*ctx).preview_elapsed_ms += 100;
                    if (*ctx).preview_elapsed_ms >= 100 {
                        (*ctx).preview_elapsed_ms = 0;
                        (*ctx).preview_frame = (*ctx).preview_frame.wrapping_add(1);
                    }
                    // Repaint only the preview area
                    let rc = RECT { left: 394, top: 288, right: 544, bottom: 438 };
                    InvalidateRect(hwnd, &rc, 0);
                }
            }
            0
        }

        WM_COMMAND => {
            let id     = (wparam & 0xffff) as i32;
            let notify = ((wparam >> 16) & 0xffff) as u16;
            let ctx = get_ctx(hwnd);
            if !ctx.is_null() {
                handle_editor_command(hwnd, id, notify, &mut *ctx);
            }
            0
        }

        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }

        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SpriteEditorCtx;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                KillTimer(hwnd, TIMER_PREVIEW);
                let ctx = Box::from_raw(ptr);
                ctx.destroy_brushes();
                // ctx dropped here
            }
            EDITOR_HWND.store(std::ptr::null_mut(), Ordering::Relaxed);
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ─── Command handler ──────────────────────────────────────────────────────────

unsafe fn handle_editor_command(
    hwnd: HWND,
    id: i32,
    notify: u16,
    ctx: &mut SpriteEditorCtx,
) {
    match id {
        ID_EDIT_ROWS | ID_EDIT_COLS => {
            if notify == EN_KILLFOCUS as u16 {
                let rows = read_u32_field(hwnd, ID_EDIT_ROWS, 1, 64).unwrap_or(1);
                let cols = read_u32_field(hwnd, ID_EDIT_COLS, 1, 64).unwrap_or(1);
                ctx.state.rows = rows;
                ctx.state.cols = cols;
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
        }

        ID_LIST_TAGS => {
            if notify == LBN_SELCHANGE as u16 {
                let lb = GetDlgItem(hwnd, ID_LIST_TAGS);
                let sel = SendMessageW(lb, LB_GETCURSEL, 0, 0) as isize;
                if sel >= 0 {
                    ctx.state.selected_tag = Some(sel as usize);
                    ctx.preview_frame = 0;
                    // Update behavior combo to show current mapping
                    if let Some(tag) = ctx.state.tags.get(sel as usize) {
                        let beh = behavior_for_tag(&ctx.state, &tag.name);
                        let combo = GetDlgItem(hwnd, ID_COMBO_BEHAVIOR);
                        let idx = match beh {
                            None         => 0,
                            Some("idle") => 1,  Some("walk")    => 2,
                            Some("run")  => 3,  Some("sit")     => 4,
                            Some("sleep")=> 5,  Some("wake")    => 6,
                            Some("grabbed")=>7, Some("petted")  => 8,
                            Some("react")=> 9,  Some("fall")    => 10,
                            Some("thrown")=>11, _               => 0,
                        };
                        SendMessageW(combo, CB_SETCURSEL, idx, 0);
                    }
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
        }

        ID_BTN_ADD_TAG => {
            ctx.add_form_visible = !ctx.add_form_visible;
            let vis = if ctx.add_form_visible { SW_SHOW } else { SW_HIDE };
            for ctrl_id in &[ID_EDIT_TAG_NAME, ID_EDIT_TAG_FROM, ID_EDIT_TAG_TO,
                              ID_COMBO_DIR, ID_BTN_TAG_OK] {
                ShowWindow(GetDlgItem(hwnd, *ctrl_id), vis);
            }
        }

        ID_BTN_TAG_OK => {
            // Read the add-tag form and create a new EditorTag
            let name = read_window_text(GetDlgItem(hwnd, ID_EDIT_TAG_NAME));
            if name.is_empty() { return; }
            let from = read_u32_field(hwnd, ID_EDIT_TAG_FROM, 0, 9999).unwrap_or(0) as usize;
            let to   = read_u32_field(hwnd, ID_EDIT_TAG_TO,   0, 9999).unwrap_or(0) as usize;
            let dir_idx = SendMessageW(GetDlgItem(hwnd, ID_COMBO_DIR), CB_GETCURSEL, 0, 0) as usize;
            let direction = match dir_idx {
                1 => TagDirection::Reverse,
                2 => TagDirection::PingPong,
                3 => TagDirection::PingPongReverse,
                _ => TagDirection::Forward,
            };
            let color = SpriteEditorState::assign_color(ctx.state.tags.len());
            ctx.state.tags.push(EditorTag { name, from, to, direction, color });
            ctx.state.selected_tag = Some(ctx.state.tags.len() - 1);
            refresh_tags_list(hwnd, &ctx.state);
            update_save_button(hwnd, &ctx.state);
            // Hide form
            ctx.add_form_visible = false;
            for ctrl_id in &[ID_EDIT_TAG_NAME, ID_EDIT_TAG_FROM, ID_EDIT_TAG_TO,
                              ID_COMBO_DIR, ID_BTN_TAG_OK] {
                ShowWindow(GetDlgItem(hwnd, *ctrl_id), SW_HIDE);
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
        }

        ID_BTN_REMOVE_TAG => {
            if let Some(sel) = ctx.state.selected_tag {
                if sel < ctx.state.tags.len() {
                    ctx.state.tags.remove(sel);
                    ctx.state.selected_tag = if ctx.state.tags.is_empty() {
                        None
                    } else {
                        Some(sel.saturating_sub(1))
                    };
                    refresh_tags_list(hwnd, &ctx.state);
                    update_save_button(hwnd, &ctx.state);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
        }

        ID_COMBO_BEHAVIOR => {
            if notify == CBN_SELCHANGE as u16 {
                let sel = ctx.state.selected_tag;
                if let Some(tag_idx) = sel {
                    if let Some(tag) = ctx.state.tags.get(tag_idx) {
                        let tag_name = tag.name.clone();
                        let combo = GetDlgItem(hwnd, ID_COMBO_BEHAVIOR);
                        let beh_idx = SendMessageW(combo, CB_GETCURSEL, 0, 0) as usize;
                        let beh_names = ["", "idle","walk","run","sit","sleep","wake",
                                         "grabbed","petted","react","fall","thrown"];
                        let beh = beh_names.get(beh_idx).copied().unwrap_or("");
                        set_behavior_mapping(&mut ctx.state.tag_map, beh, &tag_name);
                        refresh_tags_list(hwnd, &ctx.state);
                        update_save_button(hwnd, &ctx.state);
                    }
                }
            }
        }

        ID_BTN_SAVE => {
            if ctx.state.is_saveable() {
                let dir = crate::window::sprite_gallery::SpriteGallery::appdata_sprites_dir();
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    show_error(hwnd, &format!("Could not create sprites dir: {e}"));
                    return;
                }
                match ctx.state.save_to_dir(&dir) {
                    Ok(()) => {
                        let msg = wide("Sprite saved. It will appear in the gallery next time you open Configure.");
                        let title = wide("Saved");
                        MessageBoxW(hwnd, msg.as_ptr(), title.as_ptr(), MB_ICONINFORMATION | MB_OK);
                    }
                    Err(e) => show_error(hwnd, &format!("Save failed: {e}")),
                }
            }
        }

        ID_BTN_EXPORT => {
            export_dialog(hwnd, ctx);
        }

        _ => {}
    }
}

/// Map a behavior state name to the given tag name in `tag_map`.
/// Clears any previous mapping for this behavior first.
fn set_behavior_mapping(
    tm: &mut crate::sprite::behavior::AnimTagMap,
    behavior: &str,
    tag_name: &str,
) {
    match behavior {
        "idle"    => tm.idle    = tag_name.to_string(),
        "walk"    => tm.walk    = tag_name.to_string(),
        "run"     => tm.run     = Some(tag_name.to_string()),
        "sit"     => tm.sit     = Some(tag_name.to_string()),
        "sleep"   => tm.sleep   = Some(tag_name.to_string()),
        "wake"    => tm.wake    = Some(tag_name.to_string()),
        "grabbed" => tm.grabbed = Some(tag_name.to_string()),
        "petted"  => tm.petted  = Some(tag_name.to_string()),
        "react"   => tm.react   = Some(tag_name.to_string()),
        "fall"    => tm.fall    = Some(tag_name.to_string()),
        "thrown"  => tm.thrown  = Some(tag_name.to_string()),
        _         => {} // "— not set —" → no-op
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

unsafe fn read_u32_field(hwnd: HWND, id: i32, min: u32, max: u32) -> Option<u32> {
    let text = read_window_text(GetDlgItem(hwnd, id));
    let v: u32 = text.trim().parse().ok()?;
    if v >= min && v <= max { Some(v) } else { None }
}

unsafe fn read_window_text(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd) as usize;
    if len == 0 { return String::new(); }
    let mut buf = vec![0u16; len + 1];
    GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..len])
}

unsafe fn show_error(hwnd: HWND, msg: &str) {
    let w_msg   = wide(msg);
    let w_title = wide("Error");
    MessageBoxW(hwnd, w_msg.as_ptr(), w_title.as_ptr(), MB_ICONERROR | MB_OK);
}

unsafe fn export_dialog(hwnd: HWND, ctx: &SpriteEditorCtx) {
    use windows_sys::Win32::UI::Controls::Dialogs::{GetSaveFileNameW, OPENFILENAMEW};
    let stem = ctx.state.png_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let filter = wide("JSON files\0*.json\0All files\0*.*\0");
    let mut file_buf: Vec<u16> = wide(&stem);
    file_buf.resize(1024, 0);

    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner   = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile   = file_buf.as_mut_ptr();
    ofn.nMaxFile    = file_buf.len() as u32;
    ofn.Flags       = windows_sys::Win32::UI::Controls::Dialogs::OFN_OVERWRITEPROMPT;

    if GetSaveFileNameW(&mut ofn) == 0 { return; }

    let json_path = std::path::PathBuf::from(String::from_utf16_lossy(
        &file_buf[..file_buf.iter().position(|&c| c == 0).unwrap_or(0)],
    ));

    // Write clean JSON to chosen path directly
    match std::fs::write(&json_path, ctx.state.to_clean_json()) {
        Ok(()) => {
            // Also copy the PNG next to it
            let png_dest = json_path.with_extension("png");
            if let Err(e) = std::fs::copy(&ctx.state.png_path, &png_dest) {
                show_error(hwnd, &format!("Could not copy PNG: {e}"));
            } else {
                let msg = wide(&format!("Exported to {}", json_path.display()));
                let title = wide("Exported");
                MessageBoxW(hwnd, msg.as_ptr(), title.as_ptr(), MB_ICONINFORMATION | MB_OK);
            }
        }
        Err(e) => show_error(hwnd, &format!("Export failed: {e}")),
    }
}
