pub mod config_window;
pub mod sm_editor;
pub mod sprite_editor;
pub mod ui_theme;

use crate::event::AppEvent;
use anyhow::Result;
use crossbeam_channel::Sender;
use muda::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

pub struct SystemTray {
    _icon: TrayIcon,
}

impl SystemTray {
    pub fn new(tx: Sender<AppEvent>) -> Result<Self> {
        let menu = Menu::new();

        let add_pet = MenuItem::new("Add Pet", true, None);
        let configure = MenuItem::new("Configure...", true, None);
        let import_bundle = MenuItem::new("Import Bundle...", true, None);
        let edit_sm = MenuItem::new("Edit State Machines", true, None);
        let separator = PredefinedMenuItem::separator();
        let quit = MenuItem::new("Quit", true, None);

        menu.append(&add_pet)?;
        menu.append(&configure)?;
        menu.append(&import_bundle)?;
        menu.append(&edit_sm)?;
        menu.append(&separator)?;
        menu.append(&quit)?;

        let add_pet_id = add_pet.id().clone();
        let configure_id = configure.id().clone();
        let import_bundle_id = import_bundle.id().clone();
        let edit_sm_id = edit_sm.id().clone();
        let quit_id = quit.id().clone();

        let tx_clone = tx.clone();
        muda::MenuEvent::set_event_handler(Some(move |ev: muda::MenuEvent| {
            if ev.id == quit_id {
                let _ = tx_clone.send(AppEvent::TrayQuit);
            } else if ev.id == add_pet_id {
                let _ = tx_clone.send(AppEvent::TrayAddPet);
            } else if ev.id == configure_id {
                let _ = tx_clone.send(AppEvent::TrayOpenConfig);
            } else if ev.id == import_bundle_id {
                let _ = tx_clone.send(AppEvent::TrayImportBundle);
            } else if ev.id == edit_sm_id {
                let _ = tx_clone.send(AppEvent::TrayOpenSmEditor);
            }
        }));

        // RC-fix: create a visible 16×16 icon (green paw / solid square).
        // A fully-transparent icon is invisible in the system tray.
        let icon_rgba = make_tray_icon();
        let icon = tray_icon::Icon::from_rgba(icon_rgba, 16, 16)?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("My Pet")
            .with_icon(icon)
            .build()?;

        Ok(SystemTray { _icon: tray })
    }
}

/// Build a 16×16 RGBA icon: bright green fill with a 1-px dark border.
fn make_tray_icon() -> Vec<u8> {
    let mut buf = vec![0u8; 4 * 16 * 16];
    for y in 0..16u32 {
        for x in 0..16u32 {
            let i = (y * 16 + x) as usize * 4;
            let border = x == 0 || x == 15 || y == 0 || y == 15;
            if border {
                buf[i] = 0;   // R
                buf[i + 1] = 100; // G
                buf[i + 2] = 0;   // B
                buf[i + 3] = 255; // A
            } else {
                buf[i] = 50;  // R
                buf[i + 1] = 220; // G
                buf[i + 2] = 50;  // B
                buf[i + 3] = 255; // A
            }
        }
    }
    buf
}
