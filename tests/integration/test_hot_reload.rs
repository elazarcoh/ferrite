use crossbeam_channel::bounded;
use my_pet::{
    config::{save, schema::Config, watcher::spawn_watcher},
    event::AppEvent,
};
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn config_change_triggers_reload() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");

    // Write initial config.
    let cfg = Config::default();
    save(&path, &cfg).expect("save initial");

    let (tx, rx) = bounded::<AppEvent>(16);
    let _watcher = spawn_watcher(path.clone(), tx).expect("spawn watcher");

    // Modify the config.
    let mut updated = cfg.clone();
    updated.pets[0].x = 999;
    // Small delay to ensure watcher is ready before we write.
    std::thread::sleep(Duration::from_millis(100));
    save(&path, &updated).expect("save updated");

    // Wait up to 2 s for the ConfigReloaded event.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(AppEvent::ConfigReloaded(new_cfg)) => {
                assert_eq!(new_cfg.pets[0].x, 999);
                return;
            }
            Ok(_) => continue,
            Err(_) => {
                if std::time::Instant::now() > deadline {
                    panic!("ConfigReloaded not received within 2 s");
                }
            }
        }
    }
}
