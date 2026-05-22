use crate::utils::{AppError, AppResult};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const DEVIN_BASE_URL: &str = "https://app.devin.ai";

/// Devin `/api/users/post-auth` 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevinPostAuthResponse {
    pub org_id: String,
    pub org_name: String,
    #[serde(default)]
    pub is_valid_resource: bool,
    #[serde(default)]
    pub resolved_external_org_id: Option<String>,
    #[serde(default)]
    pub webapp_host: Option<String>,
}

/// Devin `/api/billing/checkout` 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevinCheckoutResponse {
    pub url: String,
}

/// Devin 平台账号服务（订阅 / 试用 checkout）
pub struct DevinService {
    client: Arc<reqwest::Client>,
}

impl Default for DevinService {
    fn default() -> Self {
        Self::new()
    }
}

impl DevinService {
    pub fn new() -> Self {
        Self {
            client: super::get_http_client(),
        }
    }

    /// Step 1: 调 `POST /api/users/post-auth`
    /// 入参：auth1 token（来自 Account.refresh_token，必须以 `auth1_` 开头）
    /// 返回：(org_id, org_name)
    pub async fn post_auth(&self, auth1_token: &str) -> AppResult<DevinPostAuthResponse> {
        let url = format!("{}/api/users/post-auth", DEVIN_BASE_URL);
        info!("[DevinService::post_auth] POST {}", url);

        let resp = match self.client
            .post(&url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", auth1_token))
            .body("{}")
            .send()
            .await
        {
            Ok(r) => {
                super::report_request_success();
                r
            }
            Err(e) => {
                if e.is_timeout() || e.is_connect() {
                    super::report_timeout_error();
                } else {
                    super::report_request_failure();
                }
                return Err(AppError::Network(format!("Devin post-auth failed: {}", e)));
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            warn!("[DevinService::post_auth] HTTP {}: {}", status, text);
            if status.as_u16() == 401
                || text.contains("Unauthenticated")
                || text.contains("invalid_token")
            {
                return Err(AppError::TokenExpired);
            }
            return Err(AppError::Api(format!(
                "Devin post-auth error ({}): {}",
                status, text
            )));
        }

        let parsed: DevinPostAuthResponse = resp.json().await
            .map_err(|e| AppError::Api(format!("Failed to parse Devin post-auth response: {}", e)))?;

        info!(
            "[DevinService::post_auth] OK: org_id={}, org_name={}",
            parsed.org_id, parsed.org_name
        );
        Ok(parsed)
    }

    /// Step 2: 调 `POST /api/billing/checkout` 创建 Stripe checkout session
    /// 入参：auth1 token + 上一步拿到的 org_id 与 org_name
    /// 返回：Stripe checkout URL（如 `https://checkout.stripe.com/c/pay/cs_live_...`）
    pub async fn create_trial_checkout(
        &self,
        auth1_token: &str,
        org_id: &str,
        org_name: &str,
        plan_id: &str,
        is_trial: bool,
    ) -> AppResult<String> {
        let url = format!("{}/api/billing/checkout", DEVIN_BASE_URL);
        let success_url = format!("{}/org/{}/plans?gads_payment={}", DEVIN_BASE_URL, org_name, plan_id);
        let cancel_url = format!("{}/org/{}/plans", DEVIN_BASE_URL, org_name);
        let body = serde_json::json!({
            "plan_id": plan_id,
            "success_url": success_url,
            "cancel_url": cancel_url,
            "is_trial": is_trial,
        });

        info!(
            "[DevinService::create_trial_checkout] POST {} plan_id={} is_trial={} org_id={}",
            url, plan_id, is_trial, org_id
        );

        let resp = match self.client
            .post(&url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", auth1_token))
            .header("X-Cog-Org-Id", org_id)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => {
                super::report_request_success();
                r
            }
            Err(e) => {
                if e.is_timeout() || e.is_connect() {
                    super::report_timeout_error();
                } else {
                    super::report_request_failure();
                }
                return Err(AppError::Network(format!("Devin checkout failed: {}", e)));
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            warn!("[DevinService::create_trial_checkout] HTTP {}: {}", status, text);
            if status.as_u16() == 401
                || text.contains("Unauthenticated")
                || text.contains("invalid_token")
            {
                return Err(AppError::TokenExpired);
            }
            return Err(AppError::Api(format!(
                "Devin checkout error ({}): {}",
                status, text
            )));
        }

        let parsed: DevinCheckoutResponse = resp.json().await
            .map_err(|e| AppError::Api(format!("Failed to parse Devin checkout response: {}", e)))?;

        if parsed.url.is_empty() {
            return Err(AppError::Api("Devin checkout returned empty url".to_string()));
        }

        info!(
            "[DevinService::create_trial_checkout] OK: stripe_url={}...",
            &parsed.url[..parsed.url.len().min(80)]
        );
        Ok(parsed.url)
    }

    /// 一站式：post_auth → create_trial_checkout（Pro + 14 天免费试用）
    /// 返回：(stripe_url, org_id, org_name)
    pub async fn get_trial_checkout_url(
        &self,
        auth1_token: &str,
    ) -> AppResult<(String, String, String)> {
        if !auth1_token.starts_with("auth1_") {
            return Err(AppError::AuthFailed(
                "Devin checkout 需要 auth1 token（refresh_token 应以 auth1_ 开头）".to_string(),
            ));
        }
        let auth = self.post_auth(auth1_token).await?;
        let stripe_url = self.create_trial_checkout(
            auth1_token,
            &auth.org_id,
            &auth.org_name,
            "pro",
            true,
        ).await?;
        Ok((stripe_url, auth.org_id, auth.org_name))
    }

    pub async fn get_plans_url(
        &self,
        auth1_token: &str,
    ) -> AppResult<(String, String, String)> {
        if !auth1_token.starts_with("auth1_") {
            return Err(AppError::AuthFailed(
                "Devin 订阅页面需要 auth1 token（refresh_token 应以 auth1_ 开头）".to_string(),
            ));
        }
        let auth = self.post_auth(auth1_token).await?;
        let plans_url = format!("{}/org/{}/plans", DEVIN_BASE_URL, auth.org_name);
        Ok((plans_url, auth.org_id, auth.org_name))
    }

    pub async fn create_billing_portal_session(
        &self,
        auth1_token: &str,
        org_id: &str,
        org_name: &str,
    ) -> AppResult<String> {
        let url = format!("{}/api/billing/subscription/manage", DEVIN_BASE_URL);
        let no_redirect_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| AppError::Network(format!("Failed to build client: {}", e)))?;

        info!(
            "[DevinService::create_billing_portal_session] GET {} org_id={} org_name={}",
            url, org_id, org_name
        );

        let resp = match no_redirect_client
            .get(&url)
            .header("Accept", "application/json,text/plain,*/*")
            .header("Authorization", format!("Bearer {}", auth1_token))
            .header("X-Cog-Org-Id", org_id)
            .header("Referer", format!("{}/org/{}/plans", DEVIN_BASE_URL, org_name))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                if e.is_timeout() || e.is_connect() {
                    super::report_timeout_error();
                } else {
                    super::report_request_failure();
                }
                return Err(AppError::Network(format!("Devin billing portal failed: {}", e)));
            }
        };

        let status = resp.status().as_u16();
        info!("[DevinService::create_billing_portal_session] status={}", status);

        if (300..400).contains(&status) {
            let location = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            if let Some(loc) = location {
                if loc.contains("billing.stripe.com") {
                    return Ok(loc);
                }
                return Err(AppError::Api(format!("Unexpected redirect target: {}", loc)));
            }
            return Err(AppError::Api(format!("{} response but no Location header", status)));
        }

        let body = resp.text().await.unwrap_or_default();
        if status == 200 {
            if let Ok(j) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(url) = j
                    .get("url")
                    .or_else(|| j.get("portal_url"))
                    .or_else(|| j.get("billing_portal_url"))
                    .and_then(|v| v.as_str())
                {
                    if url.contains("billing.stripe.com") {
                        return Ok(url.to_string());
                    }
                }
            }
            if let Some(url) = body
                .split('"')
                .find(|part| part.contains("billing.stripe.com/p/session/"))
            {
                return Ok(url.to_string());
            }
            return Err(AppError::Api(format!("200 but unexpected body (len={})", body.len())));
        }

        warn!(
            "[DevinService::create_billing_portal_session] HTTP {}: {}",
            status,
            body.chars().take(500).collect::<String>()
        );
        Err(AppError::Api(format!(
            "HTTP {} - {}",
            status,
            body.chars().take(200).collect::<String>()
        )))
    }
}
