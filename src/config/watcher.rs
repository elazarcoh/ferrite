use crate::event::AppEvent;
use anyhow::Result;
use crossbeam_channel::Sender;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;

/// Spawn a background file-watcher on `path`.
///
/// When the file changes, loads the new config and sends `AppEvent::ConfigReloaded`.
/// The returned `RecommendedWatcher` must be kept alive for as long as watching is needed.
pub fn spawn_watcher(path: PathBuf, tx: Sender<AppEvent>) -> Result<RecommendedWatcher> {
    let watch_path = path.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_)
                ) {
                    match crate::config::load(&path) {
                        Ok(cfg) => {
                            let _ = tx.send(AppEvent::ConfigReloaded(cfg));
                        }
                        Err(e) => {
                            log::warn!("config reload failed: {e}");
                        }
                    }
                }
            }
        },
        NotifyConfig::default(),
    )?;

    if watch_path.exists() {
        watcher.watch(&watch_path, RecursiveMode::NonRecursive)?;
    } else if let Some(dir) = watch_path.parent() {
        // Watch the directory so we detect file creation.
        if dir.exists() {
            watcher.watch(dir, RecursiveMode::NonRecursive)?;
        }
    }

    Ok(watcher)
}
