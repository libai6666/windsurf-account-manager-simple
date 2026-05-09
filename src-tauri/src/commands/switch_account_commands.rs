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
    let api_key = "AIzaSyDsOl-1XpT5err0Tcnx8FFod1H8gVGIycY"; // Firebase API Key (与auth_service保持一致)
    
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ];
    
    let response = client
        .post(&format!("{}?key={}", url, api_key))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Referer", "https://windsurf.com/")
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

/// RegisterUser 响应数据
pub(crate) struct RegisterUserResult {
    pub(crate) api_key: String,
    pub(crate) name: String,
    pub(crate) api_server_url: String,
}

/// 解析 RegisterUser protobuf 响应
fn parse_register_user_response(data: &[u8]) -> Option<RegisterUserResult> {
    let mut api_key = None;
    let mut name = None;
    let mut api_server_url = None;
    let mut pos = 0;
    while pos < data.len() {
        let tag = data[pos]; pos += 1;
        let wire_type = tag & 0x07;
        let field_number = tag >> 3;
        if wire_type == 2 {
            let mut length = 0usize; let mut shift = 0;
            while pos < data.len() {
                let byte = data[pos]; pos += 1;
                length |= ((byte & 0x7F) as usize) << shift;
                if byte & 0x80 == 0 { break; }
                shift += 7;
            }
            if pos + length <= data.len() {
                if let Ok(value) = std::str::from_utf8(&data[pos..pos + length]) {
                    match field_number {
                        1 => api_key = Some(value.to_string()),
                        2 => name = Some(value.to_string()),
                        3 => api_server_url = Some(value.to_string()),
                        _ => {}
                    }
                }
                pos += length;
            } else { break; }
        } else if wire_type == 0 {
            while pos < data.len() { if data[pos] & 0x80 == 0 { pos += 1; break; } pos += 1; }
        } else { break; }
    }
    Some(RegisterUserResult {
        api_key: api_key?,
        name: name.unwrap_or_default(),
        api_server_url: api_server_url.unwrap_or_else(|| "https://server.codeium.com".to_string()),
    })
}

/// 调用 register.windsurf.com/RegisterUser 获取 apiKey
async fn call_register_user(id_token: &str) -> AppResult<RegisterUserResult> {
    let client = reqwest::Client::new();
    let register_url = "https://register.windsurf.com/exa.seat_management_pb.SeatManagementService/RegisterUser";
    let request_data = serialize_protobuf_string(id_token);
    
    let response = client
        .post(register_url)
        .header("Content-Type", "application/proto")
        .header("Accept", "application/proto")
        .header("Connect-Protocol-Version", "1")
        .header("User-Agent", "connect-es/1.6.1")
        .body(request_data)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("RegisterUser request failed: {}", e)))?;
    
    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::ApiRequest(format!("RegisterUser failed ({}): {}", status, &error_text[..std::cmp::min(error_text.len(), 500)])));
    }
    
    let response_bytes = response.bytes().await
        .map_err(|e| AppError::Network(e.to_string()))?;
    
    parse_register_user_response(&response_bytes)
        .ok_or_else(|| AppError::ApiRequest("Failed to parse RegisterUser response".to_string()))
}

pub(crate) struct SwitchAuthResult {
    pub(crate) register_result: RegisterUserResult,
    pub(crate) callback_token: String,
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_in: String,
}

async fn get_auth_token_from_auth_result(
    auth_service: &crate::services::auth_service::AuthService,
    auth_result: crate::services::auth_service::WindsurfAuthResult,
) -> AppResult<SwitchAuthResult> {
    let register_result = call_register_user(&auth_result.ott).await?;
    info!("RegisterUser SUCCESS (via devin-auth): apiKey={}..., name={}, server={}", 
        &register_result.api_key[..std::cmp::min(register_result.api_key.len(), 20)],
        register_result.name,
        register_result.api_server_url);
    
    let callback_ott = auth_service
        .get_fresh_ott(&auth_result.session_token, Some(&auth_result.auth1_token))
        .await?;
    
    Ok(SwitchAuthResult {
        register_result,
        callback_token: callback_ott,
        access_token: auth_result.session_token,
        refresh_token: Some(auth_result.auth1_token),
        expires_in: "3600".to_string(),
    })
}

/// 获取 auth token 并调用 RegisterUser
/// 支持两种 refresh_token 类型：
/// - auth1_... : Windsurf 2.0 devin-auth token → WindsurfPostAuth → GetOneTimeAuthToken → RegisterUser
/// - 其他 : Firebase refresh token → securetoken refresh → RegisterUser
async fn get_auth_token(refresh_token: &str) -> AppResult<SwitchAuthResult> {
    if refresh_token.starts_with("auth1_") {
        // Windsurf 2.0 devin-auth 流程
        info!("Using devin-auth flow (auth1_ token detected)...");
        let auth_service = crate::services::auth_service::AuthService::new();
        let auth_result = auth_service.refresh_ott(refresh_token).await?;
        get_auth_token_from_auth_result(&auth_service, auth_result).await
    } else {
        // 传统 Firebase refresh token 流程
        let token_response = refresh_access_token(refresh_token).await?;
        info!("Successfully obtained Firebase ID token, calling RegisterUser...");
        
        let register_result = call_register_user(&token_response.id_token).await?;
        info!("RegisterUser SUCCESS: apiKey={}..., name={}, server={}", 
            &register_result.api_key[..std::cmp::min(register_result.api_key.len(), 20)],
            register_result.name,
            register_result.api_server_url);
        
        Ok(SwitchAuthResult {
            register_result,
            callback_token: token_response.id_token,
            access_token: token_response.access_token,
            refresh_token: Some(token_response.refresh_token),
            expires_in: token_response.expires_in,
        })
    }
}

pub(crate) async fn get_auth_token_for_account(
    store: &Arc<DataStore>,
    account_id: Uuid,
    email: &str,
    refresh_token: &str,
) -> AppResult<SwitchAuthResult> {
    match get_auth_token(refresh_token).await {
        Ok(result) => Ok(result),
        Err(e) if refresh_token.starts_with("auth1_") => {
            warn!("auth1 refresh failed for {}, retrying with password login: {}", email, e);
            let password = store.get_decrypted_password(account_id).await?;
            let auth_service = crate::services::auth_service::AuthService::new();
            let auth_result = auth_service.sign_in_v2(email, &password).await?;
            get_auth_token_from_auth_result(&auth_service, auth_result).await
        }
        Err(e) => Err(e),
    }
}

/// 直接写入指定 Windsurf 实例的 state.vscdb 完成账号切换（绕过回调URL）
/// `user_data_dir` 决定写入哪个实例：主实例 = main_user_data_dir()，分身 = profile.user_data_dir
/// 调用前必须确保该实例进程已退出，否则 state.vscdb 可能被锁
#[cfg(target_os = "windows")]
pub(crate) fn write_windsurf_auth_direct(
    api_key: &str,
    name: &str,
    api_server_url: &str,
    user_data_dir: &std::path::Path,
) -> AppResult<()> {
    use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
    use aes_gcm::aead::generic_array::GenericArray;
    use rand::RngCore;
    
    // 1. 读取 AES 密钥 (从该实例的 Local State, DPAPI 保护)
    let local_state_path = user_data_dir.join("Local State");
    let local_state_content = std::fs::read_to_string(&local_state_path)
        .map_err(|e| AppError::FileOperation(format!("Failed to read Local State: {}", e)))?;
    let local_state: serde_json::Value = serde_json::from_str(&local_state_content)
        .map_err(|e| AppError::Config(format!("Failed to parse Local State: {}", e)))?;
    let enc_key_b64 = local_state["os_crypt"]["encrypted_key"]
        .as_str()
        .ok_or_else(|| AppError::Config("No encrypted_key in Local State".to_string()))?;
    
    use base64::{Engine, engine::general_purpose};
    let enc_key_raw = general_purpose::STANDARD.decode(enc_key_b64)
        .map_err(|e| AppError::Config(format!("Failed to decode encrypted_key: {}", e)))?;
    
    // Strip 'DPAPI' prefix (5 bytes)
    if enc_key_raw.len() < 6 || &enc_key_raw[..5] != b"DPAPI" {
        return Err(AppError::Config("Invalid encrypted_key format".to_string()));
    }
    let dpapi_data = &enc_key_raw[5..];
    
    // DPAPI decrypt
    let aes_key = dpapi_decrypt(dpapi_data)?;
    if aes_key.len() != 32 {
        return Err(AppError::Config(format!("Invalid AES key length: {}", aes_key.len())));
    }
    info!("Got AES key from DPAPI ({} bytes)", aes_key.len());
    
    // 2. 构建新 session JSON
    let session_id = Uuid::new_v4().to_string();
    let session_json = serde_json::json!([{
        "id": session_id,
        "accessToken": api_key,
        "account": {"label": name, "id": name},
        "scopes": []
    }]);
    let session_str = serde_json::to_string(&session_json)
        .map_err(|e| AppError::Config(format!("Failed to serialize session: {}", e)))?;
    
    // 3. AES-256-GCM 加密
    let cipher = Aes256Gcm::new(GenericArray::from_slice(&aes_key));
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = GenericArray::from_slice(&nonce_bytes);
    
    let encrypted_session = cipher.encrypt(nonce, session_str.as_bytes())
        .map_err(|e| AppError::Config(format!("AES encryption failed: {}", e)))?;
    
    // v10 prefix + nonce + ciphertext
    let mut enc_blob = Vec::with_capacity(3 + 12 + encrypted_session.len());
    enc_blob.extend_from_slice(b"v10");
    enc_blob.extend_from_slice(&nonce_bytes);
    enc_blob.extend_from_slice(&encrypted_session);
    
    // 转为 JSON Buffer 格式 (VS Code 的存储格式)
    let buffer_json = serde_json::json!({
        "type": "Buffer",
        "data": enc_blob.iter().map(|&b| b as u64).collect::<Vec<u64>>()
    });
    let session_db_value = serde_json::to_string(&buffer_json)
        .map_err(|e| AppError::Config(format!("Failed to serialize buffer: {}", e)))?;
    
    // 4. 加密 apiServerUrl
    let mut nonce_bytes2 = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes2);
    let nonce2 = GenericArray::from_slice(&nonce_bytes2);
    let encrypted_url = cipher.encrypt(nonce2, api_server_url.as_bytes())
        .map_err(|e| AppError::Config(format!("AES encryption of apiServerUrl failed: {}", e)))?;
    
    let mut enc_blob2 = Vec::with_capacity(3 + 12 + encrypted_url.len());
    enc_blob2.extend_from_slice(b"v10");
    enc_blob2.extend_from_slice(&nonce_bytes2);
    enc_blob2.extend_from_slice(&encrypted_url);
    
    let url_buffer_json = serde_json::json!({
        "type": "Buffer",
        "data": enc_blob2.iter().map(|&b| b as u64).collect::<Vec<u64>>()
    });
    let url_db_value = serde_json::to_string(&url_buffer_json)
        .map_err(|e| AppError::Config(format!("Failed to serialize url buffer: {}", e)))?;
    
    // 5. 更新 windsurfAuthStatus (只更新 apiKey，保留其他字段)
    let db_path = user_data_dir
        .join("User")
        .join("globalStorage")
        .join("state.vscdb");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::FileOperation(format!("Failed to create globalStorage dir: {}", e)))?;
    }
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| AppError::Database(format!("Failed to open state.vscdb: {}", e)))?;
    // 空数据库时 ItemTable 不存在，主实例首次启动后才生成；这里兜底
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)",
        [],
    ).map_err(|e| AppError::Database(format!("Failed to ensure ItemTable: {}", e)))?;
    
    // 读取当前 windsurfAuthStatus
    let current_auth: Option<String> = conn.query_row(
        "SELECT value FROM ItemTable WHERE key = 'windsurfAuthStatus'",
        [],
        |row| row.get(0),
    ).ok();
    
    let new_auth_status = if let Some(ref auth_str) = current_auth {
        if let Ok(mut auth_val) = serde_json::from_str::<serde_json::Value>(auth_str) {
            auth_val["apiKey"] = serde_json::Value::String(api_key.to_string());
            serde_json::to_string(&auth_val).unwrap_or_else(|_| 
                format!(r#"{{"apiKey":"{}"}}"#, api_key))
        } else {
            format!(r#"{{"apiKey":"{}"}}"#, api_key)
        }
    } else {
        format!(r#"{{"apiKey":"{}"}}"#, api_key)
    };
    
    // 6. 写入数据库
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('windsurfAuthStatus', ?1)",
        rusqlite::params![new_auth_status],
    ).map_err(|e| AppError::Database(format!("Failed to update windsurfAuthStatus: {}", e)))?;
    
    let session_secret_key = r#"secret://{"extensionId":"codeium.windsurf","key":"windsurf_auth.sessions"}"#;
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params![session_secret_key, session_db_value],
    ).map_err(|e| AppError::Database(format!("Failed to update sessions secret: {}", e)))?;
    
    let url_secret_key = r#"secret://{"extensionId":"codeium.windsurf","key":"windsurf_auth.apiServerUrl"}"#;
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params![url_secret_key, url_db_value],
    ).map_err(|e| AppError::Database(format!("Failed to update apiServerUrl secret: {}", e)))?;
    
    info!("Successfully wrote auth data to state.vscdb: apiKey={}..., name={}, server={}", 
        &api_key[..std::cmp::min(api_key.len(), 20)], name, api_server_url);
    
    Ok(())
}

/// DPAPI 解密
#[cfg(target_os = "windows")]
fn dpapi_decrypt(data: &[u8]) -> AppResult<Vec<u8>> {
    use winapi::um::dpapi::CryptUnprotectData;
    use winapi::um::wincrypt::CRYPTOAPI_BLOB;
    use std::ptr;
    
    extern "system" {
        fn LocalFree(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }
    
    let mut input_blob = CRYPTOAPI_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output_blob = CRYPTOAPI_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    
    let result = unsafe {
        CryptUnprotectData(
            &mut input_blob,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            &mut output_blob,
        )
    };
    
    if result == 0 {
        return Err(AppError::Config(format!("DPAPI CryptUnprotectData failed: {}", std::io::Error::last_os_error())));
    }
    
    let decrypted = unsafe {
        std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize).to_vec()
    };
    
    unsafe { LocalFree(output_blob.pbData as *mut _); }
    
    Ok(decrypted)
}

/// DPAPI 加密
#[cfg(target_os = "windows")]
fn dpapi_encrypt(data: &[u8]) -> AppResult<Vec<u8>> {
    use winapi::um::dpapi::CryptProtectData;
    use winapi::um::wincrypt::CRYPTOAPI_BLOB;
    use std::ptr;

    extern "system" {
        fn LocalFree(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    let mut input_blob = CRYPTOAPI_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output_blob = CRYPTOAPI_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    let result = unsafe {
        CryptProtectData(
            &mut input_blob,
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            &mut output_blob,
        )
    };

    if result == 0 {
        return Err(AppError::Config(format!("DPAPI CryptProtectData failed: {}", std::io::Error::last_os_error())));
    }

    let encrypted = unsafe {
        std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize).to_vec()
    };

    unsafe { LocalFree(output_blob.pbData as *mut _); }

    Ok(encrypted)
}

/// 为分身预生成合法的 Local State（含 DPAPI 加密的 AES key），免去冷启动 Windsurf 一次的 UX 跳变。
/// - 文件已存在且包含 `os_crypt.encrypted_key` → 直接复用
/// - 否则：随机 32 字节 AES key → DPAPI 加密 + "DPAPI" 前缀 + base64 → 写入 `<user_data_dir>/Local State`
#[cfg(target_os = "windows")]
pub(crate) fn prepare_profile_local_state(user_data_dir: &std::path::Path) -> AppResult<()> {
    use base64::{Engine, engine::general_purpose};
    use rand::RngCore;

    let local_state_path = user_data_dir.join("Local State");

    if local_state_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&local_state_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if json.get("os_crypt").and_then(|v| v.get("encrypted_key")).and_then(|v| v.as_str()).is_some() {
                    return Ok(());
                }
            }
        }
    }

    std::fs::create_dir_all(user_data_dir)
        .map_err(|e| AppError::FileOperation(format!("Failed to create user_data_dir: {}", e)))?;

    let mut aes_key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut aes_key);

    let encrypted = dpapi_encrypt(&aes_key)?;

    let mut prefixed = Vec::with_capacity(5 + encrypted.len());
    prefixed.extend_from_slice(b"DPAPI");
    prefixed.extend_from_slice(&encrypted);

    let encoded = general_purpose::STANDARD.encode(&prefixed);

    let local_state_json = serde_json::json!({
        "os_crypt": {
            "encrypted_key": encoded
        }
    });

    let serialized = serde_json::to_string_pretty(&local_state_json)
        .map_err(|e| AppError::Config(format!("Failed to serialize Local State: {}", e)))?;

    std::fs::write(&local_state_path, serialized)
        .map_err(|e| AppError::FileOperation(format!("Failed to write Local State: {}", e)))?;

    info!("Generated Local State for profile dir: {}", user_data_dir.display());
    Ok(())
}

/// 触发Windsurf回调URL以完成登录
/// - `user_data_dir = None` → 投递给主实例（保持原行为）
/// - `user_data_dir = Some(path)` → 在 Windsurf CLI 命令前追加 `--user-data-dir <path>`，
///   把回调投递给该分身实例。注意：当指定分身时，Windows 上必须找到 Windsurf.exe，
///   不能回退到 opener（系统 URL 协议会路由到主实例，无法定向到分身）。
pub(crate) async fn trigger_windsurf_callback(
    app: &tauri::AppHandle,
    auth_token: &str,
    user_data_dir: Option<&std::path::Path>,
) -> AppResult<()> {
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
    
    info!(
        "Triggering Windsurf callback (target={}): windsurf://codeium.windsurf#access_token=<hidden>&state={}&token_type=Bearer",
        user_data_dir.map(|p| p.display().to_string()).unwrap_or_else(|| "main".to_string()),
        state
    );
    
    // Windows: 使用 Windsurf CLI --open-url 直接传递给运行中的 Windsurf 实例（避免 ShellExecuteW 弹出 Git Bash）
    #[cfg(target_os = "windows")]
    {
        if let Some(exe_path) = find_windsurf_exe() {
            use std::os::windows::process::CommandExt;
            let mut cmd = std::process::Command::new(&exe_path);
            // 分身：追加 --user-data-dir，让 Windsurf CLI 定位到该分身实例
            if let Some(dir) = user_data_dir {
                cmd.arg("--user-data-dir").arg(dir);
            }
            cmd.arg("--open-url").arg(&callback_url);
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
            let output = cmd.output();
            match output {
                Ok(o) => {
                    if !o.status.success() {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        warn!("Windsurf --open-url exited with {}: {}", o.status, stderr.trim());
                    }
                    info!("Successfully triggered Windsurf callback via CLI");
                }
                Err(e) => {
                    // 仅主实例可回退到系统 opener；分身一旦回退就会路由到主实例，反而破坏隔离。
                    if user_data_dir.is_some() {
                        return Err(AppError::FileOperation(format!(
                            "Windsurf CLI failed for profile callback: {}", e
                        )));
                    }
                    warn!("Windsurf CLI failed ({}), falling back to opener", e);
                    use tauri_plugin_opener::OpenerExt;
                    app.opener()
                        .open_url(&callback_url, None::<&str>)
                        .map_err(|e| AppError::FileOperation(format!("Failed to open URL: {}", e)))?;
                }
            }
        } else if user_data_dir.is_some() {
            // 分身必须依赖 Windsurf CLI，找不到 exe 直接报错
            return Err(AppError::FileOperation(
                "Cannot dispatch profile callback: Windsurf.exe not found".to_string()
            ));
        } else {
            // 主实例：找不到 Windsurf 可执行文件时回退到 opener
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_url(&callback_url, None::<&str>)
                .map_err(|e| AppError::FileOperation(format!("Failed to open URL: {}", e)))?;
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        if let Some(exe_path) = find_windsurf_exe() {
            let mut cmd = std::process::Command::new(&exe_path);
            if let Some(dir) = user_data_dir {
                cmd.arg("--user-data-dir").arg(dir);
            }
            cmd.arg("--open-url").arg(&callback_url);
            info!(
                "[Profile][macOS] Dispatching callback via Windsurf app binary: target={}, exe={}, arch={}",
                user_data_dir.map(|p| p.display().to_string()).unwrap_or_else(|| "main".to_string()),
                exe_path,
                std::env::consts::ARCH
            );
            match cmd.output() {
                Ok(o) => {
                    if !o.status.success() {
                        warn!(
                            "[Profile][macOS] Windsurf --open-url exited with {:?}: stdout={}, stderr={}",
                            o.status.code(),
                            String::from_utf8_lossy(&o.stdout).trim(),
                            String::from_utf8_lossy(&o.stderr).trim()
                        );
                    } else {
                        info!("[Profile][macOS] Successfully triggered Windsurf callback via CLI");
                    }
                }
                Err(e) => {
                    if user_data_dir.is_some() {
                        return Err(AppError::FileOperation(format!(
                            "Windsurf CLI failed for macOS profile callback: {}",
                            e
                        )));
                    }
                    warn!("[Profile][macOS] Windsurf CLI failed ({}), falling back to opener for main instance", e);
                    use tauri_plugin_opener::OpenerExt;
                    app.opener()
                        .open_url(&callback_url, None::<&str>)
                        .map_err(|e| AppError::FileOperation(format!("Failed to open URL: {}", e)))?;
                }
            }
        } else if user_data_dir.is_some() {
            return Err(AppError::FileOperation(
                "Cannot dispatch macOS profile callback: Windsurf executable not found".to_string()
            ));
        } else {
            warn!("[Profile][macOS] Windsurf executable not found, falling back to opener for main instance");
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_url(&callback_url, None::<&str>)
                .map_err(|e| AppError::FileOperation(format!("Failed to open URL: {}", e)))?;
        }
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = user_data_dir; // 暂不在非 Windows 平台使用
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(&callback_url, None::<&str>)
            .map_err(|e| AppError::FileOperation(format!("Failed to open URL: {}", e)))?;
    }
    
    info!("Successfully triggered Windsurf callback");
    Ok(())
}


/// 重启 Windsurf：关闭现有进程，等待退出，然后重新启动
#[cfg(target_os = "windows")]
async fn restart_windsurf() -> bool {
    use std::process::Command;
    
    // 查找 Windsurf 可执行路径
    let windsurf_exe = find_windsurf_exe();
    
    // 关闭 Windsurf 进程
    info!("Killing Windsurf processes...");
    let kill_result = Command::new("taskkill")
        .args(&["/F", "/IM", "Windsurf.exe"])
        .output();
    
    match &kill_result {
        Ok(output) => {
            if output.status.success() {
                info!("Windsurf processes killed");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("taskkill output: {}", stderr.trim());
            }
        }
        Err(e) => {
            error!("Failed to run taskkill: {}", e);
            return false;
        }
    }
    
    // 等待进程完全退出
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    
    // 重新启动 Windsurf
    if let Some(exe_path) = windsurf_exe {
        info!("Restarting Windsurf from: {}", exe_path);
        match Command::new(&exe_path)
            .spawn()
        {
            Ok(_) => {
                info!("Windsurf restarted successfully");
                return true;
            }
            Err(e) => {
                error!("Failed to restart Windsurf: {}", e);
            }
        }
    } else {
        warn!("Could not find Windsurf executable path");
    }
    
    false
}

/// 查找 Windsurf 可执行文件路径
#[cfg(target_os = "windows")]
pub(crate) fn find_windsurf_exe() -> Option<String> {
    let candidates = [
        r"C:\Program Files\Windsurf\Windsurf.exe",
        r"C:\Users\Default\AppData\Local\Programs\Windsurf\Windsurf.exe",
    ];
    
    // 先检查常见路径
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    
    // 尝试从用户 LOCALAPPDATA 查找
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let user_path = format!(r"{}\Programs\Windsurf\Windsurf.exe", local_app_data);
        if std::path::Path::new(&user_path).exists() {
            return Some(user_path);
        }
    }
    
    // 尝试 which/where 查找
    if let Ok(output) = std::process::Command::new("where").arg("Windsurf").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path.lines().next().unwrap_or(&path).to_string());
            }
        }
    }
    
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn find_windsurf_exe() -> Option<String> {
    let mut candidates: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("/Applications/Windsurf.app/Contents/MacOS/Windsurf"),
    ];

    if let Ok(home) = std::env::var("HOME") {
        candidates.push(
            std::path::PathBuf::from(&home)
                .join("Applications")
                .join("Windsurf.app")
                .join("Contents")
                .join("MacOS")
                .join("Windsurf"),
        );
    }

    for path in candidates {
        if path.exists() {
            let value = path.to_string_lossy().to_string();
            info!("[Profile][macOS] Found Windsurf executable: {}", value);
            return Some(value);
        }
    }

    match std::process::Command::new("mdfind")
        .arg("kMDItemCFBundleIdentifier == 'com.exafunction.windsurf'")
        .output()
    {
        Ok(output) if output.status.success() => {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let bundle = std::path::PathBuf::from(line.trim());
                let exe = bundle.join("Contents").join("MacOS").join("Windsurf");
                if exe.exists() {
                    let value = exe.to_string_lossy().to_string();
                    info!("[Profile][macOS] Found Windsurf executable via mdfind: {}", value);
                    return Some(value);
                }
            }
        }
        Ok(output) => warn!(
            "[Profile][macOS] mdfind failed while locating Windsurf: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(e) => warn!("[Profile][macOS] Failed to run mdfind while locating Windsurf: {}", e),
    }

    warn!("[Profile][macOS] Windsurf executable not found in common app bundle locations");
    None
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub(crate) fn find_windsurf_exe() -> Option<String> {
    None
}

/// 一键切换账号命令（简化版：使用回调URL登录）
#[tauri::command]
pub async fn switch_account(
    app: tauri::AppHandle,
    id: String,
    data_store: State<'_, Arc<DataStore>>,
    machine_id_store: State<'_, Arc<crate::commands::machine_id_commands::MachineIdStore>>,
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
    
    // Step 1: 刷新Firebase token + 调用RegisterUser获取apiKey
    info!("Getting auth token via Firebase refresh...");
    let auth = match get_auth_token_for_account(&data_store, account_id, &account.email, &refresh_token).await {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to get auth token: {:?}", e);
            return Ok(json!({
                "success": false,
                "error": format!("获取auth_token失败: {}", e)
            }));
        }
    };
    
    // Step 2: 根据设置决定是否重置机器ID
    let settings = data_store.get_settings().await.map_err(|e| e.to_string())?;
    let should_reset = settings.reset_machine_id_on_switch;
    
    let machine_id_reset = if should_reset {
        info!("Attempting to reset machine ID (setting enabled)...");
        // 重置前自动保存当前设备码
        crate::commands::machine_id_commands::auto_save_before_reset(
            &machine_id_store,
            Some(account.email.clone()),
            Some(id.clone()),
        ).await;
        
        let reset_result = reset_machine_id_internal().await;
        match reset_result {
            Ok(_) => {
                info!("Machine ID reset successful");
                true
            },
            Err(e) => {
                warn!("Failed to reset machine ID: {:?}", e);
                warn!("重置机器ID失败，可能需要管理员权限。但切换账号仍可继续。");
                false
            }
        }
    } else {
        info!("Skipping machine ID reset (setting disabled)");
        false
    };
    
    // Step 3: 通过回调URL触发无感切号（extension.js 补丁中的 handleAuthToken 会处理全部流程）
    info!("Triggering seamless account switch via callback URL...");
    if let Err(e) = trigger_windsurf_callback(&app, &auth.callback_token, None).await {
        error!("Callback failed: {}", e);
        return Ok(json!({
            "success": false,
            "error": format!("触发回调URL失败: {}", e)
        }));
    }
    
    // 更新账号的token信息
    let expires_at = Utc::now() + chrono::Duration::seconds(auth.expires_in.parse::<i64>().unwrap_or(3600));
    let update_result = if let Some(refresh_token_new) = auth.refresh_token.clone() {
        data_store.update_account_tokens(account_id, auth.access_token.clone(), refresh_token_new, expires_at).await
    } else {
        data_store.update_account_token(account_id, auth.access_token.clone(), expires_at).await
    };
    if let Err(e) = update_result {
        error!("Failed to update account token: {:?}", e);
    }
    
    info!("Successfully switched Windsurf account to: {}", auth.register_result.name);
    
    // 更新自动换号跟踪的当前账号ID
    if let Ok(mut settings) = data_store.get_settings().await {
        settings.auto_switch_current_account_id = Some(id.clone());
        let _ = data_store.update_settings(settings).await;
    }
    
    Ok(json!({
        "success": true,
        "message": if machine_id_reset {
            "已成功无感切换账号并重置机器ID"
        } else {
            "已成功无感切换账号"
        },
        "api_key": auth.register_result.api_key,
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

/// 仅为指定分身重写 storage.json 中的机器码（不动 HKLM、/etc/machine-id 等系统级标识）。
/// HKLM\MachineGuid 全机共享，分身共用一份是预期行为；分身切号默认只刷新 telemetry 层。
#[allow(dead_code)] // Phase 3 接入后启用
pub async fn reset_storage_json_for_profile(
    profile: &crate::models::WindsurfProfile,
) -> AppResult<()> {
    use std::fs;
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let machine_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    let new_machine_id = hex::encode(&machine_bytes);
    let new_mac_machine_id = format!("{:032x}", rng.gen::<u128>());
    let new_sqm_id = Uuid::new_v4().to_string().to_uppercase();
    let new_device_id = Uuid::new_v4().to_string().to_lowercase();

    let storage_path = profile.storage_json_path();

    if !storage_path.exists() {
        warn!(
            "storage.json not found for profile '{}' at {:?}; skip (profile may not have been launched yet)",
            profile.name, storage_path
        );
        return Ok(());
    }

    let content = fs::read_to_string(&storage_path)
        .map_err(|e| AppError::FileOperation(format!("Failed to read profile storage.json: {}", e)))?;
    let mut storage: Value = serde_json::from_str(&content)
        .map_err(AppError::Serialization)?;

    storage["telemetry.machineId"] = json!(new_machine_id);
    storage["telemetry.macMachineId"] = json!(new_mac_machine_id);
    storage["telemetry.sqmId"] = json!(new_sqm_id);
    storage["telemetry.devDeviceId"] = json!(new_device_id);

    let updated = serde_json::to_string_pretty(&storage)
        .map_err(AppError::Serialization)?;
    fs::write(&storage_path, updated)
        .map_err(|e| AppError::FileOperation(format!("Failed to write profile storage.json: {}", e)))?;

    info!(
        "Reset machine IDs for profile '{}' (storage.json only, HKLM unchanged)",
        profile.name
    );
    Ok(())
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
async fn wait_for_windsurf_account(target_email: &str) -> Option<crate::commands::windsurf_info::WindsurfCurrentInfo> {
    for _ in 0..12 {
        if let Ok(info) = crate::commands::windsurf_info::get_current_windsurf_info() {
            if info
                .email
                .as_deref()
                .map(|email| email.eq_ignore_ascii_case(target_email))
                .unwrap_or(false)
            {
                return Some(info);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
    None
}

#[tauri::command]
pub async fn check_auto_switch(
    app: tauri::AppHandle,
    data_store: State<'_, Arc<DataStore>>,
    machine_id_store: State<'_, Arc<crate::commands::machine_id_commands::MachineIdStore>>,
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
    
    // 通过编辑器实际登录状态识别当前账号
    let windsurf_info = crate::commands::windsurf_info::get_current_windsurf_info()
        .map_err(|e| format!("读取编辑器状态失败: {}", e))?;
    
    let editor_email = windsurf_info.email.clone();
    
    // 获取分组内所有账号
    let all_accounts = data_store.get_all_accounts().await.map_err(|e| e.to_string())?;
    
    // 在分组内匹配编辑器当前登录的账号
    let current_account = if let Some(ref email) = editor_email {
        all_accounts.iter()
            .find(|a| a.email.eq_ignore_ascii_case(email) && a.group.as_deref() == Some(group))
            .or_else(|| all_accounts.iter().find(|a| a.email.eq_ignore_ascii_case(email)))
            .cloned()
    } else {
        None
    };
    
    // null → A 情况：编辑器未登录或未匹配到账号，直接进入候选选择流程
    let (current_account, current_id_str) = match current_account {
        Some(acc) => {
            let id_str = acc.id.to_string();
            (acc, id_str)
        }
        None => {
            // 编辑器无登录账号或账号不在管理列表中，直接找候选号切换
            println!("[自动换号] 编辑器当前无已识别账号(email={:?})，将直接选择候选号切换", editor_email);
            
            // 跳到候选号选择逻辑（设置 dummy 值以继续后续流程）
            // 构造一个"需要切换"的场景
            let in_use = crate::commands::profile_commands::accounts_in_use_by_other_profiles(
                &data_store,
                crate::models::MAIN_PROFILE_ID,
            ).await;
            let group_candidates: Vec<_> = all_accounts.iter()
                .filter(|a| {
                    a.group.as_deref() == Some(group)
                    && !matches!(a.status, crate::models::AccountStatus::Error(_))
                    && a.refresh_token.is_some()
                    && !in_use.contains(&a.email.to_ascii_lowercase())
                })
                .collect();
            
            if group_candidates.is_empty() {
                return Ok(json!({
                    "action": "no_candidate",
                    "reason": format!("分组 '{}' 中没有可用账号", group),
                    "editor_email": editor_email
                }));
            }
            
            // 直接找配额最充足的候选号
            let windsurf_service = crate::services::windsurf_service::WindsurfService::new();
            let is_free_plan = |acc: &crate::models::Account| -> bool {
                acc.plan_name.as_ref().map(|p| p.to_lowercase().contains("free")).unwrap_or(true)
            };
            let is_better_candidate = |new_daily: i32, new_weekly: i32, new_is_free: bool, cur: &Option<(Uuid, String, i32, i32, bool)>| -> bool {
                match cur {
                    None => true,
                    Some((_, _, cur_daily, cur_weekly, cur_is_free)) => {
                        if !new_is_free && *cur_is_free { return true; }
                        if new_is_free && !*cur_is_free { return false; }
                        if new_daily != *cur_daily { return new_daily > *cur_daily; }
                        new_weekly > *cur_weekly
                    }
                }
            };
            
            let mut best_candidate: Option<(Uuid, String, i32, i32, bool)> = None;
            for acc in &group_candidates {
                let daily = acc.daily_quota_remaining.unwrap_or(0);
                let weekly = acc.weekly_quota_remaining.unwrap_or(0);
                let acc_is_free = is_free_plan(acc);
                if daily > threshold && weekly > 0 {
                    if is_better_candidate(daily, weekly, acc_is_free, &best_candidate) {
                        best_candidate = Some((acc.id, acc.email.clone(), daily, weekly, acc_is_free));
                    }
                }
            }
            
            // 没有缓存合适的，刷新后再找
            if best_candidate.is_none() {
                let cache_ttl = chrono::Duration::minutes(3);
                let now = chrono::Utc::now();
                for acc in &group_candidates {
                    if let Some(last_update) = acc.last_quota_update {
                        if now - last_update < cache_ttl { continue; }
                    }
                    if let Some(ref token) = acc.token {
                        if let Ok(result) = windsurf_service.get_plan_status(token).await {
                            if let Some(plan_status) = result.get("plan_status") {
                                let daily = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                                let weekly = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                                let mut updated = (*acc).clone();
                                updated.daily_quota_remaining = Some(daily);
                                updated.weekly_quota_remaining = Some(weekly);
                                updated.last_quota_update = Some(now);
                                let _ = data_store.update_account(updated).await;
                                let acc_is_free = is_free_plan(acc);
                                if daily > threshold && weekly > 0 {
                                    if is_better_candidate(daily, weekly, acc_is_free, &best_candidate) {
                                        best_candidate = Some((acc.id, acc.email.clone(), daily, weekly, acc_is_free));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            let (target_id, target_email, target_daily, target_weekly, _) = match best_candidate {
                Some(c) => c,
                None => {
                    return Ok(json!({
                        "action": "no_candidate",
                        "reason": format!("分组 '{}' 中没有配额充足的账号", group),
                        "editor_email": editor_email
                    }));
                }
            };
            
            println!("[自动换号] 首次切号: 编辑器无已知账号 → {} (日{}%, 周{}%)", target_email, target_daily, target_weekly);
            
            // 执行切换
            let target_account = data_store.get_account(target_id).await.map_err(|e| e.to_string())?;
            let refresh_token = match &target_account.refresh_token {
                Some(rt) if !rt.is_empty() => rt.clone(),
                _ => return Ok(json!({ "action": "error", "reason": format!("目标账号 {} 没有refresh_token", target_email) })),
            };
            let auth = match get_auth_token_for_account(&data_store, target_id, &target_account.email, &refresh_token).await {
                Ok(r) => r,
                Err(e) => return Ok(json!({ "action": "error", "reason": format!("获取auth_token失败: {}", e) })),
            };
            if settings.reset_machine_id_on_switch {
                crate::commands::machine_id_commands::auto_save_before_reset(&machine_id_store, editor_email.clone(), None).await;
                let _ = reset_machine_id_internal().await;
            }
            if let Err(e) = trigger_windsurf_callback(&app, &auth.callback_token, None).await {
                return Ok(json!({ "action": "error", "reason": format!("触发回调URL失败: {}", e) }));
            }
            let verified_info = match wait_for_windsurf_account(&target_email).await {
                Some(info) => info,
                None => {
                    return Ok(json!({
                        "action": "error",
                        "reason": format!("编辑器账号校验失败，未检测到目标账号: {}", target_email)
                    }));
                }
            };
            let expires_at = Utc::now() + chrono::Duration::seconds(auth.expires_in.parse::<i64>().unwrap_or(3600));
            if let Some(refresh_token_new) = auth.refresh_token {
                let _ = data_store.update_account_tokens(target_id, auth.access_token, refresh_token_new, expires_at).await;
            } else {
                let _ = data_store.update_account_token(target_id, auth.access_token, expires_at).await;
            }
            let mut new_settings = settings.clone();
            new_settings.auto_switch_current_account_id = Some(target_id.to_string());
            let _ = data_store.update_settings(new_settings).await;
            
            return Ok(json!({
                "action": "switched",
                "reason": "编辑器无已识别账号，首次自动切号",
                "from_account": editor_email,
                "to_account": target_email,
                "to_account_id": target_id.to_string(),
                "to_daily_remaining": target_daily,
                "to_weekly_remaining": target_weekly,
                "verified_editor_account": verified_info.email
            }));
        }
    };
    
    let current_uuid = current_account.id;
    
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
    let in_use = crate::commands::profile_commands::accounts_in_use_by_other_profiles(
        &data_store,
        crate::models::MAIN_PROFILE_ID,
    ).await;
    let group_accounts: Vec<_> = all_accounts.iter()
        .filter(|a| {
            a.group.as_deref() == Some(group) 
            && a.id != current_uuid
            && !matches!(a.status, crate::models::AccountStatus::Error(_))
            && a.refresh_token.is_some()
            && !in_use.contains(&a.email.to_ascii_lowercase())
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
    
    // 候选号比较逻辑：非Free优先，然后日配额最高，日配额相同时周配额最高
    // 返回true表示new_acc比current更优
    let is_better_candidate = |new_daily: i32, new_weekly: i32, new_is_free: bool, cur: &Option<(Uuid, String, i32, i32, bool)>| -> bool {
        match cur {
            None => true,
            Some((_, _, cur_daily, cur_weekly, cur_is_free)) => {
                // 非Free优先于Free
                if !new_is_free && *cur_is_free {
                    return true;
                }
                if new_is_free && !*cur_is_free {
                    return false;
                }
                // 同类型中，日配额更高的优先
                if new_daily != *cur_daily {
                    return new_daily > *cur_daily;
                }
                // 日配额相同时，周配额更高的优先
                new_weekly > *cur_weekly
            }
        }
    };
    
    // 查找配额最高的账号（候选条件：周配额>0 且 日配额>阈值，Free账号优先级最低）
    // best_candidate: (id, email, daily, weekly, is_free)
    let mut best_candidate: Option<(Uuid, String, i32, i32, bool)> = None;
    
    for acc in &group_accounts {
        let daily = acc.daily_quota_remaining.unwrap_or(0);
        let weekly = acc.weekly_quota_remaining.unwrap_or(0);
        let acc_is_free = is_free_plan(acc);
        // 候选号必须同时满足：周配额>0 且 日配额>阈值
        if daily > threshold && weekly > 0 {
            if is_better_candidate(daily, weekly, acc_is_free, &best_candidate) {
                best_candidate = Some((acc.id, acc.email.clone(), daily, weekly, acc_is_free));
            }
        }
    }
    
    // 如果没有找到已缓存的合适账号，逐个刷新分组账号配额再验证
    // 优化：跳过3分钟内已刷新过的账号，避免频繁API调用
    if best_candidate.is_none() {
        let cache_ttl = chrono::Duration::minutes(3);
        let now = chrono::Utc::now();
        let mut refreshed_count = 0;
        let mut skipped_count = 0;
        
        println!("[自动换号] 缓存数据中未找到合适账号，逐个刷新分组账号配额...");
        for acc in &group_accounts {
            // 如果账号在3分钟内已刷新过，使用缓存数据
            if let Some(last_update) = acc.last_quota_update {
                if now - last_update < cache_ttl {
                    skipped_count += 1;
                    let daily = acc.daily_quota_remaining.unwrap_or(0);
                    let weekly = acc.weekly_quota_remaining.unwrap_or(0);
                    let acc_is_free = is_free_plan(acc);
                    if daily > threshold && weekly > 0 {
                        if is_better_candidate(daily, weekly, acc_is_free, &best_candidate) {
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
                        
                        // 候选号必须同时满足：周配额>0 且 日配额>阈值
                        if daily > threshold && weekly > 0 {
                            if is_better_candidate(daily, weekly, acc_is_free, &best_candidate) {
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
    
    // 获取 auth_token (Windsurf 2.0+: RegisterUser获取apiKey)
    let auth = match get_auth_token_for_account(&data_store, target_id, &target_account.email, &refresh_token).await {
        Ok(r) => r,
        Err(e) => {
            return Ok(json!({
                "action": "error",
                "reason": format!("获取auth_token失败: {}", e)
            }));
        }
    };
    
    // 根据设置决定是否重置机器ID
    if settings.reset_machine_id_on_switch {
        // 重置前自动保存当前设备码
        crate::commands::machine_id_commands::auto_save_before_reset(
            &machine_id_store,
            Some(current_account.email.clone()),
            Some(current_id_str.clone()),
        ).await;
        let _ = reset_machine_id_internal().await;
    }
    
    // 无感切号：通过回调URL触发
    if let Err(e) = trigger_windsurf_callback(&app, &auth.callback_token, None).await {
        return Ok(json!({
            "action": "error",
            "reason": format!("触发回调URL失败: {}", e)
        }));
    }
    let verified_info = match wait_for_windsurf_account(&target_email).await {
        Some(info) => info,
        None => {
            return Ok(json!({
                "action": "error",
                "reason": format!("编辑器账号校验失败，未检测到目标账号: {}", target_email)
            }));
        }
    };
    
    // 更新目标账号的token信息
    let expires_at = Utc::now() + chrono::Duration::seconds(auth.expires_in.parse::<i64>().unwrap_or(3600));
    if let Some(refresh_token_new) = auth.refresh_token {
        let _ = data_store.update_account_tokens(target_id, auth.access_token, refresh_token_new, expires_at).await;
    } else {
        let _ = data_store.update_account_token(target_id, auth.access_token, expires_at).await;
    }
    
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
        "to_weekly_remaining": target_weekly,
        "verified_editor_account": verified_info.email
    }))
}
