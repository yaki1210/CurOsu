//! 托盘图标 + 弹出菜单。移植自 C# 的 NotifyIcon + ContextMenuStrip。

use crate::overlay::{MSG_EXIT, MSG_OPEN_SETTINGS, MSG_TOGGLE_CURSOR, WM_TRAY};
use crate::log;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, LoadIconW, PostMessageW, TrackPopupMenu, WM_APP,
    MF_STRING, TPM_CENTERALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN,
    TPM_VCENTERALIGN, CW_USEDEFAULT,
};

static mut ICON_LOADED: bool = false;

fn load_tray_icon() -> isize {
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        LoadIconW(hinst, 1 as *const u16)
    }
}

/// 添加托盘图标。返回是否成功。
pub fn add(hwnd: HWND) -> bool {
    unsafe {
        let icon = load_tray_icon();
        let tip: Vec<u16> = "osu! Cursor\0".encode_utf16().collect();
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: icon,
            szTip: [0u16; 128],
            ..Default::default()
        };
        for (i, c) in tip.iter().enumerate() {
            nid.szTip[i] = *c;
        }
        let ok = Shell_NotifyIconW(NIM_ADD, &nid) != 0;
        if ok {
            ICON_LOADED = true;
        } else {
            log("tray: Shell_NotifyIconW NIM_ADD failed");
        }
        ok
    }
}

/// 移除托盘图标。
pub fn remove(hwnd: HWND) {
    unsafe {
        if !ICON_LOADED {
            return;
        }
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            ..Default::default()
        };
        Shell_NotifyIconW(NIM_DELETE, &nid);
        ICON_LOADED = false;
    }
}

/// 弹出右键菜单。菜单项通过 PostMessage 反馈给窗口。
pub fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        let s1: Vec<u16> = "设置\0".encode_utf16().collect();
        let s2: Vec<u16> = "关闭光标\0".encode_utf16().collect();
        let s3: Vec<u16> = "退出\0".encode_utf16().collect();
        // 使用 MF_STRING，命令 ID 用 WM_APP + 序号
        windows_sys::Win32::UI::WindowsAndMessaging::AppendMenuW(
            menu,
            MF_STRING,
            WM_APP + 1,
            s1.as_ptr(),
        );
        windows_sys::Win32::UI::WindowsAndMessaging::AppendMenuW(
            menu,
            MF_STRING,
            WM_APP + 2,
            s2.as_ptr(),
        );
        windows_sys::Win32::UI::WindowsAndMessaging::AppendMenuW(
            menu,
            MF_STRING,
            WM_APP + 3,
            s3.as_ptr(),
        );
        // 显示菜单（TPM_RETURNCMD：返回值即所选命令 ID）
        let cmd = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_CENTERALIGN
                | TPM_VCENTERALIGN | TPM_RETURNCMD,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            hwnd,
            std::ptr::null(),
        );
        match cmd {
            WM_APP + 1 => PostMessageW(hwnd, MSG_OPEN_SETTINGS, 0, 0),
            WM_APP + 2 => PostMessageW(hwnd, MSG_TOGGLE_CURSOR, 0, 0),
            WM_APP + 3 => PostMessageW(hwnd, MSG_EXIT, 0, 0),
            _ => {}
        }
        windows_sys::Win32::UI::WindowsAndMessaging::DestroyMenu(menu);
    }
}