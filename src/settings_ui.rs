//! 原版风格设置窗口：三组圆角面板、粉色滑块和自绘开关。
//! 直接读写共享 Settings，变更时保存并通知覆盖层线程重新应用。

use crate::log::log;
use crate::overlay::MSG_SETTINGS_CHANGED;
use crate::settings::Settings;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

const APP_ICON_ICO: &[u8] = include_bytes!("../assets/icon.ico");
const BACKGROUND: egui::Color32 = egui::Color32::from_rgb(18, 19, 24);
const PANEL: egui::Color32 = egui::Color32::from_rgb(30, 31, 38);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(255, 102, 171);
const TEXT: egui::Color32 = egui::Color32::from_rgb(238, 239, 244);
const MUTED: egui::Color32 = egui::Color32::from_rgb(158, 160, 172);
/// 滑条轨道底色（比面板亮一档，保证轨道在面板上可见）。
const RAIL: egui::Color32 = egui::Color32::from_rgb(100, 103, 113);

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

fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.slider_width = 320.0;

    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    // egui 0.28 滑条轨道与圆形滑块底色均由 widgets.inactive.bg_fill 绘制
    // （见 egui slider.rs: rect_filled(rail_rect, ..., inactive.bg_fill)）。
    // 原值 PANEL 与面板底色重合导致轨道隐形，改用亮一档的 RAIL。
    visuals.widgets.inactive.bg_fill = RAIL;
    visuals.widgets.inactive.fg_stroke.color = TEXT;
    visuals.widgets.hovered.bg_fill = RAIL;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.fg_stroke.color = TEXT;
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.fg_stroke.color = TEXT;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke.color = TEXT;
    // 显示"起点→滑块"的填充段（用 selection.bg_fill=ACCENT 粉色），
    // 对齐原版 WPF 滑条"粉填充 + 灰轨道"外观。
    visuals.slider_trailing_fill = true;
    style.visuals = visuals;
    ctx.set_style(style);
}

/// eframe 默认会显示一个白色的 e。ICO 中的第一项是 PNG，直接解码后
/// 传给 viewport，设置窗口、任务栏和 Alt-Tab 会使用 Curosu 图标。
fn load_app_icon() -> Option<egui::IconData> {
    if APP_ICON_ICO.len() < 22 {
        return None;
    }
    let image_size = u32::from_le_bytes(APP_ICON_ICO[14..18].try_into().ok()?) as usize;
    let image_offset = u32::from_le_bytes(APP_ICON_ICO[18..22].try_into().ok()?) as usize;
    let image_end = image_offset.checked_add(image_size)?;
    let png_bytes = APP_ICON_ICO.get(image_offset..image_end)?;

    let mut decoder = png::Decoder::new(png_bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut rgba).ok()?;
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    rgba.truncate((info.width as usize) * (info.height as usize) * 4);
    Some(egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    })
}

fn section(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(PANEL)
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(14.0)
                    .strong()
                    .color(ACCENT),
            );
            ui.add_space(2.0);
            contents(ui);
        });
}

fn value_row(ui: &mut egui::Ui, left: &str, right: String) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(left).size(13.0).color(MUTED));
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new(right).size(13.0).color(TEXT));
            },
        );
    });
}

fn draw_switch(ui: &mut egui::Ui, id_source: &str, checked: &mut bool, label: &str) {
    let id = ui.make_persistent_id(id_source);
    let (row, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 28.0),
        egui::Sense::click(),
    );
    if response.clicked() {
        *checked = !*checked;
    }

    let checked_t = ui
        .ctx()
        .animate_bool_with_time(id.with("checked"), *checked, 0.18);
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), response.hovered(), 0.12);

    ui.painter().text(
        egui::pos2(row.left(), row.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        TEXT,
    );

    let scale = 1.0 + hover_t * 0.04;
    let track = egui::Rect::from_center_size(
        egui::pos2(row.right() - 25.0, row.center().y),
        egui::vec2(50.0 * scale, 15.0 * scale),
    );
    ui.painter()
        .rect_filled(track, 7.5 * scale, egui::Color32::WHITE);

    let border = 3.0 + checked_t * 3.0;
    let inner = track.shrink(border);
    let alpha = (checked_t * 255.0).round().clamp(0.0, 255.0) as u8;
    let fill = egui::Color32::from_rgba_unmultiplied(
        ACCENT.r(),
        ACCENT.g(),
        ACCENT.b(),
        alpha,
    );
    ui.painter().rect_filled(inner, 5.0 * scale, fill);

    if response.hovered() {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
    }
}

fn draw_slider(ui: &mut egui::Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>) {
    ui.add(egui::Slider::new(value, range).show_value(false));
}

fn draw_volume(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    value_row(ui, label, format!("{}%", (*value * 100.0).round() as i32));
    draw_slider(ui, value, 0.0..=1.0);
}

struct SettingsApp {
    settings: Arc<Mutex<Settings>>,
    hwnd: HWND,
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut s = {
            let g = self.settings.lock().unwrap_or_else(|e| e.into_inner());
            g.clone()
        };
        let before = s.clone();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BACKGROUND)
                    .inner_margin(egui::Margin::same(20.0)),
            )
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("设置")
                        .size(24.0)
                        .strong()
                        .color(ACCENT),
                );
                ui.add_space(18.0);

                section(ui, "光标", |ui| {
                    value_row(ui, "16 - 64", format!("{:.0} px", s.cursor_width));
                    draw_slider(ui, &mut s.cursor_width, 16.0..=64.0);

                    let reset = ui.add_sized(
                        egui::vec2(96.0, 30.0),
                        egui::Button::new(egui::RichText::new("恢复默认").color(TEXT))
                            .fill(ACCENT)
                            .stroke(egui::Stroke::NONE),
                    );
                    if reset.clicked() {
                        s.cursor_width = 30.0;
                    }
                });
                ui.add_space(12.0);

                section(ui, "音效", |ui| {
                    draw_switch(ui, "tap_sound", &mut s.tap_sound_enabled, "敲击音效");
                    ui.add_space(2.0);
                    ui.add_enabled_ui(s.tap_sound_enabled, |ui| {
                        draw_volume(ui, "音量", &mut s.tap_sound_volume);
                    });

                    ui.add_space(8.0);
                    draw_switch(ui, "hover_sound", &mut s.hover_sound_enabled, "悬停音效");
                    ui.add_space(2.0);
                    ui.add_enabled_ui(s.hover_sound_enabled, |ui| {
                        draw_volume(ui, "悬停音量", &mut s.hover_sound_volume);
                    });

                    ui.add_space(8.0);
                    draw_switch(
                        ui,
                        "resize_sound",
                        &mut s.hover_sound_as_resize_prompt,
                        "窗口拉伸时播放",
                    );
                });
                ui.add_space(12.0);

                section(ui, "系统", |ui| {
                    draw_switch(ui, "auto_start", &mut s.auto_start, "开机自启");
                });
            });

        if s != before {
            s.save();
            {
                let mut g = self.settings.lock().unwrap_or_else(|e| e.into_inner());
                *g = s;
            }
            unsafe {
                PostMessageW(self.hwnd, MSG_SETTINGS_CHANGED, 0, 0);
            }
        }
    }
}

/// 设置线程的"打开"触发通道（首次 spawn 时创建并常驻）。
/// eframe 0.28 的事件循环缓存在线程本地（native/run.rs: with_event_loop），
/// 设计意图是同一线程反复 run_native 以支持反复开关窗口；若每次打开都新开
/// 线程，跨线程反复创建 winit 事件循环不可靠（曾导致窗口打不开）。因此改为
/// 持久线程 + 通道触发，同一线程内复用事件循环，关闭后还能再次打开。
static OPEN_TX: Mutex<Option<Sender<()>>> = Mutex::new(None);

/// 幂等地打开设置窗口：首次调用启动常驻线程，之后每次调用唤醒它再开一次。
pub fn spawn(settings: Arc<Mutex<Settings>>, hwnd: HWND) {
    let hwnd_usize = hwnd as usize; // HWND 非 Send，跨线程用 usize 传递
    let mut guard = OPEN_TX.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let (tx, rx) = channel();
        *guard = Some(tx);
        std::thread::spawn(move || settings_thread(rx, settings, hwnd_usize));
    }
    if let Some(tx) = guard.as_ref() {
        let _ = tx.send(());
    }
}

fn settings_thread(rx: Receiver<()>, settings: Arc<Mutex<Settings>>, hwnd_usize: usize) {
    let hwnd = hwnd_usize as HWND;
    loop {
        if rx.recv().is_err() {
            break; // 通道关闭（进程退出）
        }
        // 每次迭代取一个 Arc 副本供闭包 move，避免跨迭代移动 settings。
        let settings_app = Arc::clone(&settings);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let native_options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([420.0, 650.0])
                    .with_title("Curosu 设置")
                    .with_resizable(false)
                    .with_minimize_button(true)
                    .with_maximize_button(false),
                // 主线程已被覆盖层消息循环占用；winit 默认拒绝在
                // 非主线程创建事件循环（此前设置窗口静默 panic 打不开）。
                event_loop_builder: Some(Box::new(|builder| {
                    #[cfg(windows)]
                    {
                        use winit::platform::windows::EventLoopBuilderExtWindows;
                        builder.with_any_thread(true);
                    }
                    let _ = builder;
                })),
                ..Default::default()
            };
            let mut native_options = native_options;
            if let Some(icon) = load_app_icon() {
                native_options.viewport = native_options.viewport.with_icon(icon);
            }
            let app_creator = move |cc: &eframe::CreationContext<'_>| {
                setup_fonts(&cc.egui_ctx);
                setup_style(&cc.egui_ctx);
                Ok(Box::new(SettingsApp {
                    settings: settings_app.clone(),
                    hwnd,
                }) as Box<dyn eframe::App>)
            };
            match eframe::run_native("Curosu", native_options, Box::new(app_creator)) {
                Err(e) => log(&format!("settings_ui: run_native error: {e:?}")),
                Ok(()) => {}
            }
        }));
        if let Err(e) = result {
            log(&format!("settings_ui: eframe thread panicked: {e:?}"));
        }
        // 窗口关闭：通知覆盖层重置打开标记
        unsafe {
            PostMessageW(hwnd, crate::overlay::MSG_SETTINGS_CLOSED, 0, 0);
        }
    }
}
