use crate::models::{MachineIdRecord, CurrentMachineIds};
use crate::utils::errors::{AppError, AppResult};
use chrono::Utc;
use log::{error, info, warn};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::RwLock;
use uuid::Uuid;

#[cfg(target_os = "windows")]
use winreg::{RegKey, enums::{HKEY_LOCAL_MACHINE, KEY_READ}};

/// 机器设备码记录存储
pub struct MachineIdStore {
    records: Arc<RwLock<Vec<MachineIdRecord>>>,
    file_path: PathBuf,
}

impl MachineIdStore {
    pub fn new(app_handle: &tauri::AppHandle) -> AppResult<Self> {
        let app_data_dir = app_handle.path().app_data_dir()
            .map_err(|e| AppError::Config(format!("Failed to get app data dir: {}", e)))?;
        
        fs::create_dir_all(&app_data_dir)?;
        
        let file_path = app_data_dir.join("machine_ids.json");
<<<<<<< HEAD
        let records: Vec<MachineIdRecord> = if file_path.exists() {
=======
        let records = if file_path.exists() {
>>>>>>> 8bd8dc7f9351f7d68f2aa0e67ad5a345970d0fca
            let data = fs::read_to_string(&file_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        
        Ok(Self {
            records: Arc::new(RwLock::new(records)),
            file_path,
        })
    }
    
    async fn save(&self) -> AppResult<()> {
        let records = self.records.read().await;
        let data = serde_json::to_string_pretty(&*records)?;
        fs::write(&self.file_path, data)?;
        Ok(())
    }
    
    pub async fn get_all(&self) -> Vec<MachineIdRecord> {
        self.records.read().await.clone()
    }
    
    pub async fn add(&self, record: MachineIdRecord) -> AppResult<()> {
        let mut records = self.records.write().await;
        records.push(record);
        drop(records);
        self.save().await
    }
    
    pub async fn update_label(&self, id: &str, label: String, note: String) -> AppResult<()> {
        let mut records = self.records.write().await;
        if let Some(record) = records.iter_mut().find(|r| r.id == id) {
            record.label = label;
            record.note = note;
        } else {
            return Err(AppError::Config("设备码记录不存在".to_string()));
        }
        drop(records);
        self.save().await
    }
    
    pub async fn delete(&self, id: &str) -> AppResult<()> {
        let mut records = self.records.write().await;
        records.retain(|r| r.id != id);
        drop(records);
        self.save().await
    }
    
    pub async fn mark_current(&self, id: &str) -> AppResult<()> {
        let mut records = self.records.write().await;
        for record in records.iter_mut() {
            record.is_current = record.id == id;
            if record.id == id {
                record.last_used_at = Some(Utc::now());
            }
        }
        drop(records);
        self.save().await
    }
    
    pub async fn clear_current(&self) -> AppResult<()> {
        let mut records = self.records.write().await;
        for record in records.iter_mut() {
            record.is_current = false;
        }
        drop(records);
        self.save().await
    }
    
    pub async fn clear_all(&self, keep_bookmarked: bool) -> AppResult<()> {
        let mut records = self.records.write().await;
        if keep_bookmarked {
            records.retain(|r| r.is_bookmarked);
        } else {
            records.clear();
        }
        drop(records);
        self.save().await
    }
    
    pub async fn toggle_bookmark(&self, id: &str, bookmarked: bool) -> AppResult<()> {
        let mut records = self.records.write().await;
        if let Some(record) = records.iter_mut().find(|r| r.id == id) {
            record.is_bookmarked = bookmarked;
        } else {
            return Err(AppError::Config("设备码记录不存在".to_string()));
        }
        drop(records);
        self.save().await
    }
}

/// 读取当前系统的机器设备码
fn read_current_machine_ids() -> CurrentMachineIds {
    let mut ids = CurrentMachineIds {
        machine_id: None,
        mac_machine_id: None,
        sqm_id: None,
        dev_device_id: None,
        registry_machine_guid: None,
    };
    
    // 读取 storage.json
    let mut storage_path = directories::BaseDirs::new()
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("C:/Users/Default/AppData/Roaming"));
    storage_path.push("Windsurf");
    storage_path.push("User");
    storage_path.push("globalStorage");
    storage_path.push("storage.json");
    
    if storage_path.exists() {
        if let Ok(content) = fs::read_to_string(&storage_path) {
            if let Ok(storage) = serde_json::from_str::<Value>(&content) {
                ids.machine_id = storage.get("telemetry.machineId")
                    .and_then(|v| v.as_str()).map(String::from);
                ids.mac_machine_id = storage.get("telemetry.macMachineId")
                    .and_then(|v| v.as_str()).map(String::from);
                ids.sqm_id = storage.get("telemetry.sqmId")
                    .and_then(|v| v.as_str()).map(String::from);
                ids.dev_device_id = storage.get("telemetry.devDeviceId")
                    .and_then(|v| v.as_str()).map(String::from);
            }
        }
    }
    
    // 读取注册表 MachineGuid
    #[cfg(target_os = "windows")]
    {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(crypto_key) = hklm.open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Cryptography",
            KEY_READ
        ) {
            if let Ok(guid) = crypto_key.get_value::<String, _>("MachineGuid") {
                ids.registry_machine_guid = Some(guid);
            }
        }
    }
    
    ids
}

/// 将指定的设备码应用到系统（写入 storage.json 和注册表）
fn apply_machine_ids_to_system(record: &MachineIdRecord) -> AppResult<()> {
    // 更新 storage.json
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
        
        storage["telemetry.machineId"] = json!(record.machine_id);
        storage["telemetry.macMachineId"] = json!(record.mac_machine_id);
        storage["telemetry.sqmId"] = json!(record.sqm_id);
        storage["telemetry.devDeviceId"] = json!(record.dev_device_id);
        
        let updated = serde_json::to_string_pretty(&storage)
            .map_err(AppError::Serialization)?;
        fs::write(&storage_path, updated)
            .map_err(|e| AppError::FileOperation(format!("Failed to write storage.json: {}", e)))?;
        
        info!("Applied machine IDs to storage.json");
    } else {
        warn!("storage.json not found");
    }
    
    // 更新注册表
    #[cfg(target_os = "windows")]
    {
        if let Some(ref guid) = record.registry_machine_guid {
            use winreg::enums::KEY_ALL_ACCESS;
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            match hklm.open_subkey_with_flags(
                "SOFTWARE\\Microsoft\\Cryptography",
                KEY_ALL_ACCESS
            ) {
                Ok(crypto_key) => {
                    match crypto_key.set_value("MachineGuid", guid) {
                        Ok(()) => info!("Applied MachineGuid to registry: {}", guid),
                        Err(e) => {
                            error!("Failed to set MachineGuid: {}", e);
                            return Err(AppError::FileOperation(format!("更新注册表失败: {}. 需要管理员权限", e)));
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to open Cryptography key: {}", e);
                    return Err(AppError::FileOperation(format!("打开注册表失败: {}. 需要管理员权限", e)));
                }
            }
        }
    }
    
    Ok(())
}

/// 获取当前系统的机器设备码
#[tauri::command]
pub async fn get_current_machine_ids() -> Result<Value, String> {
    let ids = read_current_machine_ids();
    serde_json::to_value(&ids).map_err(|e| e.to_string())
}

/// 获取所有设备码历史记录
#[tauri::command]
pub async fn get_machine_id_records(
    store: State<'_, Arc<MachineIdStore>>,
) -> Result<Value, String> {
    let records = store.get_all().await;
    serde_json::to_value(&records).map_err(|e| e.to_string())
}

/// 保存当前系统设备码到历史记录
#[tauri::command]
pub async fn save_current_machine_id(
    label: String,
    note: Option<String>,
    associated_email: Option<String>,
    associated_account_id: Option<String>,
    bookmarked: Option<bool>,
    store: State<'_, Arc<MachineIdStore>>,
) -> Result<Value, String> {
    let ids = read_current_machine_ids();
    
    // 检查是否有有效的设备码
    if ids.machine_id.is_none() {
        return Ok(json!({
            "success": false,
            "error": "无法读取当前设备码，storage.json 可能不存在"
        }));
    }
    
    let record = MachineIdRecord {
        id: Uuid::new_v4().to_string(),
        label,
        note: note.unwrap_or_default(),
        machine_id: ids.machine_id.unwrap_or_default(),
        mac_machine_id: ids.mac_machine_id.unwrap_or_default(),
        sqm_id: ids.sqm_id.unwrap_or_default(),
        dev_device_id: ids.dev_device_id.unwrap_or_default(),
        registry_machine_guid: ids.registry_machine_guid,
        last_associated_email: associated_email,
        last_associated_account_id: associated_account_id,
        created_at: Utc::now(),
        last_used_at: Some(Utc::now()),
        is_current: true,
        is_bookmarked: bookmarked.unwrap_or(false),
    };
    
    // 清除旧的 is_current 标记
    store.clear_current().await.map_err(|e| e.to_string())?;
    
    let record_id = record.id.clone();
    store.add(record).await.map_err(|e| e.to_string())?;
    
    // 设置为当前
    store.mark_current(&record_id).await.map_err(|e| e.to_string())?;
    
    info!("Saved current machine ID with label");
    
    Ok(json!({
        "success": true,
        "id": record_id,
        "message": "设备码已保存"
    }))
}

/// 应用指定的设备码到系统
#[tauri::command]
pub async fn apply_machine_id(
    id: String,
    store: State<'_, Arc<MachineIdStore>>,
) -> Result<Value, String> {
    let records = store.get_all().await;
    let record = records.iter().find(|r| r.id == id);
    
    match record {
        Some(record) => {
            // 先保存当前设备码（如果不在记录中）
            let current_ids = read_current_machine_ids();
            let current_machine_id = current_ids.machine_id.as_deref().unwrap_or("");
            let already_saved = records.iter().any(|r| r.machine_id == current_machine_id);
            
            if !already_saved && !current_machine_id.is_empty() {
                // 自动保存当前设备码
                let auto_record = MachineIdRecord {
                    id: Uuid::new_v4().to_string(),
                    label: format!("自动保存 {}", Utc::now().format("%m-%d %H:%M")),
                    note: "切换设备码前自动保存".to_string(),
                    machine_id: current_ids.machine_id.unwrap_or_default(),
                    mac_machine_id: current_ids.mac_machine_id.unwrap_or_default(),
                    sqm_id: current_ids.sqm_id.unwrap_or_default(),
                    dev_device_id: current_ids.dev_device_id.unwrap_or_default(),
                    registry_machine_guid: current_ids.registry_machine_guid,
                    last_associated_email: None,
                    last_associated_account_id: None,
                    created_at: Utc::now(),
                    last_used_at: Some(Utc::now()),
                    is_current: false,
                    is_bookmarked: false,
                };
                let _ = store.add(auto_record).await;
            }
            
            // 应用设备码
            match apply_machine_ids_to_system(record) {
                Ok(()) => {
                    store.mark_current(&id).await.map_err(|e| e.to_string())?;
                    info!("Applied machine ID: {} ({})", record.label, id);
                    Ok(json!({
                        "success": true,
                        "message": format!("已切换到设备码: {}", record.label)
                    }))
                }
                Err(e) => {
                    error!("Failed to apply machine ID: {:?}", e);
                    Ok(json!({
                        "success": false,
                        "error": format!("应用设备码失败: {}", e)
                    }))
                }
            }
        }
        None => Ok(json!({
            "success": false,
            "error": "设备码记录不存在"
        }))
    }
}

/// 更新设备码标签和备注
#[tauri::command]
pub async fn update_machine_id_label(
    id: String,
    label: String,
    note: Option<String>,
    store: State<'_, Arc<MachineIdStore>>,
) -> Result<Value, String> {
    store.update_label(&id, label, note.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(json!({
        "success": true,
        "message": "标签已更新"
    }))
}

/// 删除设备码记录
#[tauri::command]
pub async fn delete_machine_id_record(
    id: String,
    store: State<'_, Arc<MachineIdStore>>,
) -> Result<Value, String> {
    store.delete(&id).await.map_err(|e| e.to_string())?;
    
    Ok(json!({
        "success": true,
        "message": "设备码记录已删除"
    }))
}

/// 清空设备码历史记录（可选保留收藏）
#[tauri::command]
pub async fn clear_all_machine_id_records(
    keep_bookmarked: Option<bool>,
    store: State<'_, Arc<MachineIdStore>>,
) -> Result<Value, String> {
    let keep = keep_bookmarked.unwrap_or(false);
    store.clear_all(keep).await.map_err(|e| e.to_string())?;
    info!("Cleared machine ID records (keep_bookmarked={})", keep);
    Ok(json!({
        "success": true,
        "message": if keep { "已清空非收藏设备码记录" } else { "已清空所有设备码记录" }
    }))
}

/// 切换设备码收藏状态
#[tauri::command]
pub async fn toggle_machine_id_bookmark(
    id: String,
    bookmarked: bool,
    store: State<'_, Arc<MachineIdStore>>,
) -> Result<Value, String> {
    store.toggle_bookmark(&id, bookmarked).await.map_err(|e| e.to_string())?;
    Ok(json!({
        "success": true,
        "message": if bookmarked { "已收藏" } else { "已取消收藏" }
    }))
}

/// 在切号前自动保存当前设备码（由 switch_account 内部调用）
pub async fn auto_save_before_reset(
    store: &MachineIdStore,
    associated_email: Option<String>,
    associated_account_id: Option<String>,
) {
    let ids = read_current_machine_ids();
    let current_machine_id = ids.machine_id.as_deref().unwrap_or("");
    
    if current_machine_id.is_empty() {
        return;
    }
    
    // 检查是否已保存过这个设备码
    let records = store.get_all().await;
    if let Some(existing) = records.iter().find(|r| r.machine_id == current_machine_id) {
        // 已存在，更新关联信息和使用时间
        let mut updated_records = records.clone();
        if let Some(record) = updated_records.iter_mut().find(|r| r.id == existing.id) {
            record.last_used_at = Some(Utc::now());
            if associated_email.is_some() {
                record.last_associated_email = associated_email.clone();
            }
            if associated_account_id.is_some() {
                record.last_associated_account_id = associated_account_id.clone();
            }
        }
        // 直接通过 store 更新
        let _ = store.clear_current().await;
        info!("Current machine ID already saved, updated association info");
        return;
    }
    
    // 新设备码，自动保存
    let record = MachineIdRecord {
        id: Uuid::new_v4().to_string(),
        label: format!("自动保存 {}", Utc::now().format("%m-%d %H:%M")),
        note: "切号时自动保存".to_string(),
        machine_id: ids.machine_id.unwrap_or_default(),
        mac_machine_id: ids.mac_machine_id.unwrap_or_default(),
        sqm_id: ids.sqm_id.unwrap_or_default(),
        dev_device_id: ids.dev_device_id.unwrap_or_default(),
        registry_machine_guid: ids.registry_machine_guid,
        last_associated_email: associated_email,
        last_associated_account_id: associated_account_id,
        created_at: Utc::now(),
        last_used_at: Some(Utc::now()),
        is_current: false,
        is_bookmarked: false,
    };
    
    let _ = store.add(record).await;
    info!("Auto-saved current machine ID before reset");
}
