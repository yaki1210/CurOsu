//! 开机自启：HKCU Run 键。移植自 C# AutoStartManager.cs。

use crate::log;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY_CURRENT_USER, KEY_READ,
    KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "Curosu";

/// 应用自启设置。返回是否成功。
pub fn apply(enabled: bool) -> bool {
    unsafe {
        let key_wide: Vec<u16> = RUN_KEY.encode_utf16().chain(std::iter::once(0)).collect();
        let value_wide: Vec<u16> = VALUE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hkey = 0;
        let rc = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key_wide.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_READ | KEY_SET_VALUE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        );
        if rc != 0 || hkey == 0 {
            log("autostart: RegCreateKeyExW failed");
            // 尝试只读打开
            let rc2 = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                key_wide.as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_SET_VALUE,
                std::ptr::null(),
                &mut hkey,
                std::ptr::null_mut(),
            );
            if rc2 != 0 {
                return false;
            }
        }
        let result = if enabled {
            let path = startup_path();
            if path.is_empty() {
                RegCloseKey(hkey);
                return false;
            }
            let path_wide = path.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
            RegSetValueExW(
                hkey,
                value_wide.as_ptr(),
                0,
                REG_SZ,
                path_wide.as_ptr() as *const u8,
                (path_wide.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(hkey, value_wide.as_ptr())
        };
        RegCloseKey(hkey);
        result == 0
    }
}

fn startup_path() -> String {
    let installed = r"C:\Program Files\OsuCursorRs\curosu.exe";
    if std::path::Path::new(installed).exists() {
        return installed.to_string();
    }
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}