use crate::utils::errors::AppError;
use base64::{Engine as _, engine::general_purpose};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct WindsurfCurrentInfo {
    pub email: Option<String>,
    pub name: Option<String>,
    pub api_key: Option<String>,
    pub plan_name: Option<String>,
    pub team_id: Option<String>,
    pub version: Option<String>,
    pub is_active: bool,
}

/// 旧版 windsurfAuthStatus 格式（含email/name字段）
#[derive(Debug, Serialize, Deserialize)]
struct WindsurfAuthStatusLegacy {
    name: Option<String>,
    #[serde(rename = "apiKey")]
    api_key: Option<String>,
    email: Option<String>,
    #[serde(rename = "teamId")]
    team_id: Option<String>,
    #[serde(rename = "planName")]
    plan_name: Option<String>,
}

/// 新版 windsurfAuthStatus 格式（protobuf编码用户信息）
#[derive(Debug, Serialize, Deserialize)]
struct WindsurfAuthStatusNew {
    #[serde(rename = "apiKey")]
    api_key: Option<String>,
    #[serde(rename = "userStatusProtoBinaryBase64")]
    user_status_proto: Option<String>,
}

/// windsurf.settings.cachedPlanInfo 格式
#[derive(Debug, Serialize, Deserialize)]
struct CachedPlanInfo {
    #[serde(rename = "planName")]
    plan_name: Option<String>,
}

/// 从 protobuf binary 中提取可打印字符串（简易解码，不依赖proto schema）
fn extract_strings_from_protobuf(data: &[u8]) -> Vec<String> {
    let mut strings = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        let wire_type = byte & 0x07;
        
        if wire_type == 2 {
            i += 1;
            if i >= data.len() { break; }
            
            let mut length: usize = 0;
            let mut shift = 0;
            while i < data.len() {
                let b = data[i] as usize;
                length |= (b & 0x7F) << shift;
                i += 1;
                shift += 7;
                if b & 0x80 == 0 { break; }
            }
            
            if length > 0 && length < 500 && i + length <= data.len() {
                let slice = &data[i..i + length];
                if let Ok(s) = std::str::from_utf8(slice) {
                    if s.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '@' || c == '.' || c == '-' || c == '_' || c == '+') {
                        if s.len() >= 2 {
                            strings.push(s.to_string());
                        }
                    }
                }
                i += length;
            }
        } else if wire_type == 0 {
            i += 1;
            while i < data.len() && data[i] & 0x80 != 0 { i += 1; }
            i += 1;
        } else {
            i += 1;
        }
    }
    strings
}

/// 从 windsurf_auth-{name}-usages keys 中找到最近使用的用户名
fn find_latest_auth_user(connection: &rusqlite::Connection) -> Option<String> {
    let mut stmt = connection.prepare(
        "SELECT key, value FROM ItemTable WHERE key LIKE 'windsurf_auth-%-usages'"
    ).ok()?;
    
    let rows: Vec<(String, String)> = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }).ok()?
    .filter_map(|r| r.ok())
    .collect();
    
    let mut latest_key: Option<String> = None;
    let mut latest_time: i64 = 0;
    
    for (key, value) in &rows {
        if let Ok(arr) = serde_json::from_str::<Vec<Value>>(value) {
            if let Some(first) = arr.first() {
                if let Some(t) = first.get("lastUsed").and_then(|v| v.as_i64()) {
                    if t > latest_time {
                        latest_time = t;
                        latest_key = Some(key.clone());
                    }
                }
            }
        }
    }
    
    latest_key.map(|k| {
        k.strip_prefix("windsurf_auth-")
            .and_then(|s| s.strip_suffix("-usages"))
            .unwrap_or(&k)
            .to_string()
    })
}

/// 获取当前Windsurf账号信息
#[tauri::command]
pub fn get_current_windsurf_info() -> Result<WindsurfCurrentInfo, AppError> {
    let appdata = std::env::var("APPDATA")
        .map_err(|e| AppError::Config(format!("Failed to get APPDATA: {}", e)))?;
    let db_path = PathBuf::from(appdata)
        .join("Windsurf")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb");
    
    info!("[WindsurfInfo] db_path: {}", db_path.display());
    
    if !db_path.exists() {
        warn!("[WindsurfInfo] state.vscdb not found");
        return Ok(WindsurfCurrentInfo {
            email: None, name: None, api_key: None, plan_name: None,
            team_id: None, version: None, is_active: false,
        });
    }
    
    let connection = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| AppError::Database(format!("Failed to open state.vscdb: {}", e)))?;
    
    // 读取 windsurfAuthStatus
    let auth_json = connection.query_row(
        "SELECT value FROM ItemTable WHERE key = 'windsurfAuthStatus'",
        [],
        |row| -> Result<String, rusqlite::Error> { row.get(0) },
    ).ok();
    
    // 读取版本
    let version = connection.query_row(
        "SELECT value FROM ItemTable WHERE key = 'windsurfChangelog/lastVersion'",
        [],
        |row| -> Result<String, rusqlite::Error> { row.get(0) },
    ).ok();
    
    // 读取 cachedPlanInfo
    let cached_plan = connection.query_row(
        "SELECT value FROM ItemTable WHERE key = 'windsurf.settings.cachedPlanInfo'",
        [],
        |row| -> Result<String, rusqlite::Error> { row.get(0) },
    ).ok();
    
    info!("[WindsurfInfo] auth_json present: {}, version: {:?}, cached_plan present: {}",
        auth_json.is_some(), version, cached_plan.is_some());
    
    if let Some(ref auth) = auth_json {
        // 日志：打印原始 auth JSON 的前 200 个字符（避免太长）
        let preview = if auth.len() > 200 { &auth[..200] } else { auth };
        info!("[WindsurfInfo] auth_json preview: {}", preview);
    }
    
    let mut info = WindsurfCurrentInfo {
        email: None, name: None, api_key: None, plan_name: None,
        team_id: None, version, is_active: false,
    };
    
    if let Some(ref auth) = auth_json {
        // 方式1: 尝试旧版格式（直接有 email/name 字段）
        if let Ok(legacy) = serde_json::from_str::<WindsurfAuthStatusLegacy>(auth) {
            if legacy.email.is_some() || legacy.name.is_some() {
                info!("[WindsurfInfo] Parsed LEGACY format: email={:?}, name={:?}", legacy.email, legacy.name);
                info.email = legacy.email;
                info.name = legacy.name;
                info.api_key = legacy.api_key;
                info.plan_name = legacy.plan_name;
                info.team_id = legacy.team_id;
                info.is_active = true;
            }
        }
        
        // 方式2: 新版格式（apiKey + userStatusProtoBinaryBase64）
        if !info.is_active {
            if let Ok(new_auth) = serde_json::from_str::<WindsurfAuthStatusNew>(auth) {
                if new_auth.api_key.is_some() {
                    info.api_key = new_auth.api_key;
                    info.is_active = true;
                    info!("[WindsurfInfo] Parsed NEW format with apiKey, has proto: {}", new_auth.user_status_proto.is_some());
                    
                    // 从 protobuf 提取用户名/邮箱
                    if let Some(ref proto_b64) = new_auth.user_status_proto {
                        if let Ok(proto_bytes) = general_purpose::STANDARD.decode(proto_b64) {
                            let strings = extract_strings_from_protobuf(&proto_bytes);
                            info!("[WindsurfInfo] Protobuf extracted strings: {:?}", strings);
                            for s in &strings {
                                if s.contains('@') {
                                    info.email = Some(s.clone());
                                    break;
                                }
                            }
                            for s in &strings {
                                if !s.contains('-') || s.contains(' ') {
                                    if !s.contains('@') {
                                        info.name = Some(s.clone());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    
                    // 如果protobuf没有提取到名字，从 windsurf_auth-{name}-usages 获取
                    if info.name.is_none() {
                        if let Some(name) = find_latest_auth_user(&connection) {
                            info!("[WindsurfInfo] Got username from windsurf_auth usages: {}", name);
                            info.name = Some(name);
                        }
                    }
                }
            }
        }
    } else {
        // windsurfAuthStatus 不存在，尝试从 windsurf_auth-{name}-usages 判断
        if let Some(name) = find_latest_auth_user(&connection) {
            info!("[WindsurfInfo] No windsurfAuthStatus, but found auth user: {}", name);
            info.name = Some(name);
            info.is_active = true;
        }
    }
    
    // 从 cachedPlanInfo 补充套餐信息
    if let Some(ref plan_json) = cached_plan {
        if let Ok(plan) = serde_json::from_str::<CachedPlanInfo>(plan_json) {
            if info.plan_name.is_none() {
                info.plan_name = plan.plan_name;
            }
        }
    }
    
    info!("[WindsurfInfo] Final result: email={:?}, name={:?}, plan={:?}, is_active={}, version={:?}",
        info.email, info.name, info.plan_name, info.is_active, info.version);
    
    Ok(info)
}
