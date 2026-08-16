//! 低层鼠标钩子（WH_MOUSE_LL）：更新光标位置与按下/抬起边沿。
//! 移植自 C# NativeMethods + MainWindow 的钩子处理。
//! 钩子必须安装在带消息循环的线程上。

use crate::log::log;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use windows_sys::Win32::Foundation::LPARAM;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetWindowsHookExW, UnhookWindowsHookEx, CallNextHookEx, MSLLHOOKSTRUCT, WH_MOUSE_LL,
    GetCursorPos,
    WM_LBUTTONDOWN, WM_RBUTTONDOWN, WM_MBUTTONDOWN, WM_XBUTTONDOWN, WM_LBUTTONUP,
    WM_RBUTTONUP, WM_MBUTTONUP, WM_XBUTTONUP,
};

pub static CURSOR_X: AtomicI32 = AtomicI32::new(0);
pub static CURSOR_Y: AtomicI32 = AtomicI32::new(0);
/// 按下边沿（消费一次）
pub static PRESS_PENDING: AtomicBool = AtomicBool::new(false);
/// 抬起边沿（消费一次）
pub static RELEASE_PENDING: AtomicBool = AtomicBool::new(false);
/// 按下事件时的精确光标位置（在回调中同步记录，与 C# 一致）
pub static PRESS_X: AtomicI32 = AtomicI32::new(0);
pub static PRESS_Y: AtomicI32 = AtomicI32::new(0);
/// 钩子回调计数（健康监测用：区分"没动鼠标"与"钩子已死"）
pub static EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

static mut HOOK: *mut c_void = std::ptr::null_mut();

unsafe extern "system" fn low_level_mouse_proc(_ncode: i32, wparam: usize, lparam: LPARAM) -> isize {
    if _ncode >= 0 {
        let data = lparam as *const MSLLHOOKSTRUCT;
        let pt = (*data).pt;
        CURSOR_X.store(pt.x, Ordering::Relaxed);
        CURSOR_Y.store(pt.y, Ordering::Relaxed);
        EVENT_COUNT.fetch_add(1, Ordering::Relaxed);
        match wparam as u32 {
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                PRESS_X.store(pt.x, Ordering::Relaxed);
                PRESS_Y.store(pt.y, Ordering::Relaxed);
                PRESS_PENDING.store(true, Ordering::Relaxed);
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                RELEASE_PENDING.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }
    CallNextHookEx(HOOK, _ncode, wparam, lparam)
}

/// 在当前线程安装钩子。成功返回 true。
pub fn install() -> bool {
    unsafe {
        let hook = SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(low_level_mouse_proc),
            GetModuleHandleW(std::ptr::null()),
            0,
        );
        HOOK = hook;
        if hook.is_null() {
            log("mouse hook install failed");
            false
        } else {
            log("mouse hook installed");
            true
        }
    }
}

pub fn uninstall() {
    unsafe {
        if !HOOK.is_null() {
            UnhookWindowsHookEx(HOOK);
            HOOK = std::ptr::null_mut();
        }
    }
}

/// 用当前真实光标位置初始化坐标（避免首个鼠标事件前停留在 0,0）。
pub fn init_position() {
    unsafe {
        let mut pt: windows_sys::Win32::Foundation::POINT = std::mem::zeroed();
        if GetCursorPos(&mut pt) != 0 {
            CURSOR_X.store(pt.x, Ordering::Relaxed);
            CURSOR_Y.store(pt.y, Ordering::Relaxed);
        }
    }
}

/// 读取并消费一次按下边沿。
pub fn take_press() -> bool {
    PRESS_PENDING.swap(false, Ordering::Relaxed)
}

/// 读取并消费一次抬起边沿。
pub fn take_release() -> bool {
    RELEASE_PENDING.swap(false, Ordering::Relaxed)
}

pub fn cursor_pos() -> (i32, i32) {
    (CURSOR_X.load(Ordering::Relaxed), CURSOR_Y.load(Ordering::Relaxed))
}

/// 最近一次按下事件的光标位置（事件级精度）。
pub fn press_pos() -> (i32, i32) {
    (PRESS_X.load(Ordering::Relaxed), PRESS_Y.load(Ordering::Relaxed))
}

/// 当前钩子回调计数。
pub fn event_count() -> u64 {
    EVENT_COUNT.load(Ordering::Relaxed)
}

/// 每帧用 GetCursorPos 刷新位置。位置来源从此不依赖钩子存活
/// （WH_MOUSE_LL 回调超时会被系统静默摘除，导致覆盖层冻结、
/// 而系统光标已是空白 → 表现为"光标消失"）。钩子仅负责点击边沿。
pub fn poll_position() {
    let mut pt: windows_sys::Win32::Foundation::POINT = unsafe { std::mem::zeroed() };
    if unsafe { GetCursorPos(&mut pt) } != 0 {
        CURSOR_X.store(pt.x, Ordering::Relaxed);
        CURSOR_Y.store(pt.y, Ordering::Relaxed);
    }
}