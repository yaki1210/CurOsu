//! 尽力而为的日志：写入 %TEMP%\curosu.log。任何失败都不影响主程序。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

fn log_path() -> PathBuf {
    let mut guard = LOG.lock().unwrap();
    if let Some(p) = guard.as_ref() {
        return p.clone();
    }
    let mut p = std::env::temp_dir();
    p.push("curosu.log");
    *guard = Some(p.clone());
    p
}

/// 记录一条日志（尽力而为）。
pub fn log(message: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = format!("{now} {message}\n");
    let path = log_path();
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}