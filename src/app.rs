use crate::{
    assets,
    config::{self, schema::PetConfig, watcher::spawn_watcher},
    event::AppEvent,
    sprite::{
        animation::AnimationState,
        behavior::{BehaviorAi, BehaviorState, Facing},
        sheet::{self, SpriteSheet},
    },
    tray::SystemTray,
    window::pet_window::PetWindow,
};
use anyhow::{Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::collections::HashMap;
use windows_sys::Win32::{
    Foundation::RECT,
    UI::WindowsAndMessaging::*,
};

const TIMER_MS: u32 = 16;

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

        let fall_tag = cfg.tag_map.fall.as_deref().unwrap_or(&cfg.tag_map.idle).to_string();
        let anim = AnimationState::new(fall_tag);
        let mut ai = BehaviorAi::new();
        ai.state = BehaviorState::Fall { vy: 0.0 };

        let mut inst = PetInstance { x: cfg.x, y: spawn_y, cfg, sheet, window, anim, ai, elevated_ms: 0 };

        inst.render_current_frame()?;

        log::info!("pet '{}' created — falling from y={spawn_y}", inst.cfg.id);
        Ok(inst)
    }

    pub fn tick(&mut self, delta_ms: u32) -> Result<()> {
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
            self.x, self.y, pet_w, pet_h, screen_w, screen_h,
        );

        let tag = self.ai.tick(
            delta_ms,
            &mut self.x,
            &mut self.y,
            screen_w,
            pet_w,
            pet_h,
            self.cfg.walk_speed,
            floor_y,
            &self.cfg.tag_map,
        );
        self.anim.set_tag(tag);

        // After the AI has potentially moved x (Walk), recompute floor at
        // the new position and apply surface snapping / edge-fall logic.
        if !being_dragged && !matches!(
            self.ai.state,
            BehaviorState::Fall { .. } | BehaviorState::Thrown { .. } | BehaviorState::Grabbed { .. }
        ) {
            let new_floor = crate::window::surfaces::find_floor(
                self.x, self.y, pet_w, pet_h, screen_w, screen_h,
            );
            // If the floor dropped more than one pet height, the pet walked
            // off a window edge — start falling.
            if new_floor > self.y + pet_h {
                self.ai.state = BehaviorState::Fall { vy: 0.0 };
                self.ai.reset_idle();
            } else {
                // Snap to surface (handles small steps up/down between windows).
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
        if frame_changed {
            self.render_current_frame()?;
        }
        Ok(())
    }

    fn render_current_frame(&mut self) -> Result<()> {
        let abs = self.anim.absolute_frame(&self.sheet);
        let f = &self.sheet.frames[abs];
        let flip = self.cfg.flip_walk_left
            && matches!(
                self.ai.state,
                BehaviorState::Walk { ref facing, .. } | BehaviorState::Run { ref facing, .. }
                    if *facing == Facing::Left
            );
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
    /// Actual timer ID returned by SetTimer (may differ from what we passed).
    timer_id: usize,
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
            timer_id: 0, // set in run()
        })
    }

    pub fn run(&mut self) -> Result<()> {
        // RC-fix: capture the actual timer ID returned by SetTimer.
        // With hWnd=NULL Windows may assign a different ID than what we pass.
        self.timer_id = unsafe {
            SetTimer(std::ptr::null_mut(), 1, TIMER_MS, None) as usize
        };
        log::debug!("timer_id = {}", self.timer_id);

        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            while let Ok(ev) = self.rx.try_recv() {
                if self.handle_event(ev)? {
                    return Ok(());
                }
            }

            unsafe {
                if PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_QUIT {
                        return Ok(());
                    }
                    // RC-fix: compare against the captured timer_id, not a
                    // hardcoded constant.
                    if msg.message == WM_TIMER && msg.wParam == self.timer_id {
                        let now = std::time::Instant::now();
                        let delta = now.duration_since(self.last_tick_ms);
                        self.last_tick_ms = now;
                        let delta_ms = delta.as_millis().min(200) as u32;
                        for pet in self.pets.values_mut() {
                            if let Err(e) = pet.tick(delta_ms) {
                                log::warn!("pet tick error: {e}");
                            }
                        }
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        }
    }

    fn handle_event(&mut self, ev: AppEvent) -> Result<bool> {
        match ev {
            AppEvent::Quit | AppEvent::TrayQuit => {
                unsafe { PostQuitMessage(0) };
                return Ok(true);
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
                let current = config::load(&config::config_path()).unwrap_or_default();
                crate::tray::config_window::show_config_dialog(
                    std::ptr::null_mut(),
                    &current,
                    self.tx.clone(),
                );
            }
            AppEvent::ConfigReloaded(new_cfg) | AppEvent::ConfigChanged(new_cfg) => {
                self.apply_config(new_cfg)?;
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
            AppEvent::Tick(_) => {}
        }
        Ok(false)
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
