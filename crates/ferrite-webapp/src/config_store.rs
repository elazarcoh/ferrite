use ferrite_core::config::schema::Config;

const STORAGE_KEY: &str = "ferrite_config";

pub fn load() -> Config {
    let window = web_sys::window().expect("no window");
    let storage = window.local_storage().ok().flatten().expect("no localStorage");
    let toml_str = match storage.get_item(STORAGE_KEY) {
        Ok(Some(s)) => s,
        _ => return Config::default(),
    };
    toml::from_str(&toml_str).unwrap_or_default()
}

pub fn save(config: &Config) {
    let Ok(toml_str) = toml::to_string_pretty(config) else { return };
    let window = web_sys::window().expect("no window");
    let storage = window.local_storage().ok().flatten().expect("no localStorage");
    storage.set_item(STORAGE_KEY, &toml_str).ok();
}
