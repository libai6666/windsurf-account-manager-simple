use crate::models::{Account, AccountStatus, OperationLog, OperationType, OperationStatus};

use crate::repository::DataStore;

use crate::services::{AuthService, WindsurfService, UpdateSeatsResult, DevinService};

use crate::utils::AppError;
<<<<<<< HEAD

use log::info;

=======
use log::info;
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
use serde_json::json;

use std::sync::Arc;

use tauri::State;

use uuid::Uuid;



/// 确保账户有有效的Token

/// 优先使用缓存的token，只在过期或不存在时刷新

pub async fn ensure_valid_token(

    store: &Arc<DataStore>,

    account: &mut Account,

    uuid: Uuid,

) -> Result<(), String> {

    ensure_valid_token_with_force(store, account, uuid, false).await

}



/// 检查账号是否为团队所有者（Admin角色）

/// 通过 GetCurrentUser API 获取 roles 字段判断是否为 root.admin

pub async fn check_is_team_owner(windsurf_service: &WindsurfService, token: &str, _email: &str) -> bool {

    if let Ok(user_result) = windsurf_service.get_current_user(token).await {

        // 检查 user_info.is_root_admin 字段（由 proto_parser 解析）

        if let Some(user_info) = user_result.get("user_info") {

            if let Some(is_root_admin) = user_info.get("is_root_admin").and_then(|v| v.as_bool()) {

                return is_root_admin;

            }

        }

    }

    false

}



/// 检查 API 响应是否为 401 错误

pub fn is_401_error(result: &serde_json::Value) -> bool {

    result.get("status_code")

        .and_then(|v| v.as_u64())

        .map(|code| code == 401)

        .unwrap_or(false)

}

<<<<<<< HEAD


fn find_stripe_session_id_from_logs(logs: &[OperationLog], account_id: Uuid) -> Option<String> {

    logs.iter()

        .rev()

        .find_map(|log| {

            if log.account_id != Some(account_id) {

                return None;

            }



            log.details.as_ref().and_then(|details| {

                details.get("stripe_session_id")

                    .and_then(|v| v.as_str())

                    .map(|s| s.to_string())

                    .or_else(|| {

                        details.get("stripe_url")

                            .and_then(|v| v.as_str())

                            .and_then(WindsurfService::extract_checkout_session_id)

                    })

            })

        })

}



/// 按 refresh_token 类型选择刷新方式，失败时才回退到密码登录。

/// - `auth1_` 开头: 调 `WindsurfPostAuth`（新版 Windsurf 2.0 账号，无需密码）

/// - 其他: 调 Firebase `securetoken` 端点（老版 Firebase 账号）

/// - 两者都失败或没有 refresh_token 时，才调 `sign_in_compat` 走密码登录

/// 

/// 这样可以避免批量刷新时，因为 Firebase 端点对 auth1 token 的必然失败，

/// 导致 20+ 并发同时打 `_devin-auth/password/login` 触发 429 Rate Limit。

async fn refresh_token_or_relogin(

    auth_service: &AuthService,

    store: &Arc<DataStore>,

    uuid: Uuid,

    email: &str,

    refresh_token: Option<&str>,

) -> Result<(String, String, chrono::DateTime<chrono::Utc>), String> {

    if let Some(rt) = refresh_token {

        let refresh_result = if rt.starts_with("auth1_") {

            auth_service.refresh_session_with_auth1(rt).await

        } else {

            auth_service.refresh_token(rt).await

        };



        match refresh_result {

            Ok(result) => return Ok(result),

            Err(e) => {

                log::warn!(

                    "[refresh_token_or_relogin] {} refresh 失败 ({})，回退到密码登录",

                    email, e

                );

            }

        }

    }



    // 回退：调 _devin-auth/password/login 重新登录

    let password = store

        .get_decrypted_password(uuid)

        .await

        .map_err(|e| e.to_string())?;

    auth_service

        .sign_in_compat(email, &password)

        .await

        .map_err(|e| e.to_string())

}



=======
/// 按 refresh_token 类型选择刷新方式，失败时才回退到密码登录。
/// - `auth1_` 开头: 调 `WindsurfPostAuth`（新版 Windsurf 2.0 账号，无需密码）
/// - 其他: 调 Firebase `securetoken` 端点（老版 Firebase 账号）
/// - 两者都失败或没有 refresh_token 时，才调 `sign_in_compat` 走密码登录
///
/// 这样可以避免批量刷新时，因为 Firebase 端点对 auth1 token 的必然失败，
/// 导致 20+ 并发同时打 `_devin-auth/password/login` 触发 429 Rate Limit。
async fn refresh_token_or_relogin(
    auth_service: &AuthService,
    store: &Arc<DataStore>,
    uuid: Uuid,
    email: &str,
    refresh_token: Option<&str>,
) -> Result<(String, String, chrono::DateTime<chrono::Utc>), String> {
    if let Some(rt) = refresh_token {
        let refresh_result = if rt.starts_with("auth1_") {
            auth_service.refresh_session_with_auth1(rt).await
        } else {
            auth_service.refresh_token(rt).await
        };

        match refresh_result {
            Ok(result) => return Ok(result),
            Err(e) => {
                log::warn!(
                    "[refresh_token_or_relogin] {} refresh 失败 ({})，回退到密码登录",
                    email, e
                );
            }
        }
    }

    // 回退：调 _devin-auth/password/login 重新登录
    let password = store
        .get_decrypted_password(uuid)
        .await
        .map_err(|e| e.to_string())?;
    auth_service
        .sign_in_compat(email, &password)
        .await
        .map_err(|e| e.to_string())
}

>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
/// 确保账户有有效的Token（支持强制刷新）

/// force_refresh: 强制刷新token，用于处理服务器端使token失效的情况（如401错误）

pub async fn ensure_valid_token_with_force(

    store: &Arc<DataStore>,

    account: &mut Account,

    uuid: Uuid,

    force_refresh: bool,

) -> Result<(), String> {

    // 如果不是强制刷新且token有效，直接返回

    if !force_refresh && 

       account.token.is_some() && 

       account.token_expires_at.is_some() && 

       !AuthService::is_token_expired(&account.token_expires_at.unwrap()) {

        return Ok(());

    }

    

    if force_refresh {

        println!("[ensure_valid_token] 强制刷新 token (可能是 401 错误触发)");

    }

    

    let auth_service = AuthService::new();

    
<<<<<<< HEAD

    // 优先尝试使用refresh token（按 auth1_ 前缀路由，避免新账号走 Firebase 端点然后无谓地回退到密码登录）

    let (token, refresh_token_new, expires_at) = refresh_token_or_relogin(

        &auth_service,

        store,

        uuid,

        &account.email,

        account.refresh_token.as_deref(),

    ).await?;

=======
    // 优先尝试使用refresh token（按 auth1_ 前缀路由，避免新账号走 Firebase 端点然后无谓地回退到密码登录）
    let (token, refresh_token_new, expires_at) = refresh_token_or_relogin(
        &auth_service,
        store,
        uuid,
        &account.email,
        account.refresh_token.as_deref(),
    ).await?;
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
    

    // 更新token到数据库

    store.update_account_tokens(uuid, token.clone(), refresh_token_new.clone(), expires_at)

        .await

        .map_err(|e| e.to_string())?;

    

    // 更新内存中的账户对象

    account.token = Some(token);

    account.refresh_token = Some(refresh_token_new);

    account.token_expires_at = Some(expires_at);

    

    Ok(())

}



#[tauri::command]

pub async fn login_account(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 解密密码

    let password = store.get_decrypted_password(uuid)

        .await

        .map_err(|e| e.to_string())?;

    
<<<<<<< HEAD

    // 登录获取Token：先尝试 Windsurf 2.0 (devin-auth)，失败则回退到 Firebase

    let auth_service = AuthService::new();

    let (token, refresh_token, expires_at) = match auth_service.sign_in_v2_session(&account.email, &password).await {

        Ok(auth_result) => {

            info!("[login_account] sign_in_v2_session 成功: {}", account.email);

            (auth_result.session_token, auth_result.auth1_token, chrono::Utc::now() + chrono::Duration::hours(1))

=======
    // 登录获取Token：先尝试 Windsurf 2.0 (devin-auth)，失败则回退到 Firebase
    let auth_service = AuthService::new();
    let (token, refresh_token, expires_at) = match auth_service.sign_in_v2_session(&account.email, &password).await {
        Ok(auth_result) => {
            info!("[login_account] sign_in_v2_session 成功: {}", account.email);
            (auth_result.session_token, auth_result.auth1_token, chrono::Utc::now() + chrono::Duration::hours(1))
        }
        Err(e) => {
            info!("[login_account] sign_in_v2_session 失败({}), 回退到 Firebase: {}", e, account.email);
            auth_service.sign_in(&account.email, &password)
                .await
                .map_err(|e| e.to_string())?
        }
    };
    
    // 更新Token和Refresh Token
    store.update_account_tokens(uuid, token.clone(), refresh_token, expires_at)
        .await
        .map_err(|e| e.to_string())?;
    
    // 获取最新的配额信息
    let windsurf_service = WindsurfService::new();
    let mut updated_account = store.get_account(uuid).await.map_err(|e| e.to_string())?;
    
    // 读取设置，判断使用哪个 API
    let settings = store.get_settings().await.map_err(|e| e.to_string())?;
    println!("[login_account] use_lightweight_api = {}", settings.use_lightweight_api);
    
    if settings.use_lightweight_api {
        // 使用轻量级 GetPlanStatus API
        if let Ok(result) = windsurf_service.get_plan_status(&token).await {
            if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                if let Some(plan_status) = result.get("plan_status") {
                    // 更新套餐名称
                    if let Some(plan_name) = plan_status.get("plan_name").and_then(|v| v.as_str()) {
                        updated_account.plan_name = Some(plan_name.to_string());
                    }
                    
                    // 更新已用配额 (used_prompt_credits + used_flex_credits)
                    // 注意：不是 used_flow_credits，而是 used_flex_credits (int_7)
                    let used_prompt = plan_status.get("used_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);
                    let used_flex = plan_status.get("used_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);
                    updated_account.used_quota = Some((used_prompt + used_flex) as i32);
                    
                    // 更新总配额 (available_flex_credits + available_prompt_credits)
                    let available_flex = plan_status.get("available_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);
                    let available_prompt = plan_status.get("available_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);
                    if available_flex > 0 || available_prompt > 0 {
                        updated_account.total_quota = Some((available_flex + available_prompt) as i32);
                    }
                    
                    // 更新订阅到期时间 (plan_end)
                    if let Some(plan_end) = plan_status.get("plan_end").and_then(|v| v.as_i64()) {
                        updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(plan_end, 0);
                    }
                    
                    // 更新每日/每周配额信息（新配额系统）
                    if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                        updated_account.daily_quota_remaining = Some(v as i32);
                    }
                    if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                        updated_account.weekly_quota_remaining = Some(v as i32);
                    }
                    if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                        updated_account.daily_quota_reset = Some(v);
                    }
                    if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                        updated_account.weekly_quota_reset = Some(v);
                    }
                    
                    updated_account.last_quota_update = Some(chrono::Utc::now());
                    store.update_account(updated_account.clone()).await
                        .map_err(|e| format!("保存账户信息失败: {}", e))?;
                }
            }
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
        }

        Err(e) => {

            info!("[login_account] sign_in_v2_session 失败({}), 回退到 Firebase: {}", e, account.email);

            auth_service.sign_in(&account.email, &password)

                .await

                .map_err(|e| e.to_string())?

        }

    };

    

    // 更新Token和Refresh Token

    store.update_account_tokens(uuid, token.clone(), refresh_token, expires_at)

        .await

        .map_err(|e| e.to_string())?;

    

    // 获取最新的配额信息

    let windsurf_service = WindsurfService::new();

    let mut updated_account = store.get_account(uuid).await.map_err(|e| e.to_string())?;

    

    // 读取设置，判断使用哪个 API

    let settings = store.get_settings().await.map_err(|e| e.to_string())?;

    println!("[login_account] use_lightweight_api = {}", settings.use_lightweight_api);

    

    if settings.use_lightweight_api {

        // 使用轻量级 GetPlanStatus API

        if let Ok(result) = windsurf_service.get_plan_status(&token).await {

            if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

                if let Some(plan_status) = result.get("plan_status") {

                    // 更新套餐名称

                    if let Some(plan_name) = plan_status.get("plan_name").and_then(|v| v.as_str()) {

                        updated_account.plan_name = Some(plan_name.to_string());

                    }

                    

                    // 更新已用配额 (used_prompt_credits + used_flex_credits)

                    // 注意：不是 used_flow_credits，而是 used_flex_credits (int_7)

                    let used_prompt = plan_status.get("used_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    let used_flex = plan_status.get("used_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    updated_account.used_quota = Some((used_prompt + used_flex) as i32);

                    

                    // 更新总配额 (available_flex_credits + available_prompt_credits)

                    let available_flex = plan_status.get("available_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    let available_prompt = plan_status.get("available_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    if available_flex > 0 || available_prompt > 0 {

                        updated_account.total_quota = Some((available_flex + available_prompt) as i32);

                    }

                    

                    // 更新订阅到期时间 (plan_end)

                    if let Some(plan_end) = plan_status.get("plan_end").and_then(|v| v.as_i64()) {

                        updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(plan_end, 0);

                    }

                    

                    // 更新每日/每周配额信息（新配额系统）

                    if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {

                        updated_account.daily_quota_remaining = Some(v as i32);

                    }

                    if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {

                        updated_account.weekly_quota_remaining = Some(v as i32);

                    }

                    if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {

                        updated_account.daily_quota_reset = Some(v);

                    }

                    if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {

                        updated_account.weekly_quota_reset = Some(v);

                    }

                    

                    updated_account.last_quota_update = Some(chrono::Utc::now());

                    store.update_account(updated_account.clone()).await

                        .map_err(|e| format!("保存账户信息失败: {}", e))?;

                }

            }

        }

    } else {

        // 使用完整的 GetCurrentUser API

        if let Ok(user_info_result) = windsurf_service.get_current_user(&token).await {

            if let Some(user_info) = user_info_result.get("user_info") {

                // 提取用户基本信息（包含api_key）

                if let Some(user) = user_info.get("user") {

                    if let Some(api_key) = user.get("api_key").and_then(|v| v.as_str()) {

                        updated_account.windsurf_api_key = Some(api_key.to_string());

                    }

                    // 提取账户禁用状态

                    if let Some(disable_codeium) = user.get("disable_codeium").and_then(|v| v.as_bool()) {

                        updated_account.is_disabled = Some(disable_codeium);

                    }

                    // 提取免费试用使用状态

                    if let Some(used_trial) = user.get("used_trial").and_then(|v| v.as_bool()) {

                        updated_account.used_trial = Some(used_trial);

                    }

                }



                // 提取套餐信息

                if let Some(plan) = user_info.get("plan") {

                    if let Some(plan_name) = plan.get("plan_name").and_then(|v| v.as_str()) {

                        updated_account.plan_name = Some(plan_name.to_string());

                    }

                }



                // 提取配额信息

                if let Some(subscription) = user_info.get("subscription") {

                    if let Some(used) = subscription.get("used_quota").and_then(|v| v.as_i64()) {

                        updated_account.used_quota = Some(used as i32);

                    }

                    if let Some(total) = subscription.get("quota").and_then(|v| v.as_i64()) {

                        updated_account.total_quota = Some(total as i32);

                    }

                    // 提取订阅到期时间

                    if let Some(expires_at) = subscription.get("expires_at").and_then(|v| v.as_i64()) {

                        updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(expires_at, 0);

                    }

                    // 提取订阅激活状态

                    if let Some(subscription_active) = subscription.get("subscription_active").and_then(|v| v.as_bool()) {

                        updated_account.subscription_active = Some(subscription_active);

                    }

                }

                

                // 直接从 user_info 提取 is_root_admin（团队所有者）

                let is_root_admin = user_info.get("is_root_admin")

                    .and_then(|v| v.as_bool())

                    .unwrap_or(false);

                updated_account.is_team_owner = Some(is_root_admin);

<<<<<<< HEAD


                // 补充调用 GetPlanStatus 获取每日/每周配额信息

                if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {

                    if let Some(ps) = plan_result.get("plan_status") {

                        if let Some(v) = ps.get("daily_quota_remaining").and_then(|v| v.as_i64()) {

                            updated_account.daily_quota_remaining = Some(v as i32);

                        }

                        if let Some(v) = ps.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {

                            updated_account.weekly_quota_remaining = Some(v as i32);

                        }

                        if let Some(v) = ps.get("daily_quota_reset").and_then(|v| v.as_i64()) {

                            updated_account.daily_quota_reset = Some(v);

                        }

                        if let Some(v) = ps.get("weekly_quota_reset").and_then(|v| v.as_i64()) {

                            updated_account.weekly_quota_reset = Some(v);

                        }

                    }

                }



=======
                // 补充调用 GetPlanStatus 获取每日/每周配额信息（新配额系统）
                // 注意：daily_quota_remaining / weekly_quota_remaining 等字段只在 GetPlanStatus 接口返回，
                // GetCurrentUser 不带。否则前端会因 daily_quota_remaining 为 None 而显示旧的 "Trial 0/100" 卡片，
                // 用户必须手动刷新一次才能看到新版日/周额度 UI。
                if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {
                    if let Some(plan_status) = plan_result.get("plan_status") {
                        if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                            updated_account.daily_quota_remaining = Some(v as i32);
                        }
                        if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                            updated_account.weekly_quota_remaining = Some(v as i32);
                        }
                        if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                            updated_account.daily_quota_reset = Some(v);
                        }
                        if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                            updated_account.weekly_quota_reset = Some(v);
                        }
                    }
                }

>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
                updated_account.last_quota_update = Some(chrono::Utc::now());

                store.update_account(updated_account.clone()).await

                    .map_err(|e| format!("保存账户信息失败: {}", e))?;

            }

        }

    }

    

    // 如果使用轻量级 API 或者之前没有获取到，需要单独获取 is_team_owner

    if updated_account.is_team_owner.is_none() {

        let is_team_owner = check_is_team_owner(&windsurf_service, &token, &updated_account.email).await;

        updated_account.is_team_owner = Some(is_team_owner);

        store.update_account(updated_account.clone()).await

            .map_err(|e| format!("保存账户信息失败: {}", e))?;

    }



    // 记录日志

    let log = OperationLog::new(

        OperationType::Login,

        OperationStatus::Success,

        format!("账号登录成功: {}", account.email),

    )

    .with_account(uuid, account.email);

    

    let _ = store.add_log(log).await;

    

    Ok(json!({

        "success": true,

        "expires_at": expires_at.to_rfc3339(),

        "plan_name": updated_account.plan_name,

        "used_quota": updated_account.used_quota,

        "total_quota": updated_account.total_quota,

        "subscription_expires_at": updated_account.subscription_expires_at.map(|dt| dt.to_rfc3339()),

        "is_disabled": updated_account.is_disabled,

        "is_team_owner": updated_account.is_team_owner

    }))

}



#[tauri::command]

pub async fn refresh_token(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 保留过期时间信息用于参考

    let old_expires_at = account.token_expires_at

        .map(|t| t.to_rfc3339())

        .unwrap_or_else(|| "未知".to_string());

    

    let auth_service = AuthService::new();

    
<<<<<<< HEAD

    // 优先尝试使用refresh token（按 auth1_ 前缀路由，避免新账号走 Firebase 端点然后无谓地回退到密码登录）

    let (token, refresh_token_new, expires_at) = refresh_token_or_relogin(

        &auth_service,

        &store,

        uuid,

        &account.email,

        account.refresh_token.as_deref(),

    ).await?;

=======
    // 优先尝试使用refresh token（按 auth1_ 前缀路由，避免新账号走 Firebase 端点然后无谓地回退到密码登录）
    let (token, refresh_token_new, expires_at) = refresh_token_or_relogin(
        &auth_service,
        &store,
        uuid,
        &account.email,
        account.refresh_token.as_deref(),
    ).await?;
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
    

    // 更新Token和Refresh Token

    store.update_account_tokens(uuid, token.clone(), refresh_token_new, expires_at)

        .await

        .map_err(|e| e.to_string())?;

    

    // 获取最新的配额信息

    let windsurf_service = WindsurfService::new();

    let mut updated_account = store.get_account(uuid).await.map_err(|e| e.to_string())?;

    

    // 读取设置，判断使用哪个 API

    let settings = store.get_settings().await.map_err(|e| e.to_string())?;

    println!("[refresh_token] account: {}, use_lightweight_api = {}, token_len = {}", 

        updated_account.email, settings.use_lightweight_api, token.len());

    

    if settings.use_lightweight_api {

        // 使用轻量级 GetPlanStatus API

        if let Ok(result) = windsurf_service.get_plan_status(&token).await {

            let status_code = result.get("status_code").and_then(|v| v.as_u64()).unwrap_or(0);

            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

            if !success {

                println!("[refresh_token] GetPlanStatus FAILED for {}: status_code={}, error={:?}", 

                    updated_account.email, status_code, result.get("error"));

            }

            if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

                if let Some(plan_status) = result.get("plan_status") {

                    // 更新套餐名称

                    if let Some(plan_name) = plan_status.get("plan_name").and_then(|v| v.as_str()) {

                        updated_account.plan_name = Some(plan_name.to_string());

                    }

                    

                    // 更新已用配额 (used_prompt_credits + used_flex_credits)

                    // 注意：不是 used_flow_credits，而是 used_flex_credits (int_7)

                    let used_prompt = plan_status.get("used_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    let used_flex = plan_status.get("used_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    updated_account.used_quota = Some((used_prompt + used_flex) as i32);

                    

                    // 更新总配额 (available_flex_credits + available_prompt_credits)

                    let available_flex = plan_status.get("available_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    let available_prompt = plan_status.get("available_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    if available_flex > 0 || available_prompt > 0 {

                        updated_account.total_quota = Some((available_flex + available_prompt) as i32);

                    }

                    

                    // 更新订阅到期时间 (plan_end)

                    if let Some(plan_end) = plan_status.get("plan_end").and_then(|v| v.as_i64()) {

                        updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(plan_end, 0);

                    }
                    
                    // 提取每日/每周配额信息（新配额系统）
                    if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                        updated_account.daily_quota_remaining = Some(v as i32);
                    }
                    if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                        updated_account.weekly_quota_remaining = Some(v as i32);
                    }
                    if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                        updated_account.daily_quota_reset = Some(v);
                    }
                    if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                        updated_account.weekly_quota_reset = Some(v);
                    }

                    

                    // 提取每日/每周配额信息（新配额系统）

                    if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {

                        updated_account.daily_quota_remaining = Some(v as i32);

                    }

                    if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {

                        updated_account.weekly_quota_remaining = Some(v as i32);

                    }

                    if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {

                        updated_account.daily_quota_reset = Some(v);

                    }

                    if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {

                        updated_account.weekly_quota_reset = Some(v);

                    }



                    updated_account.last_quota_update = Some(chrono::Utc::now());

                    // 轻量级API路径也检查免费试用资格

                    if let Ok(eligible) = windsurf_service.check_pro_trial_eligibility(&token).await {

                        updated_account.trial_eligible = Some(eligible);

                        log::info!("[refresh_token][lightweight] email={}, trial_eligible={}", updated_account.email, eligible);

                    }

                    store.update_account(updated_account.clone()).await

                        .map_err(|e| format!("保存账户信息失败: {}", e))?;

                }

            }

        }

    } else {

        // 使用完整的 GetCurrentUser API

        if let Ok(user_info_result) = windsurf_service.get_current_user(&token).await {

            if let Some(user_info) = user_info_result.get("user_info") {

                // 提取用户基本信息（包含api_key）

                if let Some(user) = user_info.get("user") {

                    if let Some(api_key) = user.get("api_key").and_then(|v| v.as_str()) {

                        updated_account.windsurf_api_key = Some(api_key.to_string());

                    }

                    // 提取账户禁用状态

                    if let Some(disable_codeium) = user.get("disable_codeium").and_then(|v| v.as_bool()) {

                        updated_account.is_disabled = Some(disable_codeium);

                    }

                    // 提取免费试用使用状态

                    if let Some(used_trial) = user.get("used_trial").and_then(|v| v.as_bool()) {

                        updated_account.used_trial = Some(used_trial);

                    }

                    log::info!("[refresh_token] email={}, used_trial={:?}", updated_account.email, updated_account.used_trial);

                }



                // 提取套餐信息

                if let Some(plan) = user_info.get("plan") {

                    if let Some(plan_name) = plan.get("plan_name").and_then(|v| v.as_str()) {

                        updated_account.plan_name = Some(plan_name.to_string());

                    }

                }



                // 提取配额信息

                if let Some(subscription) = user_info.get("subscription") {

                    if let Some(used) = subscription.get("used_quota").and_then(|v| v.as_i64()) {

                        updated_account.used_quota = Some(used as i32);

                    }

                    if let Some(total) = subscription.get("quota").and_then(|v| v.as_i64()) {

                        updated_account.total_quota = Some(total as i32);

                    }

                    if let Some(expires_at) = subscription.get("expires_at").and_then(|v| v.as_i64()) {

                        updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(expires_at, 0);

                    }

                    // 提取订阅激活状态

                    if let Some(subscription_active) = subscription.get("subscription_active").and_then(|v| v.as_bool()) {

                        updated_account.subscription_active = Some(subscription_active);

                    }

                }

                

                // 直接从 user_info 提取 is_root_admin（团队所有者）

                let is_root_admin = user_info.get("is_root_admin")

                    .and_then(|v| v.as_bool())

                    .unwrap_or(false);

                updated_account.is_team_owner = Some(is_root_admin);

<<<<<<< HEAD


                // 补充调用 GetPlanStatus 获取每日/每周配额信息

                if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {

                    if let Some(ps) = plan_result.get("plan_status") {

                        if let Some(v) = ps.get("daily_quota_remaining").and_then(|v| v.as_i64()) {

                            updated_account.daily_quota_remaining = Some(v as i32);

                        }

                        if let Some(v) = ps.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {

                            updated_account.weekly_quota_remaining = Some(v as i32);

                        }

                        if let Some(v) = ps.get("daily_quota_reset").and_then(|v| v.as_i64()) {

                            updated_account.daily_quota_reset = Some(v);

                        }

                        if let Some(v) = ps.get("weekly_quota_reset").and_then(|v| v.as_i64()) {

                            updated_account.weekly_quota_reset = Some(v);

                        }

                    }

                }



=======
                // 补充调用 GetPlanStatus 获取每日/每周配额信息
                if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {
                    if let Some(ps) = plan_result.get("plan_status") {
                        if let Some(v) = ps.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                            updated_account.daily_quota_remaining = Some(v as i32);
                        }
                        if let Some(v) = ps.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                            updated_account.weekly_quota_remaining = Some(v as i32);
                        }
                        if let Some(v) = ps.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                            updated_account.daily_quota_reset = Some(v);
                        }
                        if let Some(v) = ps.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                            updated_account.weekly_quota_reset = Some(v);
                        }
                    }
                }

>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
                updated_account.last_quota_update = Some(chrono::Utc::now());

                // 检查免费试用资格

                if let Ok(eligible) = windsurf_service.check_pro_trial_eligibility(&token).await {

                    updated_account.trial_eligible = Some(eligible);

                    log::info!("[refresh_token] email={}, trial_eligible={}", updated_account.email, eligible);

                }

                store.update_account(updated_account.clone()).await

                    .map_err(|e| format!("保存账户信息失败: {}", e))?;

            }

        }

    }

    

    // 如果使用轻量级 API 或者之前没有获取到，需要单独获取 is_team_owner

    if updated_account.is_team_owner.is_none() {

        let is_team_owner = check_is_team_owner(&windsurf_service, &token, &updated_account.email).await;

        updated_account.is_team_owner = Some(is_team_owner);

        store.update_account(updated_account.clone()).await

            .map_err(|e| format!("保存账户信息失败: {}", e))?;

    }



    // 记录日志

    let log = OperationLog::new(

        OperationType::RefreshToken,

        OperationStatus::Success,

        format!("刷新Token成功: {}", account.email),

    )

    .with_account(uuid, account.email);



    let _ = store.add_log(log).await;



    Ok(json!({

        "success": true,

        "token": token,

        "expires_at": expires_at.to_rfc3339(),

        "old_expires_at": old_expires_at,

        "message": "Token已成功刷新",

        "plan_name": updated_account.plan_name,

        "used_quota": updated_account.used_quota,

        "total_quota": updated_account.total_quota,

        "subscription_expires_at": updated_account.subscription_expires_at.map(|dt| dt.to_rfc3339()),

        "is_disabled": updated_account.is_disabled,

        "is_team_owner": updated_account.is_team_owner,

        "windsurf_api_key": updated_account.windsurf_api_key,
<<<<<<< HEAD

        "last_quota_update": updated_account.last_quota_update.map(|t| t.to_rfc3339()),

        "daily_quota_remaining": updated_account.daily_quota_remaining,

        "weekly_quota_remaining": updated_account.weekly_quota_remaining,

        "daily_quota_reset": updated_account.daily_quota_reset,

        "weekly_quota_reset": updated_account.weekly_quota_reset,

        "used_trial": updated_account.used_trial,

        "trial_eligible": updated_account.trial_eligible

=======
        "last_quota_update": updated_account.last_quota_update.map(|t| t.to_rfc3339()),
        "daily_quota_remaining": updated_account.daily_quota_remaining,
        "weekly_quota_remaining": updated_account.weekly_quota_remaining,
        "daily_quota_reset": updated_account.daily_quota_reset,
        "weekly_quota_reset": updated_account.weekly_quota_reset
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
    }))

}



/// 获取账号的套餐状态（积分/配额信息）

/// 比 get_current_user 更轻量，专用于刷新积分状态

#[tauri::command]

pub async fn get_plan_status(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 确保有有效的Token（优先使用缓存）

    ensure_valid_token(&store, &mut account, uuid).await?;

    

    // 解密Token

    let token = store.get_decrypted_token(uuid)

        .await

        .map_err(|e| e.to_string())?

        .ok_or("No token available")?;

    

    // 调用GetPlanStatus API

    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.get_plan_status(&token)

        .await

        .map_err(|e: AppError| e.to_string())?;

    

    // 如果成功，更新账号的配额信息

    if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

        if let Some(plan_status) = result.get("plan_status") {

            let mut updated_account = store.get_account(uuid).await.map_err(|e| e.to_string())?;

            

            // 更新套餐名称

            if let Some(plan_name) = plan_status.get("plan_name").and_then(|v| v.as_str()) {

                updated_account.plan_name = Some(plan_name.to_string());

            }

            

            // 更新已用配额 (used_prompt_credits + used_flex_credits)

            let used_prompt = plan_status.get("used_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

            let used_flex = plan_status.get("used_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

            updated_account.used_quota = Some((used_prompt + used_flex) as i32);

            

            // 更新总配额 (available_flex_credits + available_prompt_credits)

            let available_flex = plan_status.get("available_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

            let available_prompt = plan_status.get("available_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

            if available_flex > 0 || available_prompt > 0 {

                updated_account.total_quota = Some((available_flex + available_prompt) as i32);

            }

            

            // 更新订阅到期时间 (plan_end)

            if let Some(plan_end) = plan_status.get("plan_end").and_then(|v| v.as_i64()) {

                updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(plan_end, 0);

            }

            

            updated_account.last_quota_update = Some(chrono::Utc::now());

            

            // 获取团队成员信息，判断是否为团队所有者（Admin）

            let is_team_owner = check_is_team_owner(&windsurf_service, &token, &updated_account.email).await;

            updated_account.is_team_owner = Some(is_team_owner);

            

            // 保存更新后的账户信息

            store.update_account(updated_account)

                .await

                .map_err(|e| format!("保存账户信息失败: {}", e))?;

        }

    }

    

    Ok(result)

}



#[tauri::command]

pub async fn reset_credits(

    id: String,

    seat_count: Option<i32>,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 确保有有效的Token（优先使用缓存）

    ensure_valid_token(&store, &mut account, uuid).await?;

    

    // 解密Token

    let token = store.get_decrypted_token(uuid)

        .await

        .map_err(|e| e.to_string())?

        .ok_or("No token available")?;

    

    // 获取座位数选项配置

    let settings = store.get_settings().await.map_err(|e| e.to_string())?;

    let seat_count_options = settings.seat_count_options;

    

    // 执行积分重置

    let windsurf_service = WindsurfService::new();

    let result: serde_json::Value = windsurf_service.reset_credits(&token, seat_count, account.last_seat_count, &seat_count_options)

        .await

        .map_err(|e: AppError| e.to_string())?;

    

    // 更新最后使用的座位数

    if let Some(used_seat_count) = result.get("seat_count_used").and_then(|v| v.as_i64()) {

        account.last_seat_count = Some(used_seat_count as i32);

        store.update_account(account.clone())

            .await

            .map_err(|e| e.to_string())?;

    }

    

    // 记录日志

    let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

    let log = OperationLog::new(

        OperationType::ResetCredits,

        if success { OperationStatus::Success } else { OperationStatus::Failed },

        format!("积分重置{}: {}", if success { "成功" } else { "失败" }, account.email),

    )

    .with_account(uuid, account.email)

    .with_details(result.clone());

    

    let _ = store.add_log(log).await;

    

    Ok(result)

}



#[tauri::command]

pub async fn update_seats(

    id: String,

    seat_count: i32,

    retry_times: i32,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 确保有有效的Token（优先使用缓存）

    ensure_valid_token(&store, &mut account, uuid).await?;

    

    // 使用缓存的或新刷新的Token

    let token = account.token.ok_or("No token available")?;

    

    // 执行座位更新

    let windsurf_service = WindsurfService::new();

    let result: UpdateSeatsResult = windsurf_service.update_seats(&token, seat_count, retry_times)

        .await

        .map_err(|e: AppError| e.to_string())?;

    

    // 记录日志

    let account = store.get_account(uuid).await.ok();

    if let Some(acc) = account {

        // 提取解析后的座位信息

        let details = if let Some(last_attempt) = result.attempts.last() {

            if let Some(raw) = &last_attempt.raw_response {

                // 尝试解析JSON格式的响应数据

                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) {

                    let mut info = Vec::new();

                    if let Some(usage) = parsed.get("seat_usage") {

                        info.push(format!("座位使用: {}", usage));

                    }

                    if let Some(price) = parsed.get("total_monthly_price") {

                        info.push(format!("月费: ${}", price));

                    }

                    if let Some(price_per) = parsed.get("price_per_seat") {

                        info.push(format!("每座位: ${}", price_per));

                    }

                    if let Some(next_billing) = parsed.get("next_billing_time") {

                        info.push(format!("下次计费: {}", next_billing));

                    }

                    if !info.is_empty() {

                        format!(" ({})", info.join(", "))

                    } else {

                        String::new()

                    }

                } else {

                    String::new()

                }

            } else {

                String::new()

            }

        } else {

            String::new()

        };

        

        let log = OperationLog::new(

            OperationType::UpdateSeats,

            if result.success { OperationStatus::Success } else { OperationStatus::Failed },

            format!("更新座位数为{}: {}{}", seat_count, acc.email, details),

        )

        .with_account(uuid, acc.email);

        

        let _ = store.add_log(log).await;

    }

    

    Ok(serde_json::to_value(result).unwrap())

}



#[tauri::command]

pub async fn get_billing(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 确保有有效的Token（优先使用缓存）

    // 新账号 (auth1_) 走 _backend/ 代理 + Devin 认证，需要新鲜的 session_token

    let force = account.refresh_token.as_ref().map_or(false, |rt| rt.starts_with("auth1_"));

    ensure_valid_token_with_force(&store, &mut account, uuid, force).await?;

    

    // 使用缓存的或新刷新的Token

    let token = account.token.ok_or("No token available")?;

    let refresh_token = account.refresh_token.clone();

    

    // 新账号把 auth1_token 传给 get_team_billing 做 Devin 认证

    let auth1 = refresh_token.as_deref().filter(|t| t.starts_with("auth1_"));

    let is_new_account = auth1.is_some();

    

    // 获取账单信息

    let windsurf_service = WindsurfService::new();

    let mut result = windsurf_service.get_team_billing(&token, auth1)

        .await

        .map_err(|e: AppError| e.to_string())?;

    

    // 新账号 (Windsurf 2.0) 的 GetTeamBilling 不返回 plan_name / 配额 / 支付方式

    // 用 GetPlanStatus 补位 plan_name 和配额（银行卡需 Stripe Portal，暂不支持）

    if is_new_account {

        match windsurf_service.get_plan_status(&token).await {

            Ok(plan_status_result) => {

                if plan_status_result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

                    if let Some(plan_status) = plan_status_result.get("plan_status") {

                        // 1. plan_name: GetTeamBilling 没给才补

                        let need_plan_name = result.get("plan_name")

                            .and_then(|v| v.as_str())

                            .map(|s| s.is_empty())

                            .unwrap_or(true);

                        if need_plan_name {

                            if let Some(name) = plan_status.get("plan_name").and_then(|v| v.as_str()) {

                                result["plan_name"] = serde_json::json!(name);

                            }

                        }

                        

                        // 2. 配额: GetTeamBilling 没给 total_quota 或为 0 才补

                        let existing_total = result.get("total_quota")

                            .and_then(|v| v.as_i64())

                            .unwrap_or(0);

                        if existing_total == 0 {

                            let monthly_prompt = plan_status.get("monthly_prompt_credits")

                                .and_then(|v| v.as_i64())

                                .unwrap_or(0);

                            let available_flex = plan_status.get("available_flex_credits")

                                .and_then(|v| v.as_i64())

                                .unwrap_or(0);

                            let used_prompt = plan_status.get("used_prompt_credits")

                                .and_then(|v| v.as_i64())

                                .unwrap_or(0);

                            let monthly_flow = plan_status.get("monthly_flow_credits")

                                .and_then(|v| v.as_i64())

                                .unwrap_or(0);

                            

                            if monthly_prompt > 0 {

                                result["base_quota"] = serde_json::json!(monthly_prompt);

                                result["total_quota"] = serde_json::json!(monthly_prompt + available_flex);

                                result["used_quota"] = serde_json::json!(used_prompt);

                                if available_flex > 0 {

                                    result["extra_credits"] = serde_json::json!(available_flex);

                                }

                                // cache_limit 用 flow credits 近似表示（老账号是 subMesssage_12.int_9）

                                if monthly_flow > 0 {

                                    result["cache_limit"] = serde_json::json!(monthly_flow);

                                }

                            }

                        }

                        

                        // 3. 标记为新账号 + 附加 plan_status 原始数据（便于前端展示"完整账单请到 Stripe Portal 查看"）

                        result["is_new_account"] = serde_json::json!(true);

                        result["plan_status_data"] = plan_status.clone();

                    }

                } else {

                    log::warn!("[get_billing] GetPlanStatus 返回失败: {:?}", plan_status_result.get("error"));

                }

            },

            Err(e) => {

                log::warn!("[get_billing] GetPlanStatus 调用失败（不影响主流程）: {}", e);

            }

        }

        

        // 4. 支付方式: _backend/ 代理不返回 payment_method，

        let mut acc_full = store.get_account(uuid).await.ok();

        if result.get("payment_method").is_none() {

            let stripe_session_id_from_logs = match store.get_logs(Some(1000)).await {

                Ok(logs) => find_stripe_session_id_from_logs(&logs, uuid),

                Err(_) => None,

            };



            let stripe_session_id = acc_full.as_ref()

                .and_then(|acc| acc.stripe_checkout_session_id.clone())

                .or(stripe_session_id_from_logs);



            if let Some(session_id) = stripe_session_id {

                log::info!("[get_billing] 尝试通过 Stripe session 回查支付方式: {}", session_id);

                match windsurf_service.get_stripe_payment_method_from_session(&session_id).await {

                    Ok(Some(pm)) => {

                        log::info!("[get_billing] Stripe 回查成功: {:?}", pm);

                        result["payment_method"] = pm.clone();



                        if let Some(acc) = acc_full.as_mut() {

                            let mut dirty = false;



                            if acc.stripe_checkout_session_id.as_deref() != Some(session_id.as_str()) {

                                acc.stripe_checkout_session_id = Some(session_id.clone());

                                dirty = true;

                            }



                            if let Some(last4) = pm.get("last4").and_then(|v| v.as_str()) {

                                if acc.bound_card_last4.as_deref() != Some(last4) {

                                    acc.bound_card_last4 = Some(last4.to_string());

                                    dirty = true;

                                }

                            }



                            if let Some(card_type) = pm.get("type").and_then(|v| v.as_str()) {

                                if acc.bound_card_brand.as_deref() != Some(card_type) {

                                    acc.bound_card_brand = Some(card_type.to_string());

                                    dirty = true;

                                }

                            }



                            if let Some(exp_month) = pm.get("exp_month").and_then(|v| v.as_str()) {

                                if acc.bound_card_exp_month.as_deref() != Some(exp_month) {

                                    acc.bound_card_exp_month = Some(exp_month.to_string());

                                    dirty = true;

                                }

                            }



                            if let Some(exp_year) = pm.get("exp_year").and_then(|v| v.as_str()) {

                                if acc.bound_card_exp_year.as_deref() != Some(exp_year) {

                                    acc.bound_card_exp_year = Some(exp_year.to_string());

                                    dirty = true;

                                }

                            }



                            if dirty {

                                acc.bound_card_at = Some(chrono::Utc::now());

                                if let Err(e) = store.update_account(acc.clone()).await {

                                    log::warn!("[get_billing] 保存 Stripe 回查卡信息失败: {}", e);

                                }

                            }

                        }

                    }

                    Ok(None) => {

                        log::info!("[get_billing] Stripe 回查未拿到支付方式");

                    }

                    Err(e) => {

                        log::warn!("[get_billing] Stripe 回查失败: {}", e);

                    }

                }

            } else {

                log::info!("[get_billing] 没有可用的 Stripe session_id");

            }

        }

        // 4. 支付方式: _backend/ 代理不返回 payment_method，
        //    但 web-backend.windsurf.com 老端点接受 session_token 且会返回 payment_method (field 10)
        //    (已验证: get_plan_status 使用同一老端点 + session_token 可正常工作)
        if result.get("payment_method").is_none() {
            log::info!("[get_billing] _backend 未返回 payment_method, 尝试 web-backend 老端点...");
            match windsurf_service.get_team_billing_via_old_endpoint(&token).await {
                Ok(old_result) => {
                    if let Some(pm) = old_result.get("payment_method") {
                        log::info!("[get_billing] 老端点返回了 payment_method: {:?}", pm);
                        result["payment_method"] = pm.clone();
                    } else {
                        log::info!("[get_billing] 老端点也没有 payment_method");
                    }
                    // 同时补充其他 _backend 缺失的字段
                    for key in ["on_trial", "plan_unit_amount", "sub_interval", "invoice_url"] {
                        if result.get(key).is_none() {
                            if let Some(v) = old_result.get(key) {
                                result[key] = v.clone();
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("[get_billing] 老端点调用失败: {}", e);
                }
            }
        }

        // 5. 最终回退: 从本地 Account 读取协议绑卡时保存的卡信息
        if result.get("payment_method").is_none() {
            if let Ok(acc_full) = store.get_account(uuid).await {
                if let Some(last4) = acc_full.bound_card_last4.as_ref() {
                    let mut pm = serde_json::json!({
                        "type": acc_full.bound_card_brand.clone().unwrap_or_else(|| "card".to_string()),
                        "last4": last4,
                    });
                    if let Some(m) = acc_full.bound_card_exp_month.as_ref() {
                        pm["exp_month"] = serde_json::json!(m);
                    }
                    if let Some(y) = acc_full.bound_card_exp_year.as_ref() {
                        pm["exp_year"] = serde_json::json!(y);
                    }
                    result["payment_method"] = pm;
                    log::info!("[get_billing] 使用本地 bound_card 数据: ****{}", last4);
                }
            }
        }

    }

    

    // 记录日志

    let account = store.get_account(uuid).await.ok();

    if let Some(acc) = account {

        let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

        let log = OperationLog::new(

            OperationType::GetBilling,

            if success { OperationStatus::Success } else { OperationStatus::Failed },

            format!("查询账单{}: {}", if success { "成功" } else { "失败" }, acc.email),

        )

        .with_account(uuid, acc.email);

        

        let _ = store.add_log(log).await;

    }



    Ok(result)

}



/// 创建 Stripe Billing Portal Session（新账号专用）
///
/// 复刻官网 "Manage billing" 按钮: GET https://app.devin.ai/api/billing/subscription/manage
/// 返回 302 重定向到 https://billing.stripe.com/p/session/live_xxx，
/// 该 URL 可直接交给系统浏览器打开，用户即可看到完整的 Stripe Billing Portal
/// （订阅详情、银行卡、发票历史、取消订阅等）。
///
/// 仅对新账号（refresh_token 以 auth1_ 开头）有效；老账号调用会返回错误。
#[tauri::command]
pub async fn create_billing_portal_session(
    id: String,
    store: State<'_, Arc<DataStore>>,
) -> Result<String, String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    let account = store.get_account(uuid).await.map_err(|e| e.to_string())?;

    let auth1_token = account
        .refresh_token
        .as_ref()
        .filter(|t| t.starts_with("auth1_"))
        .cloned()
        .ok_or_else(|| "此功能仅支持 Windsurf 2.0 新账号（refresh_token 需以 auth1_ 开头）".to_string())?;

    let windsurf_service = WindsurfService::new();
    let portal_url = windsurf_service
        .create_billing_portal_session(&auth1_token)
        .await
        .map_err(|e: AppError| e.to_string())?;

    // 记录日志
    let success = portal_url.contains("billing.stripe.com");
    let log = OperationLog::new(
        OperationType::GetBilling,
        if success { OperationStatus::Success } else { OperationStatus::Failed },
        format!("获取 Stripe Billing Portal {}: {}", if success { "成功" } else { "失败" }, account.email),
    )
    .with_account(uuid, account.email);
    let _ = store.add_log(log).await;

    Ok(portal_url)
}

/// 取消订阅

///

/// # Arguments

/// * `id` - 账户ID

/// * `reason` - 取消原因（例如："too_expensive", "not_using", "missing_features", "switching_service", "other"）

///

/// # Returns

/// 返回包含操作结果的 JSON 对象

#[tauri::command]

pub async fn cancel_subscription(

    id: String,

    reason: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    // 获取Token

    let token = store.get_decrypted_token(uuid)

        .await

        .map_err(|e| e.to_string())?

        .ok_or("No token available")?;



    // 取消订阅

    let windsurf_service = WindsurfService::new();

    let result: serde_json::Value = windsurf_service.cancel_plan(&token, &reason)

        .await

        .map_err(|e: AppError| e.to_string())?;



    // 获取账号信息用于日志记录

    let account = store.get_account(uuid).await.ok();



    // 记录日志

    if let Some(acc) = &account {

        let log = OperationLog::new(

            OperationType::UpdatePlan, // 使用 UpdatePlan 类型，因为这也是订阅管理操作

            if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

                OperationStatus::Success

            } else {

                OperationStatus::Failed

            },

            format!("取消订阅 (原因: {}): {}", reason, acc.email),

        )

        .with_account(uuid, acc.email.clone());



        let _ = store.add_log(log).await;

    }



    Ok(result)

}



/// 恢复订阅

///

/// # Arguments

/// * `id` - 账户ID

///

/// # Returns

/// 返回包含操作结果的 JSON 对象

#[tauri::command]

pub async fn resume_subscription(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    // 获取Token

    let token = store.get_decrypted_token(uuid)

        .await

        .map_err(|e| e.to_string())?

        .ok_or("No token available")?;



    // 恢复订阅

    let windsurf_service = WindsurfService::new();

    let result: serde_json::Value = windsurf_service.resume_plan(&token)

        .await

        .map_err(|e: AppError| e.to_string())?;



    // 获取账号信息用于日志记录

    let account = store.get_account(uuid).await.ok();



    // 记录日志

    if let Some(acc) = &account {

        let log = OperationLog::new(

            OperationType::UpdatePlan, // 使用 UpdatePlan 类型，因为这也是订阅管理操作

            if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

                OperationStatus::Success

            } else {

                OperationStatus::Failed

            },

            format!("恢复订阅: {}", acc.email),

        )

        .with_account(uuid, acc.email.clone());



        let _ = store.add_log(log).await;

    }



    Ok(result)

}



async fn reset_credits_internal(

    id: &str,

    seat_count: Option<i32>,

    store: &Arc<DataStore>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 确保有有效的Token（优先使用缓存）

    ensure_valid_token(&store, &mut account, uuid).await?;

    

    // 解密Token

    let token = store.get_decrypted_token(uuid)

        .await

        .map_err(|e| e.to_string())?

        .ok_or("No token available")?;

    

    // 获取座位数选项配置

    let settings = store.get_settings().await.map_err(|e| e.to_string())?;

    let seat_count_options = settings.seat_count_options;

    

    // 执行积分重置

    let windsurf_service = WindsurfService::new();

    let result: serde_json::Value = windsurf_service.reset_credits(&token, seat_count, account.last_seat_count, &seat_count_options)

        .await

        .map_err(|e: AppError| e.to_string())?;

    

    // 更新最后使用的座位数

    if let Some(used_seat_count) = result.get("seat_count_used").and_then(|v| v.as_i64()) {

        account.last_seat_count = Some(used_seat_count as i32);

        store.update_account(account.clone())

            .await

            .map_err(|e| e.to_string())?;

    }

    

    // 记录详细的操作日志

    let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

    let message = result.get("message")

        .and_then(|v| v.as_str())

        .unwrap_or(if success { "积分重置成功" } else { "积分重置失败" });

    

    let log = OperationLog::new(

        OperationType::ResetCredits,

        if success { OperationStatus::Success } else { OperationStatus::Failed },

        format!("{}: {}", account.email, message),

    )

    .with_account(uuid, account.email.clone());

    

    let _ = store.add_log(log).await;

    

    Ok(result)

}



#[tauri::command]

pub async fn update_plan(

    id: String,

    plan_type: String,

    payment_period: Option<u8>,

    preview: Option<bool>,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    let period = payment_period.unwrap_or(1); // 默认月付

    let is_preview = preview.unwrap_or(false); // 默认非预览模式



    // 获取Token

    let token = store.get_decrypted_token(uuid)

        .await

        .map_err(|e| e.to_string())?

        .ok_or("No token available")?;



    // 更换订阅计划

    let windsurf_service = WindsurfService::new();

    let result: serde_json::Value = windsurf_service.update_plan(&token, &plan_type, period, is_preview)

        .await

        .map_err(|e: AppError| e.to_string())?;



    // 获取账号信息用于日志记录

    let account = store.get_account(uuid).await.ok();

    let period_name = if period == 2 { "年付" } else { "月付" };



    // 记录日志

    if let Some(acc) = &account {

        let log = OperationLog::new(

            OperationType::UpdatePlan,

            if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

                OperationStatus::Success

            } else {

                OperationStatus::Failed

            },

            format!("更换订阅计划到{}({}): {}", plan_type, period_name, acc.email),

        )

        .with_account(uuid, acc.email.clone());



        let _ = store.add_log(log).await;

    }



    // 更换成功后,获取最新的账号信息并保存

    if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

        let mut updated_account = store.get_account(uuid).await.map_err(|e| e.to_string())?;

        let settings = store.get_settings().await.map_err(|e| e.to_string())?;

        

        if settings.use_lightweight_api {

            // 使用轻量级 GetPlanStatus API

            if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {

                if plan_result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

                    if let Some(plan_status) = plan_result.get("plan_status") {

                        // 更新套餐名称

                        if let Some(plan_name) = plan_status.get("plan_name").and_then(|v| v.as_str()) {

                            updated_account.plan_name = Some(plan_name.to_string());

                        }

                        

                        // 更新已用配额 (used_prompt_credits + used_flex_credits)

                        let used_prompt = plan_status.get("used_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                        let used_flex = plan_status.get("used_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                        updated_account.used_quota = Some((used_prompt + used_flex) as i32);

                        

                        // 更新总配额 (available_flex_credits + available_prompt_credits)

                        let available_flex = plan_status.get("available_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                        let available_prompt = plan_status.get("available_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                        if available_flex > 0 || available_prompt > 0 {

                            updated_account.total_quota = Some((available_flex + available_prompt) as i32);

                        }

                        

                        // 更新订阅到期时间 (plan_end)

                        if let Some(plan_end) = plan_status.get("plan_end").and_then(|v| v.as_i64()) {

                            updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(plan_end, 0);

                        }



                        updated_account.last_quota_update = Some(chrono::Utc::now());

                        store.update_account(updated_account.clone()).await

                            .map_err(|e| format!("保存账户信息失败: {}", e))?;

                    }

                }

            }

        } else {

            // 使用完整的 GetCurrentUser API

            if let Ok(user_info_result) = windsurf_service.get_current_user(&token).await {

                if let Some(user_info) = user_info_result.get("user_info") {

                    // 提取用户基本信息（包含api_key）

                    if let Some(user) = user_info.get("user") {

                        if let Some(api_key) = user.get("api_key").and_then(|v| v.as_str()) {

                            updated_account.windsurf_api_key = Some(api_key.to_string());

                        }

                    }



                    // 提取套餐信息

                    if let Some(plan) = user_info.get("plan") {

                        if let Some(plan_name) = plan.get("plan_name").and_then(|v| v.as_str()) {

                            updated_account.plan_name = Some(plan_name.to_string());

                        }

                    }



                    // 提取配额信息

                    if let Some(subscription) = user_info.get("subscription") {

                        if let Some(used) = subscription.get("used_quota").and_then(|v| v.as_i64()) {

                            updated_account.used_quota = Some(used as i32);

                        }

                        if let Some(total) = subscription.get("quota").and_then(|v| v.as_i64()) {

                            updated_account.total_quota = Some(total as i32);

                        }

                        if let Some(expires_at) = subscription.get("expires_at").and_then(|v| v.as_i64()) {

                            updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(expires_at, 0);

                        }

                        // 提取订阅激活状态

                        if let Some(subscription_active) = subscription.get("subscription_active").and_then(|v| v.as_bool()) {

                            updated_account.subscription_active = Some(subscription_active);

                        }

                    }



                    updated_account.last_quota_update = Some(chrono::Utc::now());

                    store.update_account(updated_account.clone()).await

                        .map_err(|e| format!("保存账户信息失败: {}", e))?;

                }

            }

        }



        // 返回包含更新后账户信息的结果

        return Ok(json!({

            "success": true,

            "plan_type": plan_type,

            "plan_name": updated_account.plan_name,

            "used_quota": updated_account.used_quota,

            "total_quota": updated_account.total_quota,

            "subscription_expires_at": updated_account.subscription_expires_at.map(|dt| dt.to_rfc3339()),

            "message": format!("成功更换到 {} 计划", plan_type.to_uppercase())

        }));

    }



    Ok(result)

}



#[tauri::command]

pub async fn get_current_user(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    get_current_user_internal(&id, &store, false).await

}



/// 内部实现，支持 401 自动重试

fn get_current_user_internal<'a>(

    id: &'a str,

    store: &'a Arc<DataStore>,

    is_retry: bool,

) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send + 'a>> {

    let id = id.to_string();

    let store = store.clone();

    Box::pin(async move {

        let id = id.as_str();

    let uuid = Uuid::parse_str(id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 确保有有效的Token（如果是重试则强制刷新）

    ensure_valid_token_with_force(&store, &mut account, uuid, is_retry).await?;

    

    // 使用缓存的或新刷新的Token

    let token = account.token.clone().ok_or("No token available")?;

    

    // 读取设置，判断使用哪个 API

    let settings = store.get_settings().await.map_err(|e| e.to_string())?;

    let windsurf_service = WindsurfService::new();

    

    println!("[get_current_user] use_lightweight_api = {}", settings.use_lightweight_api);

    

    let mut updated_account = store.get_account(uuid).await.map_err(|e| e.to_string())?;

    

    if settings.use_lightweight_api {

        // 使用轻量级 GetPlanStatus API

        println!("[get_current_user] Using GetPlanStatus API");

        

        let result = windsurf_service.get_plan_status(&token)

            .await

            .map_err(|e: AppError| e.to_string())?;

        

        // 检查是否是 401 错误，如果是且未重试过，则强制刷新 token 并重试

        let status_code = result.get("status_code").and_then(|v| v.as_u64()).unwrap_or(0);

        if status_code == 401 && !is_retry {

            println!("[get_current_user] 收到 401 错误，强制刷新 token 并重试...");

            return get_current_user_internal(id, &store, true).await;

        }

        

        let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

        

        // 提取 plan_status 中的字段，构建兼容的数据结构

        let mut plan_name = String::new();

        let mut used_quota: i64 = 0;

        let mut total_quota: i64 = 0;

        let mut expires_at: i64 = 0;

        

        if success {

            if let Some(plan_status) = result.get("plan_status") {

                // 更新套餐名称

                if let Some(pn) = plan_status.get("plan_name").and_then(|v| v.as_str()) {

                    plan_name = pn.to_string();

                    updated_account.plan_name = Some(pn.to_string());

                }

                

                // 更新已用配额 (used_prompt_credits + used_flex_credits)

                let used_prompt = plan_status.get("used_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                let used_flex = plan_status.get("used_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                used_quota = used_prompt + used_flex;

                updated_account.used_quota = Some(used_quota as i32);

                

                // 更新总配额 (available_flex_credits + available_prompt_credits)

                let available_flex = plan_status.get("available_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                let available_prompt = plan_status.get("available_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                total_quota = available_flex + available_prompt;

                if total_quota > 0 {

                    updated_account.total_quota = Some(total_quota as i32);

                }

                

                // 更新订阅到期时间 (plan_end)

                if let Some(plan_end) = plan_status.get("plan_end").and_then(|v| v.as_i64()) {

                    expires_at = plan_end;

                    updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(plan_end, 0);

                }
                
                // 更新每日/每周配额信息（新配额系统）
                if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                    updated_account.daily_quota_remaining = Some(v as i32);
                }
                if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                    updated_account.weekly_quota_remaining = Some(v as i32);
                }
                if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                    updated_account.daily_quota_reset = Some(v);
                }
                if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                    updated_account.weekly_quota_reset = Some(v);
                }

                

                // 更新每日/每周配额信息（新配额系统）

                if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {

                    updated_account.daily_quota_remaining = Some(v as i32);

                }

                if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {

                    updated_account.weekly_quota_remaining = Some(v as i32);

                }

                if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {

                    updated_account.daily_quota_reset = Some(v);

                }

                if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {

                    updated_account.weekly_quota_reset = Some(v);

                }



                updated_account.last_quota_update = Some(chrono::Utc::now());
<<<<<<< HEAD

                // API 调通说明账号有效，把 status 重置为 Active（防止被历史 Error 状态卡住前端 UI）

                updated_account.status = AccountStatus::Active;

=======
                // API 调通说明账号有效，把 status 重置为 Active（防止被历史 Error 状态卡住前端 UI）
                updated_account.status = crate::models::AccountStatus::Active;
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
                store.update_account(updated_account).await

                    .map_err(|e| format!("保存账户信息失败: {}", e))?;

            }

        }

        

        // 记录日志

        let log = OperationLog::new(

            OperationType::GetAccountInfo,

            if success { OperationStatus::Success } else { OperationStatus::Failed },

            format!("获取配额信息(轻量级){}: {}", if success { "成功" } else { "失败" }, account.email),

        )

        .with_account(uuid, account.email);

        let _ = store.add_log(log).await;

        

        // 返回与完整 API 兼容的数据格式

        if success {

            Ok(json!({

                "success": true,

                "lightweight": true,

                "user_info": {

                    "plan": {

                        "plan_name": plan_name

                    },

                    "subscription": {

                        "used_quota": used_quota,

                        "quota": total_quota,

                        "expires_at": expires_at

                    }

                },

                "plan_status": result.get("plan_status"),

                "timestamp": chrono::Utc::now().to_rfc3339()

            }))

        } else {

            Ok(result)

        }

    } else {

        // 使用完整的 GetCurrentUser API

        println!("[get_current_user] Using GetCurrentUser API");

        

        let result: serde_json::Value = windsurf_service.get_current_user(&token)

            .await

            .map_err(|e: AppError| e.to_string())?;

        

        // 检查是否是 401 错误，如果是且未重试过，则强制刷新 token 并重试

        let status_code = result.get("status_code").and_then(|v| v.as_u64()).unwrap_or(0);

        if status_code == 401 && !is_retry {

            println!("[get_current_user] 收到 401 错误，强制刷新 token 并重试...");

            return get_current_user_internal(id, &store, true).await;

        }

        

        // 提取并保存用户信息到数据库

        if let Some(user_info) = result.get("user_info") {

            // 提取用户基本信息（包含api_key）

            if let Some(user) = user_info.get("user") {

                if let Some(api_key) = user.get("api_key").and_then(|v| v.as_str()) {

                    updated_account.windsurf_api_key = Some(api_key.to_string());

                }

            }



            // 提取套餐信息

            if let Some(plan) = user_info.get("plan") {

                if let Some(plan_name) = plan.get("plan_name").and_then(|v| v.as_str()) {

                    updated_account.plan_name = Some(plan_name.to_string());

                }

            }



            // 提取配额信息

            if let Some(subscription) = user_info.get("subscription") {

                if let Some(used) = subscription.get("used_quota").and_then(|v| v.as_i64()) {

                    updated_account.used_quota = Some(used as i32);

                }

                if let Some(total) = subscription.get("quota").and_then(|v| v.as_i64()) {

                    updated_account.total_quota = Some(total as i32);

                }

                // 提取订阅到期时间

                if let Some(expires_at) = subscription.get("expires_at").and_then(|v| v.as_i64()) {

                    updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(expires_at, 0);

                }

                // 提取订阅激活状态

                if let Some(subscription_active) = subscription.get("subscription_active").and_then(|v| v.as_bool()) {

                    updated_account.subscription_active = Some(subscription_active);

                }

            }

            

            // 提取 is_root_admin（团队所有者）

            let is_root_admin = user_info.get("is_root_admin")

                .and_then(|v| v.as_bool())

                .unwrap_or(false);

            updated_account.is_team_owner = Some(is_root_admin);

<<<<<<< HEAD


            // 补充调用 GetPlanStatus 获取每日/每周配额信息

            if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {

                if let Some(plan_status) = plan_result.get("plan_status") {

                    if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {

                        updated_account.daily_quota_remaining = Some(v as i32);

                    }

                    if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {

                        updated_account.weekly_quota_remaining = Some(v as i32);

                    }

                    if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {

                        updated_account.daily_quota_reset = Some(v);

                    }

                    if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {

                        updated_account.weekly_quota_reset = Some(v);

                    }

                }

            }



=======
            // 补充调用 GetPlanStatus 获取每日/每周配额信息
            if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {
                if let Some(plan_status) = plan_result.get("plan_status") {
                    if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                        updated_account.daily_quota_remaining = Some(v as i32);
                    }
                    if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                        updated_account.weekly_quota_remaining = Some(v as i32);
                    }
                    if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                        updated_account.daily_quota_reset = Some(v);
                    }
                    if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                        updated_account.weekly_quota_reset = Some(v);
                    }
                }
            }

>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
            updated_account.last_quota_update = Some(chrono::Utc::now());
            // API 调通说明账号有效，把 status 重置为 Active（防止被历史 Error 状态卡住前端 UI）
            updated_account.status = crate::models::AccountStatus::Active;

            // API 调通说明账号有效，把 status 重置为 Active（防止被历史 Error 状态卡住前端 UI）

            updated_account.status = AccountStatus::Active;



            // 保存更新后的账户信息

            store.update_account(updated_account).await

                .map_err(|e| format!("保存账户信息失败: {}", e))?;

        }

        

        // 记录日志

        let success = result.get("user_info").is_some();

        let log = OperationLog::new(

            OperationType::GetAccountInfo,

            if success { OperationStatus::Success } else { OperationStatus::Failed },

            format!("获取用户信息{}: {}", if success { "成功" } else { "失败" }, account.email),

        )

        .with_account(uuid, account.email);

        

        let _ = store.add_log(log).await;

        

        Ok(result)

    }

    })

}



#[tauri::command]

pub async fn get_account_info(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 确保有有效的Token（优先使用缓存）

    ensure_valid_token(&store, &mut account, uuid).await?;

    

    // 使用缓存的或新刷新的Token

    let token = account.token.ok_or("No token available")?;

    

    // 使用AuthService获取Firebase账户信息

    let auth_service = AuthService::new();

    let account_info = auth_service.get_account_info(&token)

        .await

        .map_err(|e| e.to_string())?;

    

    Ok(json!({

        "success": true,

        "local_info": {

            "id": account.id,

            "email": account.email,

            "nickname": account.nickname,

            "group": account.group,

            "tags": account.tags,

            "created_at": account.created_at,

            "last_login_at": account.last_login_at,

            "last_seat_count": account.last_seat_count,

            "token_expires_at": account.token_expires_at,

            "status": account.status

        },

        "firebase_info": {

            "localId": account_info.local_id,

            "email": account_info.email,

            "displayName": account_info.display_name,

            "emailVerified": account_info.email_verified,

            "passwordHash": account_info.password_hash,

            "passwordUpdatedAt": account_info.password_updated_at,

            "validSince": account_info.valid_since,

            "disabled": account_info.disabled,

            "createdAt": account_info.created_at,

            "lastLoginAt": account_info.last_login_at,

            "lastRefreshAt": account_info.last_refresh_at,

            "providerUserInfo": account_info.provider_user_info

        }

    }))

}



#[tauri::command]

pub async fn get_team_credit_entries(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    

    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

    

    // 确保有有效的Token

    ensure_valid_token(&store, &mut account, uuid).await?;

    

    let token = account.token.ok_or("No token available")?;

    

    // 调用GetTeamCreditEntries API

    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.get_team_credit_entries(&token)

        .await

        .map_err(|e| e.to_string())?;

    

    Ok(result)

}



#[tauri::command]

pub async fn batch_reset_credits(

    ids: Vec<String>,

    seat_count: Option<i32>,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    use futures::stream::{self, StreamExt};

    

    // 设置并发限制，避免过多并发请求

    const MAX_CONCURRENT: usize = 5;

    

    // 创建任务流并并发执行

    let store_arc = store.inner().clone();

    

    let results: Vec<serde_json::Value> = stream::iter(ids.into_iter().enumerate())

        .map(|(index, id_str)| {

            let store_clone = store_arc.clone();

            let seat_count_clone = seat_count;

            

            async move {

                if let Ok(_uuid) = Uuid::parse_str(&id_str) {

                    // 每个请求添加小延迟，分散请求

                    if index > 0 {

                        tokio::time::sleep(tokio::time::Duration::from_millis(200 * index as u64)).await;

                    }

                    

                    // 直接使用 API 服务进行批量操作

                    // 注意：传递 seat_count_clone 而不是强制分配的座位数

                    // 如果 seat_count 为 None，reset_credits_internal 会使用账号的 last_seat_count

                    let result = match reset_credits_internal(&id_str, seat_count_clone, &store_clone).await {

                        Ok(res) => {

                            let seat_used = res.get("seat_count_used")

                                .and_then(|v| v.as_i64())

                                .unwrap_or(0);

                            json!({ "success": true, "data": res, "seat_count_used": seat_used })

                        },

                        Err(err) => json!({ "success": false, "error": err })

                    };

                    json!({

                        "id": id_str,

                        "result": result

                    })

                } else {

                    json!({

                        "id": id_str,

                        "result": json!({ "success": false, "error": "Invalid UUID" })

                    })

                }

            }

        })

        .buffer_unordered(MAX_CONCURRENT)

        .collect()

        .await;

    

    // 记录批量操作日志

    let success_count = results.iter()

        .filter(|r| r.get("result")

            .and_then(|res| res.get("success"))

            .and_then(|s| s.as_bool())

            .unwrap_or(false))

        .count();

    

    let log = OperationLog::new(

        OperationType::BatchOperation,

        if success_count > 0 { OperationStatus::Success } else { OperationStatus::Failed },

        format!("批量重置积分: 成功 {}/{} 个账号", success_count, results.len()),

    );

    let _ = store.add_log(log).await;

    

    Ok(json!({

        "results": results,

        "success_count": success_count,

        "total_count": results.len()

    }))

}



/// 批量刷新 Token（优化版：只在最后保存一次）

#[tauri::command]

pub async fn batch_refresh_tokens(

    ids: Vec<String>,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    use futures::stream::{self, StreamExt};

    

    let store_arc = store.inner().clone();

    let settings = store.get_settings().await.map_err(|e| e.to_string())?;

    let use_lightweight_api = settings.use_lightweight_api;

    

    let max_concurrent = if settings.unlimited_concurrent_refresh {
        ids.len().max(1)
    } else {
        settings.concurrent_limit.max(1)
    };

    

    let results: Vec<serde_json::Value> = stream::iter(ids.into_iter())

        .map(|id_str| {

            let store_clone = store_arc.clone();

            

            async move {

                if Uuid::parse_str(&id_str).is_ok() {

                    match refresh_token_internal(&id_str, &store_clone, use_lightweight_api, false).await {

                        Ok(res) => json!({

                            "id": id_str,

                            "success": true,

                            "data": res

                        }),

                        Err(err) => json!({

                            "id": id_str,

                            "success": false,

                            "error": err

                        })

                    }

                } else {

                    json!({

                        "id": id_str,

                        "success": false,

                        "error": "Invalid UUID"

                    })

                }

            }

        })

        .buffer_unordered(max_concurrent)

        .collect()

        .await;

    

    // 所有操作完成后，统一保存一次

    store.flush().await.map_err(|e| e.to_string())?;

    

    let success_count = results.iter()

        .filter(|r| r.get("success").and_then(|s| s.as_bool()).unwrap_or(false))

        .count();

    

    let log = OperationLog::new(

        OperationType::BatchOperation,

        if success_count > 0 { OperationStatus::Success } else { OperationStatus::Failed },

        format!("批量刷新Token: 成功 {}/{} 个账号", success_count, results.len()),

    );

    let _ = store.add_log(log).await;

    

    Ok(json!({

        "results": results,

        "success_count": success_count,

        "total_count": results.len()

    }))

}



/// 内部刷新 Token 方法（支持延迟保存）

async fn refresh_token_internal(

    id: &str,

    store: &Arc<DataStore>,

    use_lightweight_api: bool,

    save_immediately: bool,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(id).map_err(|e| e.to_string())?;

    

    let account = store.get_account(uuid).await.map_err(|e| e.to_string())?;

    let auth_service = AuthService::new();

    
<<<<<<< HEAD

    // 刷新 token：auth1_ 前缀走 WindsurfPostAuth，其他走 Firebase，两者失败才 fallback 到密码登录

    // 避免批量刷新时 20+ 并发同时打 _devin-auth/password/login 导致 429

    let (token, refresh_token_new, expires_at) = refresh_token_or_relogin(

        &auth_service,

        store,

        uuid,

        &account.email,

        account.refresh_token.as_deref(),

    ).await?;

=======
    // 刷新 token：auth1_ 前缀走 WindsurfPostAuth，其他走 Firebase，两者失败才 fallback 到密码登录
    // 避免批量刷新时 20+ 并发同时打 _devin-auth/password/login 导致 429
    let (token, refresh_token_new, expires_at) = refresh_token_or_relogin(
        &auth_service,
        store,
        uuid,
        &account.email,
        account.refresh_token.as_deref(),
    ).await?;
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
    

    // 使用延迟保存的方法更新 token

    if save_immediately {

        store.update_account_tokens(uuid, token.clone(), refresh_token_new, expires_at)

            .await.map_err(|e| e.to_string())?;

    } else {

        store.update_account_tokens_no_save(uuid, token.clone(), refresh_token_new, expires_at)

            .await.map_err(|e| e.to_string())?;

    }

    

    // 获取配额信息

    let windsurf_service = WindsurfService::new();

    let mut updated_account = store.get_account(uuid).await.map_err(|e| e.to_string())?;

    

    if use_lightweight_api {

        // 使用轻量级 GetPlanStatus API

        if let Ok(result) = windsurf_service.get_plan_status(&token).await {

            if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {

                if let Some(plan_status) = result.get("plan_status") {

                    if let Some(plan_name) = plan_status.get("plan_name").and_then(|v| v.as_str()) {

                        updated_account.plan_name = Some(plan_name.to_string());

                    }

                    let used_prompt = plan_status.get("used_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    let used_flex = plan_status.get("used_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    updated_account.used_quota = Some((used_prompt + used_flex) as i32);

                    

                    let available_flex = plan_status.get("available_flex_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    let available_prompt = plan_status.get("available_prompt_credits").and_then(|v| v.as_i64()).unwrap_or(0);

                    if available_flex > 0 || available_prompt > 0 {

                        updated_account.total_quota = Some((available_flex + available_prompt) as i32);

                    }

                    

                    if let Some(plan_end) = plan_status.get("plan_end").and_then(|v| v.as_i64()) {

                        updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(plan_end, 0);

                    }
<<<<<<< HEAD

                    

                    // 提取每日/每周配额信息（新配额系统）

                    if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {

                        updated_account.daily_quota_remaining = Some(v as i32);

                    }

                    if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {

                        updated_account.weekly_quota_remaining = Some(v as i32);

                    }

                    if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {

                        updated_account.daily_quota_reset = Some(v);

                    }

                    if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {

                        updated_account.weekly_quota_reset = Some(v);

                    }

=======
                    
                    // 提取每日/每周配额信息（新配额系统）
                    if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                        updated_account.daily_quota_remaining = Some(v as i32);
                    }
                    if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                        updated_account.weekly_quota_remaining = Some(v as i32);
                    }
                    if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                        updated_account.daily_quota_reset = Some(v);
                    }
                    if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                        updated_account.weekly_quota_reset = Some(v);
                    }
                    
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
                    updated_account.last_quota_update = Some(chrono::Utc::now());

                    // 轻量级API路径也检查免费试用资格

                    if let Ok(eligible) = windsurf_service.check_pro_trial_eligibility(&token).await {

                        updated_account.trial_eligible = Some(eligible);

                        log::info!("[refresh_token_internal][lightweight] email={}, trial_eligible={}", updated_account.email, eligible);

                    }

                }

            }

        }

    } else {

        // 使用完整的 GetCurrentUser API

        if let Ok(user_info_result) = windsurf_service.get_current_user(&token).await {

            if let Some(user_info) = user_info_result.get("user_info") {

                // 提取用户基本信息（包含api_key）

                if let Some(user) = user_info.get("user") {

                    if let Some(api_key) = user.get("api_key").and_then(|v| v.as_str()) {

                        updated_account.windsurf_api_key = Some(api_key.to_string());

                    }

                    // 提取账户禁用状态

                    if let Some(disable_codeium) = user.get("disable_codeium").and_then(|v| v.as_bool()) {

                        updated_account.is_disabled = Some(disable_codeium);

                    }

                    // 提取免费试用使用状态

                    if let Some(used_trial) = user.get("used_trial").and_then(|v| v.as_bool()) {

                        updated_account.used_trial = Some(used_trial);

                    }

                }



                // 提取套餐信息

                if let Some(plan) = user_info.get("plan") {

                    if let Some(plan_name) = plan.get("plan_name").and_then(|v| v.as_str()) {

                        updated_account.plan_name = Some(plan_name.to_string());

                    }

                }



                // 提取配额信息

                if let Some(subscription) = user_info.get("subscription") {

                    if let Some(used) = subscription.get("used_quota").and_then(|v| v.as_i64()) {

                        updated_account.used_quota = Some(used as i32);

                    }

                    if let Some(total) = subscription.get("quota").and_then(|v| v.as_i64()) {

                        updated_account.total_quota = Some(total as i32);

                    }

                    if let Some(expires_at) = subscription.get("expires_at").and_then(|v| v.as_i64()) {

                        updated_account.subscription_expires_at = chrono::DateTime::from_timestamp(expires_at, 0);

                    }

                    // 提取订阅激活状态

                    if let Some(subscription_active) = subscription.get("subscription_active").and_then(|v| v.as_bool()) {

                        updated_account.subscription_active = Some(subscription_active);

                    }

                }

                

                // 提取 is_root_admin（团队所有者）

                let is_root_admin = user_info.get("is_root_admin")

                    .and_then(|v| v.as_bool())

                    .unwrap_or(false);

                updated_account.is_team_owner = Some(is_root_admin);

<<<<<<< HEAD


                // 补充调用 GetPlanStatus 获取每日/每周配额信息

                if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {

                    if let Some(ps) = plan_result.get("plan_status") {

                        if let Some(v) = ps.get("daily_quota_remaining").and_then(|v| v.as_i64()) {

                            updated_account.daily_quota_remaining = Some(v as i32);

                        }

                        if let Some(v) = ps.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {

                            updated_account.weekly_quota_remaining = Some(v as i32);

                        }

                        if let Some(v) = ps.get("daily_quota_reset").and_then(|v| v.as_i64()) {

                            updated_account.daily_quota_reset = Some(v);

                        }

                        if let Some(v) = ps.get("weekly_quota_reset").and_then(|v| v.as_i64()) {

                            updated_account.weekly_quota_reset = Some(v);

                        }

                    }

                }



=======
                // 补充调用 GetPlanStatus 获取每日/每周配额信息
                if let Ok(plan_result) = windsurf_service.get_plan_status(&token).await {
                    if let Some(plan_status) = plan_result.get("plan_status") {
                        if let Some(v) = plan_status.get("daily_quota_remaining").and_then(|v| v.as_i64()) {
                            updated_account.daily_quota_remaining = Some(v as i32);
                        }
                        if let Some(v) = plan_status.get("weekly_quota_remaining").and_then(|v| v.as_i64()) {
                            updated_account.weekly_quota_remaining = Some(v as i32);
                        }
                        if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                            updated_account.daily_quota_reset = Some(v);
                        }
                        if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                            updated_account.weekly_quota_reset = Some(v);
                        }
                    }
                }

>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
                updated_account.last_quota_update = Some(chrono::Utc::now());

                // 检查免费试用资格

                if let Ok(eligible) = windsurf_service.check_pro_trial_eligibility(&token).await {

                    updated_account.trial_eligible = Some(eligible);

                    log::info!("[refresh_token_internal] email={}, trial_eligible={}", updated_account.email, eligible);

                }

            }

        }

    }

    

    // 如果使用轻量级 API，需要单独获取 is_team_owner 和 used_trial

    if updated_account.is_team_owner.is_none() {

        if let Ok(user_result) = windsurf_service.get_current_user(&token).await {

            if let Some(user_info) = user_result.get("user_info") {

                let is_root_admin = user_info.get("is_root_admin")

                    .and_then(|v| v.as_bool())

                    .unwrap_or(false);

                updated_account.is_team_owner = Some(is_root_admin);

                // 同时提取 used_trial

                if let Some(user) = user_info.get("user") {

                    if let Some(used_trial) = user.get("used_trial").and_then(|v| v.as_bool()) {

                        updated_account.used_trial = Some(used_trial);

                    }

                }

            }

        }

    }

    

    // 更新账号信息（不立即保存）

    store.update_account_no_save(updated_account.clone()).await

        .map_err(|e| format!("更新账户信息失败: {}", e))?;

    

    // 返回完整的账户信息，供前端直接更新本地 store

    Ok(json!({

        "email": account.email,

        "expires_at": expires_at.to_rfc3339(),

        "plan_name": updated_account.plan_name,

        "used_quota": updated_account.used_quota,

        "total_quota": updated_account.total_quota,

        "windsurf_api_key": updated_account.windsurf_api_key,

        "is_disabled": updated_account.is_disabled,

        "is_team_owner": updated_account.is_team_owner,

        "used_trial": updated_account.used_trial,

        "subscription_expires_at": updated_account.subscription_expires_at.map(|t| t.to_rfc3339()),

        "subscription_active": updated_account.subscription_active,
<<<<<<< HEAD

        "last_quota_update": updated_account.last_quota_update.map(|t| t.to_rfc3339()),

        "daily_quota_remaining": updated_account.daily_quota_remaining,

        "weekly_quota_remaining": updated_account.weekly_quota_remaining,

        "daily_quota_reset": updated_account.daily_quota_reset,

        "weekly_quota_reset": updated_account.weekly_quota_reset,

        "trial_eligible": updated_account.trial_eligible

=======
        "last_quota_update": updated_account.last_quota_update.map(|t| t.to_rfc3339()),
        "daily_quota_remaining": updated_account.daily_quota_remaining,
        "weekly_quota_remaining": updated_account.weekly_quota_remaining,
        "daily_quota_reset": updated_account.daily_quota_reset,
        "weekly_quota_reset": updated_account.weekly_quota_reset
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
    }))

}



/// 获取试用绑卡链接

///

/// # Arguments

/// * `id` - 账号ID

/// * `teams_tier` - 团队等级: 1=Teams, 2=Pro, 3=Enterprise

/// * `payment_period` - 支付周期: 1=月付, 2=年付

/// * `team_name` - 团队名称 (仅 Teams/Enterprise 需要)

/// * `seat_count` - 席位数量 (仅 Teams/Enterprise 需要)

/// * `turnstile_token` - Turnstile 验证令牌 (Pro 需要)

///

/// # Returns

/// 返回包含Stripe Checkout链接的JSON对象

#[tauri::command]

pub async fn get_trial_payment_link(

    id: String,

    teams_tier: Option<i32>,

    payment_period: Option<i32>,

    team_name: Option<String>,

    seat_count: Option<i32>,

    turnstile_token: Option<String>,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;

<<<<<<< HEAD


    // === Devin 平台账号：走 app.devin.ai/api/billing/checkout ===

    if account.is_devin() {

        let auth1 = account.refresh_token.clone()

            .ok_or_else(|| "Devin 账号缺少 auth1 token (refresh_token)，请先登录".to_string())?;

        let devin = DevinService::new();

        let result = match devin.get_trial_checkout_url(&auth1).await {

            Ok((stripe_url, org_id, org_name)) => {

                let session_id = WindsurfService::extract_checkout_session_id(&stripe_url);

                if let Some(sid) = session_id.clone() {

                    account.stripe_checkout_session_id = Some(sid);

                    if let Err(e) = store.update_account(account.clone()).await {

                        log::warn!("[get_trial_payment_link/devin] 保存 stripe_checkout_session_id 失败: {}", e);

                    }

                }

                let log = OperationLog::new(

                    OperationType::GetAccountInfo,

                    OperationStatus::Success,

                    format!("获取 Devin 试用绑卡链接成功: {}", account.email),

                )

                .with_account(uuid, account.email.clone())

                .with_details(json!({

                    "account_source": "devin",

                    "plan_id": "pro",

                    "is_trial": true,

                    "stripe_url": stripe_url,

                    "subscription_url": stripe_url,

                    "stripe_session_id": session_id,

                }));

                let _ = store.add_log(log).await;

                json!({

                    "success": true,

                    "stripe_url": stripe_url,

                    "subscription_url": stripe_url,

                    "org_id": org_id,

                    "org_name": org_name,

                    "stripe_session_id": session_id,

                    "teams_tier": 2,

                    "payment_period": 1,

                    "status_code": 200,

                    "account_source": "devin",

                    "timestamp": chrono::Utc::now().to_rfc3339(),

                })

            }

            Err(e) => {

                let err_msg = e.to_string();

                let log = OperationLog::new(

                    OperationType::GetAccountInfo,

                    OperationStatus::Failed,

                    format!("获取 Devin 试用绑卡链接失败: {} - {}", account.email, err_msg),

                )

                .with_account(uuid, account.email.clone())

                .with_details(json!({

                    "account_source": "devin",

                    "error": err_msg,

                }));

                let _ = store.add_log(log).await;

                json!({

                    "success": false,

                    "error": err_msg,

                    "account_source": "devin",

                    "timestamp": chrono::Utc::now().to_rfc3339(),

                })

            }

        };

        return Ok(result);

    }



    // 确保有有效的Token

    // 不因 auth1_ 无条件强制刷新；token 仍有效时沿用现有 session，避免网络抖动导致获取支付链接失败

    let force = false;

    ensure_valid_token_with_force(&store, &mut account, uuid, force).await?;



    let token = account.token.clone().ok_or("No token available")?;

    let refresh_token = account.refresh_token.clone();


=======
    // 确保有有效的Token
    // 新账号 (auth1_) 强制刷新，SubscribeToPlan 需要新鲜的 session_token
    let force = account.refresh_token.as_ref().map_or(false, |rt| rt.starts_with("auth1_"));
    ensure_valid_token_with_force(&store, &mut account, uuid, force).await?;

    let token = account.token.ok_or("No token available")?;
    let refresh_token = account.refresh_token.clone();
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca

    // 默认值

    let final_teams_tier = teams_tier.unwrap_or(2); // 默认 Pro

    let final_payment_period = payment_period.unwrap_or(1); // 默认月付



    // 调用Windsurf API获取支付链接
<<<<<<< HEAD

    // 如果 refresh_token 是 auth1_ 开头，传给 subscribe_to_plan 做 Bearer 认证

    let auth1 = refresh_token.as_deref().filter(|t| t.starts_with("auth1_"));

    let windsurf_service = WindsurfService::new();

    let mut result = windsurf_service.subscribe_to_plan(

        &token,

        auth1,

=======
    // 如果 refresh_token 是 auth1_ 开头，传给 subscribe_to_plan 做 Bearer 认证
    let auth1 = refresh_token.as_deref().filter(|t| t.starts_with("auth1_"));
    let windsurf_service = WindsurfService::new();
    let result = windsurf_service.subscribe_to_plan(
        &token,
        auth1,
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
        final_teams_tier,

        final_payment_period,

        team_name.as_deref(),

        seat_count,

        turnstile_token.as_deref()

    )
        .await
        .map_err(|e: AppError| e.to_string())?;

    let first_success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    let is_auth_fail = !first_success && (
        is_401_error(&result)
            || result.get("status_code").and_then(|v| v.as_u64()).map_or(false, |code| code == 403)
            || result.get("error_details")
                .and_then(|v| v.as_str())
                .map_or(false, |e| {
                    e.contains("unauthenticated")
                        || e.contains("invalid token")
                        || e.contains("invalid_token")
                })
    );

    if is_auth_fail {
        log::info!("[get_trial_payment_link] 认证失败，强制刷新 session 后重试: {}", account.email);
        ensure_valid_token_with_force(&store, &mut account, uuid, true).await?;
        let retry_token = account.token.clone().ok_or("No token available after refresh")?;
        let retry_refresh_token = account.refresh_token.clone();
        let retry_auth1 = retry_refresh_token.as_deref().filter(|t| t.starts_with("auth1_"));
        result = windsurf_service.subscribe_to_plan(
            &retry_token,
            retry_auth1,
            final_teams_tier,
            final_payment_period,
            team_name.as_deref(),
            seat_count,
            turnstile_token.as_deref()
        )
            .await
            .map_err(|e: AppError| e.to_string())?;
        log::info!("[get_trial_payment_link] 强制刷新后重试结果: success={}", result.get("success").and_then(|v| v.as_bool()).unwrap_or(false));
    }

    // 如果返回 failed_precondition，可能是批量导入时 auth 状态不一致

    // 尝试完整重新登录后再试一次

    let first_success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    let is_precondition_fail = !first_success && result.get("error_details")
        .and_then(|v| v.as_str())
        .map_or(false, |e| e.contains("failed_precondition"));

    if is_precondition_fail {
        log::info!("[get_trial_payment_link] failed_precondition，尝试完整重新登录后重试: {}", account.email);
        let auth_service = AuthService::new();
        let password = store.get_decrypted_password(uuid).await.map_err(|e| e.to_string())?;
        match auth_service.sign_in_v2(&account.email, &password).await {
            Ok(auth_result) => {
                let new_token = auth_result.session_token;
                let new_auth1 = auth_result.auth1_token;
                let new_expires = chrono::Utc::now() + chrono::Duration::hours(1);
                store.update_account_tokens(uuid, new_token.clone(), new_auth1.clone(), new_expires)
                    .await.map_err(|e| e.to_string())?;
                account.token = Some(new_token.clone());
                account.refresh_token = Some(new_auth1.clone());

                let retry_auth1: Option<&str> = if new_auth1.starts_with("auth1_") { Some(&new_auth1) } else { None };
                result = windsurf_service.subscribe_to_plan(
                    &new_token,
                    retry_auth1,
                    final_teams_tier,
                    final_payment_period,
                    team_name.as_deref(),
                    seat_count,
                    turnstile_token.as_deref()
                ).await.map_err(|e: AppError| e.to_string())?;
                log::info!("[get_trial_payment_link] 重新登录后重试结果: success={}", result.get("success").and_then(|v| v.as_bool()).unwrap_or(false));
            }
            Err(e) => {
                log::warn!("[get_trial_payment_link] 重新登录失败，返回原始错误: {}", e);
            }
        }
    }

    let windsurf_success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    if !windsurf_success && final_teams_tier == 2 {
        if let Some(auth1) = account.refresh_token.as_deref().filter(|t| t.starts_with("auth1_")) {
            log::info!("[get_trial_payment_link] Windsurf SubscribeToPlan 失败，尝试 Devin checkout fallback: {}", account.email);
            let devin = DevinService::new();
            match devin.get_trial_checkout_url(auth1).await {
                Ok((stripe_url, org_id, org_name)) => {
                    let session_id = WindsurfService::extract_checkout_session_id(&stripe_url);
                    result = json!({
                        "success": true,
                        "stripe_url": stripe_url,
                        "subscription_url": stripe_url,
                        "stripe_session_id": session_id,
                        "org_id": org_id,
                        "org_name": org_name,
                        "teams_tier": 2,
                        "payment_period": 1,
                        "status_code": 200,
                        "account_source": "windsurf",
                        "checkout_source": "devin_fallback",
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    });
                }
                Err(e) => {
                    log::warn!("[get_trial_payment_link] Devin checkout fallback 失败: {}", e);
                    if let Some(obj) = result.as_object_mut() {
                        obj.insert("checkout_fallback".to_string(), json!("devin_failed"));
                        obj.insert("fallback_error".to_string(), json!(e.to_string()));
                    }
                }
            }
        }
    }

    // 记录日志

    let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

    let stripe_url = result.get("stripe_url").and_then(|v| v.as_str()).unwrap_or("");

    let stripe_session_id = result.get("stripe_session_id")

        .and_then(|v| v.as_str())

        .map(|s| s.to_string())

        .or_else(|| WindsurfService::extract_checkout_session_id(stripe_url));



    if success {

        if let Some(session_id) = stripe_session_id.clone() {

            account.stripe_checkout_session_id = Some(session_id);

            if let Err(e) = store.update_account(account.clone()).await {

                log::warn!("[subscribe_to_plan] 保存 stripe_checkout_session_id 失败: {}", e);

            }

        }

    }



    let plan_name = if final_teams_tier == 1 { "Teams" } else { "Pro" };

    let period_name = if final_payment_period == 1 { "月付" } else { "年付" };

    

    let log = OperationLog::new(

        OperationType::GetAccountInfo, // 暂时使用GetAccountInfo类型，可以考虑添加新的类型

        if success { OperationStatus::Success } else { OperationStatus::Failed },

        format!(

            "获取试用绑卡链接{}: {} (计划: {} {})",

            if success { "成功" } else { "失败" },

            account.email,

            plan_name,

            period_name

        ),

    )

    .with_account(uuid, account.email.clone())

    .with_details(json!({

        "teams_tier": final_teams_tier,

        "payment_period": final_payment_period,

        "stripe_url": stripe_url,

        "stripe_session_id": stripe_session_id,

    }));



    let _ = store.add_log(log).await;



    Ok(result)

}



/// 获取团队配置

#[tauri::command]

pub async fn get_team_config(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;



    // 确保有有效的Token

    ensure_valid_token(&store, &mut account, uuid).await?;



    let token = account.token.ok_or("No token available")?;



    // 调用API获取团队配置

    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.get_team_config(&token)

        .await

        .map_err(|e: AppError| e.to_string())?;



    Ok(result)

}



/// 更新团队配置

#[tauri::command]

pub async fn update_team_config(

    id: String,

    config: serde_json::Value,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;



    // 确保有有效的Token

    ensure_valid_token(&store, &mut account, uuid).await?;



    let token = account.token.ok_or("No token available")?;



    // 调用API更新团队配置

    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.update_team_config(&token, config)

        .await

        .map_err(|e: AppError| e.to_string())?;



    // 记录日志

    let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

    let log = OperationLog::new(

        OperationType::GetAccountInfo,

        if success { OperationStatus::Success } else { OperationStatus::Failed },

        format!(

            "更新团队设置{}: {}",

            if success { "成功" } else { "失败" },

            account.email

        ),

    )

    .with_account(uuid, account.email.clone());



    let _ = store.add_log(log).await;



    Ok(result)

}



/// 获取可用模型配置

#[tauri::command]

pub async fn get_cascade_model_configs(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;



    ensure_valid_token(&store, &mut account, uuid).await?;



    let token = account.token.ok_or("No token available")?;



    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.get_cascade_model_configs(&token)

        .await

        .map_err(|e: AppError| e.to_string())?;



    Ok(result)

}



/// 获取 Command 模型配置

#[tauri::command]

pub async fn get_command_model_configs(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;



    ensure_valid_token(&store, &mut account, uuid).await?;



    let token = account.token.ok_or("No token available")?;



    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.get_command_model_configs(&token)

        .await

        .map_err(|e: AppError| e.to_string())?;



    Ok(result)

}



/// 获取团队模型控制配置

#[tauri::command]

pub async fn get_team_organizational_controls(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;



    ensure_valid_token(&store, &mut account, uuid).await?;



    let token = account.token.ok_or("No token available")?;



    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.get_team_organizational_controls(&token)

        .await

        .map_err(|e: AppError| e.to_string())?;



    Ok(result)

}



/// 更新团队模型控制配置

#[tauri::command]

pub async fn upsert_team_organizational_controls(

    id: String,

    team_id: String,

    cascade_models: Vec<String>,

    command_models: Vec<String>,

    extension_models: Vec<String>,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;



    ensure_valid_token(&store, &mut account, uuid).await?;



    let token = account.token.ok_or("No token available")?;



    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.upsert_team_organizational_controls(

        &token,

        &team_id,

        cascade_models,

        command_models,

        extension_models,

    )

        .await

        .map_err(|e: AppError| e.to_string())?;



    Ok(result)

}



/// 获取可用的 MCP 插件列表

#[tauri::command]

pub async fn get_available_mcp_plugins(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;



    // 确保有有效的Token

    ensure_valid_token(&store, &mut account, uuid).await?;



    // 获取 api_key (windsurf_api_key)

    let api_key = account.windsurf_api_key.clone().unwrap_or_default();

    if api_key.is_empty() {

        return Err("账号没有 API Key，请先刷新账号信息".to_string());

    }



    // 调用 API 获取 MCP 插件列表

    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.get_available_mcp_plugins(&api_key)

        .await

        .map_err(|e: AppError| e.to_string())?;



    Ok(result)

}



/// 删除用户 (Windsurf DeleteUser API)

#[tauri::command]

pub async fn delete_windsurf_user(

    id: String,

    store: State<'_, Arc<DataStore>>,

) -> Result<serde_json::Value, String> {

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;



    // 获取账号信息

    let mut account = store.get_account(uuid)

        .await

        .map_err(|e| e.to_string())?;



    // 确保有有效的Token

    ensure_valid_token(&store, &mut account, uuid).await?;



    let token = account.token.clone().unwrap_or_default();

    if token.is_empty() {

        return Err("账号没有有效的 Token".to_string());

    }



    // 获取 api_key

    let api_key = account.windsurf_api_key.clone().unwrap_or_default();

    if api_key.is_empty() {

        return Err("账号没有 API Key，请先刷新账号信息".to_string());

    }



    log::info!("[DeleteWindsurfUser] Deleting user for account: {}", account.email);



    // 调用 DeleteUser API

    let windsurf_service = WindsurfService::new();

    let result = windsurf_service.delete_user(&token, &api_key)

        .await

        .map_err(|e: AppError| e.to_string())?;



    Ok(result)

}

