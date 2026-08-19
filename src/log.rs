//! 日志记录（当前已禁用）。原实现写入 %TEMP%\curosu.log。
//! 如需恢复：取消下方"启用时"区域的注释，并恢复上方 use 与 log_path/LOG。

/// 记录一条日志。当前为空实现，便于日后一个开关恢复。
pub fn log(_message: &str) {
    // ==== 启用时 ====
    // let now = SystemTime::now()
    //     .duration_since(UNIX_EPOCH)
    //     .map(|d| d.as_millis())
    //     .unwrap_or(0);
    // let line = format!("{now} {message}\n");
    // let path = log_path();
    // let _ = OpenOptions::new()
    //     .create(true)
    //     .append(true)
    //     .open(path)
    //     .and_then(|mut f| f.write_all(line.as_bytes()));
    // ==== 启用结束 ====
}