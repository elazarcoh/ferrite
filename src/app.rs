use ferrite_core::geometry::{PetGeom, PlatformBounds};
use ferrite_core::sprite::collision::{detect_new_collisions, overlapping_pairs, Collidable};
use ferrite_core::sprite::sm_runner::EnvironmentSnapshot;
use crate::{
    assets,
    config::{self, schema::PetConfig, watcher::spawn_watcher},
    event::AppEvent,
    sprite::{
        animation::AnimationState,
        sheet::{self, apply_chromakey, SpriteSheet},
        sm_runner::SMRunner,
    },
    tray::{
        app_window::{new_app_window_state, open_app_window, AppWindowState},
        window_lifecycle::AppWindowLifecycle,
        SystemTray,
    },
    window::pet_window::PetWindow,
};
use anyhow::{Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use eframe::egui;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{POINT, RECT, SYSTEMTIME},
    System::SystemInformation::GetLocalTime,
    UI::WindowsAndMessaging::*,
};

// ─── Per-pet runtime state ────────────────────────────────────────────────────

/// Complete runtime state for one pet instance.
pub struct PetInstance {
    pub cfg: PetConfig,
    pub sheet: SpriteSheet,
    pub window: PetWindow,
    pub anim: AnimationState,
    pub runner: SMRunner,
    pub x: i32,
    pub y: i32,
    /// Milliseconds the pet has spent on an elevated surface (above virtual ground).
    /// Reset when grounded; forces Fall when it exceeds the drop threshold.
    elevated_ms: u32,
    /// Flip state from the last rendered frame. Re-render when this changes so
    /// direction changes take effect immediately, independent of frame timing.
    last_flip: bool,
    /// Surface metadata from the last call to `find_floor_info`.
    last_surface_hit: crate::window::surfaces::SurfaceHit,
}

impl PetInstance {
    pub fn new(cfg: PetConfig, sheet: SpriteSheet) -> Result<Self> {
        let frame_w = sheet.frames.first().map(|f| f.w).unwrap_or(32);
        let frame_h = sheet.frames.first().map(|f| f.h).unwrap_or(32);
        let dw = (frame_w as f32 * cfg.scale).round() as u32;
        let dh = (frame_h as f32 * cfg.scale).round() as u32;

        // Spawn above the top of the screen so the pet falls into view.
        let spawn_y = -(dh as i32);
        let window = PetWindow::create(cfg.x, spawn_y, dw, dh)?;

        // Register this window for per-pixel hit testing.
        crate::window::wndproc::register_hwnd(window.hwnd, cfg.id.clone());

        let runner = if cfg.state_machine == "embedded://default" || cfg.state_machine.is_empty() {
            let sm = crate::sprite::sm_runner::load_default_sm();
            SMRunner::new(sm, cfg.walk_speed)
        } else {
            // Try to load from the SM gallery by name
            let config_dir = config::config_path()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let gallery = crate::sprite::sm_gallery::SmGallery::load(&config_dir);
            let sm = gallery.get(&cfg.state_machine)
                .unwrap_or_else(crate::sprite::sm_runner::load_default_sm);
            SMRunner::new(sm, cfg.walk_speed)
        };

        let anim = AnimationState::new("fall".to_string());

        let mut inst = PetInstance {
            x: cfg.x,
            y: spawn_y,
            cfg,
            sheet,
            window,
            anim,
            runner,
            elevated_ms: 0,
            last_flip: false,
            last_surface_hit: crate::window::surfaces::SurfaceHit {
                floor_y: 0,
                surface_w: 0.0,
                surface_label: String::new(),
            },
        };

        inst.render_current_frame()?;

        log::info!("pet '{}' created — falling from y={spawn_y}", inst.cfg.id);
        Ok(inst)
    }

    pub fn tick(&mut self, delta_ms: u32, cache: &mut crate::window::surfaces::SurfaceCache) -> Result<()> {
        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let pet_w = self.window.width as i32;
        let pet_h = self.window.height as i32;
        let baseline_offset_px = (self.sheet.baseline_offset as f32 * self.cfg.scale).round() as i32;

        // Sync position from Win32 to pick up wndproc drag moves.
        unsafe {
            let mut rc: RECT = std::mem::zeroed();
            GetWindowRect(self.window.hwnd, &mut rc);
            self.x = rc.left;
            self.y = rc.top;
        }

        let being_dragged = crate::window::wndproc::is_mouse_down(self.window.hwnd);

        // Compute the nearest walkable surface below the pet before the AI tick
        // (used by Fall/Thrown physics for landing termination).
        let bounds = PlatformBounds { screen_w, screen_h };
        let geom = PetGeom { x: self.x, y: self.y, w: pet_w, h: pet_h, baseline_offset: baseline_offset_px };
        let hit = crate::window::surfaces::find_floor_info(&geom, &bounds, cache);
        let floor_y = hit.floor_y;
        self.last_surface_hit = hit;

        let tag = self.runner.tick(
            delta_ms,
            &mut self.x,
            &mut self.y,
            &bounds,
            pet_w,
            pet_h,
            floor_y,
            &self.sheet,
        );
        self.anim.set_tag(tag.to_string());

        // After the runner has potentially moved x (Walk), recompute floor at
        // the new position and apply surface snapping / edge-fall logic.
        // Reconstruct geom: runner.tick may have moved self.x / self.y.
        let geom_post_tick = PetGeom { x: self.x, y: self.y, w: pet_w, h: pet_h, baseline_offset: baseline_offset_px };
        let is_airborne = matches!(
            self.runner.active,
            crate::sprite::sm_runner::ActiveState::Airborne { .. }
            | crate::sprite::sm_runner::ActiveState::Grabbed { .. }
        );
        if !being_dragged && !is_airborne {
            let new_floor = crate::window::surfaces::find_floor(&geom_post_tick, &bounds, cache);
            // If the floor dropped more than one pet height, the pet walked
            // off a window edge — start falling.
            if new_floor > self.y + pet_h {
                self.runner.active = crate::sprite::sm_runner::ActiveState::Airborne { vx: 0.0, vy: 0.0 };
            } else {
                // Snap to surface (handles small steps up/down between windows)
                self.y = new_floor;
            }
        }

        // Elevated-surface drop: if the pet has been sitting on a raised window
        // for too long, make it fall off (eSheep-style edge drop).
        const ELEVATED_DROP_MS: u32 = 20_000; // 20 s before dropping
        let virtual_ground = geom_post_tick.floor_landing_y(bounds.virtual_ground_y());
        if is_airborne || self.y >= virtual_ground - 4 {
            // On ground or in the air — reset timer.
            self.elevated_ms = 0;
        } else {
            self.elevated_ms = self.elevated_ms.saturating_add(delta_ms);
            if self.elevated_ms >= ELEVATED_DROP_MS {
                log::debug!("elevated_drop after {}ms at y={}", self.elevated_ms, self.y);
                self.runner.active = crate::sprite::sm_runner::ActiveState::Airborne { vx: 0.0, vy: 0.0 };
                self.elevated_ms = 0;
            }
        }

        // Always update window position.
        self.window.move_to(self.x, self.y);

        let frame_changed = self.anim.tick(&self.sheet, delta_ms);
        let current_flip = self.compute_flip();
        if frame_changed || current_flip != self.last_flip {
            self.render_current_frame()?;
        }
        Ok(())
    }

    /// Returns whether the current frame should be rendered flipped horizontally.
    /// `flip_h=false` (default) = sprite faces RIGHT. Flip when going LEFT.
    /// `flip_h=true`            = sprite faces LEFT.  Flip when going RIGHT.
    pub fn compute_flip(&self) -> bool {
        self.runner.compute_flip(&self.sheet)
    }

    fn render_current_frame(&mut self) -> Result<()> {
        let abs = self.anim.absolute_frame(&self.sheet);
        let f = &self.sheet.frames[abs];
        let flip = self.compute_flip();
        self.last_flip = flip;
        self.window.render_frame(
            &self.sheet.image,
            f.x, f.y, f.w, f.h,
            self.cfg.scale,
            flip,
        )
    }

    // ─── Test-helper accessors ───────────────────────────────────────────────

    /// Returns true if the window's internal pixel buffer is empty.
    #[allow(dead_code)]
    pub fn window_frame_buf_is_empty(&self) -> bool {
        self.window.frame_buf.is_empty()
    }

    /// Returns a reference to the window's premultiplied BGRA buffer.
    #[allow(dead_code)]
    pub fn window_frame_buf(&self) -> &[u8] {
        &self.window.frame_buf
    }

    /// Returns the rendered window width in pixels (after scale).
    #[allow(dead_code)]
    pub fn window_width(&self) -> u32 {
        self.window.width
    }
}

impl Drop for PetInstance {
    fn drop(&mut self) {
        crate::window::wndproc::unregister_hwnd(self.window.hwnd);
    }
}

// ─── App ─────────────────────────────────────────────────────────────────────

pub struct App {
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
    pets: HashMap<String, PetInstance>,
    _tray: SystemTray,
    _watcher: notify::RecommendedWatcher,
    last_tick_ms: std::time::Instant,
    app_window: Arc<Mutex<AppWindowState>>,
    app_window_lifecycle: AppWindowLifecycle,
    should_quit: bool,
    surface_cache: crate::window::surfaces::SurfaceCache,
    dark_mode: bool,
    /// Pending bundle file-picker result (Some while dialog is open).
    pending_bundle_pick: Option<crossbeam_channel::Receiver<Option<std::path::PathBuf>>>,
    /// Canonical (id_min, id_max) pairs that were overlapping last frame.
    overlapping: std::collections::HashSet<(String, String)>,
}

impl App {
    pub fn new() -> Result<Self> {
        let (tx, rx) = bounded::<AppEvent>(256);

        crate::window::wndproc::init_event_sender(tx.clone());

        let cfg_path = config::config_path();
        let cfg = config::load(&cfg_path).unwrap_or_default();

        let mut pets = HashMap::new();
        for pet_cfg in &cfg.pets {
            match build_pet(pet_cfg) {
                Ok(inst) => { pets.insert(pet_cfg.id.clone(), inst); }
                Err(e) => log::warn!("failed to create pet '{}': {e}", pet_cfg.id),
            }
        }

        let tray = SystemTray::new(tx.clone()).context("create tray")?;
        let watcher = spawn_watcher(cfg_path.clone(), tx.clone()).context("create watcher")?;

        let config_dir = cfg_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let app_window = new_app_window_state(cfg, tx.clone(), true, config_dir);

        Ok(App {
            tx,
            rx,
            pets,
            _tray: tray,
            _watcher: watcher,
            last_tick_ms: std::time::Instant::now(),
            app_window,
            app_window_lifecycle: AppWindowLifecycle::new(),
            should_quit: false,
            surface_cache: crate::window::surfaces::SurfaceCache::default(),
            dark_mode: true,
            pending_bundle_pick: None,
            overlapping: std::collections::HashSet::new(),
        })
    }

    /// Load a sprite sheet from a path string. Public for use by config window.
    pub fn load_sheet_for_config(path: &str) -> Result<SpriteSheet> {
        load_sheet(path)
    }

    /// Reload the sheet for any live pet whose `sheet_path` resolves to `json_path`.
    fn reload_pets_for_sheet(&mut self, json_path: &std::path::Path) {
        for inst in self.pets.values_mut() {
            let pet_json_path = match inst.cfg.sheet_path.strip_prefix("embedded://") {
                Some(_) => None, // embedded sheets can't be reloaded from a file path
                None => Some(std::path::Path::new(&inst.cfg.sheet_path)),
            };
            let matches = pet_json_path.is_some_and(|p| {
                p == json_path
                    || p.canonicalize().ok().as_deref() == json_path.canonicalize().ok().as_deref()
            });
            if matches {
                match load_sheet(&inst.cfg.sheet_path) {
                    Ok(new_sheet) => {
                        inst.sheet = new_sheet;
                        log::info!("hot-reloaded sheet for pet '{}'", inst.cfg.id);
                    }
                    Err(e) => log::warn!("sheet reload failed for '{}': {e}", inst.cfg.id),
                }
            }
        }
    }

    fn import_bundle(&mut self, path: &std::path::Path) {
        match std::fs::read(path) {
            Ok(data) => {
                match ferrite_core::bundle::import(&data) {
                    Ok(contents) => {
                        // Get base dir (where config.toml is)
                        let base_dir = config::config_path()
                            .parent()
                            .map(|p| p.to_path_buf())
                            .unwrap_or_else(|| std::path::PathBuf::from("."));

                        // Save sprite files
                        let sprites_dir = base_dir.join("sprites");
                        let _ = std::fs::create_dir_all(&sprites_dir);

                        let sprite_id = sanitize_id(&contents.bundle_name);
                        let json_filename = format!("{}.json", sprite_id);
                        let png_filename = format!("{}.png", sprite_id);

                        let _ = std::fs::write(sprites_dir.join(&json_filename), contents.sprite_json.as_bytes());
                        let _ = std::fs::write(sprites_dir.join(&png_filename), &contents.sprite_png);

                        // Save SM if present
                        let sm_name = if let Some(sm_source) = &contents.sm_source {
                            let mut gallery = crate::sprite::sm_gallery::SmGallery::load(&base_dir);
                            let sm_name = contents.recommended_sm.clone()
                                .unwrap_or_else(|| contents.bundle_name.clone());
                            let _ = gallery.save(&sm_name, sm_source);
                            Some(sm_name)
                        } else {
                            None
                        };

                        // Update sprite gallery
                        let mut sprite_gallery = crate::sprite::sprite_gallery::SpriteGallery::load(&base_dir);
                        let entry = crate::sprite::sprite_gallery::SpriteEntry {
                            id: sprite_id.clone(),
                            json_path: format!("sprites/{}", json_filename),
                            png_path: format!("sprites/{}", png_filename),
                            recommended_sm: sm_name.clone(),
                        };
                        let _ = sprite_gallery.add(entry);

                        log::info!("Bundle imported: {} (SM: {:?})", sprite_id, sm_name);
                        let _ = self.tx.send(AppEvent::BundleImported { sprite_id, sm_name });
                    }
                    Err(e) => {
                        log::error!("Bundle import failed: {}", e);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to read bundle file: {}", e);
            }
        }
    }

    fn handle_event(&mut self, ev: AppEvent, ctx: &egui::Context) -> Result<()> {
        match ev {
            AppEvent::Quit | AppEvent::TrayQuit => {
                self.should_quit = true;
            }
            AppEvent::TrayAddPet => {
                let id = format!("pet_{}", self.pets.len());
                let cfg = PetConfig { id: id.clone(), ..PetConfig::default() };
                match build_pet(&cfg) {
                    Ok(inst) => { self.pets.insert(id, inst); }
                    Err(e) => log::warn!("add pet failed: {e}"),
                }
            }
            AppEvent::TrayRemovePet { pet_id } => {
                self.pets.remove(&pet_id);
            }
            AppEvent::TrayOpenWindow => {
                if self.app_window_lifecycle.open {
                    log::debug!(
                        "TrayOpenWindow: already open (gen {}), focusing",
                        self.app_window_lifecycle.generation
                    );
                    // Already open — bring to front
                    ctx.send_viewport_cmd_to(
                        egui::ViewportId::from_hash_of(
                            format!("app_window_{}", self.app_window_lifecycle.generation),
                        ),
                        egui::ViewportCommand::Focus,
                    );
                } else {
                    // Fresh generation: new close flag, no mutex access needed.
                    self.app_window_lifecycle.open();
                    log::debug!(
                        "TrayOpenWindow: opening gen {}",
                        self.app_window_lifecycle.generation
                    );
                }
            }
            AppEvent::TrayOpenConfig | AppEvent::TrayOpenSmEditor => {}
            AppEvent::ConfigReloaded(new_cfg) => {
                self.apply_config(new_cfg)?;
            }
            AppEvent::ConfigChanged(cfg) => {
                if let Err(e) = config::save(&config::config_path(), &cfg) {
                    log::warn!("auto-save config failed: {e}");
                }
                self.apply_config(cfg)?;
            }
            AppEvent::PetClicked { pet_id } => {
                log::debug!("PetClicked pet_id={pet_id}");
                if let Some(p) = self.pets.get_mut(&pet_id) {
                    let state_name = p.runner.current_state_name().to_string();
                    if state_name == "sleep" {
                        p.runner.interrupt("wake", None);
                    } else {
                        p.runner.interrupt("petted", None);
                    }
                }
            }
            AppEvent::PetDragStart { pet_id, cursor_x, cursor_y } => {
                log::debug!("PetDragStart pet_id={pet_id} cursor=({cursor_x},{cursor_y})");
                if let Some(p) = self.pets.get_mut(&pet_id) {
                    p.runner.interrupt("grabbed", Some((cursor_x - p.x, cursor_y - p.y)));
                }
            }
            AppEvent::PetDragEnd { pet_id, velocity } => {
                log::debug!("PetDragEnd pet_id={pet_id} vel=({:.0},{:.0})", velocity.0, velocity.1);
                if let Some(p) = self.pets.get_mut(&pet_id) {
                    p.runner.release(velocity);
                }
            }
            AppEvent::SMImported { name } => {
                log::info!("SM imported: {}", name);
                self.notify_sm_collection_changed();
            }
            AppEvent::SMChanged { pet_id, sm_name } => {
                // Reload gallery to get the named SM
                let config_dir = config::config_path()
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let gallery = crate::sprite::sm_gallery::SmGallery::load(&config_dir);
                let mut applied = false;
                if let Some(pet) = self.pets.get_mut(&pet_id) {
                    if let Some(sm) = gallery.get(&sm_name) {
                        pet.runner.replace_sm(sm);
                        pet.cfg.state_machine = sm_name.clone();
                        log::info!("SMChanged: applied '{}' to pet '{}'", sm_name, pet_id);
                        applied = true;
                    } else {
                        log::warn!("SMChanged: SM '{}' not found in gallery", sm_name);
                    }
                } else {
                    log::warn!("SMChanged: pet '{}' not found", pet_id);
                }
                if applied {
                    let current_cfg = crate::config::schema::Config {
                        pets: self.pets.values().map(|p| p.cfg.clone()).collect(),
                    };
                    if let Err(e) = config::save(&config::config_path(), &current_cfg) {
                        log::warn!("Failed to persist config after SMChanged: {}", e);
                    }
                }
            }
            AppEvent::TrayImportBundle => {
                let (tx_pick, rx_pick) = crossbeam_channel::bounded(1);
                std::thread::spawn(move || {
                    let result = rfd::FileDialog::new()
                        .add_filter("Pet Bundle", &["petbundle"])
                        .pick_file();
                    tx_pick.send(result).ok();
                });
                self.pending_bundle_pick = Some(rx_pick);
            }
            AppEvent::BundleImported { sprite_id, sm_name } => {
                if let Some(sm_name) = sm_name {
                    // Reload gallery to pick up newly saved SM
                    let config_dir = config::config_path()
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    let gallery = crate::sprite::sm_gallery::SmGallery::load(&config_dir);
                    // Auto-assign to the first pet whose sheet_path contains sprite_id
                    if let Some(pet) = self.pets.values_mut()
                        .find(|p| p.cfg.sheet_path.contains(&sprite_id))
                        && let Some(sm) = gallery.get(&sm_name)
                    {
                        pet.runner.replace_sm(sm);
                        pet.cfg.state_machine = sm_name.clone();
                        log::info!("Bundle import: auto-assigned SM '{}' to pet '{}'", sm_name, pet.cfg.id);
                    }
                    // Persist updated config
                    let current_cfg = crate::config::schema::Config {
                        pets: self.pets.values().map(|p| p.cfg.clone()).collect(),
                    };
                    if let Err(e) = config::save(&config::config_path(), &current_cfg) {
                        log::warn!("Failed to persist config after bundle import: {}", e);
                    }
                }
                // Notify open windows to refresh SM lists
                self.notify_sm_collection_changed();
            }
            AppEvent::SMCollectionChanged => {
                self.notify_sm_collection_changed();
            }
        }
        Ok(())
    }

    fn notify_sm_collection_changed(&mut self) {
        log::debug!("SM collection changed — notifying UI");
        if self.app_window_lifecycle.open
            && let Ok(mut s) = self.app_window.try_lock() {
                s.sm_gallery_dirty = true;
            }
    }

    fn apply_config(&mut self, new_cfg: crate::config::schema::Config) -> Result<()> {
        let new_ids: std::collections::HashSet<_> =
            new_cfg.pets.iter().map(|p| p.id.clone()).collect();
        self.pets.retain(|id, _| new_ids.contains(id));
        for pet_cfg in new_cfg.pets {
            // Fast path: only SM changed → hot-swap, no window rebuild
            if let Some(pet) = self.pets.get_mut(&pet_cfg.id) {
                let old_cfg = &pet.cfg;
                // f32 equality is safe here: both values come from TOML-deserialized config,
                // never from arithmetic — we are detecting whether the user changed the field.
                if old_cfg.sheet_path == pet_cfg.sheet_path
                    && old_cfg.scale == pet_cfg.scale
                    && old_cfg.walk_speed == pet_cfg.walk_speed
                    && old_cfg.state_machine != pet_cfg.state_machine
                {
                    let config_dir = config::config_path()
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    let gallery = crate::sprite::sm_gallery::SmGallery::load(&config_dir);
                    let new_sm = resolve_sm(&pet_cfg.state_machine, &gallery);
                    pet.runner.replace_sm(new_sm);
                    pet.cfg = pet_cfg.clone();
                    log::info!("hot-swapped SM for pet '{}' → '{}'", pet_cfg.id, pet_cfg.state_machine);
                    continue; // skip full rebuild
                }
            }

            let needs_rebuild = self.pets.get(&pet_cfg.id)
                .map(|inst| inst.cfg != pet_cfg)
                .unwrap_or(true);
            if needs_rebuild {
                match build_pet(&pet_cfg) {
                    Ok(inst) => { self.pets.insert(pet_cfg.id.clone(), inst); }
                    Err(e) => log::warn!("reload: create pet '{}': {e}", pet_cfg.id),
                }
            }
        }
        Ok(())
    }
}

// ── Collision helpers ─────────────────────────────────────────────────────────

fn collect_collidables(pets: &std::collections::HashMap<String, PetInstance>) -> Vec<Collidable> {
    let mut items: Vec<Collidable> = pets.iter().map(|(id, pet)| {
        let frame_idx = pet.anim.absolute_frame(&pet.sheet);
        let flip = pet.compute_flip();
        let scale = pet.cfg.scale.round() as u32;
        let (dx, dy, w, h) = pet.sheet.tight_bbox(frame_idx, scale, flip);
        let left = pet.x + dx;
        let top = pet.y + dy;
        let (vx, vy) = pet.runner.speed();
        Collidable {
            id: id.clone(),
            left,
            right: left + w as i32,
            top,
            bottom: top + h as i32,
            center_y: top + h as i32 / 2,
            vx,
            vy,
        }
    }).collect();
    items.sort_by_key(|c| c.left);
    items
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Drain events from channel.
        let events: Vec<AppEvent> = std::iter::from_fn(|| self.rx.try_recv().ok()).collect();
        for ev in events {
            if let Err(e) = self.handle_event(ev, ctx) {
                log::warn!("event error: {e}");
            }
        }

        if self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Check for pending bundle file-picker result.
        let bundle_path = self.pending_bundle_pick
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
            .flatten();
        if bundle_path.is_some() {
            self.pending_bundle_pick = None;
        }
        if let Some(path) = bundle_path {
            self.import_bundle(&path);
        }

        // Compute delta_ms for this tick.
        let now = std::time::Instant::now();
        let delta_ms = now.duration_since(self.last_tick_ms).as_millis().min(200) as u32;
        self.last_tick_ms = now;

        // Compute shared per-frame environment variables.
        let mut cursor_pt = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut cursor_pt); }

        let hour = {
            let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
            unsafe { GetLocalTime(&mut st); }
            st.wHour as u32
        };

        let focused_app = {
            let hwnd = unsafe { GetForegroundWindow() };
            if hwnd.is_null() {
                String::new()
            } else {
                let mut buf = [0u16; 256];
                let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
                if len <= 0 { String::new() } else { String::from_utf16_lossy(&buf[..len as usize]) }
            }
        };

        let pet_count = self.pets.len() as u32;

        // Tick all pets.
        for pet in self.pets.values_mut() {
            if let Err(e) = pet.tick(delta_ms, &mut self.surface_cache) {
                log::warn!("pet tick error: {e}");
            }
        }

        // Update environment vars on each pet's SM runner.
        {
            let centers: Vec<(String, i32, i32)> = self.pets.iter()
                .map(|(id, p)| (id.clone(), p.x + p.window.width as i32 / 2, p.y + p.window.height as i32 / 2))
                .collect();

            for (id, pet) in &mut self.pets {
                let cx = pet.x + pet.window.width as i32 / 2;
                let cy = pet.y + pet.window.height as i32 / 2;

                let cursor_dist = {
                    let dx = cursor_pt.x - cx;
                    let dy = cursor_pt.y - cy;
                    ((dx * dx + dy * dy) as f32).sqrt()
                };

                let other_pet_dist = centers.iter()
                    .filter(|(oid, _, _)| oid != id)
                    .map(|(_, ox, oy)| {
                        let dx = ox - cx;
                        let dy = oy - cy;
                        ((dx * dx + dy * dy) as f32).sqrt()
                    })
                    .fold(f32::INFINITY, f32::min);
                let other_pet_dist = if other_pet_dist.is_infinite() { f32::MAX } else { other_pet_dist };

                pet.runner.update_env(EnvironmentSnapshot {
                    cursor_dist,
                    hour,
                    focused_app: focused_app.clone(),
                    pet_count,
                    other_pet_dist,
                    surface_w: pet.last_surface_hit.surface_w,
                    surface_label: pet.last_surface_hit.surface_label.clone(),
                });
            }
        }

        // ── Collision detection ──────────────────────────────────────────────────
        if self.pets.len() >= 2 {
            let collidables = collect_collidables(&self.pets);
            let new_overlapping = overlapping_pairs(&collidables);
            for pair in detect_new_collisions(&collidables, &self.overlapping) {
                if let Some(pet) = self.pets.get_mut(&pair.id_a) {
                    pet.runner.on_collide(pair.data_a);
                }
                if let Some(pet) = self.pets.get_mut(&pair.id_b) {
                    pet.runner.on_collide(pair.data_b);
                }
            }
            self.overlapping = new_overlapping;
        }

        // Show unified app window if open.
        {
            let mut saved_json_path: Option<std::path::PathBuf> = None;
            let mut sm_saved_name: Option<String> = None;
            let mut force_state: Option<String> = None;
            let mut release_force = false;
            let mut step_mode = false;
            let mut step_advance = false;

            if self.app_window_lifecycle.poll_close() {
                log::debug!(
                    "app_window: close detected for gen {}, sending ViewportCommand::Close",
                    self.app_window_lifecycle.generation
                );
                ctx.send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of(
                        format!("app_window_{}", self.app_window_lifecycle.generation),
                    ),
                    egui::ViewportCommand::Close,
                );
            }

            if self.app_window_lifecycle.open {
                // Push live pet state into SM editor
                if let Ok(mut s) = self.app_window.try_lock() {
                    if let Some(pet) = self.pets.values().next() {
                        s.sm.from_app.active_state = Some(pet.runner.current_state_name().to_string());
                        let cvars = pet.runner.last_condition_vars();
                        s.sm.from_app.var_snapshot = crate::tray::sm_editor::VarSnapshot {
                            cursor_dist: cvars.cursor_dist,
                            state_time_ms: cvars.state_time_ms,
                            on_surface: cvars.on_surface,
                            near_edge: false,
                            pet_x: cvars.pet_x,
                            pet_y: cvars.pet_y,
                            pet_vx: cvars.pet_vx,
                            pet_vy: cvars.pet_vy,
                            pet_v: cvars.pet_v,
                            hour: cvars.hour,
                            focused_app: cvars.focused_app.clone(),
                        };
                        s.sm.from_app.transition_log = pet.runner.transition_log().to_vec();
                        s.sm.from_app.is_forced = pet.runner.force_state.is_some();
                    }
                    // Push current config
                    s.dark_mode = self.dark_mode;

                    // Consume outputs
                    if let Some(new_dark) = s.dark_mode_out.take() {
                        self.dark_mode = new_dark;
                    }
                    saved_json_path = s.saved_json_path.take();
                    sm_saved_name = s.sm.from_ui.saved_sm_name.take();
                    force_state = s.sm.from_ui.force_state.take();
                    release_force = s.sm.from_ui.release_force;
                    step_mode = s.sm.from_ui.step_mode;
                    step_advance = s.sm.from_ui.step_advance;
                    if release_force { s.sm.from_ui.release_force = false; }
                    if step_advance { s.sm.from_ui.step_advance = false; }
                }

                open_app_window(
                    ctx,
                    self.app_window.clone(),
                    self.tx.clone(),
                    self.app_window_lifecycle.generation,
                    self.app_window_lifecycle.current_close_flag(),
                );
            }

            // Apply SM debug commands to pets
            if let Some(state_name) = force_state
                && let Some(pet) = self.pets.values_mut().next() {
                    pet.runner.force_state = Some(state_name);
                }
            if release_force
                && let Some(pet) = self.pets.values_mut().next() {
                    pet.runner.release_force = true;
                }
            if let Some(pet) = self.pets.values_mut().next() {
                pet.runner.step_mode = step_mode;
                if step_advance { pet.runner.step_advance = true; }
            }

            // Hot-reload pets for saved sprite
            if let Some(json_path) = saved_json_path {
                self.reload_pets_for_sheet(&json_path);
            }

            // Hot-reload pets for saved SM
            if let Some(sm_name) = sm_saved_name {
                let config_dir = config::config_path()
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let gallery = crate::sprite::sm_gallery::SmGallery::load(&config_dir);
                if let Some(sm) = gallery.get(&sm_name) {
                    for pet in self.pets.values_mut() {
                        if pet.runner.sm.name == sm_name {
                            pet.runner.sm = sm.clone();
                        }
                    }
                }
                let _ = self.tx.send(AppEvent::SMCollectionChanged);
            }
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn resolve_sm(
    name: &str,
    gallery: &crate::sprite::sm_gallery::SmGallery,
) -> std::sync::Arc<crate::sprite::sm_compiler::CompiledSM> {
    if name == "embedded://default" || name.is_empty() {
        crate::sprite::sm_runner::load_default_sm()
    } else {
        match gallery.get(name) {
            Some(sm) => sm,
            None => {
                log::warn!("SM '{}' not found in gallery, falling back to default", name);
                crate::sprite::sm_runner::load_default_sm()
            }
        }
    }
}

fn sanitize_id(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' })
        .collect()
}

fn build_pet(cfg: &PetConfig) -> Result<PetInstance> {
    let sheet = load_sheet(&cfg.sheet_path)?;
    PetInstance::new(cfg.clone(), sheet)
}

fn load_sheet(path: &str) -> Result<SpriteSheet> {
    if let Some(stem) = path.strip_prefix("embedded://") {
        let (json, png) = assets::embedded_sheet(stem)
            .with_context(|| format!("embedded sheet '{stem}' not found"))?;
        let mut sheet = sheet::load_embedded(&json, &png)?;
        apply_chromakey(&mut sheet.image, &sheet.chromakey);
        return Ok(sheet);
    }
    let json = std::fs::read(path).with_context(|| format!("read {path}"))?;
    let json_path = std::path::Path::new(path);
    let png_path = json_path.with_extension("png");
    let png = std::fs::read(&png_path)
        .with_context(|| format!("read {}", png_path.display()))?;
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .context("decode PNG")?
        .into_rgba8();
    let mut sheet = sheet::SpriteSheet::from_json_and_image(&json, image)?;
    apply_chromakey(&mut sheet.image, &sheet.chromakey);
    Ok(sheet)
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// `resolve_sm` with an unknown name falls back to the embedded default SM.
    #[test]
    fn resolve_sm_unknown_name_returns_default() {
        let dir = tempdir().unwrap();
        let gallery = crate::sprite::sm_gallery::SmGallery::load(dir.path());

        // Ask for a name that doesn't exist in the empty gallery.
        let sm = resolve_sm("nonexistent-sm", &gallery);

        // The embedded default SM is named "Default Pet" (from assets/default.petstate).
        let default_sm = crate::sprite::sm_runner::load_default_sm();
        assert_eq!(
            sm.name, default_sm.name,
            "resolve_sm should fall back to default SM for unknown names; got '{}'",
            sm.name
        );
    }

    /// `resolve_sm` with the sentinel "embedded://default" also returns the default SM.
    #[test]
    fn resolve_sm_embedded_sentinel_returns_default() {
        let dir = tempdir().unwrap();
        let gallery = crate::sprite::sm_gallery::SmGallery::load(dir.path());

        let sm = resolve_sm("embedded://default", &gallery);
        let default_sm = crate::sprite::sm_runner::load_default_sm();
        assert_eq!(sm.name, default_sm.name);
    }

    /// `resolve_sm` with an empty string also returns the default SM.
    #[test]
    fn resolve_sm_empty_string_returns_default() {
        let dir = tempdir().unwrap();
        let gallery = crate::sprite::sm_gallery::SmGallery::load(dir.path());

        let sm = resolve_sm("", &gallery);
        let default_sm = crate::sprite::sm_runner::load_default_sm();
        assert_eq!(sm.name, default_sm.name);
    }
}
