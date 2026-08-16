//! 系统光标替换：把 14 种标准光标替换为 32x32 空白光标，退出时恢复。
//! 移植自 C# CursorReplacer.cs。

use crate::log;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetSystemCursor, LoadCursorW, CopyIcon, DestroyCursor, CreateCursor, SystemParametersInfoW,
    SPI_SETCURSORS, SPIF_SENDCHANGE,
};
use std::ffi::c_void;

/// 标准光标 ID（与 C# OCR_* 常量一致）。
pub const OCR_NORMAL: u32 = 32512;
pub const OCR_IBEAM: u32 = 32513;
pub const OCR_WAIT: u32 = 32514;
pub const OCR_CROSS: u32 = 32515;
pub const OCR_UP: u32 = 32516;
pub const OCR_SIZENWSE: u32 = 32642;
pub const OCR_SIZENESW: u32 = 32643;
pub const OCR_SIZEWE: u32 = 32644;
pub const OCR_SIZENS: u32 = 32645;
pub const OCR_SIZEALL: u32 = 32646;
pub const OCR_NO: u32 = 32648;
pub const OCR_HAND: u32 = 32649;
pub const OCR_APPSTARTING: u32 = 32650;
pub const OCR_HELP: u32 = 32651;

const CURSOR_IDS: [u32; 14] = [
    OCR_NORMAL,
    OCR_IBEAM,
    OCR_WAIT,
    OCR_CROSS,
    OCR_UP,
    OCR_SIZENWSE,
    OCR_SIZENESW,
    OCR_SIZEWE,
    OCR_SIZENS,
    OCR_SIZEALL,
    OCR_NO,
    OCR_HAND,
    OCR_APPSTARTING,
    OCR_HELP,
];

static mut BLANK_HANDLES: [isize; 14] = [0; 14];
static mut INSTALLED: bool = false;

/// 已替换的空白光标句柄是否包含指定 id。
pub fn get_blank_handle(cursor_id: u32) -> isize {
    unsafe {
        for (i, id) in CURSOR_IDS.iter().enumerate() {
            if *id == cursor_id {
                return BLANK_HANDLES[i];
            }
        }
    }
    0
}

/// 安装：将所有标准光标替换为空白光标。
pub fn install() -> bool {
    unsafe {
        if INSTALLED {
            return true;
        }
        let mut installed_any = false;
        for (i, id) in CURSOR_IDS.iter().enumerate() {
            let blank = create_blank_cursor();
            if blank == 0 {
                log(&format!("CreateCursor failed for id={id}"));
                continue;
            }
            if SetSystemCursor(blank as isize, *id) == 0 {
                log(&format!("SetSystemCursor failed for id={id}"));
                DestroyCursor(blank as isize);
                continue;
            }
            log(&format!("Hidden system cursor id={id}"));
            BLANK_HANDLES[i] = blank as isize;
            if *id == OCR_NORMAL {
                installed_any = true;
            }
        }
        INSTALLED = installed_any;
        log(&format!(
            "system_cursor::install installed={INSTALLED} count={}",
            BLANK_HANDLES.iter().filter(|h| **h != 0).count()
        ));
        INSTALLED
    }
}

/// 恢复系统光标。
pub fn restore() {
    unsafe {
        if !INSTALLED {
            return;
        }
        let restored = SystemParametersInfoW(SPI_SETCURSORS, 0, std::ptr::null_mut(), SPIF_SENDCHANGE);
        log(&format!("Restore system cursors ok={restored}"));
        if restored == 0 {
            restore_default_cursors();
        }
        for h in BLANK_HANDLES.iter_mut() {
            if *h != 0 {
                DestroyCursor(*h);
                *h = 0;
            }
        }
        INSTALLED = false;
    }
}

fn restore_default_cursors() {
    unsafe {
        for id in CURSOR_IDS.iter() {
            let original = LoadCursorW(0, *id as *const u16);
            if original == 0 {
                continue;
            }
            let copy = CopyIcon(original);
            if copy != 0 {
                SetSystemCursor(copy, *id);
            }
        }
        log("Restored cursors from default system cursor handles.");
    }
}

fn create_blank_cursor() -> isize {
    // and_mask 全 1（所有像素透明），xor_mask 全 0
    let and_mask = [0xFFu8; 128];
    let xor_mask = [0u8; 128];
    unsafe {
        CreateCursor(
            0,
            0,
            0,
            32,
            32,
            and_mask.as_ptr() as *const _,
            xor_mask.as_ptr() as *const _,
        )
    }
}