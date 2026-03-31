use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Lifecycle state machine for the unified app window.
///
/// Tracks whether the window is open and manages a **per-generation** close flag
/// so that stale deferred-viewport closures from a previous generation can never
/// poison the state of a newly opened window.
pub struct AppWindowLifecycle {
    pub open: bool,
    pub generation: u64,
    /// Replaced with a fresh `Arc` each time the window opens.
    close_flag: Arc<AtomicBool>,
}

impl AppWindowLifecycle {
    pub fn new() -> Self {
        Self {
            open: false,
            generation: 0,
            close_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Open the window for a new generation.
    ///
    /// Returns a **fresh** `Arc<AtomicBool>` that belongs exclusively to this
    /// generation.  Pass it (cloned) into the deferred-viewport closure.  Any
    /// `Arc` clones held by older closures are from a previous generation and
    /// cannot affect this window.
    pub fn open(&mut self) -> Arc<AtomicBool> {
        self.close_flag = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        self.open = true;
        self.close_flag.clone()
    }

    /// Check once per frame whether the current generation has been asked to
    /// close.  Returns `true` exactly once — the frame the transition fires —
    /// then the window is marked closed.
    pub fn poll_close(&mut self) -> bool {
        if self.open && self.close_flag.load(Ordering::Relaxed) {
            self.open = false;
            true
        } else {
            false
        }
    }

    /// Clone of the close flag for the current generation.  Used to pass a
    /// second clone into `open_app_window` each frame while the window is open.
    pub fn current_close_flag(&self) -> Arc<AtomicBool> {
        self.close_flag.clone()
    }
}

impl Default for AppWindowLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_starts_window() {
        let mut lc = AppWindowLifecycle::new();
        assert!(!lc.open);
        lc.open();
        assert!(lc.open);
    }

    #[test]
    fn close_flag_closes_window() {
        let mut lc = AppWindowLifecycle::new();
        let flag = lc.open();
        assert!(!lc.poll_close());
        flag.store(true, Ordering::Relaxed);
        assert!(lc.poll_close());
        assert!(!lc.open);
    }

    #[test]
    fn poll_close_returns_true_only_once() {
        let mut lc = AppWindowLifecycle::new();
        let flag = lc.open();
        flag.store(true, Ordering::Relaxed);
        assert!(lc.poll_close());
        assert!(!lc.poll_close()); // second call: already closed
    }

    /// Core regression test: a stale viewport closure firing close_requested
    /// AFTER the user has already reopened a new window must NOT kill the new
    /// window.  This is the race that caused all three reported failure modes.
    #[test]
    fn reopen_is_not_killed_by_stale_viewport() {
        let mut lc = AppWindowLifecycle::new();

        // Gen 1: open, then close
        let flag1 = lc.open();
        assert_eq!(lc.generation, 1);
        flag1.store(true, Ordering::Relaxed);
        assert!(lc.poll_close());
        assert!(!lc.open);

        // User reopens → gen 2 with a fresh flag
        let _flag2 = lc.open();
        assert_eq!(lc.generation, 2);
        assert!(lc.open);

        // *** THE RACE ***: old gen-1 viewport fires close_requested again
        // (egui deferred viewports may call their closure one extra frame after
        //  the main loop stops calling show_viewport_deferred for that ID)
        flag1.store(true, Ordering::Relaxed);

        // poll_close reads the GEN-2 flag, which was NOT set → must stay open
        assert!(
            !lc.poll_close(),
            "stale gen-1 close must not kill gen-2 window"
        );
        assert!(lc.open, "window must still be open after stale close signal");
    }

    /// Concrete property: open() always issues a fresh Arc, so gen-N and gen-N+1
    /// close flags are structurally independent (different heap objects).
    #[test]
    fn each_generation_has_independent_close_flag() {
        let mut lc = AppWindowLifecycle::new();
        let flag1 = lc.open();
        let flag2 = lc.open();
        // They must be distinct Arcs (different allocations)
        assert!(
            !Arc::ptr_eq(&flag1, &flag2),
            "each generation must own a distinct Arc<AtomicBool>"
        );
    }

    /// open() must not touch any external mutex — it only swaps an Arc.
    /// This proves the event-loop thread cannot deadlock or block when opening.
    #[test]
    fn open_does_not_block_under_contended_external_mutex() {
        use std::sync::Mutex;
        use std::time::{Duration, Instant};

        // Simulate the AppWindowState mutex held by the viewport render thread.
        let state_mutex = Arc::new(Mutex::new(()));
        let state_clone = state_mutex.clone();

        // Viewport thread: hold lock for 100 ms
        let _viewport_thread = std::thread::spawn(move || {
            let _guard = state_clone.lock().unwrap();
            std::thread::sleep(Duration::from_millis(100));
        });

        // Give viewport thread time to acquire the lock
        std::thread::sleep(Duration::from_millis(10));

        // Main-loop open: must return immediately regardless of the external mutex
        let mut lc = AppWindowLifecycle::new();
        let t0 = Instant::now();
        lc.open();
        let elapsed = t0.elapsed();

        assert!(
            elapsed < Duration::from_millis(20),
            "open() must not block on AppWindowState mutex (took {elapsed:?})"
        );
    }
}
