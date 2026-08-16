//! osu! Cursor for Windows —— Rust 重写（路线 B）。
//! 入口：单实例、加载设置、创建音频播放器、启动覆盖层。

mod audio;
mod autostart;
mod log;
mod overlay;
mod settings;
mod settings_ui;
mod system_cursor;
mod tray;

use audio::TapPlayer;
use settings::Settings;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};

fn main() {
    // 单实例互斥体
    unsafe {
        let name: Vec<u16> = "Local\\Curosu.SingleInstance\0".encode_utf16().collect();
        let _h = CreateMutexW(std::ptr::null(), true, name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
    }

    let settings = Arc::new(Mutex::new(Settings::load()));

    // 音效播放器
    let tap = TapPlayer::new(include_bytes!("../assets/cursor-tap.wav").to_vec());
    let hover = TapPlayer::new(include_bytes!("../assets/default-hover.wav").to_vec());
    {
        let s = settings.lock().unwrap();
        tap.set_enabled(s.tap_sound_enabled);
        tap.set_volume(s.tap_sound_volume);
        hover.set_enabled(s.hover_sound_enabled);
        hover.set_volume(s.hover_sound_volume);
    }

    overlay::run(settings, tap, hover);
}