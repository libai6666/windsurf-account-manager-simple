use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Windsurf 主实例（默认目录）的固定虚拟 ID
pub const MAIN_PROFILE_ID: &str = "main";

/// 分身的自动换号配置（与全局 Settings.auto_switch_* 字段语义一致，但每个分身独立）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileAutoSwitch {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_profile_group")]
    pub group: String,
    #[serde(default = "default_profile_threshold")]
    pub threshold: i32,
    #[serde(default = "default_profile_check_interval", rename = "checkInterval")]
    pub check_interval: i32,
}

fn default_profile_group() -> String {
    "默认分组".to_string()
}

fn default_profile_threshold() -> i32 {
    10
}

fn default_profile_check_interval() -> i32 {
    300
}

impl Default for ProfileAutoSwitch {
    fn default() -> Self {
        Self {
            enabled: false,
            group: default_profile_group(),
            threshold: default_profile_threshold(),
            check_interval: default_profile_check_interval(),
        }
    }
}

/// Windsurf 分身（独立 user-data-dir 实例）的元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindsurfProfile {
    pub id: String,
    pub name: String,
    #[serde(rename = "userDataDir")]
    pub user_data_dir: PathBuf,
    #[serde(default, rename = "extensionsDir")]
    pub extensions_dir: Option<PathBuf>,
    #[serde(default, rename = "boundAccountId")]
    pub bound_account_id: Option<String>,
    #[serde(default, rename = "autoSwitch")]
    pub auto_switch: ProfileAutoSwitch,
    #[serde(default, rename = "lastAccountEmail")]
    pub last_account_email: Option<String>,
    #[serde(default, rename = "lastUsedAt")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

impl WindsurfProfile {
    pub fn new(id: String, name: String, user_data_dir: PathBuf) -> Self {
        Self {
            id,
            name,
            user_data_dir,
            extensions_dir: None,
            bound_account_id: None,
            auto_switch: ProfileAutoSwitch::default(),
            last_account_email: None,
            last_used_at: None,
            created_at: Utc::now(),
        }
    }

    /// 该 profile 的 state.vscdb 路径
    pub fn state_vscdb_path(&self) -> PathBuf {
        self.user_data_dir
            .join("User")
            .join("globalStorage")
            .join("state.vscdb")
    }

    /// 该 profile 的 storage.json 路径（机器码所在）
    pub fn storage_json_path(&self) -> PathBuf {
        self.user_data_dir
            .join("User")
            .join("globalStorage")
            .join("storage.json")
    }

    /// 该 profile 的 Local State 路径（DPAPI 加密密钥所在）
    pub fn local_state_path(&self) -> PathBuf {
        self.user_data_dir.join("Local State")
    }

    /// 是否为主实例（不可删除、不入库的虚拟 profile）
    pub fn is_main(&self) -> bool {
        self.id == MAIN_PROFILE_ID
    }
}

/// 主实例的 user_data_dir（跨平台默认 Windsurf 目录）
pub fn main_user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("Windsurf"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|p| PathBuf::from(p).join("Library/Application Support/Windsurf"))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("HOME")
            .ok()
            .map(|p| PathBuf::from(p).join(".config/Windsurf"))
    }
}

/// 构造主实例的虚拟 profile（不入库，仅用于内部统一处理）
pub fn main_profile() -> Option<WindsurfProfile> {
    main_user_data_dir().map(|dir| WindsurfProfile {
        id: MAIN_PROFILE_ID.to_string(),
        name: "主实例".to_string(),
        user_data_dir: dir,
        extensions_dir: None,
        bound_account_id: None,
        auto_switch: ProfileAutoSwitch::default(),
        last_account_email: None,
        last_used_at: None,
        created_at: Utc::now(),
    })
}
