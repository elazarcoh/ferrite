use crate::window::blender::{alpha_at, blit_frame};
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT, SIZE},
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::*,
    System::LibraryLoader::GetModuleHandleW,
};

use crate::window::wndproc::wnd_proc;

const CLASS_NAME: &str = "MyPetWindowClass\0";

/// A single transparent, always-on-top, click-through layered window.
pub struct PetWindow {
    pub hwnd: HWND,
    pub width: u32,
    pub height: u32,
    /// Last rendered frame buffer (premultiplied BGRA). Exposed for tests.
    pub frame_buf: Vec<u8>,
    // ── GDI cache ──────────────────────────────────────────────────────────
    // These are created once in `create()` and reused every `render_frame()`.
    // Adding *mut u8 makes PetWindow automatically !Send + !Sync — correct for
    // Win32 GDI objects which must stay on their creation thread.
    mem_dc: HDC,
    dib: HBITMAP,
    /// Direct pointer into the DIB's pixel memory. Valid while `dib` is alive.
    dib_bits: *mut u8,
    /// Dimensions the GDI cache was allocated for. Used to detect size changes.
    cached_w: u32,
    cached_h: u32,
}

impl PetWindow {
    /// Register the window class (idempotent — safe to call multiple times).
    pub fn register_class() -> Result<()> {
        unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null());
            let class_name = wide(CLASS_NAME);
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: std::ptr::null_mut(),
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
                hIconSm: std::ptr::null_mut(),
            };
            // Returns 0 on failure, but also 0 if already registered — both are fine.
            RegisterClassExW(&wc);
        }
        Ok(())
    }

    /// Create a new pet window at (`x`, `y`) with the given pixel dimensions.
    pub fn create(x: i32, y: i32, width: u32, height: u32) -> Result<Self> {
        Self::register_class()?;
        unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null());
            let class_name = wide(CLASS_NAME);
            let window_name = wide("MyPet\0");

            let ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
            let style = WS_POPUP;

            let hwnd = CreateWindowExW(
                ex_style,
                class_name.as_ptr(),
                window_name.as_ptr(),
                style,
                x,
                y,
                width as i32,
                height as i32,
                std::ptr::null_mut(), // hWndParent
                std::ptr::null_mut(), // hMenu
                hinstance,
                std::ptr::null(),
            );

            anyhow::ensure!(!hwnd.is_null(), "CreateWindowExW failed");

            ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            let mut win = PetWindow {
                hwnd,
                width,
                height,
                frame_buf: Vec::new(),
                mem_dc: std::ptr::null_mut(),
                dib: std::ptr::null_mut(),
                dib_bits: std::ptr::null_mut(),
                cached_w: 0,
                cached_h: 0,
            };
            win.alloc_gdi_cache(width, height);
            Ok(win)
        }
    }

    /// Allocate (or reallocate) the mem_dc + DIB for the given dimensions.
    /// Destroys the previous objects if they exist (non-null).
    /// Destruction order: deselect bitmap → delete DC → delete bitmap.
    /// (A bitmap selected into a DC must be deselected before the DC is deleted,
    /// and the DC must be deleted before the bitmap to avoid GDI handle leaks.)
    unsafe fn alloc_gdi_cache(&mut self, w: u32, h: u32) {
        // Destroy previous objects in the correct order.
        if !self.mem_dc.is_null() {
            // Deselect the bitmap by selecting a stock object, then delete the DC.
            SelectObject(self.mem_dc, GetStockObject(BLACK_BRUSH as i32));
            DeleteDC(self.mem_dc);
            self.mem_dc = std::ptr::null_mut();
        }
        if !self.dib.is_null() {
            DeleteObject(self.dib);
            self.dib = std::ptr::null_mut();
            self.dib_bits = std::ptr::null_mut();
        }

        let hdc_screen = GetDC(std::ptr::null_mut());
        self.mem_dc = CreateCompatibleDC(hdc_screen);
        ReleaseDC(std::ptr::null_mut(), hdc_screen);

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        self.dib = CreateDIBSection(
            self.mem_dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        self.dib_bits = bits as *mut u8;
        SelectObject(self.mem_dc, self.dib);
        self.cached_w = w;
        self.cached_h = h;
    }

    /// Render a frame from `src_image` at the given source rectangle, with
    /// integer upscale and optional horizontal flip.
    pub fn render_frame(
        &mut self,
        src: &image::RgbaImage,
        src_x: u32,
        src_y: u32,
        src_w: u32,
        src_h: u32,
        scale: u32,
        flip_h: bool,
    ) -> Result<()> {
        blit_frame(src, src_x, src_y, src_w, src_h, &mut self.frame_buf, scale, flip_h);

        let dw = src_w * scale;
        let dh = src_h * scale;

        // Reallocate GDI cache if dimensions changed (e.g. scale change).
        if dw != self.cached_w || dh != self.cached_h {
            unsafe { self.alloc_gdi_cache(dw, dh); }
            self.width = dw;
            self.height = dh;
        }

        anyhow::ensure!(!self.dib_bits.is_null(), "GDI cache not initialized");

        unsafe {
            // Copy premultiplied BGRA pixels directly into the DIB's memory.
            std::ptr::copy_nonoverlapping(
                self.frame_buf.as_ptr(),
                self.dib_bits,
                self.frame_buf.len(),
            );

            let mut rc: RECT = std::mem::zeroed();
            GetWindowRect(self.hwnd, &mut rc);
            let pt_dst = POINT { x: rc.left, y: rc.top };
            let pt_src = POINT { x: 0, y: 0 };
            let sz = SIZE { cx: dw as i32, cy: dh as i32 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };

            let hdc_screen = GetDC(std::ptr::null_mut());
            let ok = UpdateLayeredWindow(
                self.hwnd, hdc_screen,
                &pt_dst, &sz,
                self.mem_dc, &pt_src,
                0, &blend, ULW_ALPHA,
            );
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            if ok == 0 {
                // Can fail on headless/RDP sessions without desktop composition — log and continue.
                log::warn!("UpdateLayeredWindow failed (err={})", windows_sys::Win32::Foundation::GetLastError());
            }
        }

        // Update the per-pixel hit-test registry after rendering.
        crate::window::wndproc::update_alpha_buf(self.hwnd, &self.frame_buf, dw);

        Ok(())
    }

    /// Returns the alpha of the pixel at the given window-local coordinates.
    pub fn alpha_at_local(&self, lx: u32, ly: u32) -> u8 {
        if self.frame_buf.is_empty() {
            return 0;
        }
        alpha_at(&self.frame_buf, self.width, lx, ly)
    }

    /// Move the window to a new position.
    pub fn move_to(&self, x: i32, y: i32) {
        unsafe {
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

impl Drop for PetWindow {
    fn drop(&mut self) {
        unsafe {
            // Deselect bitmap → delete DC → delete bitmap (GDI required order).
            if !self.mem_dc.is_null() {
                SelectObject(self.mem_dc, GetStockObject(BLACK_BRUSH as i32));
                DeleteDC(self.mem_dc);
            }
            if !self.dib.is_null() { DeleteObject(self.dib); }
            if !self.hwnd.is_null() { DestroyWindow(self.hwnd); }
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_creates_and_has_valid_hwnd() {
        let win = PetWindow::create(100, 100, 64, 64).expect("create window");
        assert!(!win.hwnd.is_null());
    }

    #[test]
    fn exstyle_correct() {
        let win = PetWindow::create(200, 200, 32, 32).expect("create window");
        let ex_style = unsafe { GetWindowLongW(win.hwnd, GWL_EXSTYLE) as u32 };
        assert_ne!(ex_style & WS_EX_LAYERED, 0, "WS_EX_LAYERED must be set");
        assert_ne!(ex_style & WS_EX_TOPMOST, 0, "WS_EX_TOPMOST must be set");
        assert_eq!(ex_style & WS_EX_TRANSPARENT, 0, "WS_EX_TRANSPARENT must NOT be set");
    }

    #[test]
    fn two_windows_coexist() {
        let w1 = PetWindow::create(0, 0, 32, 32).expect("w1");
        let w2 = PetWindow::create(50, 50, 32, 32).expect("w2");
        assert_ne!(w1.hwnd, w2.hwnd);
    }

    #[test]
    fn render_frame_does_not_panic() {
        let mut win = PetWindow::create(0, 0, 64, 64).expect("create");
        let sheet = crate::sprite::sheet::load_embedded(
            include_bytes!("../../assets/test_pet.json"),
            include_bytes!("../../assets/test_pet.png"),
        )
        .unwrap();
        let f = &sheet.frames[0];
        win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 1, false)
            .expect("render");
    }

    #[test]
    fn render_frame_flip_does_not_panic() {
        let mut win = PetWindow::create(0, 0, 64, 64).expect("create");
        let sheet = crate::sprite::sheet::load_embedded(
            include_bytes!("../../assets/test_pet.json"),
            include_bytes!("../../assets/test_pet.png"),
        )
        .unwrap();
        let f = &sheet.frames[0];
        win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 1, true)
            .expect("render flipped");
    }

    #[test]
    fn render_frame_twice_same_result() {
        let mut win = PetWindow::create(0, 0, 64, 64).expect("create");
        let sheet = crate::sprite::sheet::load_embedded(
            include_bytes!("../../assets/test_pet.json"),
            include_bytes!("../../assets/test_pet.png"),
        )
        .unwrap();
        let f = &sheet.frames[0];
        win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 2, false)
            .expect("first render");
        let buf1 = win.frame_buf.clone();
        win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 2, false)
            .expect("second render");
        assert_eq!(buf1, win.frame_buf, "frame buffer must be identical on repeated render");
    }
}
