# Curosu — osu! 风格光标覆盖层（Windows）

Curosu 是用 Rust 编写的 Windows 光标覆盖层：用自绘的 osu! 风格光标替换系统光标——按下缩放发光、拖动跟随旋转、释放先快速转 3 圈再弹性回正，并带有敲击 / 悬停音效和设置窗口。

## 特性

- 🖱️ 自绘光标覆盖层，隐藏并替换系统光标（鼠标钩子捕获事件，失效时自动回退轮询）
- 🔄 按下缩放 + 发光；拖动时角度连续累积、跟随旋转；释放先 0.5s 正转 3 圈，再 elastic 摆动回归初始方向
- 🔇 敲击音效 / 悬停音效（可开关、可调音量、可做窗口拉伸提示）
- ⚙️ 设置窗口：光标大小、音效、开机自启（eframe/egui，常驻线程、可反复开关）
- 📌 置顶、点击穿透、跨显示器 DPI 自适应、任务栏预览置顶修复
- 🪟 托盘图标：左键打开设置、右键菜单（设置 / 关闭光标 / 退出）

## 构建

需要 Windows 上的 Rust 稳定版工具链。

```bash
cargo build --release
```

产物：`target/release/curosu.exe`

已配置 GitHub Actions（`.github/workflows/build.yml`）：push 到 `main` 自动构建并上传 artifact；打 `v*` tag 自动发布 GitHub Release。

## 使用

直接运行 `curosu.exe`。程序会隐藏系统光标并显示自绘光标，首次运行自动打开设置窗口。

托盘操作：

| 操作 | 行为 |
|---|---|
| 左键单击 | 打开 / 聚焦设置窗口 |
| 右键 | 菜单：设置、关闭光标、退出 |

## 配置

设置保存在 `%APPDATA%\Curosu\settings.json`：

| 字段 | 含义 | 默认 |
|---|---|---|
| `cursor_width` | 光标宽度（px，16–64） | `30` |
| `tap_sound_enabled` | 敲击音效 | `true` |
| `tap_sound_volume` | 敲击音量（0–1） | `1.0` |
| `hover_sound_enabled` | 悬停音效 | `true` |
| `hover_sound_volume` | 悬停音量（0–1） | `1.0` |
| `hover_sound_as_resize_prompt` | 窗口拉伸时播放悬停音效 | `false` |
| `auto_start` | 开机自启 | `false` |

## 技术实现

- 软件合成器：两层 PNG（普通 + additive 高光）经双线性仿射旋转 / 缩放合成到预乘 ARGB 缓冲，通过 `UpdateLayeredWindow` 呈现
- `WH_MOUSE_LL` 低层鼠标钩子捕获按下 / 抬起边沿与光标位置；钩子被系统摘除时自动重装、位置每帧用 `GetCursorPos` 兜底
- 释放动画两阶段：先 0.5s 正转 3 圈（quad ease-out），随后角度归一化并做 elastic 摆动回归初始方向
- 设置窗口：eframe/egui，常驻线程 + 原生 `ShowWindow` 显示 / 隐藏，可反复开关不卡死
- 工具脚本见 `scripts/`（安装 uiAccess、恢复系统光标、停止运行实例）

## 许可

[MIT License](LICENSE)（Copyright © 2026 yaki1210）
