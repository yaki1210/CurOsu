//! 设置：serde 结构体 + %APPDATA%\OsuCursorRs\settings.json 读写。
//! 字段与原 C# 版一一对应。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MIN_CURSOR_WIDTH: f64 = 16.0;
pub const MAX_CURSOR_WIDTH: f64 = 64.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub cursor_width: f64,
    pub auto_start: bool,
    pub tap_sound_enabled: bool,
    pub tap_sound_volume: f64,
    pub hover_sound_enabled: bool,
    pub hover_sound_volume: f64,
    pub hover_sound_as_resize_prompt: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cursor_width: 30.0,
            auto_start: false,
            tap_sound_enabled: true,
            tap_sound_volume: 1.0,
            hover_sound_enabled: true,
            hover_sound_volume: 1.0,
            hover_sound_as_resize_prompt: false,
        }
    }
}

/// 设置文件路径。
pub fn settings_path() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    let mut p = PathBuf::from(base);
    p.push("OsuCursorRs");
    p.push("settings.json");
    p
}

/// 设置文件是否存在（首次启动判断）。
pub fn exists() -> bool {
    settings_path().exists()
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        {
            Some(mut s) => {
                s.cursor_width = s.cursor_width.clamp(MIN_CURSOR_WIDTH, MAX_CURSOR_WIDTH);
                s
            }
            None => Settings::default(),
        }
    }

    pub fn save(&self) {
        let path = settings_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}