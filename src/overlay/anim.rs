//! 光标动画状态机：1:1 移植自 C# MainWindow.cs 的渲染/动画逻辑。
//! 弹簧、elastic 回弹、拖动旋转、按下缩放/发光等。

use crate::log::log;

/// 基础尺寸常量（与 C# 一致）。
pub const BASE_CURSOR_WIDTH: f64 = 30.0;
pub const BASE_CURSOR_HEIGHT: f64 = 42.5;
pub const POINTER_ANGLE: f64 = 24.3;
pub const BASE_WINDOW_SIZE: f64 = 160.0;
pub const BASE_WINDOW_MARGIN: f64 = 64.0;
pub const MIN_CURSOR_WIDTH: f64 = 16.0;
pub const MAX_CURSOR_WIDTH: f64 = 64.0;
const SETTLE_POSITION_EPSILON: f64 = 0.0008;
const SETTLE_VELOCITY_EPSILON: f64 = 0.01;
// 软件合成器的帧间观感比 WPF 的原版尾巴更拖。保留原版 elastic 曲线，
// 只缩短时间基数，让释放具有更明确的回弹冲量。
const ELASTIC_BASE_DURATION: f64 = 0.42;
/// 拖拽时角度追随速率。越大光标越跟手，且快速画圈时角度累积越充分
/// （模拟：rate=8 时 2500°/s 快速画圈仅累积 131° 无法转圈；rate=16 达 858°）。
/// 原版为 8.0，此处取 16.0 以保证快速画圈后释放也能明显转圈。
const DRAG_FOLLOW_RATE: f64 = 16.0;

/// 由光标宽度推导的几何尺寸。
#[derive(Debug, Clone, Copy)]
pub struct CursorGeometry {
    pub cursor_width: f64,
    pub cursor_height: f64,
    pub window_size: f64,
    pub window_margin: f64,
}

pub fn geometry_for_width(width: f64) -> CursorGeometry {
    let w = width.clamp(MIN_CURSOR_WIDTH, MAX_CURSOR_WIDTH);
    CursorGeometry {
        cursor_width: w,
        cursor_height: w * (BASE_CURSOR_HEIGHT / BASE_CURSOR_WIDTH),
        window_size: w * (BASE_WINDOW_SIZE / BASE_CURSOR_WIDTH),
        window_margin: w * (BASE_WINDOW_MARGIN / BASE_CURSOR_WIDTH),
    }
}

/// elastic-out 缓动（与原版 MainWindow.cs 一致）。
/// 转圈幅度来自拖动阶段累计得到的 start_angle；这个函数只负责回弹，
/// 不通过修改频率或衰减参数人为放大角度。
pub fn elastic_out(t: f64) -> f64 {
    (2.0f64).powf(-10.0 * t) * ((0.5 * t - 0.075) * 20.943951023931955).sin() + 1.0
        - 0.0004882812499999998 * t
}

/// 将角度归一化到 [-180, 180]。
pub fn normalize_angle(degrees: f64) -> f64 {
    let mut d = degrees % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

/// 拖动时的目标角度。
pub fn drag_angle(dx: f64, dy: f64) -> f64 {
    (-dx).atan2(dy).to_degrees() + POINTER_ANGLE
}

/// 光标动画状态。
#[derive(Debug, Clone, Copy)]
pub struct CursorAnim {
    pub angle: f64,
    pub angle_velocity: f64,
    pub scale_value: f64,
    pub scale_velocity: f64,
    pub additive_opacity: f64,
    pub opacity_velocity: f64,

    pub mouse_down: bool,
    pub drag_active: bool,
    pub pointer_hover: bool,
    pub elastic_returning: bool,
    elastic_start_angle: f64,
    elastic_duration: f64,
    elastic_elapsed: f64,
    // 拖动方向不能直接把每帧 atan2 的结果当作最终角度：跨过 ±180°
    // 时必须保留连续变化，否则绕圈会被折叠成一次很小的摆动。
    drag_target_angle: f64,
    last_drag_raw_angle: f64,
    drag_angle_initialized: bool,
}

impl Default for CursorAnim {
    fn default() -> Self {
        Self {
            angle: 0.0,
            angle_velocity: 0.0,
            scale_value: 1.0,
            scale_velocity: 0.0,
            additive_opacity: 0.0,
            opacity_velocity: 0.0,
            mouse_down: false,
            drag_active: false,
            pointer_hover: false,
            elastic_returning: false,
            elastic_start_angle: 0.0,
            elastic_duration: ELASTIC_BASE_DURATION,
            elastic_elapsed: 0.0,
            drag_target_angle: 0.0,
            last_drag_raw_angle: 0.0,
            drag_angle_initialized: false,
        }
    }
}

impl CursorAnim {
    /// 判断这一帧是否真的需要重新栅格化。光标移动时只移动分层窗口，
    /// 不重复绘制相同位图，避免空闲状态持续占用 CPU。
    pub fn visual_changed_from(&self, previous: &Self) -> bool {
        (self.angle - previous.angle).abs() > 0.0001
            || (self.scale_value - previous.scale_value).abs() > 0.0001
            || (self.additive_opacity - previous.additive_opacity).abs() > 0.0001
    }

    /// 按下：开始缩放/发光，准备拖动。
    pub fn begin_press(&mut self) {
        self.elastic_returning = false;
        self.mouse_down = true;
        self.drag_active = false;
        self.drag_target_angle = self.angle;
        self.last_drag_raw_angle = 0.0;
        self.drag_angle_initialized = false;
    }

    /// 抬起：若曾拖动则触发弹性回弹。
    pub fn end_press(&mut self) {
        if !self.mouse_down {
            return;
        }
        if self.drag_active {
            self.start_elastic_return();
        }
        self.mouse_down = false;
        self.drag_active = false;
    }

    fn start_elastic_return(&mut self) {
        if self.angle.abs() < 0.5 {
            return;
        }
        self.elastic_start_angle = self.angle;
        self.elastic_duration = ELASTIC_BASE_DURATION * (1.0 + (self.angle / 720.0).abs());
        self.elastic_elapsed = 0.0;
        self.elastic_returning = true;
        self.angle_velocity = 0.0;
        log(&format!(
            "elastic release: start_angle={:.1} duration={:.2}s",
            self.elastic_start_angle, self.elastic_duration
        ));
    }

    fn update_elastic_return(&mut self, dt: f64) {
        self.elastic_elapsed += dt;
        let t = (self.elastic_elapsed / self.elastic_duration).min(1.0);
        self.angle = self.elastic_start_angle * (1.0 - elastic_out(t));
        if t >= 1.0 {
            self.angle = 0.0;
            self.elastic_returning = false;
            self.angle_velocity = 0.0;
        }
    }

    fn settle(value: &mut f64, velocity: &mut f64, target: f64) {
        if (*value - target).abs() < SETTLE_POSITION_EPSILON
            && velocity.abs() < SETTLE_VELOCITY_EPSILON
        {
            *value = target;
            *velocity = 0.0;
        }
    }

    /// 返回连续展开后的拖动目标角度。
    ///
    /// `atan2` 本身只返回一个主值区间。直接使用它会在分支边界把
    /// `179° -> -179°` 当成一次大跳变；原版通过 NormalizeAngle 逐帧
    /// 追随时可以保留这次跨界，但 Rust 版如果只保存当前目标角度，
    /// 在更高/不规则的定时器采样下很容易丢掉绕圈累计量。因此这里
    /// 显式累积相邻方向的最短角差，再让实际角度追随这个未归一化目标。
    fn update_drag_target(&mut self, dx: f64, dy: f64) -> f64 {
        let raw_angle = drag_angle(dx, dy);
        if !self.drag_angle_initialized {
            // 第一次激活时仍选择离当前光标最近的等价角度，避免按下后
            // 因 atan2 主值区间产生一次反向跳转。
            self.drag_target_angle = self.angle + normalize_angle(raw_angle - self.angle);
            self.last_drag_raw_angle = raw_angle;
            self.drag_angle_initialized = true;
        } else {
            self.drag_target_angle += normalize_angle(raw_angle - self.last_drag_raw_angle);
            self.last_drag_raw_angle = raw_angle;
        }
        self.drag_target_angle
    }

    /// 更新一帧（dt 秒）。
    pub fn update(&mut self, dt: f64, drag_dx: f64, drag_dy: f64) {
        let (target_scale, target_additive, new_angle) = if self.mouse_down {
            // 按下：缩小 + 发光；拖动时按拖动方向旋转
            let target_angle = if self.drag_active {
                self.update_drag_target(drag_dx, drag_dy)
            } else {
                0.0
            };
            // target_angle 已经是连续展开角度，这里不能再次归一化，
            // 否则累计超过一圈后会重新折回 [-180°, 180°]。
            let delta = target_angle - self.angle;
            let a = self.angle + delta * (dt * DRAG_FOLLOW_RATE).clamp(0.0, 1.0);
            (0.9, 1.0, a)
        } else if self.elastic_returning {
            self.update_elastic_return(dt);
            (1.0, 0.0, self.angle)
        } else {
            // 原版只把 OCR_HAND（pointer_hover）作为悬停视觉状态。
            let target_angle = if self.pointer_hover {
                POINTER_ANGLE
            } else {
                0.0
            };
            let delta = normalize_angle(target_angle - self.angle);
            self.angle_velocity += (240.0 * delta - 20.0 * self.angle_velocity) * dt;
            let a = self.angle + self.angle_velocity * dt;
            (1.0, if self.pointer_hover { 1.0 } else { 0.0 }, a)
        };
        self.angle = new_angle;

        self.scale_velocity +=
            (240.0 * (target_scale - self.scale_value) - 20.0 * self.scale_velocity) * dt;
        self.scale_value += self.scale_velocity * dt;
        self.opacity_velocity +=
            (160.0 * (target_additive - self.additive_opacity) - 18.0 * self.opacity_velocity) * dt;
        self.additive_opacity += self.opacity_velocity * dt;

        self.scale_value = self.scale_value.clamp(0.8, 1.1);
        self.additive_opacity = self.additive_opacity.clamp(0.0, 1.0);

        let target_angle = if self.mouse_down {
            if self.drag_active {
                self.drag_target_angle
            } else {
                0.0
            }
        } else if self.pointer_hover && !self.elastic_returning {
            POINTER_ANGLE
        } else if !self.elastic_returning {
            0.0
        } else {
            self.angle
        };
        if !self.elastic_returning {
            Self::settle(&mut self.angle, &mut self.angle_velocity, target_angle);
        }
        Self::settle(
            &mut self.scale_value,
            &mut self.scale_velocity,
            target_scale,
        );
        Self::settle(
            &mut self.additive_opacity,
            &mut self.opacity_velocity,
            target_additive,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::CursorAnim;

    #[test]
    fn idle_animation_settles_without_perpetual_drift() {
        let mut anim = CursorAnim::default();
        for _ in 0..120 {
            anim.update(1.0 / 60.0, 0.0, 0.0);
        }
        let settled = anim;
        anim.update(1.0 / 60.0, 0.0, 0.0);
        assert_eq!(anim.angle, 0.0);
        assert_eq!(anim.scale_value, 1.0);
        assert_eq!(anim.additive_opacity, 0.0);
        assert!(!anim.visual_changed_from(&settled));
    }

    #[test]
    fn hand_hover_uses_stable_pink_state() {
        let mut anim = CursorAnim::default();
        anim.pointer_hover = true;
        for _ in 0..120 {
            anim.update(1.0 / 60.0, 0.0, 0.0);
        }
        assert_eq!(anim.angle, super::POINTER_ANGLE);
        assert_eq!(anim.additive_opacity, 1.0);

        let settled = anim;
        anim.update(1.0 / 60.0, 0.0, 0.0);
        assert!(!anim.visual_changed_from(&settled));
    }

    #[test]
    fn press_drag_and_release_follow_original_sequence() {
        let mut anim = CursorAnim::default();
        anim.begin_press();
        anim.update(1.0 / 60.0, 0.0, 0.0);
        assert!(anim.mouse_down);
        assert!(anim.additive_opacity > 0.0);
        assert!(anim.scale_value < 1.0);

        anim.drag_active = true;
        for _ in 0..30 {
            anim.update(1.0 / 60.0, 40.0, 20.0);
        }
        assert!(anim.angle.abs() > 0.5);

        anim.end_press();
        assert!(anim.elastic_returning);
        for _ in 0..120 {
            anim.update(1.0 / 60.0, 40.0, 20.0);
        }
        assert!(!anim.mouse_down);
        assert!(!anim.elastic_returning);
        assert_eq!(anim.angle, 0.0);
        assert_eq!(anim.scale_value, 1.0);
        assert_eq!(anim.additive_opacity, 0.0);
    }

    #[test]
    fn drag_angle_accumulates_across_atan2_branch() {
        let mut anim = CursorAnim::default();
        anim.begin_press();
        anim.drag_active = true;

        // 顺着按下点周围转一圈。每个方向保持若干帧，模拟真实拖动
        // 时动画追随目标角度，而不是测试瞬间跳转。
        for (dx, dy) in [
            (0.0, -100.0),
            (100.0, 0.0),
            (0.0, 100.0),
            (-100.0, 0.0),
            (0.0, -100.0),
        ] {
            for _ in 0..30 {
                anim.update(1.0 / 60.0, dx, dy);
            }
        }

        // 如果把 atan2 的主值直接当作目标，最后会回到约 -155.7°；
        // 连续展开后应保留完整一圈，角度超过 180°。
        assert!(anim.angle > 180.0, "累计角度未跨过一圈: {}", anim.angle);

        anim.end_press();
        assert!(anim.elastic_returning);
    }

    #[test]
    fn elastic_release_returns_promptly_for_large_angle() {
        let mut anim = CursorAnim::default();
        anim.begin_press();
        anim.drag_active = true;
        anim.angle = 360.0;
        anim.end_press();

        let mut elapsed = 0.0;
        while anim.elastic_returning && elapsed < 1.0 {
            anim.update(1.0 / 60.0, 0.0, 0.0);
            elapsed += 1.0 / 60.0;
        }

        assert!(!anim.elastic_returning);
        assert!(elapsed < 0.7, "大角度释放仍然过慢: {elapsed:.3}s");
        assert_eq!(anim.angle, 0.0);
    }

    /// 原版 elastic_out 必须从 0 开始（否则释放瞬间角度跳变），
    /// 且回弹允许小幅越过终点。
    #[test]
    fn elastic_out_starts_at_zero() {
        let v0 = super::elastic_out(0.0);
        assert!(v0.abs() < 0.01, "elastic_out(0)={v0}");
        let mut max_value = 0.0f64;
        let mut t = 0.0f64;
        while t <= 1.0 {
            let v = super::elastic_out(t);
            if v > max_value {
                max_value = v;
            }
            t += 0.001;
        }
        println!("elastic_out max={max_value:.3}");
        assert!(
            max_value > 1.0,
            "elastic_out 未产生原版回弹过冲: max={max_value}"
        );
    }
}
