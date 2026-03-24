use crate::config::schema::Config;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppEvent {
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
    /// A state machine file was imported/added.
    SMImported { name: String },
    /// The active state machine for a pet was changed.
    SMChanged { pet_id: String, sm_name: String },
    /// Tray: user requested to import a bundle (open file dialog).
    TrayImportBundle,
    /// A .petbundle was successfully imported (sprite + optional SM saved to disk).
    BundleImported { sprite_id: String, sm_name: Option<String> },
    /// The SM collection on disk changed (e.g. after bundle import).
    SMCollectionChanged,
    /// Tray: open the SM editor dialog.
    TrayOpenSmEditor,
}
