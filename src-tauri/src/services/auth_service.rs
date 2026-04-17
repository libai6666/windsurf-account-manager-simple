use crate::utils::{AppError, AppResult};
use chrono::{DateTime, Duration, Utc};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const FIREBASE_API_KEY: &str = "AIzaSyDsOl-1XpT5err0Tcnx8FFod1H8gVGIycY";

// ============= Windsurf 2.0 Devin-Auth 登录相关结构 =============

#[derive(Debug, Serialize, Deserialize)]
pub struct DevinLoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DevinLoginResponse {
    pub token: String,
    pub user_id: String,
    pub email: String,
}

/// Windsurf 2.0 登录结果：包含 OTT（用于编辑器认证）和 session 信息
#[derive(Debug, Clone)]
pub struct WindsurfAuthResult {
    pub ott: String,
    pub session_token: String,
    pub auth1_token: String,
    pub account_id: String,
    pub org_id: String,
    pub user_id: String,
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignInRequest {
    #[serde(rename = "returnSecureToken")]
    return_secure_token: bool,
    email: String,
    password: String,
    #[serde(rename = "clientType")]
    client_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignInResponse {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: String,
    #[serde(rename = "localId")]
    pub local_id: String,
    pub email: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshTokenRequest {
    grant_type: String,
    refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub expires_in: String,
    pub token_type: String,
    pub refresh_token: String,
    pub id_token: String,
    pub user_id: String,
    pub project_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountInfo {
    #[serde(rename = "localId")]
    pub local_id: String,
    pub email: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "emailVerified")]
    pub email_verified: bool,
    #[serde(rename = "passwordHash", skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
    #[serde(rename = "passwordUpdatedAt", skip_serializing_if = "Option::is_none")]
    pub password_updated_at: Option<i64>,
    #[serde(rename = "validSince", skip_serializing_if = "Option::is_none")]
    pub valid_since: Option<String>,
    #[serde(rename = "disabled", default)]
    pub disabled: bool,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "lastLoginAt")]
    pub last_login_at: Option<String>,
    #[serde(rename = "lastRefreshAt", skip_serializing_if = "Option::is_none")]
    pub last_refresh_at: Option<String>,
    #[serde(rename = "providerUserInfo", skip_serializing_if = "Option::is_none")]
    pub provider_user_info: Option<Vec<ProviderInfo>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    #[serde(rename = "providerId")]
    pub provider_id: String,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "federatedId", skip_serializing_if = "Option::is_none")]
    pub federated_id: Option<String>,
    #[serde(rename = "email", skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(rename = "rawId", skip_serializing_if = "Option::is_none")]
    pub raw_id: Option<String>,
    #[serde(rename = "photoUrl", skip_serializing_if = "Option::is_none")]
    pub photo_url: Option<String>,
}

pub struct AuthService {
    client: Arc<reqwest::Client>,
}

impl AuthService {
    pub fn new() -> Self {
        // 使用专门用于 googleapis 的 HTTP 客户端（支持代理）
        Self {
            client: super::get_google_api_client(),
        }
    }
    
    /// 重新获取客户端（用于代理配置更新后）
    pub fn refresh_client(&mut self) {
        self.client = super::get_google_api_client();
    }

    pub async fn sign_in(&self, email: &str, password: &str) -> AppResult<(String, String, DateTime<Utc>)> {
        let url = format!(
            "https://identitytoolkit.googleapis.com/v1/accounts:signInWithPassword?key={}",
            FIREBASE_API_KEY
        );

        let request = SignInRequest {
            return_secure_token: true,
            email: email.to_string(),
            password: password.to_string(),
            client_type: "CLIENT_TYPE_WEB".to_string(),
        };

        let response = match self.client
            .post(&url)
            .json(&request)
            .header("Content-Type", "application/json")
            .header("X-Client-Version", "Chrome/JsCore/11.0.0/FirebaseCore-web")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
        {
            Ok(resp) => {
                super::report_request_success();
                resp
            }
            Err(e) => {
                // 检查是否是超时错误
                if e.is_timeout() || e.is_connect() {
                    super::report_timeout_error();
                } else {
                    super::report_request_failure();
                }
                return Err(AppError::Network(e.to_string()));
            }
        };

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            
            // 解析Firebase错误并提供友好提示
            if error_text.contains("TOO_MANY_ATTEMPTS_TRY_LATER") {
                return Err(AppError::AuthFailed("登录尝试次数过多，请15-30分钟后再试".to_string()));
            } else if error_text.contains("INVALID_LOGIN_CREDENTIALS") {
                return Err(AppError::AuthFailed("邮箱或密码错误，请检查后重试".to_string()));
            } else if error_text.contains("EMAIL_NOT_FOUND") {
                return Err(AppError::AuthFailed("该邮箱未注册".to_string()));
            } else if error_text.contains("USER_DISABLED") {
                return Err(AppError::AuthFailed("该账号已被禁用".to_string()));
            }
            
            return Err(AppError::AuthFailed(error_text));
        }

        let sign_in_response: SignInResponse = response.json().await?;
        
        // 计算Token过期时间
        let expires_in_secs: i64 = sign_in_response.expires_in.parse()
            .unwrap_or(3600);
        let expires_at = Utc::now() + Duration::seconds(expires_in_secs);

        Ok((sign_in_response.id_token, sign_in_response.refresh_token, expires_at))
    }

    pub async fn refresh_token(&self, refresh_token: &str) -> AppResult<(String, String, DateTime<Utc>)> {
        let url = format!(
            "https://securetoken.googleapis.com/v1/token?key={}",
            FIREBASE_API_KEY
        );

        let body = format!("grant_type=refresh_token&refresh_token={}", refresh_token);

        let response = match self.client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "*/*")
            .header("Accept-Language", "zh-CN,zh;q=0.9")
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .header("Sec-Ch-Ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("Sec-Ch-Ua-Mobile", "?0")
            .header("Sec-Ch-Ua-Platform", r#""Windows""#)
            .header("Sec-Fetch-Dest", "empty")
            .header("Sec-Fetch-Mode", "cors")
            .header("Sec-Fetch-Site", "cross-site")
            .header("X-Browser-Channel", "stable")
            .header("X-Browser-Copyright", "Copyright 2025 Google LLC. All Rights reserved.")
            .header("X-Browser-Validation", "Aj9fzfu+SaGLBY9Oqr3S7RokOtM=")
            .header("X-Browser-Year", "2025")
            .header("X-Client-Data", "CIu2yQEIo7bJAQipncoBCIiSywEIlqHLAQiFoM0BCPOYzwEI1prPAQ==")
            .header("X-Client-Version", "Chrome/JsCore/11.0.0/FirebaseCore-web")
            .header("Referer", "https://windsurf.com/")
            .body(body)
            .send()
            .await
        {
            Ok(resp) => {
                super::report_request_success();
                resp
            }
            Err(e) => {
                // 检查是否是超时错误
                if e.is_timeout() || e.is_connect() {
                    super::report_timeout_error();
                } else {
                    super::report_request_failure();
                }
                return Err(AppError::Network(e.to_string()));
            }
        };

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            
            // 如果refresh token失败，返回特定错误
            if error_text.contains("TOKEN_EXPIRED") || error_text.contains("INVALID_REFRESH_TOKEN") {
                return Err(AppError::TokenExpired);
            }
            
            return Err(AppError::Api(error_text));
        }

        let refresh_response: RefreshTokenResponse = response.json().await?;
        
        // 计算Token过期时间
        let expires_in_secs: i64 = refresh_response.expires_in.parse()
            .unwrap_or(3600);
        let expires_at = Utc::now() + Duration::seconds(expires_in_secs);

        Ok((refresh_response.id_token, refresh_response.refresh_token, expires_at))
    }

    pub async fn get_account_info(&self, id_token: &str) -> AppResult<AccountInfo> {
        let url = format!(
            "https://identitytoolkit.googleapis.com/v1/accounts:lookup?key={}",
            FIREBASE_API_KEY
        );

        let body = serde_json::json!({
            "idToken": id_token
        });

        let response = match self.client
            .post(&url)
            .json(&body)
            .header("Content-Type", "application/json")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
        {
            Ok(resp) => {
                super::report_request_success();
                resp
            }
            Err(e) => {
                // 检查是否是超时错误
                if e.is_timeout() || e.is_connect() {
                    super::report_timeout_error();
                } else {
                    super::report_request_failure();
                }
                return Err(AppError::Network(e.to_string()));
            }
        };

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AppError::Api(error_text));
        }

        let response_data: serde_json::Value = response.json().await?;
        
        if let Some(users) = response_data.get("users").and_then(|u| u.as_array()) {
            if let Some(user) = users.first() {
                let account_info: AccountInfo = serde_json::from_value(user.clone())
                    .map_err(|e| AppError::Api(e.to_string()))?;
                return Ok(account_info);
            }
        }

        Err(AppError::Api("No user info found".to_string()))
    }

    pub fn is_token_expired(expires_at: &DateTime<Utc>) -> bool {
        Utc::now() >= *expires_at
    }

    pub fn should_refresh_token(expires_at: &DateTime<Utc>) -> bool {
        // 如果Token在5分钟内过期，则刷新
        let buffer = Duration::minutes(5);
        Utc::now() + buffer >= *expires_at
    }

    /// Windsurf 2.0 兼容登录：返回与旧 sign_in 相同的 (token, refresh_token, expires_at) 格式
    /// token = session_token（用于 API 调用），refresh_token = auth1_token（用于后续刷新）
    pub async fn sign_in_compat(&self, email: &str, password: &str) -> AppResult<(String, String, DateTime<Utc>)> {
        let result = self.sign_in_v2(email, password).await?;
        let expires_at = Utc::now() + Duration::hours(1);
        Ok((result.session_token, result.auth1_token, expires_at))
    }

    // ============= Windsurf 2.0 新认证方法 =============

    /// Windsurf 2.0 登录：通过 devin-auth + WindsurfPostAuth + GetOneTimeAuthToken
    /// 返回 WindsurfAuthResult，包含 OTT（用于 handleAuthToken 回调）
    pub async fn sign_in_v2(&self, email: &str, password: &str) -> AppResult<WindsurfAuthResult> {
        // Step 1: _devin-auth/password/login
        info!("[sign_in_v2] Step 1: Calling _devin-auth/password/login for {}", email);
        let login_resp = match self.client
            .post("https://windsurf.com/_devin-auth/password/login")
            .json(&DevinLoginRequest {
                email: email.to_string(),
                password: password.to_string(),
            })
            .send()
            .await
        {
            Ok(resp) => {
                super::report_request_success();
                resp
            }
            Err(e) => {
                if e.is_timeout() || e.is_connect() {
                    super::report_timeout_error();
                } else {
                    super::report_request_failure();
                }
                return Err(AppError::Network(format!("devin-auth login failed: {}", e)));
            }
        };

        if !login_resp.status().is_success() {
            let status = login_resp.status();
            let error_text = login_resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            if error_text.contains("invalid_credentials") || error_text.contains("Invalid") || status.as_u16() == 401 {
                return Err(AppError::AuthFailed("邮箱或密码错误，请检查后重试".to_string()));
            }
            return Err(AppError::AuthFailed(format!("登录失败({}): {}", status, error_text)));
        }

        let login_data: DevinLoginResponse = login_resp.json().await
            .map_err(|e| AppError::Api(format!("Failed to parse login response: {}", e)))?;
        info!("[sign_in_v2] Step 1 OK: user_id={}, email={}", login_data.user_id, login_data.email);

        // Step 2: WindsurfPostAuth (protobuf)
        info!("[sign_in_v2] Step 2: Calling WindsurfPostAuth...");
        let post_auth_body = encode_protobuf_string(1, &login_data.token);
        let post_auth_resp = match self.client
            .post("https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/WindsurfPostAuth")
            .header("Content-Type", "application/proto")
            .header("Connect-Protocol-Version", "1")
            .body(post_auth_body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => return Err(AppError::Network(format!("WindsurfPostAuth failed: {}", e))),
        };

        if !post_auth_resp.status().is_success() {
            let error_text = post_auth_resp.text().await.unwrap_or_default();
            return Err(AppError::Api(format!("WindsurfPostAuth error: {}", error_text)));
        }

        let post_auth_bytes = post_auth_resp.bytes().await
            .map_err(|e| AppError::Api(format!("Failed to read WindsurfPostAuth response: {}", e)))?;
        let post_auth_fields = parse_protobuf_fields(&post_auth_bytes);

        let session_token = post_auth_fields.get(&1)
            .ok_or_else(|| AppError::Api("WindsurfPostAuth: missing session_token (field 1)".to_string()))?
            .clone();
        let refreshed_auth1 = post_auth_fields.get(&3).cloned().unwrap_or_default();
        let account_id = post_auth_fields.get(&4).cloned().unwrap_or_default();
        let org_id = post_auth_fields.get(&5).cloned().unwrap_or_default();
        info!("[sign_in_v2] Step 2 OK: account_id={}, org_id={}", account_id, org_id);

        // Step 3: GetOneTimeAuthToken (protobuf)
        info!("[sign_in_v2] Step 3: Calling GetOneTimeAuthToken...");
        let ott_body = encode_protobuf_string(1, &session_token);
        let ott_resp = match self.client
            .post("https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/GetOneTimeAuthToken")
            .header("Content-Type", "application/proto")
            .header("Connect-Protocol-Version", "1")
            .body(ott_body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => return Err(AppError::Network(format!("GetOneTimeAuthToken failed: {}", e))),
        };

        if !ott_resp.status().is_success() {
            let error_text = ott_resp.text().await.unwrap_or_default();
            return Err(AppError::Api(format!("GetOneTimeAuthToken error: {}", error_text)));
        }

        let ott_bytes = ott_resp.bytes().await
            .map_err(|e| AppError::Api(format!("Failed to read OTT response: {}", e)))?;
        let ott_fields = parse_protobuf_fields(&ott_bytes);

        let ott = ott_fields.get(&1)
            .ok_or_else(|| AppError::Api("GetOneTimeAuthToken: missing auth_token (field 1)".to_string()))?
            .clone();
        info!("[sign_in_v2] Step 3 OK: OTT={}...", &ott[..std::cmp::min(ott.len(), 20)]);

        Ok(WindsurfAuthResult {
            ott,
            session_token,
            auth1_token: refreshed_auth1,
            account_id,
            org_id,
            user_id: login_data.user_id,
            email: login_data.email,
        })
    }

    /// 使用 auth1_token 刷新 session_token（不获取 OTT）
    /// 返回与旧 sign_in 相同的 (session_token, auth1_token, expires_at) 格式，便于替换 Firebase refresh_token 调用
    pub async fn refresh_session_with_auth1(&self, auth1_token: &str) -> AppResult<(String, String, DateTime<Utc>)> {
        info!("[refresh_session_with_auth1] Refreshing session via WindsurfPostAuth...");

        let post_auth_body = encode_protobuf_string(1, auth1_token);
        let post_auth_resp = match self.client
            .post("https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/WindsurfPostAuth")
            .header("Content-Type", "application/proto")
            .header("Connect-Protocol-Version", "1")
            .body(post_auth_body)
            .send()
            .await
        {
            Ok(resp) => {
                super::report_request_success();
                resp
            }
            Err(e) => {
                if e.is_timeout() || e.is_connect() {
                    super::report_timeout_error();
                } else {
                    super::report_request_failure();
                }
                return Err(AppError::Network(format!("WindsurfPostAuth failed: {}", e)));
            }
        };

        if !post_auth_resp.status().is_success() {
            let status = post_auth_resp.status();
            let error_text = post_auth_resp.text().await.unwrap_or_default();
            // auth1 失效 -> TokenExpired，让调用方回退到密码登录
            if status.as_u16() == 401
                || error_text.contains("unauthenticated")
                || error_text.contains("invalid_token")
            {
                warn!("[refresh_session_with_auth1] auth1_token expired: {}", error_text);
                return Err(AppError::TokenExpired);
            }
            return Err(AppError::Api(format!(
                "WindsurfPostAuth error ({}): {}",
                status, error_text
            )));
        }

        let post_auth_bytes = post_auth_resp
            .bytes()
            .await
            .map_err(|e| AppError::Api(format!("Failed to read WindsurfPostAuth response: {}", e)))?;
        let fields = parse_protobuf_fields(&post_auth_bytes);

        let session_token = fields
            .get(&1)
            .cloned()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Api("WindsurfPostAuth: missing session_token (field 1)".to_string()))?;

        // 服务端未返回新 auth1 时，继续沿用旧的（避免把 refresh_token 覆盖成空字符串）
        let refreshed_auth1 = fields
            .get(&3)
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| auth1_token.to_string());

        let expires_at = Utc::now() + Duration::hours(1);
        Ok((session_token, refreshed_auth1, expires_at))
    }

    /// 使用 auth1_token 刷新 session 并获取新 OTT
    pub async fn refresh_ott(&self, auth1_token: &str) -> AppResult<WindsurfAuthResult> {
        info!("[refresh_ott] Refreshing OTT with auth1_token...");
        
        // WindsurfPostAuth
        let post_auth_body = encode_protobuf_string(1, auth1_token);
        let post_auth_resp = self.client
            .post("https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/WindsurfPostAuth")
            .header("Content-Type", "application/proto")
            .header("Connect-Protocol-Version", "1")
            .body(post_auth_body)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("WindsurfPostAuth refresh failed: {}", e)))?;

        if !post_auth_resp.status().is_success() {
            let error_text = post_auth_resp.text().await.unwrap_or_default();
            return Err(AppError::Api(format!("WindsurfPostAuth refresh error: {}", error_text)));
        }

        let post_auth_bytes = post_auth_resp.bytes().await
            .map_err(|e| AppError::Api(format!("Failed to read response: {}", e)))?;
        let fields = parse_protobuf_fields(&post_auth_bytes);

        let session_token = fields.get(&1)
            .ok_or_else(|| AppError::Api("Missing session_token".to_string()))?
            .clone();
        let refreshed_auth1 = fields.get(&3).cloned().unwrap_or_default();
        let account_id = fields.get(&4).cloned().unwrap_or_default();
        let org_id = fields.get(&5).cloned().unwrap_or_default();

        // GetOneTimeAuthToken
        let ott_body = encode_protobuf_string(1, &session_token);
        let ott_resp = self.client
            .post("https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/GetOneTimeAuthToken")
            .header("Content-Type", "application/proto")
            .header("Connect-Protocol-Version", "1")
            .body(ott_body)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("GetOneTimeAuthToken refresh failed: {}", e)))?;

        if !ott_resp.status().is_success() {
            let error_text = ott_resp.text().await.unwrap_or_default();
            return Err(AppError::Api(format!("GetOneTimeAuthToken refresh error: {}", error_text)));
        }

        let ott_bytes = ott_resp.bytes().await
            .map_err(|e| AppError::Api(format!("Failed to read OTT response: {}", e)))?;
        let ott_fields = parse_protobuf_fields(&ott_bytes);
        let ott = ott_fields.get(&1)
            .ok_or_else(|| AppError::Api("Missing OTT".to_string()))?
            .clone();

        info!("[refresh_ott] OK: OTT={}...", &ott[..std::cmp::min(ott.len(), 20)]);

        Ok(WindsurfAuthResult {
            ott,
            session_token,
            auth1_token: refreshed_auth1,
            account_id,
            org_id,
            user_id: String::new(),
            email: String::new(),
        })
    }

    /// 用现有的 session_token 获取一个新的 OTT（一次性令牌）
    pub async fn get_fresh_ott(&self, session_token: &str) -> AppResult<String> {
        let ott_body = encode_protobuf_string(1, session_token);
        let ott_resp = self.client
            .post("https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/GetOneTimeAuthToken")
            .header("Content-Type", "application/proto")
            .header("Connect-Protocol-Version", "1")
            .body(ott_body)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("GetOneTimeAuthToken failed: {}", e)))?;

        if !ott_resp.status().is_success() {
            let error_text = ott_resp.text().await.unwrap_or_default();
            return Err(AppError::Api(format!("GetOneTimeAuthToken error: {}", error_text)));
        }

        let ott_bytes = ott_resp.bytes().await
            .map_err(|e| AppError::Api(format!("Failed to read OTT response: {}", e)))?;
        let ott_fields = parse_protobuf_fields(&ott_bytes);
        let ott = ott_fields.get(&1)
            .ok_or_else(|| AppError::Api("Missing OTT in response".to_string()))?
            .clone();

        info!("[get_fresh_ott] New OTT={}...", &ott[..std::cmp::min(ott.len(), 20)]);
        Ok(ott)
    }
}

// ============= Protobuf 编解码工具 =============

/// 编码 protobuf 的 string 字段（field_number, string_value）
fn encode_protobuf_string(field_number: u32, value: &str) -> Vec<u8> {
    let tag = ((field_number << 3) | 2) as u8;
    let value_bytes = value.as_bytes();
    let mut result = Vec::with_capacity(1 + 5 + value_bytes.len());
    result.push(tag);
    // 编码 varint 长度
    let mut len = value_bytes.len();
    loop {
        if len <= 0x7F {
            result.push(len as u8);
            break;
        }
        result.push(((len & 0x7F) | 0x80) as u8);
        len >>= 7;
    }
    result.extend_from_slice(value_bytes);
    result
}

/// 解析 protobuf 消息中的所有 string 字段
fn parse_protobuf_fields(data: &[u8]) -> std::collections::HashMap<u32, String> {
    let mut fields = std::collections::HashMap::new();
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        let field_num = (tag >> 3) as u32;
        let wire_type = tag & 0x07;
        i += 1;

        match wire_type {
            2 => { // length-delimited (string)
                let mut len: usize = 0;
                let mut shift = 0;
                while i < data.len() {
                    let b = data[i];
                    i += 1;
                    len |= ((b & 0x7F) as usize) << shift;
                    if b & 0x80 == 0 { break; }
                    shift += 7;
                }
                if i + len <= data.len() {
                    if let Ok(s) = std::str::from_utf8(&data[i..i + len]) {
                        fields.insert(field_num, s.to_string());
                    }
                    i += len;
                } else {
                    break;
                }
            }
            0 => { // varint
                while i < data.len() {
                    let b = data[i];
                    i += 1;
                    if b & 0x80 == 0 { break; }
                }
            }
            _ => break, // 不支持的 wire type
        }
    }
    fields
}
