use crate::models::{Account, OperationLog, OperationType, OperationStatus};
use crate::repository::DataStore;
use crate::services::{AuthService, WindsurfService};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportAccountInput {
    pub email: String,
    pub password: String,
    pub remark: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchImportItemResult {
    #[serde(skip_serializing)]
    input_key: String,
    email: String,
    success: bool,
    account_id: Option<String>,
    error: Option<String>,
    skipped: bool,
    login_success: bool,
    offline: bool,
    retry_success: bool,
}

#[tauri::command]
pub async fn add_account(
    email: String,
    password: String,
    nickname: String,
    tags: Vec<String>,
    group: Option<String>,
    account_source: Option<String>,
    store: State<'_, Arc<DataStore>>,
) -> Result<Account, String> {
    let mut account = store.add_account_no_save(email.clone(), password, nickname, group, account_source.clone())
        .await
        .map_err(|e| e.to_string())?;
    
    // 设置标签和分组
    account.tags = tags;
    // 设置账号来源（windsurf / devin），未提供则保持 None（视为 windsurf）
    if let Some(src) = account_source {
        let trimmed = src.trim();
        if !trimmed.is_empty() {
            account.account_source = Some(trimmed.to_string());
        }
    }
    
    store.update_account_no_save(account.clone())
        .await
        .map_err(|e| e.to_string())?;
    store.inner().request_save_coalesced();
    
    // 记录日志
    let log = OperationLog::new(
        OperationType::AddAccount,
        OperationStatus::Success,
        format!("添加账号: {}", email),
    )
    .with_account(account.id, email);
    
    let _ = store.add_log(log).await;
    
    Ok(account)
}

/// 通过 refresh_token 添加账号
/// 使用 refresh_token 获取 access_token，然后获取用户信息并创建账号
#[tauri::command]
pub async fn add_account_by_refresh_token(
    refresh_token: String,
    nickname: Option<String>,
    tags: Vec<String>,
    group: Option<String>,
    account_source: Option<String>,
    store: State<'_, Arc<DataStore>>,
) -> Result<serde_json::Value, String> {
    let auth_service = AuthService::new();
    
    // Step 1: 使用 refresh_token 获取 access_token
    let (token, new_refresh_token, expires_at, email) = if refresh_token.starts_with("auth1_") {
        let (token, new_refresh_token, expires_at) = auth_service.refresh_session_with_auth1(&refresh_token)
            .await
            .map_err(|e| format!("刷新Token失败: {}", e))?;

        let windsurf_service = WindsurfService::new();
        let user_info_result = windsurf_service.get_current_user(&token)
            .await
            .map_err(|e| format!("获取用户信息失败: {}", e))?;
        let email = user_info_result
            .get("user_info")
            .and_then(|v| v.get("user"))
            .and_then(|v| v.get("email"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if email.is_empty() {
            return Err("获取用户信息失败: 未找到邮箱".to_string());
        }

        (token, new_refresh_token, expires_at, email)
    } else {
        let (token, new_refresh_token, expires_at) = auth_service.refresh_token(&refresh_token)
            .await
            .map_err(|e| format!("刷新Token失败: {}", e))?;

        // Step 2: 使用 token 获取用户信息
        let account_info = auth_service.get_account_info(&token)
            .await
            .map_err(|e| format!("获取用户信息失败: {}", e))?;

        (token, new_refresh_token, expires_at, account_info.email.clone())
    };
    
    // 检查账号是否已存在
    let existing_accounts = store.get_all_accounts()
        .await
        .map_err(|e| e.to_string())?;
    
    if existing_accounts.iter().any(|acc| acc.email.to_lowercase() == email.to_lowercase()) {
        return Err(format!("账号 {} 已存在", email));
    }
    
    // Step 3: 创建账号（使用空密码，因为我们有 refresh_token）
    let final_nickname = nickname.unwrap_or_else(|| email.split('@').next().unwrap_or(&email).to_string());
    
    let mut account = store.add_account_no_save(email.clone(), String::new(), final_nickname, group, account_source.clone())
        .await
        .map_err(|e| e.to_string())?;
    
    // 设置标签和分组
    account.tags = tags;
    account.token = Some(token.clone());
    account.token_expires_at = Some(expires_at);
    account.refresh_token = Some(new_refresh_token);
    account.status = crate::models::account::AccountStatus::Active;
    account.last_login_at = Some(chrono::Utc::now());
    // 设置账号来源（windsurf / devin），未提供则保持 None（视为 windsurf）
    if let Some(src) = account_source {
        let trimmed = src.trim();
        if !trimmed.is_empty() {
            account.account_source = Some(trimmed.to_string());
        }
    }
    
    // 获取账号详细信息（套餐、积分等）
    let windsurf_service = WindsurfService::new();
    if let Ok(user_info_result) = windsurf_service.get_current_user(&token).await {
        if let Some(user_info) = user_info_result.get("user_info") {
            // 提取用户基本信息（包含api_key）
            if let Some(user) = user_info.get("user") {
                if let Some(api_key) = user.get("api_key").and_then(|v| v.as_str()) {
                    account.windsurf_api_key = Some(api_key.to_string());
                }
                // 提取账户禁用状态
                let disable_codeium = user.get("disable_codeium")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                account.is_disabled = Some(disable_codeium);
            }

            // 提取套餐信息
            if let Some(plan) = user_info.get("plan") {
                if let Some(plan_name) = plan.get("plan_name").and_then(|v| v.as_str()) {
                    account.plan_name = Some(plan_name.to_string());
                }
            }

            // 提取配额信息
            if let Some(subscription) = user_info.get("subscription") {
                if let Some(used) = subscription.get("used_quota").and_then(|v| v.as_i64()) {
                    account.used_quota = Some(used as i32);
                }
                if let Some(total) = subscription.get("quota").and_then(|v| v.as_i64()) {
                    account.total_quota = Some(total as i32);
                }
                // 提取订阅到期时间
                if let Some(expires_at) = subscription.get("expires_at").and_then(|v| v.as_i64()) {
                    account.subscription_expires_at = chrono::DateTime::from_timestamp(expires_at, 0);
                }
                // 提取订阅激活状态
                if let Some(subscription_active) = subscription.get("subscription_active").and_then(|v| v.as_bool()) {
                    account.subscription_active = Some(subscription_active);
                }
            }

            // 提取团队所有者状态（is_root_admin）
            if let Some(is_root_admin) = user_info.get("is_root_admin").and_then(|v| v.as_bool()) {
                account.is_team_owner = Some(is_root_admin);
            }

            account.last_quota_update = Some(chrono::Utc::now());
        }
    }

    // 补充调用 GetPlanStatus 获取每日/每周配额信息（新配额系统）
    // 注意：daily_quota_remaining / weekly_quota_remaining 等字段只在 GetPlanStatus 接口返回，
    // GetCurrentUser 不带。如果不调用这个接口，前端会因为 daily_quota_remaining 为 None
    // 而回退到旧的 "Trial 0/100" 积分卡片，导致用户首次导入后必须再手动刷新一次才能看到新 UI。
    if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {
        if let Some(plan_status) = plan_result.get("plan_status") {
            if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                account.daily_quota_remaining = Some(v as i32);
            }
            if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                account.weekly_quota_remaining = Some(v as i32);
            }
            if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                account.daily_quota_reset = Some(v);
            }
            if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                account.weekly_quota_reset = Some(v);
            }
            if let Some(v) = plan_status.get("overage_balance_micros").and_then(|v| v.as_i64()) {
                account.overage_balance_micros = Some(v);
            } else {
                account.overage_balance_micros = Some(0);
            }
            account.last_quota_update = Some(chrono::Utc::now());
        }
    }

    store.update_account_no_save(account.clone())
        .await
        .map_err(|e| e.to_string())?;
    store.inner().request_save_coalesced();
    
    // 记录日志
    let log = OperationLog::new(
        OperationType::AddAccount,
        OperationStatus::Success,
        format!("通过RefreshToken添加账号: {}", email),
    )
    .with_account(account.id, email.clone());
    
    let _ = store.add_log(log).await;
    
    Ok(json!({
        "success": true,
        "account": account,
        "email": email,
        "plan_name": account.plan_name,
        "used_quota": account.used_quota,
        "total_quota": account.total_quota
    }))
}

#[tauri::command]
pub async fn batch_import_accounts(
    accounts: Vec<BatchImportAccountInput>,
    auto_login: bool,
    group: Option<String>,
    tags: Vec<String>,
    mode: String,
    account_source: Option<String>,
    concurrent_limit: Option<usize>,
    store: State<'_, Arc<DataStore>>,
) -> Result<serde_json::Value, String> {
    let settings = store.get_settings().await.map_err(|e| e.to_string())?;
    let target_group = group
        .and_then(|g| {
            let trimmed = g.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .unwrap_or_else(|| "默认分组".to_string());
    let account_source = account_source.and_then(|source| {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    let mode = if mode == "refresh_token" { "refresh_token" } else { "password" }.to_string();
    let existing_accounts = store.get_all_accounts().await.map_err(|e| e.to_string())?;
    let existing_emails: HashSet<String> = existing_accounts
        .iter()
        .map(|account| account.email.to_lowercase())
        .collect();
    let existing_tokens: HashSet<String> = existing_accounts
        .iter()
        .filter_map(|account| account.refresh_token.clone())
        .filter(|token| !token.is_empty())
        .collect();

    let mut seen_emails = HashSet::new();
    let mut seen_tokens = HashSet::new();
    let mut skipped_results = Vec::new();
    let mut candidates = Vec::new();

    for item in accounts {
        if mode == "refresh_token" {
            let token = item.refresh_token.clone().unwrap_or_default();
            if token.is_empty() {
                skipped_results.push(batch_import_skipped_result(item.email, "Refresh Token为空"));
                continue;
            }
            if existing_tokens.contains(&token) || !seen_tokens.insert(token) {
                skipped_results.push(batch_import_skipped_result(item.email, "Refresh Token已存在"));
                continue;
            }
        } else {
            let email = item.email.trim().to_lowercase();
            if email.is_empty() {
                skipped_results.push(batch_import_skipped_result(item.email, "邮箱为空"));
                continue;
            }
            if existing_emails.contains(&email) || !seen_emails.insert(email) {
                skipped_results.push(batch_import_skipped_result(item.email, "账号已存在"));
                continue;
            }
        }
        candidates.push(item);
    }

    let max_concurrent = if candidates.is_empty() {
        1
    } else if settings.unlimited_concurrent_refresh {
        candidates.len()
    } else {
        concurrent_limit
            .unwrap_or(settings.concurrent_limit)
            .max(1)
            .min(candidates.len())
    };
    let retry_times = settings.retry_times.max(0) as usize;
    let store_arc = store.inner().clone();

    let mut results = run_batch_import_round(
        candidates.clone(),
        auto_login,
        target_group.clone(),
        tags.clone(),
        mode.clone(),
        account_source.clone(),
        retry_times,
        store_arc.clone(),
        max_concurrent,
    ).await;

    for retry_round in 0..retry_times {
        let retry_candidates = collect_failed_import_candidates(&candidates, &results);
        if retry_candidates.is_empty() {
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(1000 + retry_round as u64 * 1000)).await;
        let retry_candidate_keys: HashSet<String> = retry_candidates
            .iter()
            .map(|item| item.email.clone())
            .collect();
        let retry_concurrent = if retry_candidates.is_empty() {
            1
        } else if settings.unlimited_concurrent_refresh {
            retry_candidates.len()
        } else {
            max_concurrent.min(retry_candidates.len()).max(1)
        };
        let mut retry_results = run_batch_import_round(
            retry_candidates,
            auto_login,
            target_group.clone(),
            tags.clone(),
            mode.clone(),
            account_source.clone(),
            retry_times,
            store_arc.clone(),
            retry_concurrent,
        ).await;

        for result in retry_results.iter_mut() {
            if result.success {
                result.retry_success = true;
            }
        }
        results.retain(|result| !retry_candidate_keys.contains(&result.input_key));
        results.extend(retry_results);
    }

    results.extend(skipped_results);

    let logs: Vec<OperationLog> = results
        .iter()
        .filter(|result| result.success)
        .filter_map(|result| {
            let account_id = result.account_id.as_ref()?;
            let uuid = Uuid::parse_str(account_id).ok()?;
            Some(OperationLog::new(
                OperationType::AddAccount,
                OperationStatus::Success,
                format!("批量导入账号: {}", result.email),
            ).with_account(uuid, result.email.clone()))
        })
        .collect();

    store.flush().await.map_err(|e| e.to_string())?;
    store.add_logs_batch(logs).await.map_err(|e| e.to_string())?;

    let success_count = results.iter().filter(|result| result.success).count();
    let skipped_count = results.iter().filter(|result| result.skipped).count();
    let login_success_count = results.iter().filter(|result| result.login_success).count();
    let offline_retry_total = results.iter().filter(|result| result.offline).count();
    let offline_retry_success = results.iter().filter(|result| result.retry_success).count();
    let total_count = results.len();

    Ok(json!({
        "total_count": total_count,
        "success_count": success_count,
        "failed_count": total_count.saturating_sub(success_count + skipped_count),
        "skipped_count": skipped_count,
        "login_success_count": login_success_count,
        "offline_retry_total": offline_retry_total,
        "offline_retry_success": offline_retry_success,
        "results": results
    }))
}

fn batch_import_skipped_result(email: String, error: &str) -> BatchImportItemResult {
    BatchImportItemResult {
        input_key: email.clone(),
        email,
        success: false,
        account_id: None,
        error: Some(error.to_string()),
        skipped: true,
        login_success: false,
        offline: false,
        retry_success: false,
    }
}

async fn run_batch_import_round(
    candidates: Vec<BatchImportAccountInput>,
    auto_login: bool,
    target_group: String,
    tags: Vec<String>,
    mode: String,
    account_source: Option<String>,
    retry_times: usize,
    store: Arc<DataStore>,
    max_concurrent: usize,
) -> Vec<BatchImportItemResult> {
    stream::iter(candidates.into_iter())
        .map(|item| {
            let store = store.clone();
            let tags = tags.clone();
            let group = target_group.clone();
            let mode = mode.clone();
            let account_source = account_source.clone();
            async move {
                batch_import_account_item(item, auto_login, group, tags, mode, account_source, retry_times, store).await
            }
        })
        .buffer_unordered(max_concurrent.max(1))
        .collect()
        .await
}

fn collect_failed_import_candidates(
    candidates: &[BatchImportAccountInput],
    results: &[BatchImportItemResult],
) -> Vec<BatchImportAccountInput> {
    let failed_keys: HashSet<String> = results
        .iter()
        .filter(|result| !result.success && !result.skipped)
        .map(|result| result.input_key.clone())
        .collect();

    candidates
        .iter()
        .filter(|candidate| failed_keys.contains(&candidate.email))
        .cloned()
        .collect()
}

async fn batch_import_account_item(
    item: BatchImportAccountInput,
    auto_login: bool,
    group: String,
    tags: Vec<String>,
    mode: String,
    account_source: Option<String>,
    retry_times: usize,
    store: Arc<DataStore>,
) -> BatchImportItemResult {
    if mode == "refresh_token" {
        return batch_import_refresh_token_item(item, group, tags, account_source, retry_times, store).await;
    }

    match batch_import_password_item(item.clone(), auto_login, group, tags, account_source, retry_times, store).await {
        Ok(result) => result,
        Err(error) => BatchImportItemResult {
            input_key: item.email.clone(),
            email: item.email,
            success: false,
            account_id: None,
            error: Some(error),
            skipped: false,
            login_success: false,
            offline: false,
            retry_success: false,
        }
    }
}

async fn batch_import_password_item(
    item: BatchImportAccountInput,
    auto_login: bool,
    group: String,
    tags: Vec<String>,
    account_source: Option<String>,
    retry_times: usize,
    store: Arc<DataStore>,
) -> Result<BatchImportItemResult, String> {
    let nickname = if item.remark.trim().is_empty() {
        item.email.split('@').next().unwrap_or(&item.email).to_string()
    } else {
        item.remark.clone()
    };
    let mut account = store.add_account_no_save(
        item.email.clone(),
        item.password.clone(),
        nickname,
        Some(group),
        account_source.clone(),
    ).await.map_err(|e| e.to_string())?;

    account.tags = tags;
    apply_import_account_source(&mut account, account_source.as_deref());

    let login_success = if auto_login {
        login_imported_account(&mut account, &item.password, retry_times).await
    } else {
        false
    };
    let offline = !account.is_token_valid();
    store.update_account_no_save(account.clone()).await.map_err(|e| e.to_string())?;

    Ok(BatchImportItemResult {
        input_key: item.email.clone(),
        email: account.email.clone(),
        success: true,
        account_id: Some(account.id.to_string()),
        error: None,
        skipped: false,
        login_success,
        offline,
        retry_success: login_success,
    })
}

async fn batch_import_refresh_token_item(
    item: BatchImportAccountInput,
    group: String,
    tags: Vec<String>,
    account_source: Option<String>,
    retry_times: usize,
    store: Arc<DataStore>,
) -> BatchImportItemResult {
    let mut last_error = "添加失败".to_string();
    for attempt in 0..=retry_times {
        match import_refresh_token_once(item.clone(), group.clone(), tags.clone(), account_source.clone(), store.clone()).await {
            Ok(result) => return result,
            Err(error) => last_error = error,
        }
        wait_for_batch_import_retry(attempt, retry_times).await;
    }

    BatchImportItemResult {
        input_key: item.email.clone(),
        email: item.email,
        success: false,
        account_id: None,
        error: Some(last_error),
        skipped: false,
        login_success: false,
        offline: false,
        retry_success: false,
    }
}

async fn import_refresh_token_once(
    item: BatchImportAccountInput,
    group: String,
    tags: Vec<String>,
    account_source: Option<String>,
    store: Arc<DataStore>,
) -> Result<BatchImportItemResult, String> {
    let refresh_token = item.refresh_token.as_deref().ok_or("Refresh Token为空")?;
    let auth_service = AuthService::new();
    let windsurf_service = WindsurfService::new();
    let mut user_info_for_details = None;

    let (token, new_refresh_token, expires_at, email) = if refresh_token.starts_with("auth1_") {
        let (token, new_refresh_token, expires_at) = auth_service.refresh_session_with_auth1(refresh_token)
            .await
            .map_err(|e| format!("刷新Token失败: {}", e))?;
        let user_info_result = windsurf_service.get_current_user(&token)
            .await
            .map_err(|e| format!("获取用户信息失败: {}", e))?;
        let email = user_info_result
            .get("user_info")
            .and_then(|v| v.get("user"))
            .and_then(|v| v.get("email"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if email.is_empty() {
            return Err("获取用户信息失败: 未找到邮箱".to_string());
        }
        user_info_for_details = Some(user_info_result);
        (token, new_refresh_token, expires_at, email)
    } else {
        let (token, new_refresh_token, expires_at) = auth_service.refresh_token(refresh_token)
            .await
            .map_err(|e| format!("刷新Token失败: {}", e))?;
        let account_info = auth_service.get_account_info(&token)
            .await
            .map_err(|e| format!("获取用户信息失败: {}", e))?;
        (token, new_refresh_token, expires_at, account_info.email.clone())
    };

    let nickname = if item.remark.trim().is_empty() {
        email.split('@').next().unwrap_or(&email).to_string()
    } else {
        item.remark.clone()
    };
    let mut account = store.add_account_no_save(
        email.clone(),
        String::new(),
        nickname,
        Some(group),
        account_source.clone(),
    ).await.map_err(|e| e.to_string())?;

    account.tags = tags;
    account.token = Some(token.clone());
    account.refresh_token = Some(new_refresh_token);
    account.token_expires_at = Some(expires_at);
    account.status = crate::models::account::AccountStatus::Active;
    account.last_login_at = Some(chrono::Utc::now());
    apply_import_account_source(&mut account, account_source.as_deref());

    if let Some(user_info_result) = user_info_for_details.as_ref() {
        apply_current_user_to_account(&mut account, user_info_result);
    } else if let Ok(user_info_result) = windsurf_service.get_current_user(&token).await {
        apply_current_user_to_account(&mut account, &user_info_result);
    }
    apply_plan_status_to_account(&mut account, &windsurf_service, &token).await;

    store.update_account_no_save(account.clone()).await.map_err(|e| e.to_string())?;

    Ok(BatchImportItemResult {
        input_key: item.email.clone(),
        email: account.email.clone(),
        success: true,
        account_id: Some(account.id.to_string()),
        error: None,
        skipped: false,
        login_success: true,
        offline: false,
        retry_success: true,
    })
}

async fn login_imported_account(account: &mut Account, password: &str, retry_times: usize) -> bool {
    for attempt in 0..=retry_times {
        let auth_service = AuthService::new();
        let login_result = auth_service.sign_in_v2_session(&account.email, password).await
            .map(|auth_result| (
                auth_result.session_token,
                auth_result.auth1_token,
                chrono::Utc::now() + chrono::Duration::hours(1),
            ));

        if let Ok((token, refresh_token, expires_at)) = login_result {
            let windsurf_service = WindsurfService::new();
            account.token = Some(token.clone());
            account.refresh_token = Some(refresh_token);
            account.token_expires_at = Some(expires_at);
            account.status = crate::models::account::AccountStatus::Active;
            account.last_login_at = Some(chrono::Utc::now());
            if let Ok(user_info_result) = windsurf_service.get_current_user(&token).await {
                apply_current_user_to_account(account, &user_info_result);
            }
            apply_plan_status_to_account(account, &windsurf_service, &token).await;
            return true;
        }

        wait_for_batch_import_retry(attempt, retry_times).await;
    }

    false
}

async fn wait_for_batch_import_retry(attempt: usize, retry_times: usize) {
    if attempt < retry_times {
        tokio::time::sleep(tokio::time::Duration::from_millis(600 + attempt as u64 * 500)).await;
    }
}

fn apply_import_account_source(account: &mut Account, account_source: Option<&str>) {
    if let Some(source) = account_source {
        let trimmed = source.trim();
        if !trimmed.is_empty() {
            account.account_source = Some(trimmed.to_string());
        }
    }
}

fn apply_current_user_to_account(account: &mut Account, user_info_result: &serde_json::Value) {
    if let Some(user_info) = user_info_result.get("user_info") {
        if let Some(user) = user_info.get("user") {
            if let Some(api_key) = user.get("api_key").and_then(|v| v.as_str()) {
                account.windsurf_api_key = Some(api_key.to_string());
            }
            let disable_codeium = user.get("disable_codeium")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            account.is_disabled = Some(disable_codeium);
        }

        if let Some(plan) = user_info.get("plan") {
            if let Some(plan_name) = plan.get("plan_name").and_then(|v| v.as_str()) {
                account.plan_name = Some(plan_name.to_string());
            }
        }

        if let Some(subscription) = user_info.get("subscription") {
            if let Some(used) = subscription.get("used_quota").and_then(|v| v.as_i64()) {
                account.used_quota = Some(used as i32);
            }
            if let Some(total) = subscription.get("quota").and_then(|v| v.as_i64()) {
                account.total_quota = Some(total as i32);
            }
            if let Some(expires_at) = subscription.get("expires_at").and_then(|v| v.as_i64()) {
                account.subscription_expires_at = chrono::DateTime::from_timestamp(expires_at, 0);
            }
            if let Some(subscription_active) = subscription.get("subscription_active").and_then(|v| v.as_bool()) {
                account.subscription_active = Some(subscription_active);
            }
        }

        if let Some(is_root_admin) = user_info.get("is_root_admin").and_then(|v| v.as_bool()) {
            account.is_team_owner = Some(is_root_admin);
        }

        account.last_quota_update = Some(chrono::Utc::now());
    }
}

async fn apply_plan_status_to_account(account: &mut Account, windsurf_service: &WindsurfService, token: &str) {
    if let Ok(plan_result) = windsurf_service.get_plan_status(token).await {
        if let Some(plan_status) = plan_result.get("plan_status") {
            if let Some(plan_name) = plan_status.get("plan_name").and_then(|v| v.as_str()) {
                account.plan_name = Some(plan_name.to_string());
            }
            let used_prompt = plan_status.get("used_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);
            let used_flex = plan_status.get("used_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);
            account.used_quota = Some((used_prompt + used_flex) as i32);
            let available_flex = plan_status.get("available_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);
            let available_prompt = plan_status.get("available_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);
            if available_flex > 0 || available_prompt > 0 {
                account.total_quota = Some((available_flex + available_prompt) as i32);
            }
            if let Some(plan_end) = plan_status.get("plan_end").and_then(|v| v.as_i64()) {
                account.subscription_expires_at = chrono::DateTime::from_timestamp(plan_end, 0);
            }
            if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                account.daily_quota_remaining = Some(v as i32);
            }
            if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                account.weekly_quota_remaining = Some(v as i32);
            }
            if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                account.daily_quota_reset = Some(v);
            }
            if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                account.weekly_quota_reset = Some(v);
            }
            if let Some(v) = plan_status.get("overage_balance_micros").and_then(|v| v.as_i64()) {
                account.overage_balance_micros = Some(v);
            } else {
                account.overage_balance_micros = Some(0);
            }
            account.last_quota_update = Some(chrono::Utc::now());
        }
    }
}

#[tauri::command]
pub async fn get_all_accounts(
    store: State<'_, Arc<DataStore>>,
) -> Result<Vec<Account>, String> {
    store.get_all_accounts()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_account(
    id: String,
    store: State<'_, Arc<DataStore>>,
) -> Result<Account, String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    store.get_account(uuid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_account(
    account: serde_json::Value,
    store: State<'_, Arc<DataStore>>,
) -> Result<(), String> {
    // 解析账号JSON，处理可能的密码更新
    let account_id = account.get("id")
        .and_then(|v| v.as_str())
        .ok_or("Invalid account ID")?;

    let id = Uuid::parse_str(account_id).map_err(|e| e.to_string())?;

    // 获取现有账号
    let mut existing_account = store.get_account(id)
        .await
        .map_err(|e| e.to_string())?;

    // 更新基本信息
    if let Some(nickname) = account.get("nickname").and_then(|v| v.as_str()) {
        existing_account.nickname = nickname.to_string();
    }

    if let Some(group_value) = account.get("group") {
        if group_value.is_null() {
            existing_account.group = None;
        } else if let Some(group) = group_value.as_str() {
            let trimmed_group = group.trim();
            if !trimmed_group.is_empty() {
                existing_account.group = Some(trimmed_group.to_string());
            }
        }
    }

    if let Some(tags) = account.get("tags").and_then(|v| v.as_array()) {
        existing_account.tags = tags.iter()
            .filter_map(|t| t.as_str().map(|s| s.to_string()))
            .collect();
    }

    // 更新配额和套餐信息（从API获取的数据）
    if let Some(plan_name) = account.get("plan_name").and_then(|v| v.as_str()) {
        existing_account.plan_name = Some(plan_name.to_string());
    }

    if let Some(used_quota) = account.get("used_quota").and_then(|v| v.as_i64()) {
        existing_account.used_quota = Some(used_quota as i32);
    }

    if let Some(total_quota) = account.get("total_quota").and_then(|v| v.as_i64()) {
        existing_account.total_quota = Some(total_quota as i32);
    }

    if let Some(last_quota_update) = account.get("last_quota_update").and_then(|v| v.as_str()) {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(last_quota_update) {
            existing_account.last_quota_update = Some(dt.with_timezone(&chrono::Utc));
        }
    }

    // 更新订阅到期时间
    if let Some(subscription_expires_at) = account.get("subscription_expires_at").and_then(|v| v.as_str()) {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(subscription_expires_at) {
            existing_account.subscription_expires_at = Some(dt.with_timezone(&chrono::Utc));
        }
    }

    // 更新账户禁用状态
    if let Some(is_disabled) = account.get("is_disabled") {
        if is_disabled.is_null() {
            existing_account.is_disabled = None;
        } else if let Some(disabled) = is_disabled.as_bool() {
            existing_account.is_disabled = Some(disabled);
        }
    }

    // 更新 Windsurf API Key
    if let Some(windsurf_api_key) = account.get("windsurf_api_key").and_then(|v| v.as_str()) {
        existing_account.windsurf_api_key = Some(windsurf_api_key.to_string());
    }

    // 更新账号来源（windsurf / devin），允许 null 清空
    if let Some(src_value) = account.get("account_source").or_else(|| account.get("accountSource")) {
        if src_value.is_null() {
            existing_account.account_source = None;
        } else if let Some(src) = src_value.as_str() {
            let trimmed = src.trim();
            existing_account.account_source = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
        }
    }

    // 更新 Token（如果有）
    if let Some(token) = account.get("token").and_then(|v| v.as_str()) {
        if !token.is_empty() {
            existing_account.token = Some(token.to_string());
        }
    }

    // 更新账户状态
    if let Some(status) = account.get("status").and_then(|v| v.as_str()) {
        existing_account.status = match status {
            "active" => crate::models::account::AccountStatus::Active,
            "inactive" => crate::models::account::AccountStatus::Inactive,
            "error" => crate::models::account::AccountStatus::Error("API错误".to_string()),
            _ => crate::models::account::AccountStatus::Error(status.to_string()),
        };
    }

    // 先更新基本信息
    store.update_account(existing_account.clone())
        .await
        .map_err(|e| e.to_string())?;

    // 如果有新密码，单独更新
    if let Some(new_password) = account.get("password").and_then(|v| v.as_str()) {
        if !new_password.is_empty() {
            // 调用专门的密码更新方法
            store.update_account_password(id, new_password.to_string())
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    // 记录日志
    let log = OperationLog::new(
        OperationType::EditAccount,
        OperationStatus::Success,
        format!("更新账号: {}", existing_account.email),
    )
    .with_account(existing_account.id, existing_account.email.clone());

    let _ = store.add_log(log).await;

    Ok(())
}

#[tauri::command]
pub async fn delete_account(
    id: String,
    store: State<'_, Arc<DataStore>>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    
    // 获取账号信息用于日志
    let account = store.get_account(uuid).await.ok();
    
    store.delete_account(uuid)
        .await
        .map_err(|e| e.to_string())?;
    
    // 记录日志
    if let Some(acc) = account {
        let log = OperationLog::new(
            OperationType::DeleteAccount,
            OperationStatus::Success,
            format!("删除账号: {}", acc.email),
        );
        let _ = store.add_log(log).await;
    }
    
    Ok(())
}

#[tauri::command]
pub async fn delete_accounts_batch(
    ids: Vec<String>,
    store: State<'_, Arc<DataStore>>,
) -> Result<serde_json::Value, String> {
    let mut failed_ids = Vec::new();

    let mut valid_ids = Vec::new();
    for id_str in ids {
        match Uuid::parse_str(&id_str) {
            Ok(uuid) => valid_ids.push(uuid),
            Err(_) => failed_ids.push(id_str),
        }
    }

    let (deleted_ids, not_found_ids) = store.delete_accounts_batch(&valid_ids)
        .await
        .map_err(|e| e.to_string())?;

    for id in not_found_ids {
        failed_ids.push(id.to_string());
    }

    let success_count = deleted_ids.len();
    
    // 记录批量操作日志
    let log = OperationLog::new(
        OperationType::BatchOperation,
        if failed_ids.is_empty() { OperationStatus::Success } else { OperationStatus::Failed },
        format!("批量删除账号: 成功{}个，失败{}个", success_count, failed_ids.len()),
    );
    let _ = store.add_log(log).await;
    
    Ok(json!({
        "success_count": success_count,
        "failed_ids": failed_ids
    }))
}

#[tauri::command]
pub async fn search_accounts(
    query: String,
    store: State<'_, Arc<DataStore>>,
) -> Result<Vec<Account>, String> {
    let all_accounts = store.get_all_accounts()
        .await
        .map_err(|e| e.to_string())?;
    
    let query_lower = query.to_lowercase();
    let filtered: Vec<Account> = all_accounts
        .into_iter()
        .filter(|acc| {
            acc.email.to_lowercase().contains(&query_lower) ||
            acc.nickname.to_lowercase().contains(&query_lower) ||
            acc.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
        })
        .collect();
    
    Ok(filtered)
}

#[tauri::command]
pub async fn filter_accounts_by_group(
    group: String,
    store: State<'_, Arc<DataStore>>,
) -> Result<Vec<Account>, String> {
    let all_accounts = store.get_all_accounts()
        .await
        .map_err(|e| e.to_string())?;
    
    let filtered: Vec<Account> = all_accounts
        .into_iter()
        .filter(|acc| acc.group.as_ref() == Some(&group))
        .collect();
    
    Ok(filtered)
}

#[tauri::command]
pub async fn filter_accounts_by_tags(
    tags: Vec<String>,
    store: State<'_, Arc<DataStore>>,
) -> Result<Vec<Account>, String> {
    let all_accounts = store.get_all_accounts()
        .await
        .map_err(|e| e.to_string())?;
    
    let filtered: Vec<Account> = all_accounts
        .into_iter()
        .filter(|acc| {
            tags.iter().any(|tag| acc.tags.contains(tag))
        })
        .collect();
    
    Ok(filtered)
}
