//! 光标动画状态机：1:1 移植自 C# MainWindow.cs 的渲染/动画逻辑。
//! 弹簧、elastic 回弹、拖动旋转、按下缩放/发光等。

/// 基础尺寸常量（与 C# 一致）。
pub const BASE_CURSOR_WIDTH: f64 = 30.0;
pub const BASE_CURSOR_HEIGHT: f64 = 42.5;
pub const POINTER_ANGLE: f64 = 24.3;
pub const BASE_WINDOW_SIZE: f64 = 160.0;
pub const BASE_WINDOW_MARGIN: f64 = 64.0;
pub const MIN_CURSOR_WIDTH: f64 = 16.0;
pub const MAX_CURSOR_WIDTH: f64 = 64.0;

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

/// elastic-out 缓动（与 C# ElasticOut 一致）。
pub fn elastic_out(t: f64) -> f64 {
    (2.0f64).powf(-10.0 * t) * ((0.5 * t - 0.075) * 20.943951023931955).sin()
        + 1.0
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
            elastic_duration: 0.6,
            elastic_elapsed: 0.0,
        }
    }
}

impl CursorAnim {
    /// 按下：开始缩放/发光，准备拖动。
    pub fn begin_press(&mut self) {
        self.elastic_returning = false;
        self.mouse_down = true;
        self.drag_active = false;
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
        self.elastic_duration = 0.6 * (1.0 + (self.angle / 720.0).abs());
        self.elastic_elapsed = 0.0;
        self.elastic_returning = true;
        self.angle_velocity = 0.0;
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

    /// 更新一帧（dt 秒）。
    pub fn update(&mut self, dt: f64, drag_dx: f64, drag_dy: f64) {
        let (target_scale, target_additive, new_angle) = if self.mouse_down {
            // 按下：缩小 + 发光；拖动时按拖动方向旋转
            let target_angle = if self.drag_active {
                drag_angle(drag_dx, drag_dy)
            } else {
                0.0
            };
            let delta = normalize_angle(target_angle - self.angle);
            let a = self.angle + delta * (dt * 8.0).clamp(0.0, 1.0);
            (0.9, 1.0, a)
        } else if self.elastic_returning {
            self.update_elastic_return(dt);
            (1.0, 0.0, self.angle)
        } else {
            // 空闲：悬停时偏转 + 发光
            let target_angle = if self.pointer_hover { POINTER_ANGLE } else { 0.0 };
            let delta = normalize_angle(target_angle - self.angle);
            self.angle_velocity += (240.0 * delta - 20.0 * self.angle_velocity) * dt;
            let a = self.angle + self.angle_velocity * dt;
            (
                1.0,
                if self.pointer_hover { 1.0 } else { 0.0 },
                a,
            )
        };
        self.angle = new_angle;

        self.scale_velocity += (240.0 * (target_scale - self.scale_value) - 20.0 * self.scale_velocity) * dt;
        self.scale_value += self.scale_velocity * dt;
        self.opacity_velocity += (160.0 * (target_additive - self.additive_opacity) - 18.0 * self.opacity_velocity) * dt;
        self.additive_opacity += self.opacity_velocity * dt;

        self.scale_value = self.scale_value.clamp(0.8, 1.1);
        self.additive_opacity = self.additive_opacity.clamp(0.0, 1.0);
    }
}