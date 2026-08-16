//! 软件合成器：把光标两层 PNG（普通 + additive 发光）用双线性仿射
//! （旋转 + 缩放）合成到 160px 预乘 ARGB 缓冲，再通过 UpdateLayeredWindow 呈现。
//! 移植自 C# 的 WPF 渲染管线（两层 Image 叠加 + RotateTransform/ScaleTransform）。

use super::anim::{CursorAnim, CursorGeometry};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HGDIOBJ, HDC,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, GetWindowRect, ULW_ALPHA};
use windows_sys::Win32::Foundation::{HWND, RECT};

/// 解码后的 RGBA8 位图。
pub struct Texture {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>, // RGBA8, 行优先
}

impl Texture {
    /// 采样（双线性）。返回直线（非预乘）RGBA，范围 [0,1]。
    fn sample(&self, px: f64, py: f64) -> (f64, f64, f64, f64) {
        let w = self.w as f64;
        let h = self.h as f64;
        if px < 0.0 || px >= w || py < 0.0 || py >= h {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let x0 = (px.floor()).clamp(0.0, w - 1.0) as usize;
        let y0 = (py.floor()).clamp(0.0, h - 1.0) as usize;
        let x1 = (px.ceil()).clamp(0.0, w - 1.0) as usize;
        let y1 = (py.ceil()).clamp(0.0, h - 1.0) as usize;
        let fx = px - x0 as f64;
        let fy = py - y0 as f64;
        let get = |x: usize, y: usize, c: usize| -> f64 {
            self.data[(y * self.w as usize + x) * 4 + c] as f64 / 255.0
        };
        let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
        let mut out = [0.0f64; 4];
        for c in 0..4 {
            let top = lerp(get(x0, y0, c), get(x1, y0, c), fx);
            let bot = lerp(get(x0, y1, c), get(x1, y1, c), fx);
            out[c] = lerp(top, bot, fy);
        }
        (out[0], out[1], out[2], out[3])
    }
}

/// 从嵌入字节解码 PNG（启动时调用一次）。
pub fn decode_png(bytes: &[u8]) -> Option<Texture> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let w = info.width;
    let h = info.height;
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    buf.truncate((w as usize) * (h as usize) * 4);
    Some(Texture { w, h, data: buf })
}

/// 两层光标纹理。
pub struct CursorTextures {
    pub cursor: Texture,
    pub additive: Texture,
}

/// 合成器：持有窗口缓冲和重绘用的内存 DC / DIB。
pub struct Compositor {
    pub w: u32,
    pub h: u32,
    buffer: Vec<u32>, // 预乘 ARGB
    bits: *mut u8,
    mem_dc: HDC,
    dib: HGDIOBJ,
    old_obj: HGDIOBJ,
}

impl Compositor {
    pub fn new(w: u32, h: u32) -> Option<Self> {
        unsafe {
            let screen_dc = GetDC(std::ptr::null_mut());
            let mem_dc = CreateCompatibleDC(screen_dc);
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            if mem_dc.is_null() {
                return None;
            }
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32), // 自上而下
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD {
                    rgbBlue: 0,
                    rgbGreen: 0,
                    rgbRed: 0,
                    rgbReserved: 0,
                }],
            };
            let mut bits_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(
                mem_dc,
                &mut bmi,
                DIB_RGB_COLORS,
                &mut bits_ptr,
                std::ptr::null_mut(),
                0,
            );
            if dib.is_null() {
                DeleteDC(mem_dc);
                return None;
            }
            let old_obj = SelectObject(mem_dc, dib);
            Some(Self {
                w,
                h,
                buffer: vec![0u32; (w * h) as usize],
                bits: bits_ptr as *mut u8,
                mem_dc,
                dib,
                old_obj,
            })
        }
    }

    /// 合成一帧到缓冲。
    pub fn draw(&mut self, geom: &CursorGeometry, anim: &CursorAnim, tex: &CursorTextures) {
        let margin = geom.window_margin;
        let cw = geom.cursor_width;
        let ch = geom.cursor_height;
        let s = anim.scale_value;
        let theta = anim.angle.to_radians();
        let (sin, cos) = theta.sin_cos();
        let add_op = anim.additive_opacity;
        // 有效源矩形（加一点缓冲避免边缘断层）
        let x0 = margin - cw * s.max(1.2);
        let x1 = margin + cw * s.max(1.2);
        let y0 = margin - ch * s.max(1.2);
        let y1 = margin + ch * s.max(1.2);

        for by in 0..self.h {
            for bx in 0..self.w {
                let pw = bx as f64;
                let ph = by as f64;
                if pw < x0 || pw > x1 || ph < y0 || ph > y1 {
                    self.buffer[(by as usize) * self.w as usize + (bx as usize)] = 0;
                    continue;
                }
                // 逆映射：窗口 -> 图像单位
                let wx = pw - margin;
                let wy = ph - margin;
                let rx = wx * cos + wy * sin;
                let ry = -wx * sin + wy * cos;
                let ix = rx / s;
                let iy = ry / s;
                if ix < 0.0 || iy < 0.0 || ix >= cw || iy >= ch {
                    self.buffer[(by as usize) * self.w as usize + (bx as usize)] = 0;
                    continue;
                }
                // 图像单位 -> PNG 像素
                let cpx = ix * tex.cursor.w as f64 / cw;
                let cpy = iy * tex.cursor.h as f64 / ch;
                let (cr, cg, cb, ca) = tex.cursor.sample(cpx, cpy);
                let apx = ix * tex.additive.w as f64 / cw;
                let apy = iy * tex.additive.h as f64 / ch;
                let (ar, ag, ab, aa) = tex.additive.sample(apx, apy);

                // 光标 over 透明底
                let mut or = cr * ca;
                let mut og = cg * ca;
                let mut ob = cb * ca;
                let mut oa = ca;
                // additive 叠加（带合成透明度）
                let ma = aa * add_op;
                or = ar * ma + or * (1.0 - ma);
                og = ag * ma + og * (1.0 - ma);
                ob = ab * ma + ob * (1.0 - ma);
                oa = ma + oa * (1.0 - ma);

                let a = (oa * 255.0).round().clamp(0.0, 255.0) as u32;
                let r = (or * 255.0).round().clamp(0.0, 255.0) as u32;
                let g = (og * 255.0).round().clamp(0.0, 255.0) as u32;
                let b = (ob * 255.0).round().clamp(0.0, 255.0) as u32;
                self.buffer[(by as usize) * self.w as usize + (bx as usize)] =
                    (a << 24) | (r << 16) | (g << 8) | b;
            }
        }
    }

    /// 把缓冲写入 DIB 并调用 UpdateLayeredWindow 呈现。
    pub fn present(&mut self, hwnd: HWND) {
        unsafe {
            if !self.bits.is_null() {
                std::ptr::copy_nonoverlapping(
                    self.buffer.as_ptr() as *const u8,
                    self.bits,
                    (self.w as usize) * (self.h as usize) * 4,
                );
            }
            let screen_dc = GetDC(std::ptr::null_mut());
            let size = windows_sys::Win32::Foundation::SIZE {
                cx: self.w as i32,
                cy: self.h as i32,
            };
            let pt_src = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            // 先获取窗口当前位置
            let mut rc: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut rc);
            let pt_dst = windows_sys::Win32::Foundation::POINT { x: rc.left, y: rc.top };
            let ok = UpdateLayeredWindow(
                hwnd,
                screen_dc,
                &pt_dst,
                &size,
                self.mem_dc,
                &pt_src,
                0,
                &blend,
                ULW_ALPHA,
            );
            if ok == 0 {
                crate::log::log("render: UpdateLayeredWindow failed");
            }
            ReleaseDC(std::ptr::null_mut(), screen_dc);
        }
    }
}

impl Drop for Compositor {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.mem_dc, self.old_obj);
            DeleteObject(self.dib);
            DeleteDC(self.mem_dc);
        }
    }
}