use ferrite::window::pet_window::PetWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[test]
fn window_creates_valid_hwnd() {
    let win = PetWindow::create(10, 10, 64, 64).expect("create window");
    assert!(!win.hwnd.is_null());
}

#[test]
fn exstyle_layered_topmost_no_transparent() {
    let win = PetWindow::create(10, 10, 32, 32).expect("create");
    let ex = unsafe { GetWindowLongW(win.hwnd, GWL_EXSTYLE) as u32 };
    assert_ne!(ex & WS_EX_LAYERED, 0, "WS_EX_LAYERED required");
    assert_ne!(ex & WS_EX_TOPMOST, 0, "WS_EX_TOPMOST required");
    assert_eq!(ex & WS_EX_TRANSPARENT, 0, "WS_EX_TRANSPARENT must be absent");
}

#[test]
fn two_windows_distinct_hwnds() {
    let w1 = PetWindow::create(0, 0, 32, 32).unwrap();
    let w2 = PetWindow::create(100, 0, 32, 32).unwrap();
    assert_ne!(w1.hwnd, w2.hwnd);
}

#[test]
fn render_frame_succeeds() {
    let mut win = PetWindow::create(0, 0, 64, 64).unwrap();
    let sheet = ferrite::sprite::sheet::load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap();
    let f = &sheet.frames[0];
    win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 1, false)
        .expect("render_frame");
}

#[test]
fn render_frame_flip_succeeds() {
    let mut win = PetWindow::create(0, 0, 64, 64).unwrap();
    let sheet = ferrite::sprite::sheet::load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap();
    let f = &sheet.frames[0];
    win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 1, true)
        .expect("render_frame flipped");
}
