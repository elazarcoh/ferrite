use crate::{
    assets,
    config::{self, schema::PetConfig, watcher::spawn_watcher},
    event::AppEvent,
    sprite::{
        animation::AnimationState,
        behavior::{BehaviorAi, BehaviorState, Facing},
        sheet::{self, SpriteSheet},
    },
    tray::{
        config_window::{open_config_viewport, ConfigWindowState},
        sprite_editor::{open_sprite_editor_viewport, SpriteEditorViewport},
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
    Foundation::RECT,
    UI::WindowsAndMessaging::*,
};

// ─── Per-pet runtime state ────────────────────────────────────────────────────

/// Complete runtime state for one pet instance.
pub struct PetInstance {
    pub cfg: PetConfig,
    pub sheet: SpriteSheet,
    pub window: PetWindow,
    pub anim: AnimationState,
    pub ai: BehaviorAi,
    pub x: i32,
    pub y: i32,
    /// Milliseconds the pet has spent on an elevated surface (above virtual ground).
    /// Reset when grounded; forces Fall when it exceeds the drop threshold.
    elevated_ms: u32,
    /// Flip state from the last rendered frame. Re-render when this changes so
    /// direction changes take effect immediately, independent of frame timing.
    last_flip: bool,
}

impl PetInstance {
    pub fn new(cfg: PetConfig, sheet: SpriteSheet) -> Result<Self> {
        let dw = sheet.frames.first().map(|f| f.w).unwrap_or(32) * cfg.scale;
        let dh = sheet.frames.first().map(|f| f.h).unwrap_or(32) * cfg.scale;

        // Spawn above the top of the screen so the pet falls into view.
        let spawn_y = -(dh as i32);
        let window = PetWindow::create(cfg.x, spawn_y, dw, dh)?;

        // Register this window for per-pixel hit testing.
        crate::window::wndproc::register_hwnd(window.hwnd, cfg.id.clone());

        // TODO(Task-13): derive fall_tag from SM runner instead of tag_map
        let fall_tag = "fall".to_string();
        let anim = AnimationState::new(fall_tag);
        let mut ai = BehaviorAi::new();
        ai.state = BehaviorState::Fall { vy: 0.0 };

        let mut inst = PetInstance { x: cfg.x, y: spawn_y, cfg, sheet, window, anim, ai, elevated_ms: 0, last_flip: false };

        inst.render_current_frame()?;

        log::info!("pet '{}' created — falling from y={spawn_y}", inst.cfg.id);
        Ok(inst)
    }

    pub fn tick(&mut self, delta_ms: u32, cache: &mut crate::window::surfaces::SurfaceCache) -> Result<()> {
        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let pet_w = self.window.width as i32;
        let pet_h = self.window.height as i32;

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
        let floor_y = crate::window::surfaces::find_floor(
            self.x, self.y, pet_w, pet_h, screen_w, screen_h, cache,
        );

        // TODO(Task-13): restore runner.tick() — replace BehaviorAi with SMRunner
        let _tag_unused = self.ai.tick(
            delta_ms,
            &mut self.x,
            &mut self.y,
            screen_w,
            pet_w,
            pet_h,
            self.cfg.walk_speed,
            floor_y,
            &crate::sprite::behavior::AnimTagMap::default(),
        );
        let tag = "idle"; // TODO(Task-13): get tag from SMRunner
        self.anim.set_tag(tag.to_string());

        // After the AI has potentially moved x (Walk), recompute floor at
        // the new position and apply surface snapping / edge-fall logic.
        if !being_dragged && !matches!(
            self.ai.state,
            BehaviorState::Fall { .. } | BehaviorState::Thrown { .. } | BehaviorState::Grabbed { .. }
        ) {
            let new_floor = crate::window::surfaces::find_floor(
                self.x, self.y, pet_w, pet_h, screen_w, screen_h, cache,
            );
            // If the floor dropped more than one pet height, the pet walked
            // off a window edge — start falling.
            if new_floor > self.y + pet_h {
                self.ai.state = BehaviorState::Fall { vy: 0.0 };
                self.ai.reset_idle();
            } else {
                // Snap to surface (handles small steps up/down between windows)
                self.y = new_floor;
            }
        }

        // Elevated-surface drop: if the pet has been sitting on a raised window
        // for too long, make it fall off (eSheep-style edge drop).
        const ELEVATED_DROP_MS: u32 = 20_000; // 20 s before dropping
        let virtual_ground = screen_h - 4 - pet_h;
        let is_airborne = matches!(
            self.ai.state,
            BehaviorState::Fall { .. } | BehaviorState::Thrown { .. } | BehaviorState::Grabbed { .. }
        );
        if is_airborne || self.y >= virtual_ground - 4 {
            // On ground or in the air — reset timer.
            self.elevated_ms = 0;
        } else {
            self.elevated_ms = self.elevated_ms.saturating_add(delta_ms);
            if self.elevated_ms >= ELEVATED_DROP_MS {
                log::debug!("elevated_drop after {}ms at y={}", self.elevated_ms, self.y);
                self.ai.state = BehaviorState::Fall { vy: 0.0 };
                self.ai.reset_idle();
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
        let tag_flip_h = self.sheet.tag(&self.anim.current_tag).map_or(false, |t| t.flip_h);
        let facing = match &self.ai.state {
            BehaviorState::Walk { facing, .. } | BehaviorState::Run { facing, .. } => facing,
            _ => return false,
        };
        match facing {
            Facing::Right => tag_flip_h,   // facing right, going right  → only flip if sprite faces left
            Facing::Left  => !tag_flip_h,  // facing right, going left   → flip unless sprite already faces left
        }
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
    pub fn window_frame_buf_is_empty(&self) -> bool {
        self.window.frame_buf.is_empty()
    }

    /// Returns a reference to the window's premultiplied BGRA buffer.
    pub fn window_frame_buf(&self) -> &[u8] {
        &self.window.frame_buf
    }

    /// Returns the rendered window width in pixels (after scale).
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
    config_window_state: Option<Arc<Mutex<ConfigWindowState>>>,
    sprite_editor_state: Option<Arc<Mutex<SpriteEditorViewport>>>,
    should_quit: bool,
    surface_cache: crate::window::surfaces::SurfaceCache,
    dark_mode: bool,
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
        let watcher = spawn_watcher(cfg_path, tx.clone()).context("create watcher")?;

        Ok(App {
            tx,
            rx,
            pets,
            _tray: tray,
            _watcher: watcher,
            last_tick_ms: std::time::Instant::now(),
            config_window_state: None,
            sprite_editor_state: None,
            should_quit: false,
            surface_cache: crate::window::surfaces::SurfaceCache::default(),
            dark_mode: true,
        })
    }

    /// Load a sprite sheet from a path string. Public for use by config window.
    pub fn load_sheet_for_config(path: &str) -> Result<SpriteSheet> {
        load_sheet(path)
    }

    /// Load a SpriteEditorState from an existing sheet JSON path.
    fn load_editor_state_from_sheet(sheet_path: &str) -> Result<crate::sprite::editor_state::SpriteEditorState> {
        use crate::sprite::editor_state::{EditorTag, SpriteEditorState};
        let sheet = load_sheet(sheet_path)?;
        let json_path = std::path::Path::new(sheet_path);
        let png_path = json_path.with_extension("png");

        // Infer grid from first frame size
        let (cols, rows) = if let Some(f) = sheet.frames.first() {
            if f.w > 0 && f.h > 0 {
                if sheet.image.width() % f.w != 0 || sheet.image.height() % f.h != 0 {
                    log::warn!("non-uniform sheet '{sheet_path}': image {}x{} not divisible by frame {}x{} — grid may be wrong",
                        sheet.image.width(), sheet.image.height(), f.w, f.h);
                }
                (sheet.image.width() / f.w, sheet.image.height() / f.h)
            } else {
                (1, 1)
            }
        } else {
            (1, 1)
        };

        let tags: Vec<EditorTag> = sheet.tags.iter().enumerate().map(|(i, t)| {
            EditorTag {
                name: t.name.clone(),
                from: t.from,
                to: t.to,
                direction: t.direction.clone(),
                flip_h: t.flip_h,
                color: SpriteEditorState::assign_color(i),
            }
        }).collect();

        let tag_map = crate::sprite::behavior::AnimTagMap::default();

        let mut state = SpriteEditorState::new(png_path, sheet.image);
        state.rows = rows;
        state.cols = cols;
        state.tags = tags;
        state.tag_map = tag_map;
        Ok(state)
    }

    /// Create a SpriteEditorState from a raw PNG file (new sprite sheet).
    fn new_editor_state_from_png(png_path: &std::path::Path) -> Result<crate::sprite::editor_state::SpriteEditorState> {
        let png = std::fs::read(png_path).context("read PNG")?;
        let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .context("decode PNG")?
            .into_rgba8();
        Ok(crate::sprite::editor_state::SpriteEditorState::new(png_path.to_path_buf(), image))
    }

    /// Reload the sheet for any live pet whose `sheet_path` resolves to `json_path`.
    fn reload_pets_for_sheet(&mut self, json_path: &std::path::Path) {
        for inst in self.pets.values_mut() {
            let pet_json_path = match inst.cfg.sheet_path.strip_prefix("embedded://") {
                Some(_) => None, // embedded sheets can't be reloaded from a file path
                None => Some(std::path::Path::new(&inst.cfg.sheet_path)),
            };
            let matches = pet_json_path.map_or(false, |p| {
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
            AppEvent::TrayOpenConfig => {
                if let Some(ref state) = self.config_window_state {
                    let _ = state; // already open — focus handled via viewport command
                    ctx.send_viewport_cmd_to(
                        egui::ViewportId::from_hash_of("config_window"),
                        egui::ViewportCommand::Focus,
                    );
                } else {
                    let current = config::load(&config::config_path()).unwrap_or_default();
                    let state = Arc::new(Mutex::new(ConfigWindowState::new(current, self.tx.clone())));
                    self.config_window_state = Some(state);
                }
            }
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
                    if matches!(p.ai.state, BehaviorState::Sleep) {
                        p.ai.wake();
                    } else {
                        p.ai.pet();
                    }
                }
            }
            AppEvent::PetDragStart { pet_id, cursor_x, cursor_y } => {
                log::debug!("PetDragStart pet_id={pet_id} cursor=({cursor_x},{cursor_y})");
                if let Some(p) = self.pets.get_mut(&pet_id) {
                    p.ai.grab((cursor_x - p.x, cursor_y - p.y));
                }
            }
            AppEvent::PetDragEnd { pet_id, velocity } => {
                log::debug!("PetDragEnd pet_id={pet_id} vel=({:.0},{:.0})", velocity.0, velocity.1);
                if let Some(p) = self.pets.get_mut(&pet_id) {
                    p.ai.release(velocity);
                }
            }
        }
        Ok(())
    }

    fn apply_config(&mut self, new_cfg: crate::config::schema::Config) -> Result<()> {
        let new_ids: std::collections::HashSet<_> =
            new_cfg.pets.iter().map(|p| p.id.clone()).collect();
        self.pets.retain(|id, _| new_ids.contains(id));
        for pet_cfg in new_cfg.pets {
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

        // Compute delta_ms for this tick.
        let now = std::time::Instant::now();
        let delta_ms = now.duration_since(self.last_tick_ms).as_millis().min(200) as u32;
        self.last_tick_ms = now;

        // Tick all pets.
        for pet in self.pets.values_mut() {
            if let Err(e) = pet.tick(delta_ms, &mut self.surface_cache) {
                log::warn!("pet tick error: {e}");
            }
        }

        // Show config viewport if open.
        {
            let mut config_should_close = false;
            if let Some(ref state) = self.config_window_state {
                // Push dark_mode into viewport state before rendering.
                if let Ok(mut s) = state.lock() {
                    s.dark_mode = self.dark_mode;
                }
                open_config_viewport(ctx, state.clone());
                // Read dark_mode_out back.
                if let Ok(mut s) = state.lock() {
                    if let Some(new_dark) = s.dark_mode_out.take() {
                        self.dark_mode = new_dark;
                    }
                    config_should_close = s.should_close;
                }
            }
            if config_should_close {
                self.config_window_state = None;
            }
        }

        // Handle editor open requests from the config window.
        if let Some(ref state) = self.config_window_state {
            let req = state.lock().ok().and_then(|mut s| s.open_editor_request.take());
            if let Some(req) = req {
                if self.sprite_editor_state.is_none() {
                    let editor_state = match req {
                        crate::tray::config_window::OpenEditorRequest::Edit(sheet_path) => {
                            Self::load_editor_state_from_sheet(&sheet_path).ok()
                        }
                        crate::tray::config_window::OpenEditorRequest::New(png_path) => {
                            Self::new_editor_state_from_png(&png_path).ok()
                        }
                    };
                    if let Some(es) = editor_state {
                        let viewport = SpriteEditorViewport::new(es);
                        self.sprite_editor_state =
                            Some(Arc::new(Mutex::new(viewport)));
                    }
                }
            }
        }

        // Show sprite editor viewport if open.
        // Opened via ConfigWindowState::open_editor_request (Edit or New from PNG).
        {
            let mut editor_should_close = false;
            let mut saved_json_path: Option<std::path::PathBuf> = None;
            if let Some(ref state) = self.sprite_editor_state {
                // Push dark_mode into viewport state before rendering.
                if let Ok(mut s) = state.lock() {
                    s.dark_mode = self.dark_mode;
                }
                open_sprite_editor_viewport(ctx, state.clone());
                // Read dark_mode_out and saved_json_path back.
                if let Ok(mut s) = state.lock() {
                    if let Some(new_dark) = s.dark_mode_out.take() {
                        self.dark_mode = new_dark;
                    }
                    editor_should_close = s.should_close;
                    saved_json_path = s.saved_json_path.take();
                }
            }
            // Reload sheets outside the sprite_editor_state borrow so &mut self is free.
            if let Some(json_path) = saved_json_path {
                self.reload_pets_for_sheet(&json_path);
            }
            if editor_should_close {
                self.sprite_editor_state = None;
            }
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn build_pet(cfg: &PetConfig) -> Result<PetInstance> {
    let sheet = load_sheet(&cfg.sheet_path)?;
    PetInstance::new(cfg.clone(), sheet)
}

fn load_sheet(path: &str) -> Result<SpriteSheet> {
    if let Some(stem) = path.strip_prefix("embedded://") {
        let (json, png) = assets::embedded_sheet(stem)
            .with_context(|| format!("embedded sheet '{stem}' not found"))?;
        return sheet::load_embedded(&json, &png);
    }
    let json = std::fs::read(path).with_context(|| format!("read {path}"))?;
    let json_path = std::path::Path::new(path);
    let png_path = json_path.with_extension("png");
    let png = std::fs::read(&png_path)
        .with_context(|| format!("read {}", png_path.display()))?;
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .context("decode PNG")?
        .into_rgba8();
    sheet::SpriteSheet::from_json_and_image(&json, image)
}
