use crate::utils::{AppError, AppResult};
use base64::{Engine, engine::general_purpose};
use log::{info, warn};
use reqwest;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

const WINDSURF_BASE_URL: &str = "https://web-backend.windsurf.com";
const STRIPE_API: &str = "https://api.stripe.com";
const STRIPE_VERSION_BASE: &str = "2025-03-31.basil";
const KNOWN_PK: &str = "pk_live_51NRMxXFKuRRGjKOF8UiLeVezJmJe3xlk8tHCRctncoDJmMElhArAYMgN1n5s3tOMdlDyJZZkm1KcEa386dj5XS8d00TmPn497w";

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSeatsResult {
    pub success: bool,
    pub attempts: Vec<AttemptResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AttemptResult {
    pub attempt: usize,
    pub status_code: Option<u16>,
    pub raw_response: Option<String>,
    pub error: Option<String>,
    pub timestamp: String,
}


pub struct WindsurfService {
    client: Arc<reqwest::Client>,
}

fn stripe_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("Accept", HeaderValue::from_static("application/json"));
    headers.insert("Origin", HeaderValue::from_static("https://js.stripe.com"));
    headers.insert("Referer", HeaderValue::from_static("https://js.stripe.com/"));
    headers
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn extract_token_with_prefix(raw: &str, prefix: &str) -> Option<String> {
    raw.find(prefix).map(|start| {
        let suffix = &raw[start..];
        let end = suffix
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(suffix.len());
        suffix[..end].to_string()
    })
}

fn extract_card_details(value: &Value) -> Option<Value> {
    match value {
        Value::Object(map) => {
            if let Some(last4) = map.get("last4").and_then(value_to_string) {
                let brand = map
                    .get("display_brand")
                    .or_else(|| map.get("brand"))
                    .or_else(|| map.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_else(|| "card".to_string());
                let mut payment_method = json!({
                    "type": brand,
                    "last4": last4,
                });
                if let Some(exp_month) = map.get("exp_month").and_then(value_to_string) {
                    payment_method["exp_month"] = json!(exp_month);
                }
                if let Some(exp_year) = map.get("exp_year").and_then(value_to_string) {
                    payment_method["exp_year"] = json!(exp_year);
                }
                return Some(payment_method);
            }

            for child in map.values() {
                if let Some(payment_method) = extract_card_details(child) {
                    return Some(payment_method);
                }
            }
            None
        }
        Value::Array(items) => {
            for item in items {
                if let Some(payment_method) = extract_card_details(item) {
                    return Some(payment_method);
                }
            }
            None
        }
        _ => None,
    }
}

impl WindsurfService {
    pub fn new() -> Self {
        // 使用全局共享的 HTTP 客户端，避免每次请求都创建新实例
        Self {
            client: super::get_http_client(),
        }
    }

    pub fn extract_checkout_session_id(raw: &str) -> Option<String> {
        extract_token_with_prefix(raw, "cs_live_")
            .or_else(|| extract_token_with_prefix(raw, "cs_test_"))
    }

    pub async fn get_stripe_payment_method_from_session(&self, session_id: &str) -> AppResult<Option<Value>> {
        let poll_url = format!("{}/v1/payment_pages/{}/poll", STRIPE_API, session_id);
        let poll_params = [
            ("key", KNOWN_PK),
            ("_stripe_version", STRIPE_VERSION_BASE),
        ];

        let poll_response = self.client
            .get(&poll_url)
            .headers(stripe_headers())
            .query(&poll_params)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Stripe poll request failed: {}", e)))?;

        if !poll_response.status().is_success() {
            warn!("[StripeLookup] poll status={} for {}", poll_response.status(), session_id);
            return Ok(None);
        }

        let poll_result: Value = poll_response
            .json()
            .await
            .map_err(|e| AppError::Api(format!("Stripe poll parse failed: {}", e)))?;

        if let Some(payment_method) = extract_card_details(&poll_result) {
            info!("[StripeLookup] card found directly from poll for {}", session_id);
            return Ok(Some(payment_method));
        }

        let raw = serde_json::to_string(&poll_result).unwrap_or_default();
        let setup_intent_id = poll_result
            .get("setup_intent")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| extract_token_with_prefix(&raw, "seti_"));

        let Some(setup_intent_id) = setup_intent_id else {
            info!("[StripeLookup] no setup_intent found for {}", session_id);
            return Ok(None);
        };

        let Some(client_secret) = extract_token_with_prefix(&raw, &format!("{}_secret_", setup_intent_id)) else {
            info!("[StripeLookup] no client_secret found for {}", session_id);
            return Ok(None);
        };

        let setup_intent_url = format!("{}/v1/setup_intents/{}", STRIPE_API, setup_intent_id);
        let setup_intent_params = vec![
            ("client_secret".to_string(), client_secret),
            ("is_stripe_sdk".to_string(), "false".to_string()),
            ("key".to_string(), KNOWN_PK.to_string()),
            ("_stripe_version".to_string(), STRIPE_VERSION_BASE.to_string()),
        ];

        let setup_intent_response = self.client
            .get(&setup_intent_url)
            .headers(stripe_headers())
            .query(&setup_intent_params)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Stripe setup_intent request failed: {}", e)))?;

        if !setup_intent_response.status().is_success() {
            warn!("[StripeLookup] setup_intent status={} for {}", setup_intent_response.status(), session_id);
            return Ok(None);
        }

        let setup_intent_result: Value = setup_intent_response
            .json()
            .await
            .map_err(|e| AppError::Api(format!("Stripe setup_intent parse failed: {}", e)))?;

        if let Some(payment_method) = extract_card_details(&setup_intent_result) {
            info!("[StripeLookup] card found from setup_intent for {}", session_id);
            return Ok(Some(payment_method));
        }

        let setup_raw = serde_json::to_string(&setup_intent_result).unwrap_or_default();
        let payment_method_id = setup_intent_result
            .get("payment_method")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| extract_token_with_prefix(&setup_raw, "pm_"));

        let Some(payment_method_id) = payment_method_id else {
            info!("[StripeLookup] no payment_method id found for {}", session_id);
            return Ok(None);
        };

        let payment_method_url = format!("{}/v1/payment_methods/{}", STRIPE_API, payment_method_id);
        let payment_method_params = [
            ("key", KNOWN_PK),
            ("_stripe_version", STRIPE_VERSION_BASE),
        ];

        let payment_method_response = self.client
            .get(&payment_method_url)
            .headers(stripe_headers())
            .query(&payment_method_params)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Stripe payment_method request failed: {}", e)))?;

        if !payment_method_response.status().is_success() {
            warn!("[StripeLookup] payment_method status={} for {}", payment_method_response.status(), session_id);
            return Ok(None);
        }

        let payment_method_result: Value = payment_method_response
            .json()
            .await
            .map_err(|e| AppError::Api(format!("Stripe payment_method parse failed: {}", e)))?;

        if let Some(payment_method) = extract_card_details(&payment_method_result) {
            info!("[StripeLookup] card found from payment_method for {}", session_id);
            return Ok(Some(payment_method));
        }

        Ok(None)
    }

    fn build_request_body(&self, token: &str, seat_count: i32) -> Vec<u8> {
        // UpdateSeats的body格式: 0x0a + token长度(varint) + token + 0x10 + seat_count
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();
        
        let mut body = vec![0x0a];
        
        // Token长度（使用varint编码）
        if token_length < 128 {
            body.push(token_length as u8);
        } else {
            // 对于JWT token（通常>1000字节），需要两字节的varint
            body.push(((token_length & 0x7F) | 0x80) as u8);
            body.push((token_length >> 7) as u8);
        }
        
        // Token内容
        body.extend_from_slice(token_bytes);
        
        // 座位数（field 2, varint）
        body.push(0x10);
        body.push(seat_count as u8);
        
        body
    }

    /// 构建更新计划请求体
    /// 
    /// Protobuf 结构 (UpdatePlanRequest):
    /// - Field 1 (LengthDelimited): auth_token (string)
    /// - Field 2 (Varint): price (StripePrice enum)
    /// - Field 3 (Varint): preview (bool) - 预览模式
    /// - Field 4 (Varint): payment_period (PaymentPeriod enum: 1=月付, 2=年付)
    /// - Field 5 (Varint): teams_tier (TeamsTier enum: 1-11)
    fn build_update_plan_body(&self, token: &str, plan_type: &str, payment_period: u8, preview: bool) -> Vec<u8> {
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();

        let mut body = vec![0x0a];

        // Token长度（使用varint编码）
        if token_length < 128 {
            body.push(token_length as u8);
        } else {
            body.push(((token_length & 0x7F) | 0x80) as u8);
            body.push((token_length >> 7) as u8);
        }

        body.extend_from_slice(token_bytes);
        
        // Field 2: price (StripePrice)
        // 1 = STRIPE_PRICE_TEAMS_MONTHLY (月付价格)
        // 2 = STRIPE_PRICE_TEAMS_YEARLY (年付价格)
        body.push(0x10);
        body.push(if payment_period == 2 { 0x02 } else { 0x01 });
        
        // Field 3: preview (bool) - 0x18 = field 3 varint
        if preview {
            body.push(0x18);
            body.push(0x01);
        }
        
        // Field 4: payment_period (0x20 = field 4 varint)
        // 1 = PAYMENT_PERIOD_MONTH (月付)
        // 2 = PAYMENT_PERIOD_YEAR (年付)
        body.push(0x20);
        body.push(if payment_period == 2 { 0x02 } else { 0x01 });
        
        // Field 5: teams_tier (0x28 = field 5 varint)
        body.push(0x28);

        // 根据订阅类型添加不同的后缀字节 (TeamsTier枚举值)
        match plan_type.to_lowercase().as_str() {
            "teams" => body.push(0x01),                    // 1 = TEAMS_TIER_TEAMS
            "pro" => body.push(0x02),                      // 2 = TEAMS_TIER_PRO
            "enterprise_saas" => body.push(0x03),          // 3 = TEAMS_TIER_ENTERPRISE_SAAS
            "hybrid" => body.push(0x04),                   // 4 = TEAMS_TIER_HYBRID
            "enterprise_self_hosted" => body.push(0x05),   // 5 = TEAMS_TIER_ENTERPRISE_SELF_HOSTED
            "waitlist_pro" => body.push(0x06),             // 6 = TEAMS_TIER_WAITLIST_PRO
            "teams_ultimate" => body.push(0x07),           // 7 = TEAMS_TIER_TEAMS_ULTIMATE
            "pro_ultimate" => body.push(0x08),             // 8 = TEAMS_TIER_PRO_ULTIMATE
            "trial" => body.push(0x09),                    // 9 = TEAMS_TIER_TRIAL
            "enterprise_self_serve" => body.push(0x0a),    // 10 = TEAMS_TIER_ENTERPRISE_SELF_SERVE
            "enterprise_saas_pooled" => body.push(0x0b),   // 11 = TEAMS_TIER_ENTERPRISE_SAAS_POOLED
            "enterprise" | _ => body.push(0x0a),           // 默认使用 ENTERPRISE_SELF_SERVE
        }

        body
    }

    /// 构建取消订阅请求体
    ///
    /// Protobuf 结构：
    /// - Field 1 (LengthDelimited): Firebase ID Token
    /// - Field 2 (Varint): 1 (表示取消操作)
    /// - Field 5 (LengthDelimited): 取消原因字符串
    fn build_cancel_plan_body(&self, token: &str, reason: &str) -> Vec<u8> {
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();
        let reason_bytes = reason.as_bytes();
        let reason_length = reason_bytes.len();

        let mut body = vec![0x0a]; // Field 1, wire type 2 (LengthDelimited)

        // Token长度（使用varint编码）
        if token_length < 128 {
            body.push(token_length as u8);
        } else {
            body.push(((token_length & 0x7F) | 0x80) as u8);
            body.push((token_length >> 7) as u8);
        }

        // Token内容
        body.extend_from_slice(token_bytes);

        // Field 2: int32 = 1 (表示取消操作)
        body.push(0x10); // Field 2, wire type 0 (Varint)
        body.push(0x01); // value = 1

        // Field 5: 取消原因字符串
        body.push(0x2a); // Field 5, wire type 2 (LengthDelimited)

        // 原因字符串长度
        if reason_length < 128 {
            body.push(reason_length as u8);
        } else {
            body.push(((reason_length & 0x7F) | 0x80) as u8);
            body.push((reason_length >> 7) as u8);
        }

        // 原因字符串内容
        body.extend_from_slice(reason_bytes);

        body
    }

    /// 构建恢复订阅请求体
    ///
    /// Protobuf 结构：
    /// - Field 1 (LengthDelimited): Firebase ID Token
    /// - Field 3 (Varint): 1 (表示恢复操作)
    fn build_resume_plan_body(&self, token: &str) -> Vec<u8> {
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();

        let mut body = vec![0x0a]; // Field 1, wire type 2 (LengthDelimited)

        // Token长度（使用varint编码）
        if token_length < 128 {
            body.push(token_length as u8);
        } else {
            body.push(((token_length & 0x7F) | 0x80) as u8);
            body.push((token_length >> 7) as u8);
        }

        // Token内容
        body.extend_from_slice(token_bytes);

        // Field 3: int32 = 1 (表示恢复操作)
        body.push(0x18); // Field 3, wire type 0 (Varint)
        body.push(0x01); // value = 1

        body
    }

    fn build_subscribe_to_plan_body(
        &self, 
        token: &str, 
        success_url: &str, 
        cancel_url: &str, 
        teams_tier: i32,
        payment_period: i32,
        team_name: Option<&str>,
        seats: Option<i32>,
        turnstile_token: Option<&str>
    ) -> Vec<u8> {
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();
        let success_url_bytes = success_url.as_bytes();
        let success_url_length = success_url_bytes.len();
        let cancel_url_bytes = cancel_url.as_bytes();
        let cancel_url_length = cancel_url_bytes.len();

        let mut body = Vec::new();

        // 字段1: auth_token (string, field number 1, wire type 2)
        body.push(0x0a); // field 1, wire type 2 (length-delimited)
        let mut len = token_length;
        while len >= 0x80 {
            body.push(((len & 0x7F) | 0x80) as u8);
            len >>= 7;
        }
        body.push(len as u8);
        body.extend_from_slice(token_bytes);

        // 字段3: start_trial = true (bool, field number 3, wire type 0)
        body.push(0x18); // field 3, wire type 0 (0x18 = (3 << 3) | 0)
        body.push(0x01); // value = true

        // 字段4: Success URL (string, field number 4, wire type 2)
        body.push(0x22); // field 4, wire type 2 (0x22 = (4 << 3) | 2)
        body.push(success_url_length as u8);
        body.extend_from_slice(success_url_bytes);

        // 字段5: Cancel URL (string, field number 5, wire type 2)
        body.push(0x2a); // field 5, wire type 2 (0x2a = (5 << 3) | 2)
        body.push(cancel_url_length as u8);
        body.extend_from_slice(cancel_url_bytes);

        // 字段6: seats (int64, field number 6, wire type 0)
        // 只有 Teams/Enterprise 计划需要 seats，Pro 计划不能设置
        if teams_tier == 1 || teams_tier == 3 {
            let seat_count = seats.unwrap_or(1);
            if seat_count > 0 {
                body.push(0x30); // field 6, wire type 0 (0x30 = (6 << 3) | 0)
                body.push(seat_count as u8);
            }
        }

        // 字段7: team_name (string, field number 7, wire type 2) - Teams/Enterprise 需要
        if let Some(name) = team_name {
            if !name.is_empty() {
                let name_bytes = name.as_bytes();
                body.push(0x3a); // field 7, wire type 2 (0x3a = (7 << 3) | 2)
                body.push(name_bytes.len() as u8);
                body.extend_from_slice(name_bytes);
            }
        }

        // 字段8: teams_tier (enum, field number 8, wire type 0)
        body.push(0x40); // field 8, wire type 0 (varint)
        body.push(teams_tier as u8);

        // 字段9: payment_period (enum, field number 9, wire type 0)
        body.push(0x48); // field 9, wire type 0 (varint)
        body.push(payment_period as u8);

        // 字段10: turnstile_token (string, field number 10, wire type 2) - Pro 需要
        if let Some(turnstile) = turnstile_token {
            let turnstile_bytes = turnstile.as_bytes();
            body.push(0x52); // field 10, wire type 2 (0x52 = (10 << 3) | 2)
            let mut tlen = turnstile_bytes.len();
            while tlen >= 0x80 {
                body.push(((tlen & 0x7F) | 0x80) as u8);
                tlen >>= 7;
            }
            body.push(tlen as u8);
            body.extend_from_slice(turnstile_bytes);
        }

        body
    }

    pub async fn update_seats(&self, token: &str, seat_count: i32, retry_times: i32) -> AppResult<UpdateSeatsResult> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/UpdateSeats", WINDSURF_BASE_URL);
        
        let mut attempts = Vec::new();
        let mut success = false;
        
        for i in 0..retry_times {
            let body = self.build_request_body(token, seat_count);
            
            let result = self.client
                .post(&url)
                .body(body)
                .header("accept", "*/*")
                .header("accept-language", "zh-CN,zh;q=0.9")
                .header("cache-control", "no-cache")
                .header("connect-protocol-version", "1")
                .header("content-type", "application/proto")
                .header("pragma", "no-cache")
                .header("priority", "u=1, i")
                .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
                .header("sec-ch-ua-mobile", "?0")
                .header("sec-ch-ua-platform", r#""Windows""#)
                .header("sec-fetch-dest", "empty")
                .header("sec-fetch-mode", "cors")
                .header("sec-fetch-site", "same-site")
                .header("x-debug-email", "")
                .header("x-debug-team-name", "")
                .header("Referer", "https://windsurf.com/")
                .send()
                .await;
            
            match result {
                Ok(response) => {
                    let status_code = response.status().as_u16();
                    let response_bytes = response.bytes().await.unwrap_or_default();
                    
                    // 尝试解析响应
                    let mut raw_response = String::from_utf8_lossy(&response_bytes).to_string();
                    let mut parsed_data = None;
                    
                    // 200 或 204 都表示成功
                    if status_code == 200 || status_code == 204 {
                        // 尝试解析Protobuf响应
                        if response_bytes.len() > 0 {
                            match crate::services::proto_parser::ProtobufParser::parse_update_seats_response(&response_bytes) {
                                Ok(parsed) => {
                                    println!("[UpdateSeats] Successfully parsed response: {:?}", parsed);
                                    parsed_data = Some(parsed.clone());
                                    
                                    // 检查解析后的成功状态
                                    if let Some(parsed_success) = parsed.get("success").and_then(|v| v.as_bool()) {
                                        success = parsed_success;
                                    } else {
                                        success = true; // 如果没有明确的失败标志，视为成功
                                    }
                                    
                                    // 构造更详细的响应
                                    raw_response = parsed.to_string();
                                },
                                Err(e) => {
                                    println!("[UpdateSeats] Failed to parse response: {}", e);
                                    // 解析失败但状态码是200/204，仍视为成功
                                    success = true;
                                }
                            }
                        } else {
                            success = true; // 204 No Content
                        }
                    }
                    
                    // 构造尝试结果
                    let mut attempt_result = AttemptResult {
                        attempt: i as usize + 1,
                        status_code: Some(status_code),
                        raw_response: Some(raw_response),
                        error: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    
                    // 如果有解析数据，添加到结果中
                    if let Some(data) = parsed_data {
                        // 将解析的数据作为JSON字符串存储
                        if let Ok(json_str) = serde_json::to_string_pretty(&data) {
                            attempt_result.raw_response = Some(json_str);
                        }
                    }
                    
                    attempts.push(attempt_result);
                    
                    // 如果成功，直接返回，不需要继续重试
                    if success {
                        break;
                    }
                },
                Err(e) => {
                    attempts.push(AttemptResult {
                        attempt: i as usize + 1,
                        status_code: None,
                        raw_response: None,
                        error: Some(e.to_string()),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }
            }
            
            // 两次请求之间稍作延迟
            if i < retry_times - 1 {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
        
        Ok(UpdateSeatsResult {
            success,
            attempts,
        })
    }

    pub async fn get_team_credit_entries(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GetTeamCreditEntries", WINDSURF_BASE_URL);
        
        // GetTeamCreditEntries的body格式: 0x0a + token长度 + token
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();
        
        let mut full_body = vec![0x0a];
        
        // Token长度（使用varint编码）
        if token_length < 128 {
            full_body.push(token_length as u8);
        } else {
            full_body.push(((token_length & 0x7F) | 0x80) as u8);
            full_body.push((token_length >> 7) as u8);
        }
        
        full_body.extend_from_slice(token_bytes);
        
        println!("[GetTeamCreditEntries] Sending request to {}", url);
        println!("[GetTeamCreditEntries] Token length: {} bytes", token_length);
        println!("[GetTeamCreditEntries] Request body length: {} bytes", full_body.len());
        
        // 打印前几个字节用于调试
        if full_body.len() >= 3 {
            println!("[GetTeamCreditEntries] Body prefix: {:02x} {:02x} {:02x}", full_body[0], full_body[1], full_body[2]);
        }
        
        let result = self.client
            .post(&url)
            .body(full_body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("x-auth-token", token)
            .header("x-debug-email", "")
            .header("x-debug-team-name", "")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await;
        
        match result {
            Ok(response) => {
                let status_code = response.status().as_u16();
                println!("[GetTeamCreditEntries] Response status: {}", status_code);
                
                let response_bytes = response.bytes().await.unwrap_or_default();
                println!("[GetTeamCreditEntries] Response size: {} bytes", response_bytes.len());
                
                if status_code == 200 {
                    // 空响应可能表示没有积分记录
                    if response_bytes.len() == 0 {
                        println!("[GetTeamCreditEntries] Empty response - no credit entries found");
                        return Ok(json!({
                            "success": true,
                            "entries": [],
                            "total_entries": 0,
                            "message": "该团队暂无积分记录"
                        }));
                    }
                    // 打印响应的前100个字节用于调试
                    let preview = if response_bytes.starts_with(b"data:application/proto;base64,") {
                        "Base64 encoded response"
                    } else {
                        "Binary response"
                    };
                    println!("[GetTeamCreditEntries] Response format: {}", preview);
                    
                    // 尝试解析Protobuf响应
                    match crate::services::proto_parser::ProtobufParser::parse_get_team_credit_entries_response(&response_bytes) {
                        Ok(parsed) => {
                            println!("[GetTeamCreditEntries] Successfully parsed credit entries response");
                            println!("[GetTeamCreditEntries] Total entries: {}", 
                                parsed.get("total_entries").and_then(|v| v.as_i64()).unwrap_or(0));
                            Ok(parsed)
                        },
                        Err(e) => {
                            println!("[GetTeamCreditEntries] Failed to parse response: {}", e);
                            // 返回原始响应以便调试
                            let raw_response = if response_bytes.starts_with(b"data:application/proto;base64,") {
                                String::from_utf8_lossy(&response_bytes).to_string()
                            } else {
                                format!("data:application/proto;base64,{}", general_purpose::STANDARD.encode(&response_bytes))
                            };
                            Ok(json!({
                                "success": false,
                                "error": format!("Parse error: {}", e),
                                "raw_response": raw_response
                            }))
                        }
                    }
                } else {
                    println!("[GetTeamCreditEntries] Unexpected status code: {}", status_code);
                    Ok(json!({
                        "success": false,
                        "status_code": status_code,
                        "error": format!("HTTP error: {}", status_code)
                    }))
                }
            },
            Err(e) => {
                println!("[GetTeamCreditEntries] Request failed: {}", e);
                Ok(json!({
                    "success": false,
                    "error": format!("Request failed: {}", e)
                }))
            }
        }
    }
    
    pub async fn get_team_billing(&self, token: &str, auth1_token: Option<&str>) -> AppResult<serde_json::Value> {
        // 新账号 (Windsurf 2.0, refresh_token 以 auth1_ 开头) 走 _backend/ 代理 + Devin 认证头
        // 老账号 (Firebase) 直连 web-backend.windsurf.com + x-auth-token
        let is_new_account = auth1_token.is_some();
        let url = if is_new_account {
            "https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/GetTeamBilling".to_string()
        } else {
            format!("{}/exa.seat_management_pb.SeatManagementService/GetTeamBilling", WINDSURF_BASE_URL)
        };

        // GetTeamBilling的body格式: 0x0a + token长度 + token
        // 注意：不是 0x0a 0xa1 0x07，那是UpdatePlan用的
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();
        
        let mut full_body = vec![0x0a];
        
        // Token长度（使用varint编码）
        if token_length < 128 {
            full_body.push(token_length as u8);
        } else {
            full_body.push(((token_length & 0x7F) | 0x80) as u8);
            full_body.push((token_length >> 7) as u8);
        }
        
        full_body.extend_from_slice(token_bytes);
        
        println!("[GetTeamBilling] Sending request to {} (new_account={})", url, is_new_account);
        
        let result = if is_new_account {
            let a1 = auth1_token.unwrap(); // is_new_account 保证 auth1_token.is_some()
            self.client
                .post(&url)
                .body(full_body)
                .header("content-type", "application/proto")
                .header("connect-protocol-version", "1")
                .header("X-Devin-Auth1-Token", a1)
                .header("X-Devin-Session-Token", token)
                .header("Referer", "https://windsurf.com/")
                .send()
                .await
        } else {
            self.client
                .post(&url)
                .body(full_body)
                .header("accept", "*/*")
                .header("accept-language", "zh-CN,zh;q=0.9")
                .header("cache-control", "no-cache")
                .header("connect-protocol-version", "1")
                .header("content-type", "application/proto")
                .header("pragma", "no-cache")
                .header("priority", "u=1, i")
                .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
                .header("sec-ch-ua-mobile", "?0")
                .header("sec-ch-ua-platform", r#""Windows""#)
                .header("sec-fetch-dest", "empty")
                .header("sec-fetch-mode", "cors")
                .header("sec-fetch-site", "same-site")
                .header("x-auth-token", token)
                .header("x-debug-email", "")
                .header("x-debug-team-name", "")
                .header("Referer", "https://windsurf.com/")
                .send()
                .await
        };
        
        match result {
            Ok(response) => {
                let status_code = response.status().as_u16();
                println!("[GetTeamBilling] Response status: {}", status_code);
                
                let response_bytes = response.bytes().await.unwrap_or_default();
                println!("[GetTeamBilling] Response size: {} bytes", response_bytes.len());
                
                if status_code == 200 && response_bytes.len() > 0 {
                    // 尝试解析Protobuf响应
                    match crate::services::proto_parser::ProtobufParser::parse_get_team_billing_response(&response_bytes) {
                        Ok(parsed) => {
                            println!("[GetTeamBilling] Successfully parsed billing response");
                            Ok(parsed)
                        },
                        Err(e) => {
                            println!("[GetTeamBilling] Failed to parse response: {}", e);
                            Ok(json!({
                                "success": false,
                                "error": format!("Parse error: {}", e),
                                "raw_response": general_purpose::STANDARD.encode(&response_bytes)
                            }))
                        }
                    }
                } else {
                    Ok(json!({
                        "success": false,
                        "status_code": status_code,
                        "error": "Invalid response"
                    }))
                }
            },
            Err(e) => {
                println!("[GetTeamBilling] Request failed: {}", e);
                Ok(json!({
                    "success": false,
                    "error": e.to_string()
                }))
            }
        }
    }

    /// 通过老端点 (web-backend.windsurf.com) 获取 GetTeamBilling
    /// 老端点会返回 payment_method 字段 (subMesssage_10)，而 _backend/ 代理不返回。
    /// get_plan_status 已验证 web-backend.windsurf.com 接受 Devin session_token，
    /// 所以这里也能工作。
    pub async fn get_team_billing_via_old_endpoint(&self, session_token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GetTeamBilling", WINDSURF_BASE_URL);

        let token_bytes = session_token.as_bytes();
        let token_length = token_bytes.len();
        let mut full_body = vec![0x0a];
        if token_length < 128 {
            full_body.push(token_length as u8);
        } else {
            full_body.push(((token_length & 0x7F) | 0x80) as u8);
            full_body.push((token_length >> 7) as u8);
        }
        full_body.extend_from_slice(token_bytes);

        log::info!("[GetTeamBilling-OldEndpoint] Trying web-backend for payment_method...");

        let response = self.client
            .post(&url)
            .body(full_body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", session_token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Old endpoint request failed: {}", e)))?;

        let status_code = response.status().as_u16();
        let response_bytes = response.bytes().await.unwrap_or_default();
        log::info!("[GetTeamBilling-OldEndpoint] status={}, size={}", status_code, response_bytes.len());

        if status_code == 200 && !response_bytes.is_empty() {
            match crate::services::proto_parser::ProtobufParser::parse_get_team_billing_response(&response_bytes) {
                Ok(parsed) => Ok(parsed),
                Err(e) => {
                    log::warn!("[GetTeamBilling-OldEndpoint] parse failed: {}", e);
                    Err(AppError::Api(format!("Parse error: {}", e)))
                }
            }
        } else {
            Err(AppError::Api(format!("HTTP {} (size={})", status_code, response_bytes.len())))
        }
    }

    /// 创建 Stripe Billing Portal Session（新账号专用）
    ///
    /// 对应 Devin 的 `GET https://app.devin.ai/api/billing/subscription/manage` 接口。
    /// 该端点返回 302 重定向到 `https://billing.stripe.com/p/session/live_...`，
    /// 浏览器跟随后即进入官方 Stripe Billing Portal（完整银行卡/发票/订阅管理）。
    ///
    /// # Arguments
    /// * `auth1_token` - auth1_ 开头的 refresh_token，用作 Bearer 认证
    ///
    /// # Returns
    /// 成功: 返回 live_xxx 的完整 URL 字符串，可直接交给系统浏览器打开
    pub async fn create_billing_portal_session(&self, auth1_token: &str) -> AppResult<String> {
        let url = "https://app.devin.ai/api/billing/subscription/manage";

        // 禁用重定向跟随，这样才能从 302 响应里读到 Location
        let no_redirect_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| AppError::Network(format!("Failed to build client: {}", e)))?;

        info!("[CreateBillingPortalSession] GET {}", url);

        let response = no_redirect_client
            .get(url)
            .bearer_auth(auth1_token)
            .header("Accept", "application/json")
            .header("Accept-Language", "zh-CN,zh;q=0.9")
            .header("Referer", "https://app.devin.ai/")
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Request failed: {}", e)))?;

        let status = response.status().as_u16();
        info!("[CreateBillingPortalSession] Response status: {}", status);

        // 3xx 重定向：从 Location 头读取 Stripe URL
        if (300..400).contains(&status) {
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            if let Some(loc) = location {
                info!("[CreateBillingPortalSession] Got Stripe URL via 3xx: {}", loc);
                if loc.contains("billing.stripe.com") {
                    return Ok(loc);
                }
                return Err(AppError::Api(format!(
                    "Unexpected redirect target: {}",
                    loc
                )));
            }
            return Err(AppError::Api(format!(
                "{} response but no Location header",
                status
            )));
        }

        // 2xx 但返回 JSON 的兜底路径: { "url": "https://billing.stripe.com/p/session/live_..." }
        if status == 200 {
            let body_text = response.text().await.unwrap_or_default();
            if let Ok(j) = serde_json::from_str::<serde_json::Value>(&body_text) {
                if let Some(url_s) = j.get("url").and_then(|v| v.as_str()) {
                    if url_s.contains("billing.stripe.com") {
                        info!("[CreateBillingPortalSession] Got Stripe URL via JSON body");
                        return Ok(url_s.to_string());
                    }
                }
            }
            return Err(AppError::Api(format!(
                "200 but unexpected body (len={})",
                body_text.len()
            )));
        }

        // 其它状态码
        let body_text = response.text().await.unwrap_or_default();
        Err(AppError::Api(format!(
            "HTTP {} - {}",
            status,
            body_text.chars().take(200).collect::<String>()
        )))
    }

    /// 更新订阅计划
    /// 
    /// # Arguments
    /// * `token` - Firebase ID Token
    /// * `plan_type` - 计划类型（teams, pro, enterprise 等）
    /// * `payment_period` - 付款周期（1=月付, 2=年付）
    /// * `preview` - 预览模式（true=仅预览不实际执行）
    pub async fn update_plan(&self, token: &str, plan_type: &str, payment_period: u8, preview: bool) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/UpdatePlan", WINDSURF_BASE_URL);
        
        // 验证 payment_period
        let period = if payment_period == 2 { 2 } else { 1 };
        let period_name = if period == 2 { "年付" } else { "月付" };
        
        println!("[UpdatePlan] plan_type={}, period={}, preview={}", plan_type, period_name, preview);
        
        let body = self.build_update_plan_body(token, plan_type, period, preview);
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("x-debug-email", "")
            .header("x-debug-team-name", "")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await?;
        
        let status_code = response.status().as_u16();
        let response_bytes = response.bytes().await.unwrap_or_default();
        
        println!("[UpdatePlan] Response status: {}, size: {} bytes", status_code, response_bytes.len());
        
        // 尝试解析 Protobuf 响应
        if status_code == 200 && response_bytes.len() > 0 {
            match crate::services::proto_parser::ProtobufParser::parse_update_plan_response(&response_bytes) {
                Ok(parsed) => {
                    println!("[UpdatePlan] Successfully parsed response");
                    
                    // 检查是否有支付失败原因
                    let payment_failure = parsed.get("payment_failure_reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    
                    let applied_changes = parsed.get("applied_changes")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    
                    return Ok(serde_json::json!({
                        "success": payment_failure.is_empty() && (preview || applied_changes),
                        "preview": preview,
                        "plan_type": plan_type,
                        "payment_period": period,
                        "payment_period_name": period_name,
                        "status_code": status_code,
                        "applied_changes": applied_changes,
                        "payment_failure_reason": if payment_failure.is_empty() { None } else { Some(payment_failure) },
                        "billing_update": parsed.get("billing_update"),
                        "requires_password_reset": parsed.get("requires_password_reset"),
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }));
                },
                Err(e) => {
                    println!("[UpdatePlan] Failed to parse response: {}", e);
                }
            }
        }
        
        // 解析失败时返回原始响应
        let raw_response = if response_bytes.starts_with(b"data:application/proto;base64,") {
            String::from_utf8_lossy(&response_bytes).to_string()
        } else {
            format!("data:application/proto;base64,{}", general_purpose::STANDARD.encode(&response_bytes))
        };
        
        Ok(serde_json::json!({
            "success": status_code == 200,
            "preview": preview,
            "plan_type": plan_type,
            "payment_period": period,
            "payment_period_name": period_name,
            "status_code": status_code,
            "raw_response": raw_response,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }

    /// 取消订阅
    ///
    /// # Arguments
    /// * `token` - Firebase ID Token
    /// * `reason` - 取消原因（例如："too_expensive", "not_using", "missing_features", "switching_service", "other"）
    ///
    /// # Returns
    /// 返回包含操作结果的 JSON 对象
    pub async fn cancel_plan(&self, token: &str, reason: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/CancelPlan", WINDSURF_BASE_URL);

        println!("[CancelPlan] Canceling subscription with reason: {}", reason);

        let body = self.build_cancel_plan_body(token, reason);

        println!("[CancelPlan] Request body length: {} bytes", body.len());
        println!("[CancelPlan] Request body hex: {}", body.iter().map(|b| format!("{:02x}", b)).collect::<String>());

        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("x-api-key", token)
            .header("x-debug-email", "")
            .header("x-debug-team-name", "")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await?;

        let status_code = response.status().as_u16();
        let response_bytes = response.bytes().await.unwrap_or_default();
        let response_text = String::from_utf8_lossy(&response_bytes).to_string();

        println!("[CancelPlan] Response status: {}", status_code);
        println!("[CancelPlan] Response length: {} bytes", response_bytes.len());

        Ok(serde_json::json!({
            "success": status_code == 200,
            "reason": reason,
            "status_code": status_code,
            "raw_response": response_text,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }

    /// 恢复订阅
    ///
    /// # Arguments
    /// * `token` - Firebase ID Token
    ///
    /// # Returns
    /// 返回包含操作结果的 JSON 对象
    pub async fn resume_plan(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/CancelPlan", WINDSURF_BASE_URL);

        println!("[ResumePlan] Resuming subscription");

        let body = self.build_resume_plan_body(token);

        println!("[ResumePlan] Request body length: {} bytes", body.len());
        println!("[ResumePlan] Request body hex: {}", body.iter().map(|b| format!("{:02x}", b)).collect::<String>());

        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("x-api-key", token)
            .header("x-debug-email", "")
            .header("x-debug-team-name", "")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await?;

        let status_code = response.status().as_u16();
        let response_bytes = response.bytes().await.unwrap_or_default();
        let response_text = String::from_utf8_lossy(&response_bytes).to_string();

        println!("[ResumePlan] Response status: {}", status_code);
        println!("[ResumePlan] Response length: {} bytes", response_bytes.len());

        Ok(serde_json::json!({
            "success": status_code == 200,
            "status_code": status_code,
            "raw_response": response_text,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }

    pub async fn get_current_user(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GetCurrentUser", WINDSURF_BASE_URL);
        
        // 构建请求体：0x0a + token长度(varint) + token + 0x10 0x01 0x18 0x01 0x20 0x01
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();
        
        let mut body = vec![0x0a];
        
        // 添加varint编码的token长度
        if token_length < 128 {
            body.push(token_length as u8);
        } else {
            body.push(((token_length & 0x7F) | 0x80) as u8);
            body.push((token_length >> 7) as u8);
        }
        
        body.extend_from_slice(token_bytes);
        
        // 添加额外的字段
        body.extend_from_slice(&[0x10, 0x01, 0x18, 0x01, 0x20, 0x01]);
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("x-auth-token", token)
            .header("x-debug-email", "")
            .header("x-debug-team-name", "")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await?;
        
        let status_code = response.status().as_u16();
        let response_body = response.bytes().await?;
        
        println!("[GetCurrentUser] Status code: {}", status_code);
        println!("[GetCurrentUser] Response size: {} bytes", response_body.len());
        
        if status_code == 200 {
            // 使用proto_parser解析响应
            match super::proto_parser::parse_get_current_user_response(&response_body) {
                Ok(parsed_result) => {
                    Ok(serde_json::json!({
                        "success": true,
                        "status_code": status_code,
                        "parsed_data": parsed_result["parsed_data"],
                        "user_info": parsed_result["user_info"],
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }))
                },
                Err(parse_error) => {
                    // 解析失败，返回原始响应
                    let response_str = String::from_utf8_lossy(&response_body);
                    let base64_data = if response_str.starts_with("data:application/proto;base64,") {
                        &response_str[31..]
                    } else {
                        &response_str
                    };
                    
                    Ok(serde_json::json!({
                        "success": true,
                        "status_code": status_code,
                        "raw_response": base64_data.trim(),
                        "parse_error": parse_error,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }))
                }
            }
        } else {
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "Failed to get current user",
                "raw_response": String::from_utf8_lossy(&response_body).to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 获取套餐状态（积分/配额信息）
    /// 比 GetCurrentUser 更轻量，专门用于刷新积分状态
    pub async fn get_plan_status(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GetPlanStatus", WINDSURF_BASE_URL);
        
        // 构建请求体：GetPlanStatusRequest { auth_token = 1 }
        // 格式：0x0a + token长度(varint) + token
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();
        
        let mut body = vec![0x0a];
        
        // 添加varint编码的token长度
        if token_length < 128 {
            body.push(token_length as u8);
        } else {
            body.push(((token_length & 0x7F) | 0x80) as u8);
            body.push((token_length >> 7) as u8);
        }
        
        body.extend_from_slice(token_bytes);
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("x-auth-token", token)
            .header("x-debug-email", "")
            .header("x-debug-team-name", "")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await?;
        
        let status_code = response.status().as_u16();
        let response_body = response.bytes().await?;
        
        info!("[GetPlanStatus] Status code: {}, token_prefix={}..., response_size={} bytes", 
            status_code, &token[..std::cmp::min(token.len(), 25)], response_body.len());
        
        if status_code == 200 {
            // 使用proto_parser解析响应
            match super::proto_parser::ProtobufParser::parse_get_plan_status_response(&response_body) {
                Ok(parsed_result) => {
                    Ok(serde_json::json!({
                        "success": true,
                        "status_code": status_code,
                        "plan_status": parsed_result,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }))
                },
                Err(parse_error) => {
                    // 解析失败，返回原始响应
                    let response_str = String::from_utf8_lossy(&response_body);
                    let base64_data = if response_str.starts_with("data:application/proto;base64,") {
                        &response_str[31..]
                    } else {
                        &response_str
                    };
                    
                    Ok(serde_json::json!({
                        "success": true,
                        "status_code": status_code,
                        "raw_response": base64_data.trim(),
                        "parse_error": parse_error,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }))
                }
            }
        } else {
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "Failed to get plan status",
                "raw_response": String::from_utf8_lossy(&response_body).to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 检查用户是否有免费Pro试用资格
    /// CheckProTrialEligibilityRequest: auth_token=1
    /// CheckProTrialEligibilityResponse: is_eligible=1 (bool)
    pub async fn check_pro_trial_eligibility(&self, token: &str) -> AppResult<bool> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/CheckProTrialEligibility", WINDSURF_BASE_URL);

        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();
        let mut body = vec![0x0a];
        if token_length < 128 {
            body.push(token_length as u8);
        } else {
            body.push(((token_length & 0x7F) | 0x80) as u8);
            body.push((token_length >> 7) as u8);
        }
        body.extend_from_slice(token_bytes);

        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await?;

        if status_code == 200 && !response_body.is_empty() {
            // Response: field 1 (varint bool) → 0x08 0x01 = true, 0x08 0x00 or empty = false
            if response_body.len() >= 2 && response_body[0] == 0x08 {
                return Ok(response_body[1] == 0x01);
            }
            // Empty or unexpected body → not eligible
            Ok(false)
        } else {
            Ok(false)
        }
    }

    pub async fn reset_credits(&self, token: &str, seat_count: Option<i32>, last_seat_count: Option<i32>, seat_count_options: &[i32]) -> AppResult<serde_json::Value> {
        // 确定使用的座位数
        let seat_count = if let Some(sc) = seat_count {
            sc
        } else if seat_count_options.is_empty() {
            // 如果没有配置选项，使用默认值
            18
        } else if let Some(last) = last_seat_count {
            // 根据配置的选项轮番切换
            if let Some(current_idx) = seat_count_options.iter().position(|&x| x == last) {
                // 找到当前座位数在选项中的位置，选择下一个
                let next_idx = (current_idx + 1) % seat_count_options.len();
                seat_count_options[next_idx]
            } else {
                // 如果上次使用的座位数不在选项中，使用第一个选项
                seat_count_options[0]
            }
        } else {
            // 没有上次记录，使用第一个选项
            seat_count_options[0]
        };
        
        println!("[ResetCredits] 使用座位数: {}", seat_count);
        
        // 执行一次座位更新即可触发积分重置
        let seats_result = self.update_seats(token, seat_count, 1).await?;
        
        // 直接返回座位更新的结果
        Ok(serde_json::json!({
            "success": seats_result.success,
            "seat_count_used": seat_count,
            "steps": {
                "update_seats": seats_result
            },
            "message": if seats_result.success {
                format!("积分重置成功，座位数已更新为{}", seat_count)
            } else {
                "积分重置失败".to_string()
            },
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }

    /// 重置团队成员的积分
    /// 通过移除成员再重新邀请来重置积分（与团队管理中的逻辑一致）
    pub async fn reset_member_credits(&self, master_token: &str, member_api_key: &str, member_name: &str, member_email: &str) -> AppResult<serde_json::Value> {
        println!("[ResetMemberCredits] 开始重置成员积分: {} ({})", member_name, member_email);
        
        // Step 1: 移除成员
        let remove_result = self.remove_user_from_team(master_token, member_api_key).await;
        if let Err(e) = &remove_result {
            println!("[ResetMemberCredits] 移除成员失败: {}", e);
            return Ok(serde_json::json!({
                "success": false,
                "step": "remove",
                "error": format!("移除成员失败: {}", e),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }));
        }
        
        let remove_data = remove_result.unwrap();
        if !remove_data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Ok(serde_json::json!({
                "success": false,
                "step": "remove",
                "error": "移除成员失败",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }));
        }
        
        println!("[ResetMemberCredits] 成员已移除，开始重新邀请...");
        
        // Step 2: 重新邀请
        let invite_result = self.grant_preapproval(master_token, vec![(member_name.to_string(), member_email.to_string())]).await;
        if let Err(e) = &invite_result {
            println!("[ResetMemberCredits] 重新邀请失败: {}", e);
            return Ok(serde_json::json!({
                "success": false,
                "step": "invite",
                "error": format!("重新邀请失败: {}", e),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }));
        }
        
        let invite_data = invite_result.unwrap();
        if !invite_data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Ok(serde_json::json!({
                "success": false,
                "step": "invite",
                "error": "重新邀请失败",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }));
        }
        
        println!("[ResetMemberCredits] 成员积分重置成功: {}", member_email);
        
        Ok(serde_json::json!({
            "success": true,
            "message": format!("{} 积分已重置，等待接受邀请", member_name),
            "member_email": member_email,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }

    /// 获取试用绑卡链接
    ///
    /// # Arguments
    /// * `token` - JWT token
    /// * `teams_tier` - 团队等级: 1=Teams, 2=Pro, 3=Enterprise
    /// * `payment_period` - 支付周期: 1=月付, 2=年付
    /// * `team_name` - 团队名称 (仅 Teams/Enterprise 需要)
    /// * `seats` - 席位数量 (仅 Teams/Enterprise 需要)
    /// * `turnstile_token` - Turnstile 验证令牌 (Pro 需要)
    ///
    /// # Returns
    /// 返回包含Stripe Checkout链接的JSON对象
    pub async fn subscribe_to_plan(
        &self, 
        token: &str,
        auth1_token: Option<&str>,
        teams_tier: i32,
        payment_period: i32,
        team_name: Option<&str>,
        seats: Option<i32>,
        turnstile_token: Option<&str>
    ) -> AppResult<serde_json::Value> {
        // 新账号用 _backend/ 代理 + Origin 头模拟同源请求（浏览器行为）
        // 老账号直连 web-backend.windsurf.com
        let is_new_account = auth1_token.is_some();
        let url = if is_new_account {
            "https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/SubscribeToPlan".to_string()
        } else {
            format!("{}/exa.seat_management_pb.SeatManagementService/SubscribeToPlan", WINDSURF_BASE_URL)
        };

        // 调试日志
        info!("[SubscribeToPlan] teams_tier={}, payment_period={}, team_name={:?}, seats={:?}, has_turnstile={}, has_auth1={}, url={}", 
            teams_tier, payment_period, team_name, seats, turnstile_token.is_some(), is_new_account, url);

        // 根据计划类型设置回调URL
        // 新账号: 使用与浏览器一致的 subscription-pending URL
        let plan_tier_str = if teams_tier == 1 { "teams" } else if is_new_account { "trial" } else { "pro" };
        let success_url = if is_new_account {
            format!("https://windsurf.com/subscription-pending?expect_tier={}", plan_tier_str)
        } else {
            format!("https://windsurf.com/billing/payment-success?plan_tier={}", plan_tier_str)
        };
        let cancel_url = format!("https://windsurf.com/plan?plan_cancelled=true&plan_tier={}", plan_tier_str);

        let body = self.build_subscribe_to_plan_body(
            token, 
            &success_url, 
            &cancel_url, 
            teams_tier,
            payment_period,
            team_name,
            seats,
            turnstile_token
        );
        
        info!("[SubscribeToPlan] 请求体大小: {} bytes, token_prefix={}...", body.len(), &token[..std::cmp::min(token.len(), 20)]);

        // 新账号: _backend/ 代理 + Devin 认证 headers（通过逆向浏览器 Connect RPC interceptor 发现）
        // 老账号: web-backend 直连 + x-auth-token
        let response = if is_new_account {
            let a1 = auth1_token.unwrap(); // is_new_account 保证 auth1_token.is_some()
            info!("[SubscribeToPlan] 新账号: 使用 X-Devin-Auth1-Token + X-Devin-Session-Token 认证");
            self.client
                .post(&url)
                .body(body)
                .header("content-type", "application/proto")
                .header("connect-protocol-version", "1")
                .header("X-Devin-Auth1-Token", a1)
                .header("X-Devin-Session-Token", token)
                .send()
                .await?
        } else {
            self.client
                .post(&url)
                .body(body)
                .header("accept", "*/*")
                .header("accept-language", "zh-CN,zh;q=0.9")
                .header("cache-control", "no-cache")
                .header("connect-protocol-version", "1")
                .header("content-type", "application/proto")
                .header("pragma", "no-cache")
                .header("priority", "u=1, i")
                .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not:A-Brand";v="99""#)
                .header("sec-ch-ua-mobile", "?0")
                .header("sec-ch-ua-platform", r#""Windows""#)
                .header("sec-fetch-dest", "empty")
                .header("sec-fetch-mode", "cors")
                .header("sec-fetch-site", "same-site")
                .header("x-auth-token", token)
                .header("x-debug-email", "")
                .header("x-debug-team-name", "")
                .header("Referer", "https://windsurf.com/")
                .send()
                .await?
        };

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await?;
        
        info!("[SubscribeToPlan] 响应状态码: {}, 响应体大小: {} bytes", status_code, response_body.len());

        if status_code == 200 {
            // 响应直接是Protobuf二进制数据
            match super::proto_parser::ProtobufParser::new(response_body.to_vec()).parse_message() {
                Ok(parsed) => {
                    // 提取string_1字段（Stripe Checkout链接）
                    let stripe_url = parsed.get("string_1")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if !stripe_url.is_empty() {
                        return Ok(serde_json::json!({
                            "success": true,
                            "status_code": status_code,
                            "stripe_url": stripe_url,
                            "stripe_session_id": Self::extract_checkout_session_id(stripe_url),
                            "teams_tier": teams_tier,
                            "payment_period": payment_period,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }));
                    } else {
                        return Ok(serde_json::json!({
                            "success": false,
                            "status_code": status_code,
                            "error": "响应中未找到Stripe链接",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }));
                    }
                },
                Err(e) => {
                    return Ok(serde_json::json!({
                        "success": false,
                        "status_code": status_code,
                        "error": format!("解析响应失败: {}", e),
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }));
                }
            }
        } else {
            let error_msg = String::from_utf8_lossy(&response_body).to_string();
            info!("[SubscribeToPlan] 错误响应: status={}, body={}", status_code, error_msg);

            // 解析错误信息，提供更友好的提示
            let friendly_error = if status_code == 400 {
                if error_msg.contains("failed_precondition") {
                    if error_msg.contains("no longer able to subscribe") {
                        "该账号已被Windsurf限制订阅，可能之前已使用过试用或被风控，请更换账号".to_string()
                    } else if error_msg.contains("already subscribed") || error_msg.contains("already_subscribed") {
                        "该账号已有订阅计划，无需重复订阅".to_string()
                    } else {
                        format!("账号状态异常，无法订阅: {}", error_msg)
                    }
                } else if error_msg.contains("invalid_argument") {
                    "请求参数错误，可能是价格ID无效或账号不支持此操作".to_string()
                } else if error_msg.contains("turnstile") || error_msg.contains("Turnstile") {
                    "Turnstile 验证失败，请重新验证".to_string()
                } else {
                    format!("请求格式错误: {}", error_msg)
                }
            } else if status_code == 401 || status_code == 403 {
                "认证失败，请先刷新Token后重试".to_string()
            } else if status_code == 404 {
                "API接口不存在".to_string()
            } else {
                format!("获取支付链接失败: {}", error_msg)
            };

            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": friendly_error,
                "error_details": error_msg,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 获取团队配置
    pub async fn get_team_config(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GetTeamConfigRecord", WINDSURF_BASE_URL);

        // 构建请求体 (field 1 = auth_token)
        let mut body = Vec::new();
        let token_bytes = token.as_bytes();
        body.push(0x0A); // field 1, wire type 2 (length-delimited)
        // 写入长度
        let len = token_bytes.len();
        if len < 128 {
            body.push(len as u8);
        } else {
            body.push((len & 0x7F | 0x80) as u8);
            body.push((len >> 7) as u8);
        }
        body.extend_from_slice(token_bytes);

        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        if status_code == 200 {
            // 解析响应为通用格式
            let mut parser = super::proto_parser::ProtobufParser::new(response_body.to_vec());
            let parsed = parser.parse_message().unwrap_or_else(|_| serde_json::json!({}));
            
            Ok(serde_json::json!({
                "success": true,
                "data": parsed,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_text = String::from_utf8_lossy(&response_body).to_string();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "获取团队配置失败",
                "error_details": error_text,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 更新团队配置
    pub async fn update_team_config(&self, token: &str, config: serde_json::Value) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/UpdateTeamConfigExternal", WINDSURF_BASE_URL);

        // 构建请求体
        let mut body = Vec::new();
        
        // field 1 = auth_token
        let token_bytes = token.as_bytes();
        body.push(0x0A);
        // 写入长度
        let len = token_bytes.len();
        if len < 128 {
            body.push(len as u8);
        } else {
            body.push((len & 0x7F | 0x80) as u8);
            body.push((len >> 7) as u8);
        }
        body.extend_from_slice(token_bytes);

        // 根据 config 添加各个字段
        // field 2 = allow_auto_run_commands (bool)
        if let Some(val) = config.get("allow_auto_run_commands").and_then(|v| v.as_bool()) {
            body.push(0x10); // field 2, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 3 = allow_mcp_servers (bool)
        if let Some(val) = config.get("allow_mcp_servers").and_then(|v| v.as_bool()) {
            body.push(0x18); // field 3, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 4 = allow_app_deployments (bool)
        if let Some(val) = config.get("allow_app_deployments").and_then(|v| v.as_bool()) {
            body.push(0x20); // field 4, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 5 = allow_github_reviews (bool)
        if let Some(val) = config.get("allow_github_reviews").and_then(|v| v.as_bool()) {
            body.push(0x28); // field 5, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 6 = allow_github_description_edits (bool)
        if let Some(val) = config.get("allow_github_description_edits").and_then(|v| v.as_bool()) {
            body.push(0x30); // field 6, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 10 = allow_conversation_sharing (bool)
        if let Some(val) = config.get("allow_conversation_sharing").and_then(|v| v.as_bool()) {
            body.push(0x50); // field 10, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 11 = allow_sandbox_app_deployments (bool)
        if let Some(val) = config.get("allow_sandbox_app_deployments").and_then(|v| v.as_bool()) {
            body.push(0x58); // field 11, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 12 = allow_teams_app_deployments (bool)
        if let Some(val) = config.get("allow_teams_app_deployments").and_then(|v| v.as_bool()) {
            body.push(0x60); // field 12, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 13 = allow_attribution (bool)
        if let Some(val) = config.get("allow_attribution").and_then(|v| v.as_bool()) {
            body.push(0x68); // field 13, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 9 = allow_individual_level_analytics (bool)
        if let Some(val) = config.get("allow_individual_level_analytics").and_then(|v| v.as_bool()) {
            body.push(0x48); // field 9, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 16 = allow_browser_experimental_features (bool)
        if let Some(val) = config.get("allow_browser_experimental_features").and_then(|v| v.as_bool()) {
            body.push(0x80); body.push(0x01); // field 16, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 17 = allow_vibe_and_replace (bool)
        if let Some(val) = config.get("allow_vibe_and_replace").and_then(|v| v.as_bool()) {
            body.push(0x88); body.push(0x01); // field 17, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 18 = disable_deepwiki (bool)
        if let Some(val) = config.get("disable_deepwiki").and_then(|v| v.as_bool()) {
            body.push(0x90); body.push(0x01); // field 18, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 19 = disable_codemaps (bool)
        if let Some(val) = config.get("disable_codemaps").and_then(|v| v.as_bool()) {
            body.push(0x98); body.push(0x01); // field 19, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }
        
        // field 20 = allow_codemap_sharing (string)
        if let Some(val) = config.get("allow_codemap_sharing").and_then(|v| v.as_str()) {
            let val_bytes = val.as_bytes();
            body.push(0xA2); body.push(0x01); // field 20, wire type 2
            let len = val_bytes.len();
            if len < 128 {
                body.push(len as u8);
            } else {
                body.push((len & 0x7F | 0x80) as u8);
                body.push((len >> 7) as u8);
            }
            body.extend_from_slice(val_bytes);
        }
        
        // field 21 = disable_fast_context (bool)
        if let Some(val) = config.get("disable_fast_context").and_then(|v| v.as_bool()) {
            body.push(0xA8); body.push(0x01); // field 21, wire type 0
            body.push(if val { 0x01 } else { 0x00 });
        }

        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": "团队配置已更新",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "更新团队配置失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 获取可用模型列表
    pub async fn get_cascade_model_configs(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.api_server_pb.ApiServerService/GetCascadeModelConfigsForSite", WINDSURF_BASE_URL);

        // 构建请求体 (field 6 = auth_token)
        let mut body = Vec::new();
        let token_bytes = token.as_bytes();
        body.push(0x32); // field 6, wire type 2 (length-delimited)
        let len = token_bytes.len();
        if len < 128 {
            body.push(len as u8);
        } else {
            body.push((len & 0x7F | 0x80) as u8);
            body.push((len >> 7) as u8);
        }
        body.extend_from_slice(token_bytes);

        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        if status_code == 200 {
            println!("[GetCascadeModelConfigs] Response size: {} bytes", response_body.len());
            
            let mut parser = super::proto_parser::ProtobufParser::new(response_body.to_vec());
            let parsed = parser.parse_message().unwrap_or_else(|e| {
                println!("[GetCascadeModelConfigs] Parse error: {}", e);
                serde_json::json!({})
            });
            
            println!("[GetCascadeModelConfigs] Parsed keys: {:?}", parsed.as_object().map(|o| o.keys().collect::<Vec<_>>()));
            println!("[GetCascadeModelConfigs] default_off_models_for_teams (int_3): {:?}", parsed.get("int_3"));
            
            Ok(serde_json::json!({
                "success": true,
                "data": parsed,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            println!("[GetCascadeModelConfigs] Error status: {}", status_code);
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "获取模型配置失败",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 获取 Command 模型列表
    pub async fn get_command_model_configs(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.api_server_pb.ApiServerService/GetCommandModelConfigsForSite", WINDSURF_BASE_URL);

        // 构建请求体 (field 1 = auth_token)
        let mut body = Vec::new();
        let token_bytes = token.as_bytes();
        body.push(0x0A); // field 1, wire type 2 (length-delimited)
        let len = token_bytes.len();
        if len < 128 {
            body.push(len as u8);
        } else {
            body.push((len & 0x7F | 0x80) as u8);
            body.push((len >> 7) as u8);
        }
        body.extend_from_slice(token_bytes);

        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        if status_code == 200 {
            println!("[GetCommandModelConfigs] Response size: {} bytes", response_body.len());
            
            let mut parser = super::proto_parser::ProtobufParser::new(response_body.to_vec());
            let parsed = parser.parse_message().unwrap_or_else(|e| {
                println!("[GetCommandModelConfigs] Parse error: {}", e);
                serde_json::json!({})
            });
            
            println!("[GetCommandModelConfigs] Parsed keys: {:?}", parsed.as_object().map(|o| o.keys().collect::<Vec<_>>()));
            
            Ok(serde_json::json!({
                "success": true,
                "data": parsed,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            println!("[GetCommandModelConfigs] Error status: {}", status_code);
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "获取 Command 模型配置失败",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 获取团队模型控制配置
    pub async fn get_team_organizational_controls(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.api_server_pb.ApiServerService/GetTeamOrganizationalControlsForSite", WINDSURF_BASE_URL);

        // 构建请求体 (field 1 = auth_token)
        let mut body = Vec::new();
        let token_bytes = token.as_bytes();
        body.push(0x0A); // field 1, wire type 2
        let len = token_bytes.len();
        if len < 128 {
            body.push(len as u8);
        } else {
            body.push((len & 0x7F | 0x80) as u8);
            body.push((len >> 7) as u8);
        }
        body.extend_from_slice(token_bytes);

        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        if status_code == 200 {
            println!("[GetTeamOrganizationalControls] Response size: {} bytes", response_body.len());
            
            let mut parser = super::proto_parser::ProtobufParser::new(response_body.to_vec());
            let parsed = parser.parse_message().unwrap_or_else(|e| {
                println!("[GetTeamOrganizationalControls] Parse error: {}", e);
                serde_json::json!({})
            });
            
            println!("[GetTeamOrganizationalControls] Parsed keys: {:?}", parsed.as_object().map(|o| o.keys().collect::<Vec<_>>()));
            println!("[GetTeamOrganizationalControls] Full response: {}", serde_json::to_string_pretty(&parsed).unwrap_or_default());
            
            // 检查 subMesssage_1 (controls) 中的字段
            if let Some(controls) = parsed.get("subMesssage_1") {
                println!("[GetTeamOrganizationalControls] Controls keys: {:?}", controls.as_object().map(|o| o.keys().collect::<Vec<_>>()));
                println!("[GetTeamOrganizationalControls] team_id (string_1): {:?}", controls.get("string_1"));
                println!("[GetTeamOrganizationalControls] cascade_model_labels (string_2): {:?}", controls.get("string_2"));
                println!("[GetTeamOrganizationalControls] command_model_labels (string_3): {:?}", controls.get("string_3"));
            }
            
            Ok(serde_json::json!({
                "success": true,
                "data": parsed,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            println!("[GetTeamOrganizationalControls] Error status: {}", status_code);
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "获取团队模型配置失败",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 更新团队模型控制配置
    pub async fn upsert_team_organizational_controls(
        &self, 
        token: &str, 
        team_id: &str,
        cascade_models: Vec<String>,
        command_models: Vec<String>,
        extension_models: Vec<String>,
    ) -> AppResult<serde_json::Value> {
        println!("[UpsertTeamOrgControls] team_id={}, cascade={:?}, command={:?}, extension={:?}", 
            team_id, cascade_models, command_models, extension_models);
        
        // 验证 team_id 不为空
        if team_id.is_empty() {
            return Ok(serde_json::json!({
                "success": false,
                "error": "团队ID为空，无法保存模型配置",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }));
        }
        
        let url = format!("{}/exa.api_server_pb.ApiServerService/UpsertTeamOrganizationalControlsForSite", WINDSURF_BASE_URL);

        // 构建请求体
        let mut body = Vec::new();
        
        // field 1 = TeamOrganizationalControls (嵌套消息)
        let mut controls = Vec::new();
        
        // TeamOrganizationalControls.team_id (field 1)
        let team_id_bytes = team_id.as_bytes();
        controls.push(0x0A); // field 1, wire type 2
        controls.push(team_id_bytes.len() as u8);
        controls.extend_from_slice(team_id_bytes);
        
        // TeamOrganizationalControls.cascade_model_labels (field 2, repeated)
        for model in &cascade_models {
            let model_bytes = model.as_bytes();
            controls.push(0x12); // field 2, wire type 2
            controls.push(model_bytes.len() as u8);
            controls.extend_from_slice(model_bytes);
        }
        
        // TeamOrganizationalControls.command_model_labels (field 3, repeated)
        for model in &command_models {
            let model_bytes = model.as_bytes();
            controls.push(0x1A); // field 3, wire type 2
            controls.push(model_bytes.len() as u8);
            controls.extend_from_slice(model_bytes);
        }
        
        // TeamOrganizationalControls.extension_model_labels (field 6, repeated)
        for model in &extension_models {
            let model_bytes = model.as_bytes();
            controls.push(0x32); // field 6, wire type 2
            controls.push(model_bytes.len() as u8);
            controls.extend_from_slice(model_bytes);
        }
        
        // 写入 controls 到 body (field 1)
        body.push(0x0A); // field 1, wire type 2
        let controls_len = controls.len();
        if controls_len < 128 {
            body.push(controls_len as u8);
        } else if controls_len < 16384 {
            body.push((controls_len & 0x7F | 0x80) as u8);
            body.push((controls_len >> 7) as u8);
        } else {
            body.push((controls_len & 0x7F | 0x80) as u8);
            body.push(((controls_len >> 7) & 0x7F | 0x80) as u8);
            body.push((controls_len >> 14) as u8);
        }
        body.extend_from_slice(&controls);
        
        // field 2 = auth_token
        let token_bytes = token.as_bytes();
        body.push(0x12); // field 2, wire type 2
        let len = token_bytes.len();
        if len < 128 {
            body.push(len as u8);
        } else {
            body.push((len & 0x7F | 0x80) as u8);
            body.push((len >> 7) as u8);
        }
        body.extend_from_slice(token_bytes);

        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": "模型配置已更新",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "更新模型配置失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 获取可用的 MCP 插件列表
    pub async fn get_available_mcp_plugins(&self, api_key: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.cascade_plugins_pb.CascadePluginsService/GetAvailableCascadePlugins", WINDSURF_BASE_URL);

        let request_body = serde_json::json!({
            "metadata": {
                "ideName": "windsurf",
                "extensionVersion": "1.0.0",
                "apiKey": api_key,
                "os": "unknown",
                "ideVersion": "1.3.7"
            }
        });

        let response = self.client
            .post(&url)
            .json(&request_body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/json")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="142", "Google Chrome";v="142", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.text().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        if status_code == 200 {
            // 解析 JSON 响应
            let parsed: serde_json::Value = serde_json::from_str(&response_body)
                .unwrap_or_else(|_| serde_json::json!({}));
            
            Ok(serde_json::json!({
                "success": true,
                "data": parsed,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "获取MCP插件列表失败",
                "error_details": response_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 删除用户 (DeleteUser API)
    /// DeleteUserRequest: auth_token=1, api_key=3
    pub async fn delete_user(&self, token: &str, api_key: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/DeleteUser", WINDSURF_BASE_URL);

        // 构造 protobuf 请求体
        // field 1: auth_token (string)
        // field 3: api_key (string)
        let mut request_body = Vec::new();
        
        // Field 1: auth_token
        let token_bytes = token.as_bytes();
        request_body.push(0x0a); // field 1, wire type 2 (length-delimited)
        let token_len = token_bytes.len();
        if token_len < 128 {
            request_body.push(token_len as u8);
        } else {
            request_body.push((token_len & 0x7F | 0x80) as u8);
            request_body.push((token_len >> 7) as u8);
        }
        request_body.extend_from_slice(token_bytes);
        
        // Field 3: api_key
        let api_key_bytes = api_key.as_bytes();
        request_body.push(0x1a); // field 3, wire type 2 (length-delimited)
        let api_key_len = api_key_bytes.len();
        if api_key_len < 128 {
            request_body.push(api_key_len as u8);
        } else {
            request_body.push((api_key_len & 0x7F | 0x80) as u8);
            request_body.push((api_key_len >> 7) as u8);
        }
        request_body.extend_from_slice(api_key_bytes);

        log::info!("[DeleteUser] Request body size: {} bytes", request_body.len());

        let response = self.client
            .post(&url)
            .body(request_body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Chromium";v="136", "Google Chrome";v="136", "Not_A Brand";v="99""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Windows""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .header("x-debug-email", "")
            .header("x-debug-team-name", "")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        log::info!("[DeleteUser] Response status: {}", status_code);

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": "用户已删除",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            
            log::error!("[DeleteUser] Error: {}", error_body);
            
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "删除用户失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    // ==================== 团队成员管理 API ====================

    /// 辅助方法：编码 varint 长度的字符串字段
    fn encode_string_field(&self, field_num: u8, value: &str) -> Vec<u8> {
        let mut result = Vec::new();
        let bytes = value.as_bytes();
        let len = bytes.len();
        
        // field tag: (field_num << 3) | 2 (wire type 2 = length-delimited)
        result.push((field_num << 3) | 2);
        
        // varint length
        if len < 128 {
            result.push(len as u8);
        } else {
            result.push((len & 0x7F | 0x80) as u8);
            result.push((len >> 7) as u8);
        }
        
        result.extend_from_slice(bytes);
        result
    }

    /// 获取团队成员列表 (GetUsers API)
    /// 需要管理员权限
    pub async fn get_team_members(&self, token: &str, group_id: Option<&str>) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GetUsers", WINDSURF_BASE_URL);
        
        let mut body = self.encode_string_field(1, token);
        
        // field 2: group_id (optional)
        if let Some(gid) = group_id {
            body.extend(self.encode_string_field(2, gid));
        }
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        println!("[GetTeamMembers] Status: {}, Size: {} bytes", status_code, response_body.len());

        if status_code == 200 && !response_body.is_empty() {
            let mut parser = super::proto_parser::ProtobufParser::new(response_body.to_vec());
            let parsed = parser.parse_message().unwrap_or_else(|_| serde_json::json!({}));
            
            Ok(serde_json::json!({
                "success": true,
                "data": parsed,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "获取团队成员失败",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 邀请成员加入团队 (GrantPreapproval API)
    /// 需要管理员权限
    pub async fn grant_preapproval(&self, token: &str, users: Vec<(String, String)>) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GrantPreapproval", WINDSURF_BASE_URL);
        
        let mut body = self.encode_string_field(1, token);
        
        // field 2: repeated PreapprovalUserItem
        for (name, email) in &users {
            let mut item = Vec::new();
            item.extend(self.encode_string_field(1, name));
            item.extend(self.encode_string_field(2, email));
            
            // 嵌入消息: field 2, wire type 2
            body.push(0x12);
            let item_len = item.len();
            if item_len < 128 {
                body.push(item_len as u8);
            } else {
                body.push((item_len & 0x7F | 0x80) as u8);
                body.push((item_len >> 7) as u8);
            }
            body.extend(item);
        }
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        println!("[GrantPreapproval] Status: {}, Size: {} bytes", status_code, response_body.len());

        if status_code == 200 {
            let mut parser = super::proto_parser::ProtobufParser::new(response_body.to_vec());
            let parsed = parser.parse_message().unwrap_or_else(|_| serde_json::json!({}));
            
            Ok(serde_json::json!({
                "success": true,
                "data": parsed,
                "invited_count": users.len(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_text = String::from_utf8_lossy(&response_body).to_string();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "邀请成员失败",
                "error_details": error_text,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 从团队中移除成员 (RemoveUserFromTeam API)
    /// 需要管理员权限
    pub async fn remove_user_from_team(&self, token: &str, api_key: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/RemoveUserFromTeam", WINDSURF_BASE_URL);
        
        let mut body = self.encode_string_field(1, token);
        body.extend(self.encode_string_field(2, api_key));
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        println!("[RemoveUserFromTeam] Status: {}", status_code);

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": "成员已移除",
                "removed_api_key": api_key,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "移除成员失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 撤销预审批邀请 (RevokePreapproval API)
    /// 需要管理员权限
    pub async fn revoke_preapproval(&self, token: &str, approval_id: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/RevokePreapproval", WINDSURF_BASE_URL);
        
        let mut body = self.encode_string_field(1, token);
        body.extend(self.encode_string_field(2, approval_id));
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        println!("[RevokePreapproval] Status: {}", status_code);

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": "邀请已撤销",
                "approval_id": approval_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "撤销邀请失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 获取所有待处理的预审批邀请 (GetPreapprovals API)
    /// 需要管理员权限
    pub async fn get_preapprovals(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GetPreapprovals", WINDSURF_BASE_URL);
        
        let body = self.encode_string_field(1, token);
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        println!("[GetPreapprovals] Status: {}, Size: {} bytes", status_code, response_body.len());

        if status_code == 200 {
            if response_body.is_empty() {
                return Ok(serde_json::json!({
                    "success": true,
                    "preapprovals": [],
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
            }
            
            let mut parser = super::proto_parser::ProtobufParser::new(response_body.to_vec());
            let parsed = parser.parse_message().unwrap_or_else(|_| serde_json::json!({}));
            
            Ok(serde_json::json!({
                "success": true,
                "data": parsed,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "获取预审批列表失败",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 获取当前用户的待处理邀请 (GetPreapprovalForUser API)
    /// 普通用户权限
    pub async fn get_preapproval_for_user(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GetPreapprovalForUser", WINDSURF_BASE_URL);
        
        let body = self.encode_string_field(1, token);
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        println!("[GetPreapprovalForUser] Status: {}, Size: {} bytes", status_code, response_body.len());

        if status_code == 200 {
            if response_body.is_empty() {
                return Ok(serde_json::json!({
                    "success": true,
                    "has_pending_invitation": false,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
            }
            
            let mut parser = super::proto_parser::ProtobufParser::new(response_body.to_vec());
            let parsed = parser.parse_message().unwrap_or_else(|_| serde_json::json!({}));
            
            Ok(serde_json::json!({
                "success": true,
                "has_pending_invitation": true,
                "data": parsed,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            // 404 通常表示没有待处理邀请
            if status_code == 404 {
                return Ok(serde_json::json!({
                    "success": true,
                    "has_pending_invitation": false,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
            }
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "获取邀请信息失败",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 接受团队邀请 (AcceptPreapproval API)
    /// 普通用户权限
    pub async fn accept_preapproval(&self, token: &str, approval_id: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/AcceptPreapproval", WINDSURF_BASE_URL);
        
        let mut body = self.encode_string_field(1, token);
        body.extend(self.encode_string_field(2, approval_id));
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        println!("[AcceptPreapproval] Status: {}", status_code);

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": "已接受邀请，成功加入团队",
                "approval_id": approval_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "接受邀请失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 拒绝团队邀请 (RejectPreapproval API)
    /// 普通用户权限
    pub async fn reject_preapproval(&self, token: &str, approval_id: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/RejectPreapproval", WINDSURF_BASE_URL);
        
        let mut body = self.encode_string_field(1, token);
        body.extend(self.encode_string_field(2, approval_id));
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        println!("[RejectPreapproval] Status: {}", status_code);

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": "已拒绝邀请",
                "approval_id": approval_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "拒绝邀请失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 申请加入团队 (RequestTeamAccess API)
    /// 普通用户通过邀请链接申请加入团队
    pub async fn request_team_access(&self, api_key: &str, invite_id: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/RequestTeamAccess", WINDSURF_BASE_URL);
        
        let mut body = self.encode_string_field(1, api_key);
        body.extend(self.encode_string_field(2, invite_id));
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await
            .map_err(|e| AppError::Api(e.to_string()))?;

        println!("[RequestTeamAccess] Status: {}, Size: {} bytes", status_code, response_body.len());

        if status_code == 200 {
            if response_body.is_empty() {
                return Ok(serde_json::json!({
                    "success": true,
                    "message": "加入申请已提交，等待管理员审批",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
            }
            
            let mut parser = super::proto_parser::ProtobufParser::new(response_body.to_vec());
            let parsed = parser.parse_message().unwrap_or_else(|_| serde_json::json!({}));
            
            Ok(serde_json::json!({
                "success": true,
                "message": "加入申请已提交",
                "data": parsed,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_text = String::from_utf8_lossy(&response_body).to_string();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "申请加入团队失败",
                "error_details": error_text,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 更新用户团队状态 (UpdateUserTeamStatus API)
    /// 管理员审批用户的加入申请
    /// status: 2=APPROVED(同意), 3=REJECTED(拒绝)
    pub async fn update_user_team_status(&self, token: &str, user_api_key: &str, status: u8) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/UpdateUserTeamStatus", WINDSURF_BASE_URL);
        
        // 构建嵌套消息: { api_key: string, status: int }
        let mut inner_msg = self.encode_string_field(1, user_api_key);
        // field 2 (status), wire type 0 (varint)
        inner_msg.push(0x10);
        inner_msg.push(status);
        
        // 构建外层消息
        let mut body = self.encode_string_field(1, token);
        // field 2, wire type 2 (嵌套消息)
        body.push(0x12);
        let inner_len = inner_msg.len();
        if inner_len < 128 {
            body.push(inner_len as u8);
        } else {
            body.push((inner_len & 0x7F | 0x80) as u8);
            body.push((inner_len >> 7) as u8);
        }
        body.extend(inner_msg);
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        println!("[UpdateUserTeamStatus] Status: {}", status_code);

        let status_text = match status {
            2 => "已批准加入",
            3 => "已拒绝加入",
            _ => "状态已更新",
        };

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": status_text,
                "user_api_key": user_api_key,
                "new_status": status,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "更新用户状态失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    // ==================== 自动充值管理 API ====================

    /// 辅助方法：编码 varint
    fn encode_varint(&self, value: u64) -> Vec<u8> {
        let mut result = Vec::new();
        let mut val = value;
        loop {
            let mut byte = (val & 0x7F) as u8;
            val >>= 7;
            if val != 0 {
                byte |= 0x80;
            }
            result.push(byte);
            if val == 0 {
                break;
            }
        }
        result
    }

    /// 更新自动充值设置 (UpdateCreditTopUpSettings API)
    /// 需要管理员权限
    pub async fn update_credit_top_up_settings(
        &self,
        token: &str,
        enabled: bool,
        monthly_top_up_amount: i32,
        top_up_increment: i32,
    ) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/UpdateCreditTopUpSettings", WINDSURF_BASE_URL);
        
        // 构建 protobuf 消息
        let mut body = self.encode_string_field(1, token);
        
        // field 2: enabled (bool as varint)
        body.push(0x10); // field 2, wire type 0
        body.push(if enabled { 1 } else { 0 });
        
        // field 3: monthly_top_up_amount (int32 as varint)
        body.push(0x18); // field 3, wire type 0
        body.extend(self.encode_varint(monthly_top_up_amount as u64));
        
        // field 4: top_up_increment (int32 as varint)
        body.push(0x20); // field 4, wire type 0
        body.extend(self.encode_varint(top_up_increment as u64));
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        println!("[UpdateCreditTopUpSettings] Status: {}", status_code);

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": if enabled { "自动充值已启用" } else { "自动充值已禁用" },
                "enabled": enabled,
                "monthly_top_up_amount": monthly_top_up_amount,
                "top_up_increment": top_up_increment,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "更新自动充值设置失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 获取自动充值设置（从 GetPlanStatus 响应的 subMessage_10 中提取）
    /// subMessage_10 包含:
    /// - int_1: 状态
    /// - int_2: enabled (1=启用)
    /// - int_3: monthly_top_up_amount (单位: 分/100)
    /// - int_5: top_up_increment (单位: 分)
    pub async fn get_credit_top_up_settings(&self, token: &str) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/GetPlanStatus", WINDSURF_BASE_URL);
        
        // 构建请求体
        let token_bytes = token.as_bytes();
        let token_length = token_bytes.len();
        
        let mut body = vec![0x0a];
        if token_length < 128 {
            body.push(token_length as u8);
        } else {
            body.push(((token_length & 0x7F) | 0x80) as u8);
            body.push((token_length >> 7) as u8);
        }
        body.extend_from_slice(token_bytes);
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("accept-language", "zh-CN,zh;q=0.9")
            .header("cache-control", "no-cache")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("pragma", "no-cache")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await?;
        
        let status_code = response.status().as_u16();
        let response_body = response.bytes().await?;
        
        println!("[GetCreditTopUpSettings] Status: {}", status_code);
        
        if status_code == 200 {
            // 解析 protobuf 响应
            match super::proto_parser::ProtobufParser::parse_get_plan_status_response(&response_body) {
                Ok(parsed) => {
                    // 从 raw_data.subMesssage_1.subMesssage_10 提取自动充值设置
                    let top_up_status = parsed
                        .get("raw_data")
                        .and_then(|d| d.get("subMesssage_1"))
                        .and_then(|s| s.get("subMesssage_10"));
                    
                    if let Some(top_up) = top_up_status {
                        let enabled = top_up["int_2"].as_i64().unwrap_or(0) == 1;
                        // API 返回的值单位已经是美分，直接使用
                        let monthly_top_up_amount = top_up["int_3"].as_i64().unwrap_or(0) as i32;
                        let top_up_increment = top_up["int_5"].as_i64().unwrap_or(0) as i32;
                        
                        return Ok(serde_json::json!({
                            "success": true,
                            "top_up_enabled": enabled,
                            "monthly_top_up_amount": monthly_top_up_amount,
                            "top_up_increment": top_up_increment as i32,
                            "top_up_spent": 0,
                            "raw_top_up_status": top_up,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        }));
                    }
                    
                    // 如果没有 top_up_status，返回完整解析结果便于调试
                    Ok(serde_json::json!({
                        "success": true,
                        "top_up_enabled": false,
                        "monthly_top_up_amount": 0,
                        "top_up_increment": 0,
                        "top_up_spent": 0,
                        "note": "No top_up_status found in response",
                        "raw_parsed": parsed,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }))
                },
                Err(e) => {
                    Ok(serde_json::json!({
                        "success": false,
                        "error": format!("解析响应失败: {}", e),
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }))
                }
            }
        } else {
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "获取自动充值设置失败",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 更新成员的 Windsurf 访问权限 (UpdateCodeiumAccess API)
    /// disable_access: true = 禁用访问, false = 启用访问
    pub async fn update_codeium_access(&self, token: &str, api_key: &str, disable_access: bool) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/UpdateCodeiumAccess", WINDSURF_BASE_URL);
        
        // 构建请求体：auth_token(1) + api_key(2) + disable_codeium_access(3)
        let mut body = self.encode_string_field(1, token);
        body.extend(self.encode_string_field(2, api_key));
        // bool 字段编码：field_num << 3 | 0, 然后是值（0或1）
        body.push((3 << 3) | 0); // field 3, wire type 0 (varint)
        body.push(if disable_access { 1 } else { 0 });
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        println!("[UpdateCodeiumAccess] Status: {}, disable={}", status_code, disable_access);

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": if disable_access { "已禁用 Windsurf 访问" } else { "已启用 Windsurf 访问" },
                "api_key": api_key,
                "disabled": disable_access,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "更新访问权限失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 添加用户角色 (AddUserRole API)
    /// role: 角色名称，如 "admin", "billing.admin" 等
    pub async fn add_user_role(&self, token: &str, api_key: &str, role: &str, group_id: Option<&str>) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/AddUserRole", WINDSURF_BASE_URL);
        
        // 构建请求体：auth_token(1) + api_key(2) + role(3) + group_id(4, optional)
        let mut body = self.encode_string_field(1, token);
        body.extend(self.encode_string_field(2, api_key));
        body.extend(self.encode_string_field(3, role));
        if let Some(gid) = group_id {
            body.extend(self.encode_string_field(4, gid));
        }
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        println!("[AddUserRole] Status: {}, role={}", status_code, role);

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": format!("已添加角色: {}", role),
                "api_key": api_key,
                "role": role,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "添加角色失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }

    /// 移除用户角色 (RemoveUserRole API)
    pub async fn remove_user_role(&self, token: &str, api_key: &str, role: &str, group_id: Option<&str>) -> AppResult<serde_json::Value> {
        let url = format!("{}/exa.seat_management_pb.SeatManagementService/RemoveUserRole", WINDSURF_BASE_URL);
        
        // 构建请求体：auth_token(1) + api_key(2) + role(3) + group_id(4, optional)
        let mut body = self.encode_string_field(1, token);
        body.extend(self.encode_string_field(2, api_key));
        body.extend(self.encode_string_field(3, role));
        if let Some(gid) = group_id {
            body.extend(self.encode_string_field(4, gid));
        }
        
        let response = self.client
            .post(&url)
            .body(body)
            .header("accept", "*/*")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/proto")
            .header("x-auth-token", token)
            .header("Referer", "https://windsurf.com/")
            .send()
            .await
            .map_err(|e| AppError::Api(e.to_string()))?;

        let status_code = response.status().as_u16();
        println!("[RemoveUserRole] Status: {}, role={}", status_code, role);

        if status_code == 200 {
            Ok(serde_json::json!({
                "success": true,
                "message": format!("已移除角色: {}", role),
                "api_key": api_key,
                "role": role,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        } else {
            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            Ok(serde_json::json!({
                "success": false,
                "status_code": status_code,
                "error": "移除角色失败",
                "error_details": error_body,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
    }
}
