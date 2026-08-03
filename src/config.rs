use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Result;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::{fs::File, io::BufReader};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreSettings {
    pub allow_duplicates: bool,
    pub max_history: usize,
    pub max_entry_bytes: usize,
}

impl Default for StoreSettings {
    fn default() -> Self {
        Self {
            allow_duplicates: false,
            max_history: 250,
            max_entry_bytes: 1_000_000,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub storage: StoreSettings,
}

pub fn create_default_config_json() -> Result<String> {
    let text = AppConfig::default();
    let json = serde_json::to_string_pretty(&text)?;

    Ok(json)
}

pub fn find_or_create_config_path() -> anyhow::Result<PathBuf> {
    let mut state_path = dirs::config_dir().context("error finding config directory")?;

    state_path.push("clipd");

    fs::create_dir_all(&state_path).context("failed to create clipboard config")?;

    if let Err(err) = fs::set_permissions(&state_path, fs::Permissions::from_mode(0o700)) {
        eprintln!(
            "failed to set 0700 permissions on config directory: {}",
            err
        );
    }

    state_path.push("config.json");

    Ok(state_path)
}

pub fn load_or_create_config() -> anyhow::Result<AppConfig> {
    let config_path = find_or_create_config_path()?;

    if !config_path.exists() {
        let json = create_default_config_json()?;
        fs::write(&config_path, json).context("failed to write default json to config")?;
    }

    let config_file = File::open(config_path)?;
    let reader = BufReader::new(config_file);

    let app_config: AppConfig = serde_json::from_reader(reader)?;

    Ok(app_config)
}
