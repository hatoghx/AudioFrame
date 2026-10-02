

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn app_data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("AudioFrame")
}

fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapUnit {
    Off,
    #[default]
    Frame,
    Second,
    Marker,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default = "def_lang")]
    pub lang: String,
    #[serde(default)]
    pub preview_offset_ms: i32,
    #[serde(default)]
    pub main_volume_db: f32,
    
    #[serde(default)]
    pub mmd_wav_output: bool,
    #[serde(default = "def_win")]
    pub window_size: [f32; 2],
    #[serde(default)]
    pub snap_unit: SnapUnit,
    #[serde(default)]
    pub subframe_movement: bool,
    
    #[serde(default)]
    pub preview_time_mode: bool,
    #[serde(default = "def_true")]
    pub ae_jsx: bool,
}

fn def_true() -> bool {
    true
}

fn def_lang() -> String {
    "ja".into()
}
fn def_win() -> [f32; 2] {
    [800.0, 400.0]
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            always_on_top: false,
            lang: def_lang(),
            preview_offset_ms: 0,
            main_volume_db: 0.0,
            mmd_wav_output: false,
            window_size: def_win(),
            snap_unit: SnapUnit::default(),
            subframe_movement: false,
            preview_time_mode: false,
            ae_jsx: true,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        match std::fs::read_to_string(settings_path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("settings.json 解釈失敗: {e}。既定値を使います");
                AppConfig::default()
            }),
            Err(_) => AppConfig::default(),
        }
    }

    pub fn save(&self) {
        let dir = app_data_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("設定フォルダ作成失敗: {e}");
            return;
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(settings_path(), json) {
                    log::warn!("settings.json 保存失敗: {e}");
                }
            }
            Err(e) => log::warn!("settings 直列化失敗: {e}"),
        }
    }
}
