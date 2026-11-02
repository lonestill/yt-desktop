use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_locale() -> String {
    "auto".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub first_run_completed: bool,
    #[serde(default = "default_locale")]
    pub app_locale: String,

    // DPI Settings
    pub selected_strategy: String, // "auto" or preset id or "custom"
    pub active_strategy: String,
    pub custom_tpws_args: String,
    pub last_latency_ms: Option<u64>,

    // Content & Extension Settings
    pub block_ads: bool,
    pub sponsor_block: bool,
    pub return_dislikes: bool,
    pub block_shorts: bool,
    pub auto_confirm_watching: bool,
    pub enable_pip: bool,
    pub oled_black: bool,
    pub max_quality: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            first_run_completed: false,
            app_locale: "auto".to_string(),
            selected_strategy: "auto".to_string(),
            active_strategy: "rostelecom".to_string(),
            custom_tpws_args: "".to_string(),
            last_latency_ms: None,
            block_ads: true,
            sponsor_block: true,
            return_dislikes: true,
            block_shorts: true,
            auto_confirm_watching: true,
            enable_pip: true,
            oled_black: false,
            max_quality: true,
        }
    }
}

pub fn get_config_dir() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("yt-desktop");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn get_config_path() -> PathBuf {
    get_config_dir().join("config.json")
}

pub fn load_config() -> (AppConfig, bool) {
    let path = get_config_path();
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str(&data) {
            return (cfg, false);
        }
    }
    let default_cfg = AppConfig::default();
    save_config(&default_cfg);
    (default_cfg, true)
}

pub fn save_config(config: &AppConfig) {
    let path = get_config_path();
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, json);
    }
}
