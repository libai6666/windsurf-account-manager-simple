use tauri::{command, AppHandle, Manager};
use serde_json::json;
use crate::services;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// 获取应用版本信息
#[command]
pub async fn get_app_version(app: AppHandle) -> Result<serde_json::Value, String> {
    let package_info = app.package_info();
    
    Ok(json!({
        "version": package_info.version.to_string(),
        "name": package_info.name,
        "authors": package_info.authors,
        "description": package_info.description
    }))
}

/// 获取应用标题（包含版本号）
#[command]
pub async fn get_app_title(app: AppHandle) -> Result<String, String> {
    let version = app.package_info().version.to_string();
    Ok(format!("windsurf-account-manager-simple v{}", version))
}

/// 重置HTTP客户端（用于从网络故障中恢复）
#[command]
pub async fn reset_http_client() -> Result<serde_json::Value, String> {
    services::rebuild_http_client();
    Ok(json!({
        "success": true,
        "message": "HTTP客户端已重置"
    }))
}

/// 获取日志目录（可执行文件同级目录下的 logs 文件夹）
fn get_log_dir() -> Result<PathBuf, String> {
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Failed to get executable path: {}", e))?;
    
    let exe_dir = exe_path.parent()
        .ok_or_else(|| "Failed to get executable directory".to_string())?;
    
    let log_dir = exe_dir.join("logs");
    fs::create_dir_all(&log_dir)
        .map_err(|e| format!("Failed to create log dir: {}", e))?;
    
    Ok(log_dir)
}

/// 获取日志文件路径
fn get_log_file_path(_app: &AppHandle) -> Result<PathBuf, String> {
    let log_dir = get_log_dir()?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    Ok(log_dir.join(format!("app_{}.log", today)))
}

/// 追加日志到文件
#[command]
pub async fn append_log_file(app: AppHandle, content: String) -> Result<(), String> {
    let log_path = get_log_file_path(&app)?;
    
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open log file: {}", e))?;
    
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write log: {}", e))?;
    
    Ok(())
}

/// 获取日志目录路径
#[command]
pub async fn get_log_directory(_app: AppHandle) -> Result<String, String> {
    let log_dir = get_log_dir()?;
    Ok(log_dir.to_string_lossy().to_string())
}
