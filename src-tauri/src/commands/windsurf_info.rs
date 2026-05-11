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
            // Length-delimited field, next byte(s) is the length (varint)
            i += 1;
            if i >= data.len() { break; }
            
            // 解码 varint 长度
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
                // 检查是否是有效的 UTF-8 可打印字符串
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
            // Varint, skip
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

/// 获取 Windsurf 数据库路径（跨平台）
fn get_windsurf_db_path() -> Result<PathBuf, AppError> {
    let main_dir = crate::models::main_user_data_dir()
        .ok_or_else(|| AppError::Config("Failed to resolve Windsurf main user data dir".to_string()))?;
    Ok(state_vscdb_path_for(&main_dir))
}

fn state_vscdb_path_for(user_data_dir: &std::path::Path) -> PathBuf {
    user_data_dir
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

pub fn get_windsurf_info_from_dir(user_data_dir: &std::path::Path) -> Result<WindsurfCurrentInfo, AppError> {
    let db_path = state_vscdb_path_for(user_data_dir);
    read_current_info(&db_path)
}

/// 获取当前Windsurf账号信息
#[tauri::command]
pub fn get_current_windsurf_info() -> Result<WindsurfCurrentInfo, AppError> {
    let db_path = get_windsurf_db_path()?;
    read_current_info(&db_path)
}

fn read_current_info(db_path: &std::path::Path) -> Result<WindsurfCurrentInfo, AppError> {
    if !db_path.exists() {
        return Ok(WindsurfCurrentInfo {
            email: None, name: None, api_key: None, plan_name: None,
            team_id: None, version: None, is_active: false,
        });
    }
    
    let connection = rusqlite::Connection::open_with_flags(
        db_path,
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
    
    // 读取 cachedPlanInfo（新版套餐信息）
    let cached_plan = connection.query_row(
        "SELECT value FROM ItemTable WHERE key = 'windsurf.settings.cachedPlanInfo'",
        [],
        |row| -> Result<String, rusqlite::Error> { row.get(0) },
    ).ok();
    
    let mut info = WindsurfCurrentInfo {
        email: None, name: None, api_key: None, plan_name: None,
        team_id: None, version, is_active: false,
    };
    
    if let Some(ref auth) = auth_json {
        // 方式1: 尝试旧版格式（直接有 email/name 字段）
        if let Ok(legacy) = serde_json::from_str::<WindsurfAuthStatusLegacy>(auth) {
            if legacy.email.is_some() || legacy.name.is_some() {
                info!("Parsed legacy auth format with email/name");
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
                    info!("Parsed new auth format with apiKey");
                    
                    // 从 protobuf 提取用户名
                    if let Some(ref proto_b64) = new_auth.user_status_proto {
                        if let Ok(proto_bytes) = general_purpose::STANDARD.decode(proto_b64) {
                            let strings = extract_strings_from_protobuf(&proto_bytes);
                            info!("Extracted strings from protobuf: {:?}", strings);
                            // 第一个有意义的字符串通常是用户名
                            for s in &strings {
                                if s.contains('@') {
                                    // 看起来像email
                                    info.email = Some(s.clone());
                                    break;
                                }
                            }
                            // 找用户名（非UUID、非email的第一个字符串）
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
                            info!("Got username from windsurf_auth usages: {}", name);
                            info.name = Some(name);
                        }
                    }
                }
            }
        }
    } else {
        // windsurfAuthStatus 不存在，尝试从 windsurf_auth-{name}-usages 判断是否有登录
        if let Some(name) = find_latest_auth_user(&connection) {
            info!("No windsurfAuthStatus, but found auth user: {}", name);
            info.name = Some(name);
            info.is_active = true;
        }
    }
    
    // 从 cachedPlanInfo 补充套餐信息（新版）
    if let Some(ref plan_json) = cached_plan {
        if let Ok(plan) = serde_json::from_str::<CachedPlanInfo>(plan_json) {
            // 只在还没有 plan_name 时才覆盖
            if info.plan_name.is_none() {
                info.plan_name = plan.plan_name;
            }
        }
    }
    
    if info.is_active {
        info!("Windsurf info: name={:?}, email={:?}, plan={:?}, version={:?}",
            info.name, info.email, info.plan_name, info.version);
    } else {
        warn!("Could not detect active Windsurf session");
    }
    
    Ok(info)
}
