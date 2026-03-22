use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// 机器设备码快照记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineIdRecord {
    /// 唯一标识
    pub id: String,
    /// 用户自定义标签（如"主力机"、"备用"）
    pub label: String,
    /// 备注信息
    #[serde(default)]
    pub note: String,
    /// storage.json 中的 telemetry.machineId
    pub machine_id: String,
    /// storage.json 中的 telemetry.macMachineId
    pub mac_machine_id: String,
    /// storage.json 中的 telemetry.sqmId
    pub sqm_id: String,
    /// storage.json 中的 telemetry.devDeviceId
    pub dev_device_id: String,
    /// Windows 注册表 HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid
    #[serde(default)]
    pub registry_machine_guid: Option<String>,
    /// 最后关联的账号邮箱（切号时自动记录）
    #[serde(default)]
    pub last_associated_email: Option<String>,
    /// 最后关联的账号ID
    #[serde(default)]
    pub last_associated_account_id: Option<String>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后使用时间
    #[serde(default)]
    pub last_used_at: Option<DateTime<Utc>>,
    /// 是否为当前正在使用的设备码
    #[serde(default)]
    pub is_current: bool,
    /// 是否已收藏
    #[serde(default)]
    pub is_bookmarked: bool,
}

/// 当前系统的机器设备码（从 storage.json 和注册表读取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentMachineIds {
    pub machine_id: Option<String>,
    pub mac_machine_id: Option<String>,
    pub sqm_id: Option<String>,
    pub dev_device_id: Option<String>,
    pub registry_machine_guid: Option<String>,
}
