use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// 带颜色的标签
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagWithColor {
    pub name: String,
    pub color: String, // RGBA格式，如 "rgba(255, 100, 100, 1)"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub email: String,
    pub password: String, // 加密后的密码
    pub nickname: String,
    pub tags: Vec<String>,
    #[serde(default, rename = "tagColors")]
    pub tag_colors: Vec<TagWithColor>, // 带颜色的标签
    pub group: Option<String>,
    pub token: Option<String>, // 加密后的Token
    pub refresh_token: Option<String>, // 加密后的Refresh Token
    pub token_expires_at: Option<DateTime<Utc>>,
    pub last_seat_count: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub status: AccountStatus,
    // 配额和套餐信息
    pub plan_name: Option<String>,
    pub used_quota: Option<i32>,
    pub total_quota: Option<i32>,
    pub last_quota_update: Option<DateTime<Utc>>,
    // 每日/每周配额信息（新配额系统）
    #[serde(default)]
    pub daily_quota_remaining: Option<i32>,   // 每日配额剩余百分比 (0-100)
    #[serde(default)]
    pub weekly_quota_remaining: Option<i32>,  // 每周配额剩余百分比 (0-100)
    #[serde(default)]
    pub daily_quota_reset: Option<i64>,       // 每日配额重置时间 (Unix时间戳)
    #[serde(default)]
    pub weekly_quota_reset: Option<i64>,      // 每周配额重置时间 (Unix时间戳)
    // 订阅到期时间
    pub subscription_expires_at: Option<DateTime<Utc>>,
    // 订阅是否激活 (从 GetCurrentUser API 的 subscription.subscription_active 获取)
    #[serde(default)]
    pub subscription_active: Option<bool>,
    // Windsurf API Key (用户的 UUID，从 GetCurrentUser API 获取)
    pub windsurf_api_key: Option<String>,
    // 账户是否被禁用 (从 GetCurrentUser API 的 user.disable_codeium 获取)
    #[serde(default)]
    pub is_disabled: Option<bool>,
    // 是否为团队所有者（Admin角色，有团队成员的主账号）
    #[serde(default)]
    pub is_team_owner: Option<bool>,
    // 是否已使用过免费试用 (从 GetCurrentUser API 的 user.used_trial 获取)
    #[serde(default)]
    pub used_trial: Option<bool>,
    // 是否有免费Pro试用资格 (从 CheckProTrialEligibility API 获取)
    #[serde(default)]
    pub trial_eligible: Option<bool>,
    // 已绑定支付方式信息（来自本应用的 Stripe 协议绑卡成功结果）
    // 用于展示银行卡尾号，避免依赖 Stripe Billing Portal
    #[serde(default)]
    pub bound_card_last4: Option<String>,
    #[serde(default)]
    pub bound_card_brand: Option<String>,
    #[serde(default)]
    pub bound_card_exp_month: Option<String>,
    #[serde(default)]
    pub bound_card_exp_year: Option<String>,
    #[serde(default)]
    pub bound_card_at: Option<DateTime<Utc>>,
    // 自定义排序顺序（用于拖拽排序）
    // SubscribeToPlan 返回的 Stripe checkout session ID (cs_live_xxx)
    // 用于后续查询 Stripe API 获取已完成支付的银行卡信息
    #[serde(default)]
    pub stripe_checkout_session_id: Option<String>,
    #[serde(default, rename = "sortOrder")]
    pub sort_order: i32,
    // 账号来源平台：
    // - "windsurf"（默认）：在 windsurf.com 注册，订阅走 windsurf SubscribeToPlan
    // - "devin"：在 app.devin.ai 注册，订阅走 Devin /api/billing/checkout
    // 注：序列化字段名为 snake_case `account_source`，与前端 TS 接口保持一致；
    // 同时通过 alias 兼容历史版本写入磁盘的 camelCase `accountSource`。
    #[serde(default, alias = "accountSource")]
    pub account_source: Option<String>,
}

pub const ACCOUNT_SOURCE_WINDSURF: &str = "windsurf";
pub const ACCOUNT_SOURCE_DEVIN: &str = "devin";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    Inactive,
    Error(String),
}

impl Account {
    pub fn new(email: String, password: String, nickname: String, tags: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            email,
            password,
            nickname,
            tags,
            tag_colors: Vec::new(),
            group: None,
            token: None,
            refresh_token: None,
            token_expires_at: None,
            last_seat_count: None,
            created_at: Utc::now(),
            last_login_at: None,
            status: AccountStatus::Inactive,
            plan_name: None,
            used_quota: None,
            total_quota: None,
            last_quota_update: None,
            daily_quota_remaining: None,
            weekly_quota_remaining: None,
            daily_quota_reset: None,
            weekly_quota_reset: None,
            subscription_expires_at: None,
            subscription_active: None,
            windsurf_api_key: None,
            is_disabled: None,
            is_team_owner: None,
            used_trial: None,
            trial_eligible: None,
            bound_card_last4: None,
            bound_card_brand: None,
            bound_card_exp_month: None,
            bound_card_exp_year: None,
            bound_card_at: None,
            stripe_checkout_session_id: None,
            sort_order: 0,
            account_source: None,
        }
    }

    pub fn is_token_valid(&self) -> bool {
        if let Some(expires_at) = self.token_expires_at {
            expires_at > Utc::now()
        } else {
            false
        }
    }

    /// 判断账号是否为 Devin 平台账号
    pub fn is_devin(&self) -> bool {
        matches!(self.account_source.as_deref(), Some(ACCOUNT_SOURCE_DEVIN))
    }
}
