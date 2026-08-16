//! 设置窗口：独立线程上的 eframe 应用，osu 深色风格。
//! 直接读写共享 Settings，变更时保存并通知覆盖层线程重新应用。

use crate::log::log;
use crate::overlay::MSG_SETTINGS_CHANGED;
use crate::settings::Settings;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

/// 加载中文字体（Microsoft YaHei）注入 egui。
fn setup_fonts(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\msyhbd.ttc",
    ];
    for path in candidates.iter() {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("msyh".to_owned(), egui::FontData::from_owned(bytes));
            if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                f.insert(0, "msyh".to_owned());
            }
            if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                f.push("msyh".to_owned());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
    log("settings_ui: no CJK font found, using default");
}

struct SettingsApp {
    settings: Arc<Mutex<Settings>>,
    hwnd: HWND,
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut changed = false;
        let mut s = {
            let g = self.settings.lock().unwrap_or_else(|e| e.into_inner());
            g.clone()
        };
        let before = s.clone();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("设置");
            ui.add_space(12.0);

            ui.label("光标大小");
            ui.add(
                egui::Slider::new(&mut s.cursor_width, 16.0..=64.0).show_value(true),
            );
            ui.add_space(12.0);

            ui.checkbox(&mut s.auto_start, "开机自启");
            ui.add_space(10.0);

            ui.checkbox(&mut s.tap_sound_enabled, "点按音效");
            ui.add(
                egui::Slider::new(&mut s.tap_sound_volume, 0.0..=1.0).show_value(true),
            );
            ui.add_space(10.0);

            ui.checkbox(&mut s.hover_sound_enabled, "悬停音效");
            ui.add(
                egui::Slider::new(&mut s.hover_sound_volume, 0.0..=1.0).show_value(true),
            );
            ui.add_space(10.0);

            ui.radio_value(&mut s.hover_sound_as_resize_prompt, false, "悬停可点击元素时播放");
            ui.radio_value(&mut s.hover_sound_as_resize_prompt, true, "窗口拉伸时播放");
            ui.add_space(8.0);
        });

        if s != before {
            changed = true;
            s.save();
            {
                let mut g = self.settings.lock().unwrap_or_else(|e| e.into_inner());
                *g = s;
            }
        }
        if changed {
            unsafe {
                PostMessageW(self.hwnd, MSG_SETTINGS_CHANGED, 0, 0);
            }
        }
    }
}

/// 在独立线程启动 eframe 设置窗口。
pub fn spawn(settings: Arc<Mutex<Settings>>, hwnd: HWND) {
    let hwnd_usize = hwnd as usize;
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let native_options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([420.0, 460.0])
                    .with_title("osu! Cursor 设置")
                    .with_resizable(false),
                ..Default::default()
            };
            let app_creator = move |cc: &eframe::CreationContext<'_>| {
                setup_fonts(&cc.egui_ctx);
                Ok(Box::new(SettingsApp {
                    settings,
                    hwnd: hwnd_usize as HWND,
                }) as Box<dyn eframe::App>)
            };
            let _ = eframe::run_native("curosu-settings", native_options, Box::new(app_creator));
        }));
        if let Err(e) = result {
            log(&format!("settings_ui: eframe thread panicked: {e:?}"));
        }
        // 窗口关闭：通知覆盖层重置打开标记
        unsafe {
            PostMessageW(hwnd_usize as HWND, crate::overlay::MSG_SETTINGS_CLOSED, 0, 0);
        }
    });
}