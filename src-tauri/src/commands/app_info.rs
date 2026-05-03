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

/// 写入导出文件到指定路径
#[command]
pub async fn write_export_file(path: String, content: String) -> Result<(), String> {
    fs::write(&path, content.as_bytes())
        .map_err(|e| format!("Failed to write export file: {}", e))?;
    Ok(())
}

/// 获取日志目录路径
#[command]
pub async fn get_log_directory(_app: AppHandle) -> Result<String, String> {
    let log_dir = get_log_dir()?;
    Ok(log_dir.to_string_lossy().to_string())
}

#[command]
pub async fn detect_installed_browsers() -> Result<Vec<serde_json::Value>, String> {
    let mut browsers = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let mut candidates: Vec<(&str, PathBuf)> = Vec::new();

        if let Ok(program_files) = std::env::var("ProgramFiles") {
            candidates.push(("Chrome", PathBuf::from(&program_files).join("Google\\Chrome\\Application\\chrome.exe")));
            candidates.push(("Edge", PathBuf::from(&program_files).join("Microsoft\\Edge\\Application\\msedge.exe")));
        }
        if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
            candidates.push(("Chrome", PathBuf::from(&program_files_x86).join("Google\\Chrome\\Application\\chrome.exe")));
            candidates.push(("Edge", PathBuf::from(&program_files_x86).join("Microsoft\\Edge\\Application\\msedge.exe")));
        }
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            candidates.push(("Chrome", PathBuf::from(&local_app_data).join("Google\\Chrome\\Application\\chrome.exe")));
            candidates.push(("Edge", PathBuf::from(&local_app_data).join("Microsoft\\Edge\\Application\\msedge.exe")));
        }

        for (name, path) in candidates {
            if path.exists() {
                browsers.push(json!({
                    "name": name,
                    "path": path.to_string_lossy().to_string()
                }));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = vec![
            ("Chrome", PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")),
            ("Edge", PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge")),
        ];
        for (name, path) in candidates {
            if path.exists() {
                browsers.push(json!({
                    "name": name,
                    "path": path.to_string_lossy().to_string()
                }));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = vec![
            ("Chrome", PathBuf::from("/usr/bin/google-chrome")),
            ("Chromium", PathBuf::from("/usr/bin/chromium")),
            ("Edge", PathBuf::from("/usr/bin/microsoft-edge")),
        ];
        for (name, path) in candidates {
            if path.exists() {
                browsers.push(json!({
                    "name": name,
                    "path": path.to_string_lossy().to_string()
                }));
            }
        }
    }

    Ok(browsers)
}

#[command]
pub async fn reset_windsurf() -> Result<serde_json::Value, String> {
    let mut removed = Vec::new();
    let mut failed = Vec::new();
    let mut targets: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            targets.push(PathBuf::from(&appdata).join("Windsurf"));
            targets.push(PathBuf::from(&appdata).join("Codeium"));
        }
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            targets.push(PathBuf::from(&localappdata).join("Windsurf"));
        }
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            targets.push(PathBuf::from(&userprofile).join(".codeium"));
            targets.push(PathBuf::from(&userprofile).join(".windsurf"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            targets.push(PathBuf::from(&home).join("Library/Application Support/Windsurf"));
            targets.push(PathBuf::from(&home).join("Library/Application Support/Codeium"));
            targets.push(PathBuf::from(&home).join(".codeium"));
            targets.push(PathBuf::from(&home).join(".windsurf"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            targets.push(PathBuf::from(&home).join(".config/Windsurf"));
            targets.push(PathBuf::from(&home).join(".config/Codeium"));
            targets.push(PathBuf::from(&home).join(".codeium"));
            targets.push(PathBuf::from(&home).join(".windsurf"));
        }
    }

    targets.sort();
    targets.dedup();

    for target in targets {
        if !target.exists() {
            continue;
        }

        let result = if target.is_dir() {
            fs::remove_dir_all(&target)
        } else {
            fs::remove_file(&target)
        };

        match result {
            Ok(_) => removed.push(target.to_string_lossy().to_string()),
            Err(e) => failed.push(json!({
                "path": target.to_string_lossy().to_string(),
                "error": e.to_string()
            })),
        }
    }

    Ok(json!({
        "success": failed.is_empty(),
        "message": if failed.is_empty() { "Windsurf 已初始化" } else { "部分 Windsurf 数据初始化失败，请确认 Windsurf 已关闭后重试" },
        "removed": removed,
        "failed": failed
    }))
}
