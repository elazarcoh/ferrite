use crate::config::schema::Config;

#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Milliseconds since last tick.
    Tick(u32),
    /// Config file changed on disk.
    ConfigReloaded(Config),
    /// Config changed via the config dialog (live apply).
    ConfigChanged(Config),
    /// Tray: add a new pet with defaults.
    TrayAddPet,
    /// Tray: remove a specific pet.
    TrayRemovePet { pet_id: String },
    /// Tray: open the config dialog.
    TrayOpenConfig,
    /// Tray: quit the application.
    TrayQuit,
    /// Pet was clicked (left click on opaque pixel).
    PetClicked { pet_id: String },
    /// User started dragging a pet.
    PetDragStart { pet_id: String, cursor_x: i32, cursor_y: i32 },
    /// User released the pet after dragging.
    PetDragEnd { pet_id: String, velocity: (f32, f32) },
    /// Quit the application.
    Quit,
}
