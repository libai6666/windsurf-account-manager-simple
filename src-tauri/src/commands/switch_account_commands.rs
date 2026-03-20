use crate::repository::DataStore;
use crate::utils::errors::{AppError, AppResult};
use chrono::Utc;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;
use std::path::PathBuf;

#[cfg(target_os = "windows")]
use winreg::{RegKey, enums::{HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS}};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Serialize, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    expires_in: String,
    token_type: String,
    refresh_token: String,
    id_token: String,
    user_id: String,
    project_id: String,
}

/// 使用refresh_token获取新的access_token
async fn refresh_access_token(refresh_token: &str) -> AppResult<GoogleTokenResponse> {
    // 使用专门用于 googleapis 的 HTTP 客户端（支持代理）
    let client = crate::services::get_google_api_client();
    
    // Google Token API
    let url = "https://securetoken.googleapis.com/v1/token";
    let api_key = "AIzaSyBPFmef6bkwMJAYP0sJZAi4k5XP1lXJXuY"; // Firebase API Key
    
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ];
    
    let response = client
        .post(&format!("{}?key={}", url, api_key))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    
    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        error!("Failed to refresh token: {}", error_text);
        return Err(AppError::ApiRequest(format!("Failed to refresh token: {}", error_text)));
    }
    
    let token_response = response.json::<GoogleTokenResponse>().await
        .map_err(|e| AppError::Network(e.to_string()))?;
    
    Ok(token_response)
}

/// 序列化Protobuf字符串（field 1, wire type 2）
fn serialize_protobuf_string(value: &str) -> Vec<u8> {
    if value.is_empty() {
        return vec![];
    }
    
    let value_bytes = value.as_bytes();
    let value_length = value_bytes.len();
    
    // Field 1, wire type 2 (length-delimited): (1 << 3) | 2 = 0x0A
    let mut result = vec![0x0A];
    
    // Encode length as varint
    let mut length = value_length;
    while length > 127 {
        result.push((length as u8 & 0x7F) | 0x80);
        length >>= 7;
    }
    result.push(length as u8 & 0x7F);
    
    // Append value bytes
    result.extend_from_slice(value_bytes);
    result
}

/// 反序列化Protobuf响应获取auth_token
fn deserialize_protobuf_response(data: &[u8]) -> Option<String> {
    if data.len() < 2 {
        return None;
    }
    
    let mut pos = 0;
    while pos < data.len() {
        // Read field tag
        let tag = data[pos];
        pos += 1;
        
        // Get wire type (low 3 bits)
        let wire_type = tag & 0x07;
        let field_number = tag >> 3;
        
        // If it's length-delimited type (wire_type = 2)
        if wire_type == 2 {
            // Read varint length
            let mut length = 0;
            let mut shift = 0;
            while pos < data.len() {
                let byte = data[pos];
                pos += 1;
                length |= ((byte & 0x7F) as usize) << shift;
                if byte & 0x80 == 0 {
                    break;
                }
                shift += 7;
            }
            
            // Read string content
            if pos + length <= data.len() {
                if let Ok(value) = std::str::from_utf8(&data[pos..pos + length]) {
                    // auth_token is typically field 1
                    if field_number == 1 && !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
                pos += length;
            }
        } else if wire_type == 0 {
            // Skip varint field
            while pos < data.len() {
                if data[pos] & 0x80 == 0 {
                    pos += 1;
                    break;
                }
                pos += 1;
            }
        } else {
            // Skip other types
            break;
        }
    }
    
    None
}

/// 使用access_token获取auth_token
async fn get_auth_token(access_token: &str) -> AppResult<String> {
    let client = reqwest::Client::new();
    
    // Windsurf GetOneTimeAuthToken endpoint
    let url = "https://web-backend.windsurf.com/exa.seat_management_pb.SeatManagementService/GetOneTimeAuthToken";
    
    // Serialize request as Protobuf
    let request_data = serialize_protobuf_string(access_token);
    
    let response = client
        .post(url)
        .header("Content-Type", "application/proto")
        .header("Accept", "application/proto")
        .header("User-Agent", "Windsurf/1.4.2")
        .body(request_data)
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    
    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        error!("Failed to get auth token: {}", error_text);
        return Err(AppError::ApiRequest(format!("Failed to get auth token: {}", error_text)));
    }
    
    // Deserialize response
    let response_bytes = response.bytes().await
        .map_err(|e| AppError::Network(e.to_string()))?;
    
    let auth_token = deserialize_protobuf_response(&response_bytes)
        .ok_or_else(|| AppError::ApiRequest("Failed to parse auth token from response".to_string()))?;
    
    info!("Successfully obtained auth token");
    Ok(auth_token)
}

/// 触发Windsurf回调URL以完成登录
async fn trigger_windsurf_callback(auth_token: &str) -> AppResult<()> {
    // 生成state参数
    let state = Uuid::new_v4().to_string();
    
    // 构建回调URL
    // windsurf://codeium.windsurf#access_token=<auth_token>&state=<state>&token_type=Bearer
    let params = [
        ("access_token", auth_token),
        ("state", &state),
        ("token_type", "Bearer"),
    ];
    
    let fragment = serde_urlencoded::to_string(&params)
        .map_err(|e| AppError::ApiRequest(format!("Failed to encode URL parameters: {}", e)))?;
    
    let callback_url = format!("windsurf://codeium.windsurf#{}", fragment);
    
    info!("Triggering Windsurf callback: {}", callback_url);
    
    // 使用系统默认程序打开URL（触发Windsurf处理）
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        // 使用 PowerShell 的 Start-Process 来正确处理包含特殊字符的 URL
        Command::new("powershell")
            .args(&["-NoProfile", "-Command", &format!("Start-Process '{}'", callback_url)])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| AppError::FileOperation(format!("Failed to open URL: {}", e)))?;
    }
    
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("open")
            .arg(&callback_url)
            .spawn()
            .map_err(|e| AppError::FileOperation(format!("Failed to open URL: {}", e)))?;
    }
    
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        Command::new("xdg-open")
            .arg(&callback_url)
            .spawn()
            .map_err(|e| AppError::FileOperation(format!("Failed to open URL: {}", e)))?;
    }
    
    info!("Successfully triggered Windsurf callback");
    Ok(())
}


/// 一键切换账号命令（简化版：使用回调URL登录）
#[tauri::command]
pub async fn switch_account(
    id: String,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<Value, String> {
    info!("Switching account: {}", id);
    
    let account_id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    
    // 获取账号信息
    let account = data_store
        .get_account(account_id)
        .await
        .map_err(|e| e.to_string())?;
    
    // 检查是否有refresh_token
    if account.refresh_token.is_none() || account.refresh_token.as_ref().unwrap().is_empty() {
        return Ok(json!({
            "success": false,
            "error": "账号没有refresh_token，请先登录"
        }));
    }
    
    let refresh_token = account.refresh_token.unwrap();
    
    // Step 1: 检查本地token是否有效
    let (access_token, expires_in) = if let (Some(token), Some(expires_at)) = (&account.token, &account.token_expires_at) {
        // 检查token是否还有至少5分钟有效期
        let now = Utc::now();
        let buffer = chrono::Duration::minutes(5);
        if *expires_at > now + buffer {
            info!("Using cached access token, expires at: {}", expires_at);
            let remaining_seconds = (*expires_at - now).num_seconds();
            (token.clone(), remaining_seconds.to_string())
        } else {
            info!("Token expired or expiring soon, refreshing...");
            let token_response = match refresh_access_token(&refresh_token).await {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Failed to refresh access token: {:?}", e);
                    return Ok(json!({
                        "success": false,
                        "error": format!("获取access_token失败: {}", e)
                    }));
                }
            };
            (token_response.access_token, token_response.expires_in)
        }
    } else {
        // 没有本地token，需要刷新
        info!("No cached token, refreshing access token...");
        let token_response = match refresh_access_token(&refresh_token).await {
            Ok(resp) => resp,
            Err(e) => {
                error!("Failed to refresh access token: {:?}", e);
                return Ok(json!({
                    "success": false,
                    "error": format!("获取access_token失败: {}", e)
                }));
            }
        };
        (token_response.access_token, token_response.expires_in)
    };
    
    // Step 2: 获取auth_token
    info!("Getting auth token...");
    let auth_token = match get_auth_token(&access_token).await {
        Ok(token) => token,
        Err(e) => {
            error!("Failed to get auth token: {:?}", e);
            return Ok(json!({
                "success": false,
                "error": format!("获取auth_token失败: {}", e)
            }));
        }
    };
    
    // Step 3: 尝试重置机器ID（可能需要管理员权限）
    info!("Attempting to reset machine ID...");
    let reset_result = reset_machine_id_internal().await;
    let machine_id_reset = match reset_result {
        Ok(_) => {
            info!("Machine ID reset successful");
            true
        },
        Err(e) => {
            warn!("Failed to reset machine ID: {:?}", e);
            warn!("重置机器ID失败，可能需要管理员权限。但切换账号仍可继续。");
            false
        }
    };
    
    // Step 4: 触发Windsurf回调URL以自动登录
    info!("Triggering Windsurf callback...");
    if let Err(e) = trigger_windsurf_callback(&auth_token).await {
        error!("Failed to trigger callback: {:?}", e);
        return Ok(json!({
            "success": false,
            "error": format!("触发Windsurf登录失败: {}", e)
        }));
    }
    
    // 更新账号的token信息
    let expires_at = Utc::now() + chrono::Duration::seconds(expires_in.parse::<i64>().unwrap_or(3600));
    if let Err(e) = data_store.update_account_token(
        account_id,
        access_token.clone(),
        expires_at
    ).await {
        error!("Failed to update account token: {:?}", e);
    }
    
    // 更新自动换号设置中的当前账号ID（手动切号时同步更新）
    if let Ok(mut current_settings) = data_store.get_settings().await {
        current_settings.auto_switch_current_account_id = Some(id.clone());
        let _ = data_store.update_settings(current_settings).await;
    }
    
    info!("Successfully triggered Windsurf login for account");
    
    Ok(json!({
        "success": true,
        "message": if machine_id_reset {
            "已成功触发Windsurf登录并重置机器ID"
        } else {
            "已成功触发Windsurf登录（未重置机器ID，可能需要管理员权限）"
        },
        "auth_token": auth_token,
        "machine_id_reset": machine_id_reset
    }))
}

/// 内部重置机器ID函数
async fn reset_machine_id_internal() -> AppResult<()> {
    use std::fs;
    use rand::Rng;
    
    // 生成新的机器ID（符合VSCode格式）
    let mut rng = rand::thread_rng();
    
    // machineId: 64位hex字符串（256位）
    let machine_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    let new_machine_id = hex::encode(&machine_bytes);
    
    // macMachineId: 32位hex字符串（MD5格式）
    let new_mac_machine_id = format!("{:032x}", rng.gen::<u128>());
    
    // sqmId: UUID格式，不带括号
    let new_sqm_id = Uuid::new_v4().to_string().to_uppercase();
    
    // devDeviceId: 标准UUID格式
    let new_device_id = Uuid::new_v4().to_string().to_lowercase();
    
    // 更新storage.json
    let mut storage_path = directories::BaseDirs::new()
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("C:/Users/Default/AppData/Roaming"));
    storage_path.push("Windsurf");
    storage_path.push("User");
    storage_path.push("globalStorage");
    storage_path.push("storage.json");
    
    if storage_path.exists() {
        let content = fs::read_to_string(&storage_path)
            .map_err(|e| AppError::FileOperation(format!("Failed to read storage.json: {}", e)))?;
        let mut storage: Value = serde_json::from_str(&content)
            .map_err(AppError::Serialization)?;
        
        storage["telemetry.machineId"] = json!(new_machine_id);
        storage["telemetry.macMachineId"] = json!(new_mac_machine_id);
        storage["telemetry.sqmId"] = json!(new_sqm_id);
        storage["telemetry.devDeviceId"] = json!(new_device_id);
        
        let updated = serde_json::to_string_pretty(&storage)
            .map_err(AppError::Serialization)?;
        fs::write(&storage_path, updated)
            .map_err(|e| AppError::FileOperation(format!("Failed to write storage.json: {}. 可能需要管理员权限", e)))?;
        
        info!("Updated storage.json with new machine IDs");
    } else {
        warn!("storage.json not found at {:?}", storage_path);
    }
    
    // Windows特定：更新注册表（程序启动时已要求管理员权限）
    #[cfg(target_os = "windows")]
    {
        // 只更新 HKEY_LOCAL_MACHINE 下的 Cryptography MachineGuid（需要管理员权限）
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        
        // 生成新的GUID（不带大括号的格式）
        let new_machine_guid = Uuid::new_v4().to_string().to_uppercase();
        
        match hklm.open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Cryptography",
            KEY_ALL_ACCESS
        ) {
            Ok(crypto_key) => {
                match crypto_key.set_value("MachineGuid", &new_machine_guid) {
                    Ok(()) => {
                        info!("Updated HKLM\\SOFTWARE\\Microsoft\\Cryptography\\MachineGuid to: {}", new_machine_guid);
                        Ok(())
                    }
                    Err(e) => {
                        let msg = format!("Failed to update MachineGuid: {}. 确保以管理员权限运行", e);
                        error!("{}", msg);
                        Err(AppError::FileOperation(msg))
                    }
                }
            }
            Err(e) => {
                let msg = format!("Failed to open HKLM\\SOFTWARE\\Microsoft\\Cryptography: {}. 需要管理员权限", e);
                error!("{}", msg);
                Err(AppError::FileOperation(msg))
            }
        }
    }
    
    // macOS特定：尝试重置系统级机器标识
    #[cfg(target_os = "macos")]
    {
        // macOS 的硬件 UUID 无法修改，但可以尝试重置一些软件级别的标识
        // 注意：某些操作可能需要 sudo 权限
        
        // 尝试删除 Windsurf 的本地缓存标识文件
        let home = std::env::var("HOME").unwrap_or_default();
        let cache_paths = vec![
            format!("{}/.config/Windsurf/machineid", home),
            format!("{}/Library/Application Support/Windsurf/.installerId", home),
        ];
        
        for cache_path in cache_paths {
            let path = PathBuf::from(&cache_path);
            if path.exists() {
                match fs::remove_file(&path) {
                    Ok(()) => info!("Removed cache file: {}", cache_path),
                    Err(e) => warn!("Failed to remove {}: {}", cache_path, e),
                }
            }
        }
        
        // 尝试重置系统级 machine-id（需要 sudo 权限）
        // /var/lib/dbus/machine-id 在 macOS 上通常不存在
        // 但某些应用可能会读取 IOPlatformUUID
        
        info!("macOS machine ID reset completed (software level only)");
        Ok(())
    }
    
    // Linux特定：尝试重置 /etc/machine-id 和 /var/lib/dbus/machine-id
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        
        // 生成新的 machine-id（32位hex字符串）
        let new_linux_machine_id = format!("{:032x}", rand::thread_rng().gen::<u128>());
        
        // 尝试更新 /etc/machine-id（需要 root 权限）
        let etc_machine_id = PathBuf::from("/etc/machine-id");
        if etc_machine_id.exists() {
            match fs::write(&etc_machine_id, format!("{}\n", new_linux_machine_id)) {
                Ok(()) => {
                    info!("Updated /etc/machine-id to: {}", new_linux_machine_id);
                }
                Err(e) => {
                    warn!("Failed to update /etc/machine-id: {}. 需要 sudo 权限", e);
                    // 尝试使用 sudo
                    let result = Command::new("sudo")
                        .args(["bash", "-c", &format!("echo '{}' > /etc/machine-id", new_linux_machine_id)])
                        .output();
                    match result {
                        Ok(output) if output.status.success() => {
                            info!("Updated /etc/machine-id via sudo");
                        }
                        _ => {
                            warn!("Could not update /etc/machine-id even with sudo");
                        }
                    }
                }
            }
        }
        
        // 尝试更新 /var/lib/dbus/machine-id（通常是 /etc/machine-id 的符号链接）
        let dbus_machine_id = PathBuf::from("/var/lib/dbus/machine-id");
        if dbus_machine_id.exists() && !dbus_machine_id.is_symlink() {
            match fs::write(&dbus_machine_id, format!("{}\n", new_linux_machine_id)) {
                Ok(()) => {
                    info!("Updated /var/lib/dbus/machine-id");
                }
                Err(e) => {
                    warn!("Failed to update /var/lib/dbus/machine-id: {}", e);
                }
            }
        }
        
        // 尝试删除 Windsurf 的本地缓存标识文件
        let home = std::env::var("HOME").unwrap_or_default();
        let cache_paths = vec![
            format!("{}/.config/Windsurf/machineid", home),
            format!("{}/.local/share/Windsurf/.installerId", home),
        ];
        
        for cache_path in cache_paths {
            let path = PathBuf::from(&cache_path);
            if path.exists() {
                match fs::remove_file(&path) {
                    Ok(()) => info!("Removed cache file: {}", cache_path),
                    Err(e) => warn!("Failed to remove {}: {}", cache_path, e),
                }
            }
        }
        
        info!("Linux machine ID reset completed");
        Ok(())
    }
}

/// 重置机器ID命令（供前端调用）
#[tauri::command]
pub async fn reset_machine_id() -> Result<Value, String> {
    match reset_machine_id_internal().await {
        Ok(()) => Ok(json!({
            "success": true,
            "message": "机器ID重置成功"
        })),
        Err(e) => Ok(json!({
            "success": false,
            "message": format!("机器ID重置失败: {}", e)
        }))
    }
}

#[cfg(target_os = "windows")]
pub fn is_elevated() -> bool {
    use std::ptr;
    use winapi::um::securitybaseapi::GetTokenInformation;
    use winapi::um::winnt::{TokenElevation, HANDLE, TOKEN_ELEVATION, TOKEN_QUERY};
    use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
    use winapi::um::handleapi::CloseHandle;
    
    unsafe {
        let mut token_handle: HANDLE = ptr::null_mut();
        
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &mut token_handle
        ) == 0 {
            return false;
        }
        
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut size = 0u32;
        
        let result = GetTokenInformation(
            token_handle,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size
        );
        
        CloseHandle(token_handle);
        
        result != 0 && elevation.TokenIsElevated != 0
    }
}

/// 检查应用程序是否以管理员/root权限运行
#[tauri::command]
pub async fn check_admin_privileges() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        Ok(is_elevated())
    }
    
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        // Unix系统：检查 euid 是否为 0 (root)
        Ok(is_root())
    }
}

/// 检查是否以 root 权限运行 (Unix)
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// 自动换号检测命令
/// 检查当前账号的每日配额和每周配额，满足以下任一条件时自动切换：
/// 1. 每日配额低于阈值
/// 2. 每周配额为0（即使日配额充足）
/// 候选账号需同时满足：周配额>0 且 日配额>阈值
#[tauri::command]
pub async fn check_auto_switch(
    data_store: State<'_, Arc<DataStore>>,
) -> Result<Value, String> {
    let settings = data_store.get_settings().await.map_err(|e| e.to_string())?;
    
    // 检查是否启用了自动换号
    if !settings.auto_switch_enabled || !settings.seamless_switch_enabled {
        return Ok(json!({
            "action": "skip",
            "reason": "自动换号未启用"
        }));
    }
    
    let group = &settings.auto_switch_group;
    let threshold = settings.auto_switch_threshold;
    
    // 获取当前正在使用的账号ID
    let current_id_str = match &settings.auto_switch_current_account_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return Ok(json!({
                "action": "skip",
                "reason": "未设置当前使用的账号，请先手动切换一次账号"
            }));
        }
    };
    
    let current_uuid = Uuid::parse_str(&current_id_str).map_err(|e| e.to_string())?;
    
    // 获取当前账号信息
    let current_account = match data_store.get_account(current_uuid).await {
        Ok(acc) => acc,
        Err(_) => {
            return Ok(json!({
                "action": "skip",
                "reason": "当前账号不存在，请重新设置"
            }));
        }
    };
    
    // 先刷新当前账号的配额信息（日配额+周配额）
    let windsurf_service = crate::services::windsurf_service::WindsurfService::new();
    let mut current_daily_remaining = current_account.daily_quota_remaining.unwrap_or(100);
    let mut current_weekly_remaining = current_account.weekly_quota_remaining.unwrap_or(100);
    
    if let Some(ref token) = current_account.token {
        if let Ok(result) = windsurf_service.get_plan_status(token).await {
            if let Some(plan_status) = result.get("plan_status") {
                if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                    current_daily_remaining = v as i32;
                }
                if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                    current_weekly_remaining = v as i32;
                }
                // 更新数据库
                let mut updated = current_account.clone();
                updated.daily_quota_remaining = Some(current_daily_remaining);
                updated.weekly_quota_remaining = Some(current_weekly_remaining);
                if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                    updated.daily_quota_reset = Some(v);
                }
                if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                    updated.weekly_quota_reset = Some(v);
                }
                updated.last_quota_update = Some(chrono::Utc::now());
                let _ = data_store.update_account(updated).await;
            }
        }
    }
    
    println!("[自动换号] 当前账号: {}, 每日配额剩余: {}%, 每周配额剩余: {}%, 阈值: {}%", 
        current_account.email, current_daily_remaining, current_weekly_remaining, threshold);
    
    // 检查是否需要切换（满足任一条件即触发）：
    // 1. 每日配额 <= 阈值
    // 2. 每周配额 == 0（即使日配额充足）
    let need_switch_daily = current_daily_remaining <= threshold;
    let need_switch_weekly = current_weekly_remaining <= 0;
    let switch_reason = if need_switch_daily && need_switch_weekly {
        format!("日配额不足 ({}% <= {}%) 且周配额耗尽 ({}%)", current_daily_remaining, threshold, current_weekly_remaining)
    } else if need_switch_daily {
        format!("日配额不足 ({}% <= {}%)", current_daily_remaining, threshold)
    } else if need_switch_weekly {
        format!("周配额耗尽 ({}%)，即使日配额充足 ({}%)", current_weekly_remaining, current_daily_remaining)
    } else {
        String::new()
    };
    
    if !need_switch_daily && !need_switch_weekly {
        return Ok(json!({
            "action": "skip",
            "reason": format!("当前账号配额充足 (日{}% > {}%, 周{}%)", current_daily_remaining, threshold, current_weekly_remaining),
            "current_account": current_account.email,
            "daily_remaining": current_daily_remaining,
            "weekly_remaining": current_weekly_remaining
        }));
    }
    
    // 需要切换，从分组中查找配额充足的账号
    println!("[自动换号] {}，从分组 '{}' 中查找可用账号...", switch_reason, group);
    
    let all_accounts = data_store.get_all_accounts().await.map_err(|e| e.to_string())?;
    let group_accounts: Vec<_> = all_accounts.iter()
        .filter(|a| {
            a.group.as_deref() == Some(group) 
            && a.id != current_uuid
            && !matches!(a.status, crate::models::AccountStatus::Error(_))
            && a.refresh_token.is_some()
        })
        .collect();
    
    if group_accounts.is_empty() {
        return Ok(json!({
            "action": "no_candidate",
            "reason": format!("分组 '{}' 中没有其他可用账号", group),
            "current_account": current_account.email,
            "daily_remaining": current_daily_remaining
        }));
    }
    
    // 辅助函数：判断是否为Free计划（优先级最低）
    let is_free_plan = |acc: &crate::models::Account| -> bool {
        acc.plan_name.as_ref().map(|p| p.to_lowercase().contains("free")).unwrap_or(true)
    };
    
    // 候选号比较逻辑：非Free优先，然后日配额最高
    let is_better_candidate = |new_daily: i32, new_is_free: bool, cur: &Option<(Uuid, String, i32, i32, bool)>| -> bool {
        match cur {
            None => true,
            Some((_, _, cur_daily, _, cur_is_free)) => {
                if !new_is_free && *cur_is_free {
                    return true;
                }
                if new_is_free && !*cur_is_free {
                    return false;
                }
                new_daily > *cur_daily
            }
        }
    };
    
    // 查找配额最高的账号（候选条件：周配额>0 且 日配额>阈值，Free账号优先级最低）
    let mut best_candidate: Option<(Uuid, String, i32, i32, bool)> = None;
    
    for acc in &group_accounts {
        let daily = acc.daily_quota_remaining.unwrap_or(0);
        let weekly = acc.weekly_quota_remaining.unwrap_or(0);
        let acc_is_free = is_free_plan(acc);
        if daily > threshold && weekly > 0 {
            if is_better_candidate(daily, acc_is_free, &best_candidate) {
                best_candidate = Some((acc.id, acc.email.clone(), daily, weekly, acc_is_free));
            }
        }
    }
    
    // 如果没有找到已缓存的合适账号，逐个刷新分组账号配额再验证
    if best_candidate.is_none() {
        let cache_ttl = chrono::Duration::minutes(3);
        let now = chrono::Utc::now();
        let mut refreshed_count = 0;
        let mut skipped_count = 0;
        
        println!("[自动换号] 缓存数据中未找到合适账号，逐个刷新分组账号配额...");
        for acc in &group_accounts {
            if let Some(last_update) = acc.last_quota_update {
                if now - last_update < cache_ttl {
                    skipped_count += 1;
                    let daily = acc.daily_quota_remaining.unwrap_or(0);
                    let weekly = acc.weekly_quota_remaining.unwrap_or(0);
                    let acc_is_free = is_free_plan(acc);
                    if daily > threshold && weekly > 0 {
                        if is_better_candidate(daily, acc_is_free, &best_candidate) {
                            best_candidate = Some((acc.id, acc.email.clone(), daily, weekly, acc_is_free));
                        }
                    }
                    continue;
                }
            }
            
            if let Some(ref token) = acc.token {
                if let Ok(result) = windsurf_service.get_plan_status(token).await {
                    if let Some(plan_status) = result.get("plan_status") {
                        let daily = plan_status.get("daily_quota_remaining")
                            .and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let weekly = plan_status.get("weekly_quota_remaining")
                            .and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        
                        // 更新数据库
                        let mut updated = (*acc).clone();
                        updated.daily_quota_remaining = Some(daily);
                        updated.weekly_quota_remaining = Some(weekly);
                        if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                            updated.daily_quota_reset = Some(v);
                        }
                        if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                            updated.weekly_quota_reset = Some(v);
                        }
                        updated.last_quota_update = Some(now);
                        let _ = data_store.update_account(updated).await;
                        refreshed_count += 1;
                        
                        let acc_is_free = is_free_plan(acc);
                        println!("[自动换号] 刷新账号 {}: 日{}%, 周{}%{}", acc.email, daily, weekly, if acc_is_free { " (Free)" } else { "" });
                        
                        if daily > threshold && weekly > 0 {
                            if is_better_candidate(daily, acc_is_free, &best_candidate) {
                                best_candidate = Some((acc.id, acc.email.clone(), daily, weekly, acc_is_free));
                            }
                        }
                    }
                }
            }
        }
        println!("[自动换号] 刷新完成: 实际刷新 {} 个, 跳过(缓存有效) {} 个", refreshed_count, skipped_count);
    }
    
    let (target_id, target_email, target_daily, target_weekly, _is_free) = match best_candidate {
        Some(c) => c,
        None => {
            return Ok(json!({
                "action": "no_candidate",
                "reason": format!("分组 '{}' 中没有配额充足的账号 (需日配额>{}% 且 周配额>0%)", group, threshold),
                "current_account": current_account.email,
                "daily_remaining": current_daily_remaining,
                "weekly_remaining": current_weekly_remaining
            }));
        }
    };
    
    println!("[自动换号] 找到目标账号: {} (日配额: {}%, 周配额: {}%)，开始切换...", target_email, target_daily, target_weekly);
    
    // 执行切换
    let target_account = data_store.get_account(target_id).await.map_err(|e| e.to_string())?;
    
    let refresh_token = match &target_account.refresh_token {
        Some(rt) if !rt.is_empty() => rt.clone(),
        _ => {
            return Ok(json!({
                "action": "error",
                "reason": format!("目标账号 {} 没有refresh_token", target_email)
            }));
        }
    };
    
    // 获取 access token
    let (access_token, expires_in) = if let (Some(token), Some(expires_at)) = (&target_account.token, &target_account.token_expires_at) {
        let now = Utc::now();
        let buffer = chrono::Duration::minutes(5);
        if *expires_at > now + buffer {
            let remaining = (*expires_at - now).num_seconds();
            (token.clone(), remaining.to_string())
        } else {
            match refresh_access_token(&refresh_token).await {
                Ok(resp) => (resp.access_token, resp.expires_in),
                Err(e) => {
                    return Ok(json!({
                        "action": "error",
                        "reason": format!("刷新目标账号token失败: {}", e)
                    }));
                }
            }
        }
    } else {
        match refresh_access_token(&refresh_token).await {
            Ok(resp) => (resp.access_token, resp.expires_in),
            Err(e) => {
                return Ok(json!({
                    "action": "error",
                    "reason": format!("获取目标账号token失败: {}", e)
                }));
            }
        }
    };
    
    // 获取 auth_token
    let auth_token = match get_auth_token(&access_token).await {
        Ok(token) => token,
        Err(e) => {
            return Ok(json!({
                "action": "error",
                "reason": format!("获取auth_token失败: {}", e)
            }));
        }
    };
    
    // 重置机器ID
    let _ = reset_machine_id_internal().await;
    
    // 触发 Windsurf 回调
    if let Err(e) = trigger_windsurf_callback(&auth_token).await {
        return Ok(json!({
            "action": "error",
            "reason": format!("触发Windsurf登录失败: {}", e)
        }));
    }
    
    // 更新目标账号的token信息
    let expires_at = Utc::now() + chrono::Duration::seconds(expires_in.parse::<i64>().unwrap_or(3600));
    let _ = data_store.update_account_token(target_id, access_token, expires_at).await;
    
    // 更新设置中的当前账号ID
    let mut new_settings = settings.clone();
    new_settings.auto_switch_current_account_id = Some(target_id.to_string());
    let _ = data_store.update_settings(new_settings).await;
    
    println!("[自动换号] 成功切换到账号: {}", target_email);
    
    Ok(json!({
        "action": "switched",
        "reason": switch_reason,
        "from_account": current_account.email,
        "from_daily_remaining": current_daily_remaining,
        "from_weekly_remaining": current_weekly_remaining,
        "to_account": target_email,
        "to_account_id": target_id.to_string(),
        "to_daily_remaining": target_daily,
        "to_weekly_remaining": target_weekly
    }))
}
