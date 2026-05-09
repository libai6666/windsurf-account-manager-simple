use crate::commands::switch_account_commands::{
    find_windsurf_exe,
    get_auth_token_for_account,
    reset_storage_json_for_profile,
    trigger_windsurf_callback,
};
#[cfg(target_os = "windows")]
use crate::commands::switch_account_commands::{prepare_profile_local_state, write_windsurf_auth_direct};
#[cfg(target_os = "macos")]
use crate::commands::switch_account_commands::write_windsurf_auth_direct_macos;
use crate::commands::windsurf_info::{get_windsurf_info_from_dir, WindsurfCurrentInfo};
use crate::models::{main_profile, main_user_data_dir, Account, AccountStatus, ProfileAutoSwitch, WindsurfProfile, MAIN_PROFILE_ID};
use crate::repository::DataStore;
use crate::utils::errors::{AppError, AppResult};
use chrono::Utc;
use log::{error, info, warn};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ProfileRuntimeInfo {
    pub profile: WindsurfProfile,
    #[serde(rename = "isRunning")]
    pub is_running: bool,
    #[serde(rename = "currentInfo")]
    pub current_info: Option<WindsurfCurrentInfo>,
}

#[derive(Debug, Clone)]
struct WindsurfProcessInfo {
    pid: u32,
    command_line: String,
}

fn profiles_root_dir() -> AppResult<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA")
            .map_err(|e| AppError::Config(format!("Failed to get APPDATA: {}", e)))?;
        Ok(PathBuf::from(appdata).join("WindsurfProfiles"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME")
            .map_err(|e| AppError::Config(format!("Failed to get HOME: {}", e)))?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("WindsurfProfiles"))
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let home = std::env::var("HOME")
            .map_err(|e| AppError::Config(format!("Failed to get HOME: {}", e)))?;
        Ok(PathBuf::from(home).join(".windsurf-profiles"))
    }
}

fn normalize_text(value: &str) -> String {
    value.replace('/', "\\").to_lowercase()
}

fn path_text(path: &Path) -> String {
    normalize_text(&path.to_string_lossy())
}

fn list_windsurf_processes() -> Vec<WindsurfProcessInfo> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let output = Command::new("powershell")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg("Get-CimInstance Win32_Process -Filter \"Name='Windsurf.exe'\" | ForEach-Object { \"{0}`t{1}\" -f $_.ProcessId, $_.CommandLine }")
            .creation_flags(0x08000000)
            .output();

        match output {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let line = line.trim();
                    let (pid, command_line) = line.split_once('\t')?;
                    let pid = pid.trim().parse::<u32>().ok()?;
                    Some(WindsurfProcessInfo {
                        pid,
                        command_line: command_line.trim().to_string(),
                    })
                })
                .collect(),
            Ok(output) => {
                warn!(
                    "Failed to query Windsurf processes: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                Vec::new()
            }
            Err(e) => {
                warn!("Failed to run process query: {}", e);
                Vec::new()
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ps")
            .args(["-axo", "pid=,command="])
            .output();

        match output {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let line = line.trim();
                    let mut parts = line.splitn(2, char::is_whitespace);
                    let pid = parts.next()?.trim().parse::<u32>().ok()?;
                    let command_line = parts.next().unwrap_or("").trim().to_string();
                    let lower = command_line.to_lowercase();
                    let is_windsurf = lower.contains("windsurf.app/contents")
                        || lower.contains("windsurf helper")
                        || lower.contains("/windsurf --")
                        || lower.ends_with("/windsurf")
                        || lower.contains("/windsurf ");
                    if !is_windsurf || lower.contains("windsurf-account-manager") {
                        return None;
                    }
                    Some(WindsurfProcessInfo { pid, command_line })
                })
                .collect(),
            Ok(output) => {
                warn!(
                    "[Profile][macOS] Failed to query Windsurf processes: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                Vec::new()
            }
            Err(e) => {
                warn!("[Profile][macOS] Failed to run ps process query: {}", e);
                Vec::new()
            }
        }
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Vec::new()
    }
}

fn list_windsurf_process_command_lines() -> Vec<String> {
    list_windsurf_processes()
        .into_iter()
        .map(|process| process.command_line)
        .collect()
}

fn matching_profile_process_ids(profile: &WindsurfProfile, processes: &[WindsurfProcessInfo]) -> Vec<u32> {
    let target = path_text(&profile.user_data_dir);
    processes
        .iter()
        .filter_map(|process| {
            let normalized = normalize_text(&process.command_line);
            let matched = if profile.is_main() {
                !normalized.contains("--user-data-dir") && !normalized.contains("windsurfprofiles")
            } else {
                normalized.contains(&target)
            };
            matched.then_some(process.pid)
        })
        .collect()
}

fn is_profile_running_from_cmds(profile: &WindsurfProfile, command_lines: &[String]) -> bool {
    if profile.is_main() {
        return command_lines.iter().any(|cmd| {
            let normalized = normalize_text(cmd);
            !normalized.contains("--user-data-dir")
                && !normalized.contains("windsurfprofiles")
        });
    }

    let target = path_text(&profile.user_data_dir);
    command_lines.iter().any(|cmd| normalize_text(cmd).contains(&target))
}

fn runtime_info(profile: WindsurfProfile, command_lines: &[String]) -> ProfileRuntimeInfo {
    let current_info = get_windsurf_info_from_dir(&profile.user_data_dir).ok();
    let is_running = is_profile_running_from_cmds(&profile, command_lines);
    ProfileRuntimeInfo {
        profile,
        is_running,
        current_info,
    }
}

async fn resolve_profile(store: &Arc<DataStore>, profile_id: &str) -> AppResult<WindsurfProfile> {
    if profile_id == MAIN_PROFILE_ID {
        main_profile().ok_or_else(|| AppError::Config("Failed to resolve main profile".to_string()))
    } else {
        store.get_profile(profile_id).await
    }
}

async fn wait_for_profile_account(user_data_dir: &Path, target_email: &str) -> Option<WindsurfCurrentInfo> {
    for _ in 0..12 {
        if let Ok(info) = get_windsurf_info_from_dir(user_data_dir) {
            if info.email.as_deref().map(|email| email.eq_ignore_ascii_case(target_email)).unwrap_or(false) {
                return Some(info);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
    None
}

#[cfg(target_os = "macos")]
async fn trigger_macos_profile_callback_with_retry(
    app: &tauri::AppHandle,
    profile: &WindsurfProfile,
    callback_token: &str,
    target_email: &str,
) -> Result<Option<WindsurfCurrentInfo>, String> {
    info!(
        "[Profile][macOS] Waiting for profile window before callback: profile_id={}, user_data_dir={}",
        profile.id,
        profile.user_data_dir.display()
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(3200)).await;

    trigger_windsurf_callback(app, callback_token, Some(&profile.user_data_dir))
        .await
        .map_err(|e| format!("触发分身回调失败: {}", e))?;

    if let Some(info) = wait_for_profile_account(&profile.user_data_dir, target_email).await {
        info!(
            "[Profile][macOS] Profile callback verified after first dispatch: profile_id={}, target_email={}",
            profile.id,
            target_email
        );
        return Ok(Some(info));
    }

    warn!(
        "[Profile][macOS] First callback dispatch did not update profile auth, retrying once: profile_id={}, target_email={}, user_data_dir={}",
        profile.id,
        target_email,
        profile.user_data_dir.display()
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;

    trigger_windsurf_callback(app, callback_token, Some(&profile.user_data_dir))
        .await
        .map_err(|e| format!("重试触发分身回调失败: {}", e))?;

    Ok(wait_for_profile_account(&profile.user_data_dir, target_email).await)
}

/// 检查分身是否已登录（state.vscdb 存在 windsurfAuthStatus 或 auth-usages 记录）
fn is_profile_authenticated(user_data_dir: &Path) -> bool {
    get_windsurf_info_from_dir(user_data_dir)
        .map(|info| info.is_active)
        .unwrap_or(false)
}

/// 同步关闭指定分身的所有 Windsurf 进程（写 state.vscdb 前必须调用）
#[cfg(target_os = "windows")]
fn stop_profile_processes_sync(profile: &WindsurfProfile) -> Result<usize, String> {
    use std::os::windows::process::CommandExt;
    let processes = list_windsurf_processes();
    let pids = matching_profile_process_ids(profile, &processes);
    if pids.is_empty() {
        return Ok(0);
    }
    for pid in &pids {
        let output = Command::new("taskkill")
            .arg("/PID")
            .arg(pid.to_string())
            .arg("/T")
            .arg("/F")
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("关闭分身失败: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("taskkill failed for PID {}: {}", pid, stderr.trim());
        }
    }
    Ok(pids.len())
}

#[cfg(target_os = "macos")]
fn stop_profile_processes_sync(profile: &WindsurfProfile) -> Result<usize, String> {
    let processes = list_windsurf_processes();
    let pids = matching_profile_process_ids(profile, &processes);
    if pids.is_empty() {
        return Ok(0);
    }

    info!(
        "[Profile][macOS] Stopping Windsurf profile processes: profile_id={}, pids={:?}, user_data_dir={}",
        profile.id,
        pids,
        profile.user_data_dir.display()
    );
    for pid in &pids {
        match Command::new("kill").arg("-TERM").arg(pid.to_string()).output() {
            Err(e) => warn!("[Profile][macOS] Failed to run kill -TERM for PID {}: {}", pid, e),
            Ok(o) if !o.status.success() => warn!(
                "[Profile][macOS] kill -TERM returned non-zero for PID {}, code={:?}",
                pid,
                o.status.code()
            ),
            Ok(_) => {}
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(1200));
    let remaining = matching_profile_process_ids(profile, &list_windsurf_processes());
    if !remaining.is_empty() {
        warn!(
            "[Profile][macOS] Profile processes still alive after TERM, sending KILL: profile_id={}, pids={:?}",
            profile.id,
            remaining
        );
        for pid in &remaining {
            match Command::new("kill").arg("-KILL").arg(pid.to_string()).output() {
                Err(e) => warn!("[Profile][macOS] Failed to run kill -KILL for PID {}: {}", pid, e),
                Ok(o) if !o.status.success() => warn!(
                    "[Profile][macOS] kill -KILL returned non-zero for PID {}, code={:?}",
                    pid,
                    o.status.code()
                ),
                Ok(_) => {}
            }
        }
    }

    Ok(pids.len())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn stop_profile_processes_sync(_profile: &WindsurfProfile) -> Result<usize, String> {
    Err("当前平台暂不支持关闭分身进程".to_string())
}

/// 启动分身窗口
#[cfg(target_os = "windows")]
fn spawn_profile_window(profile: &WindsurfProfile, exe_path: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(exe_path);
    if !profile.is_main() {
        command.arg("--user-data-dir").arg(&profile.user_data_dir);
        command.arg("--new-window");
    }
    info!(
        "[Profile][Windows] Launching Windsurf: profile_id={}, name={}, exe={}, user_data_dir={}",
        profile.id,
        profile.name,
        exe_path,
        profile.user_data_dir.display()
    );
    command.creation_flags(0x08000000);
    let child = command.spawn().map_err(|e| format!("启动分身失败: {}", e))?;
    info!("[Profile][Windows] Windsurf spawn requested: profile_id={}, pid={}", profile.id, child.id());
    Ok(())
}

#[cfg(target_os = "macos")]
fn spawn_profile_window(profile: &WindsurfProfile, exe_path: &str) -> Result<(), String> {
    let mut command = Command::new(exe_path);
    if !profile.is_main() {
        command.arg("--user-data-dir").arg(&profile.user_data_dir);
        command.arg("--new-window");
    }
    info!(
        "[Profile][macOS] Launching Windsurf: profile_id={}, name={}, exe={}, user_data_dir={}, arch={}",
        profile.id,
        profile.name,
        exe_path,
        profile.user_data_dir.display(),
        std::env::consts::ARCH
    );
    let child = command.spawn().map_err(|e| format!("启动分身失败: {}", e))?;
    info!("[Profile][macOS] Windsurf spawn requested: profile_id={}, pid={}", profile.id, child.id());
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn spawn_profile_window(_profile: &WindsurfProfile, _exe_path: &str) -> Result<(), String> {
    Err("当前平台暂不支持启动分身".to_string())
}

/// 确保分身的 Local State 文件存在；不存在则直接用 DPAPI 生成一份合法 AES key，
/// 避免冷启动 Windsurf 让用户看到第一个窗口闪现。
#[cfg(target_os = "windows")]
fn ensure_profile_local_state(profile: &WindsurfProfile) -> Result<(), String> {
    prepare_profile_local_state(&profile.user_data_dir)
        .map_err(|e| format!("生成分身 Local State 失败: {}", e))
}

#[cfg(target_os = "macos")]
fn ensure_profile_local_state(_profile: &WindsurfProfile) -> Result<(), String> {
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn ensure_profile_local_state(_profile: &WindsurfProfile) -> Result<(), String> {
    Err("当前平台暂不支持分身首登 direct-write".to_string())
}

fn is_free_plan(acc: &Account) -> bool {
    acc.plan_name
        .as_ref()
        .map(|p| p.to_lowercase().contains("free"))
        .unwrap_or(true)
}

fn is_better_candidate(new_daily: i32, new_weekly: i32, new_is_free: bool, current: &Option<(Uuid, String, i32, i32, bool)>) -> bool {
    match current {
        None => true,
        Some((_, _, cur_daily, cur_weekly, cur_is_free)) => {
            if !new_is_free && *cur_is_free {
                return true;
            }
            if new_is_free && !*cur_is_free {
                return false;
            }
            if new_daily != *cur_daily {
                return new_daily > *cur_daily;
            }
            new_weekly > *cur_weekly
        }
    }
}

fn consider_candidate(
    acc: &Account,
    daily: i32,
    weekly: i32,
    threshold: i32,
    best: &mut Option<(Uuid, String, i32, i32, bool)>,
) {
    if daily > threshold && weekly > 0 {
        let acc_is_free = is_free_plan(acc);
        if is_better_candidate(daily, weekly, acc_is_free, best) {
            *best = Some((acc.id, acc.email.clone(), daily, weekly, acc_is_free));
        }
    }
}

/// 收集"已被其它分身或主实例占用"的账号 email（小写）。
/// 用于自动换号候选过滤，避免一号同时登录多个编辑器。
/// 排除规则：
/// - 主实例（若不是 exclude_profile_id）：实际登录账号 + autoSwitchCurrentAccountId 对应账号
/// - 其它分身：实际登录账号 + boundAccountId 对应账号
pub(crate) async fn accounts_in_use_by_other_profiles(
    store: &Arc<DataStore>,
    exclude_profile_id: &str,
) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    let mut emails: HashSet<String> = HashSet::new();

    if exclude_profile_id != MAIN_PROFILE_ID {
        if let Some(main_p) = main_profile() {
            if let Ok(info) = get_windsurf_info_from_dir(&main_p.user_data_dir) {
                if let Some(email) = info.email {
                    emails.insert(email.to_ascii_lowercase());
                }
            }
        }
        if let Ok(settings) = store.get_settings().await {
            if let Some(account_id) = settings.auto_switch_current_account_id.clone() {
                if let Ok(uuid) = Uuid::parse_str(&account_id) {
                    if let Ok(acc) = store.get_account(uuid).await {
                        emails.insert(acc.email.to_ascii_lowercase());
                    }
                }
            }
        }
    }

    if let Ok(profiles) = store.get_profiles().await {
        for p in profiles {
            if p.id == exclude_profile_id {
                continue;
            }
            if let Ok(info) = get_windsurf_info_from_dir(&p.user_data_dir) {
                if let Some(email) = info.email {
                    emails.insert(email.to_ascii_lowercase());
                }
            }
            if let Some(ref bid) = p.bound_account_id {
                if let Ok(uuid) = Uuid::parse_str(bid) {
                    if let Ok(acc) = store.get_account(uuid).await {
                        emails.insert(acc.email.to_ascii_lowercase());
                    }
                }
            }
        }
    }

    emails
}

async fn choose_best_candidate(
    store: &Arc<DataStore>,
    candidates: &[Account],
    threshold: i32,
) -> Option<(Uuid, String, i32, i32, bool)> {
    let mut best_candidate: Option<(Uuid, String, i32, i32, bool)> = None;

    for acc in candidates {
        consider_candidate(
            acc,
            acc.daily_quota_remaining.unwrap_or(0),
            acc.weekly_quota_remaining.unwrap_or(0),
            threshold,
            &mut best_candidate,
        );
    }

    if best_candidate.is_some() {
        return best_candidate;
    }

    let cache_ttl = chrono::Duration::minutes(3);
    let now = Utc::now();
    let windsurf_service = crate::services::windsurf_service::WindsurfService::new();

    for acc in candidates {
        if let Some(last_update) = acc.last_quota_update {
            if now - last_update < cache_ttl {
                continue;
            }
        }

        let Some(token) = acc.token.as_ref() else {
            continue;
        };

        let Ok(result) = windsurf_service.get_plan_status(token).await else {
            continue;
        };

        let Some(plan_status) = result.get("plan_status") else {
            continue;
        };

        let daily = plan_status
            .get("daily_quota_remaining")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let weekly = plan_status
            .get("weekly_quota_remaining")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;

        let mut updated = acc.clone();
        updated.daily_quota_remaining = Some(daily);
        updated.weekly_quota_remaining = Some(weekly);
        if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
            updated.daily_quota_reset = Some(v);
        }
        if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
            updated.weekly_quota_reset = Some(v);
        }
        updated.last_quota_update = Some(now);
        let _ = store.update_account(updated).await;

        consider_candidate(acc, daily, weekly, threshold, &mut best_candidate);
    }

    best_candidate
}

async fn switch_profile_to_account(
    app: &tauri::AppHandle,
    store: &Arc<DataStore>,
    profile: &WindsurfProfile,
    account_id: &str,
) -> Result<Value, String> {
    let target_id = Uuid::parse_str(account_id).map_err(|e| e.to_string())?;
    let account = store.get_account(target_id).await.map_err(|e| e.to_string())?;
    let refresh_token = account
        .refresh_token
        .clone()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "账号没有refresh_token，请先登录".to_string())?;

    info!("Switching profile '{}' to account {}", profile.name, account.email);
    let auth = match get_auth_token_for_account(store, target_id, &account.email, &refresh_token).await {
        Ok(auth) => auth,
        Err(e) => {
            error!("Failed to get auth token for profile switch: {}", e);
            return Ok(json!({
                "success": false,
                "error": format!("获取auth_token失败: {}", e)
            }));
        }
    };

    let settings = store.get_settings().await.map_err(|e| e.to_string())?;
    let machine_id_reset = if settings.reset_machine_id_on_switch {
        match reset_storage_json_for_profile(profile).await {
            Ok(()) => true,
            Err(e) => {
                warn!("Failed to reset profile storage.json: {}", e);
                false
            }
        }
    } else {
        false
    };

    let already_authenticated = is_profile_authenticated(&profile.user_data_dir);
    let was_running = is_profile_running_from_cmds(profile, &list_windsurf_process_command_lines());

    let used_direct_write = if already_authenticated && was_running {
        // 已认证且正在运行 → 走 callback URL，让正在跑的 Windsurf 实例直接接管，无需重启窗口
        if let Err(e) = trigger_windsurf_callback(app, &auth.callback_token, Some(&profile.user_data_dir)).await {
            error!("Profile callback failed: {}", e);
            return Ok(json!({
                "success": false,
                "error": format!("触发分身回调失败: {}", e)
            }));
        }
        false
    } else {
        // 首次登录 / 编辑器停留在 Sign up 页 / 未运行：直接写 state.vscdb 后重启分身
        #[cfg(target_os = "windows")]
        {
            let exe_path = find_windsurf_exe()
                .ok_or_else(|| "找不到 Windsurf.exe，请确认已安装 Windsurf".to_string())?;

            if was_running {
                if let Err(e) = stop_profile_processes_sync(profile) {
                    warn!("stop profile before direct-write failed: {}", e);
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
            }

            ensure_profile_local_state(profile)
                .map_err(|e| format!("分身初始化失败: {}", e))?;

            if let Err(e) = write_windsurf_auth_direct(
                &auth.register_result.api_key,
                &auth.register_result.name,
                &auth.register_result.api_server_url,
                &profile.user_data_dir,
            ) {
                error!("Direct-write profile auth failed: {}", e);
                return Ok(json!({
                    "success": false,
                    "error": format!("写入分身认证失败: {}", e)
                }));
            }

            spawn_profile_window(profile, &exe_path)
                .map_err(|e| format!("启动分身失败: {}", e))?;
            true
        }
        #[cfg(target_os = "macos")]
        {
            let exe_path = find_windsurf_exe()
                .ok_or_else(|| "找不到 Windsurf，请确认已安装到 /Applications 或 ~/Applications".to_string())?;

            info!(
                "[Profile][macOS] Preparing profile callback login: profile_id={}, was_running={}, already_authenticated={}, user_data_dir={}, arch={}",
                profile.id,
                was_running,
                already_authenticated,
                profile.user_data_dir.display(),
                std::env::consts::ARCH
            );

            std::fs::create_dir_all(profile.user_data_dir.join("User").join("globalStorage"))
                .map_err(|e| format!("创建分身目录失败: {}", e))?;
            ensure_profile_local_state(profile)
                .map_err(|e| format!("分身初始化失败: {}", e))?;

            if was_running {
                match stop_profile_processes_sync(profile) {
                    Ok(count) => info!(
                        "[Profile][macOS][DirectWrite] Stopped profile before writing auth: profile_id={}, count={}",
                        profile.id,
                        count
                    ),
                    Err(e) => warn!(
                        "[Profile][macOS][DirectWrite] Failed to stop profile before writing auth: profile_id={}, error={}",
                        profile.id,
                        e
                    ),
                }
            }

            match write_windsurf_auth_direct_macos(
                &auth.register_result.api_key,
                &auth.register_result.name,
                &account.email,
                &auth.register_result.api_server_url,
                &profile.user_data_dir,
            ) {
                Ok(()) => {
                    info!(
                        "[Profile][macOS][DirectWrite] Direct-write auth succeeded, launching profile once: profile_id={}, target_email={}",
                        profile.id,
                        account.email
                    );
                    spawn_profile_window(profile, &exe_path)
                        .map_err(|e| format!("启动分身失败: {}", e))?;
                    true
                }
                Err(e) => {
                    warn!(
                        "[Profile][macOS][DirectWrite] Direct-write auth failed, falling back to callback: profile_id={}, error={}",
                        profile.id,
                        e
                    );
                    if !is_profile_running_from_cmds(profile, &list_windsurf_process_command_lines()) {
                        spawn_profile_window(profile, &exe_path)
                            .map_err(|e| format!("启动分身失败: {}", e))?;
                    }
                    if let Err(e) = trigger_macos_profile_callback_with_retry(
                        app,
                        profile,
                        &auth.callback_token,
                        &account.email,
                    ).await {
                        error!("[Profile][macOS] Profile callback failed: {}", e);
                        return Ok(json!({
                            "success": false,
                            "error": e
                        }));
                    }
                    false
                }
            }
        }
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            return Ok(json!({
                "success": false,
                "error": "当前平台暂不支持分身首次登录的 direct-write 路径"
            }));
        }
    };

    // 切号已经执行（direct-write 写入文件 / callback URL 已投递给分身实例），
    // 这里的 wait 只是尝试在短时间内拿到最新的 state.vscdb 解析结果。
    // 即使等不到也不应判定为失败：分身可能仍在启动/处理 URL，前端 5s 轮询会兜底刷新。
    let verified_info = wait_for_profile_account(&profile.user_data_dir, &account.email).await;
    if verified_info.is_none() {
        if used_direct_write {
            info!(
                "[Profile] Direct-write completed but target email is not reflected yet: profile_id={}, target_email={}, user_data_dir={}, os={}, arch={}",
                profile.id,
                account.email,
                profile.user_data_dir.display(),
                std::env::consts::OS,
                std::env::consts::ARCH
            );
        } else {
            warn!(
                "[Profile] 切号后未在短时间内读到目标账号，等待编辑器异步生效: profile_id={}, target_email={}, user_data_dir={}, os={}, arch={}",
                profile.id,
                account.email,
                profile.user_data_dir.display(),
                std::env::consts::OS,
                std::env::consts::ARCH
            );
        }
    }

    let expires_at = Utc::now() + chrono::Duration::seconds(auth.expires_in.parse::<i64>().unwrap_or(3600));
    if let Some(refresh_token_new) = auth.refresh_token {
        let _ = store
            .update_account_tokens(target_id, auth.access_token, refresh_token_new, expires_at)
            .await;
    } else {
        let _ = store.update_account_token(target_id, auth.access_token, expires_at).await;
    }

    store
        .update_profile_bound_account(&profile.id, Some(account_id.to_string()), Some(account.email.clone()))
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "success": true,
        "message": if machine_id_reset { "已成功切换分身账号并重置分身机器码" } else { "已成功切换分身账号" },
        "profile_id": profile.id,
        "account_id": account_id,
        "email": account.email,
        "api_key": auth.register_result.api_key,
        "machine_id_reset": machine_id_reset,
        "verified_editor_account": verified_info.and_then(|info| info.email)
    }))
}

#[tauri::command]
pub async fn list_profiles(
    data_store: State<'_, Arc<DataStore>>,
) -> Result<Vec<ProfileRuntimeInfo>, String> {
    let store = data_store.inner().clone();
    let command_lines = list_windsurf_process_command_lines();
    let mut result = Vec::new();

    if let Some(profile) = main_profile() {
        result.push(runtime_info(profile, &command_lines));
    }

    let profiles = store.get_profiles().await.map_err(|e| e.to_string())?;
    for profile in profiles {
        result.push(runtime_info(profile, &command_lines));
    }

    Ok(result)
}

/// 递归复制目录(目标已存在则合并覆盖)
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_child = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_child)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &dst_child)?;
        }
    }
    Ok(())
}

/// 从主实例复制核心编辑器配置到新分身：
/// - `User/settings.json`、`User/keybindings.json`、`User/locale.json`
/// - `User/snippets/` 整目录
/// 不复制：
/// - `User/globalStorage`（含 state.vscdb 账号、storage.json 机器码，必须独立）
/// - `User/workspaceStorage`、`User/History`（项目状态、历史记录，独立更合理）
/// - 扩展目录（已通过 `~/.windsurf/extensions` 全局共享）
fn copy_main_user_settings_to_profile(profile_user_data_dir: &Path) {
    let main_dir = match main_user_data_dir() {
        Some(d) => d,
        None => return,
    };
    let main_user = main_dir.join("User");
    if !main_user.exists() {
        info!(
            "[Profile] 主实例 User 目录不存在,跳过配置复制: main_user={}, target_profile_dir={}",
            main_user.display(),
            profile_user_data_dir.display()
        );
        return;
    }

    let target_user = profile_user_data_dir.join("User");
    if let Err(e) = std::fs::create_dir_all(&target_user) {
        warn!(
            "[Profile] 创建分身 User 目录失败: target_user={}, error={}",
            target_user.display(),
            e
        );
        return;
    }

    let mut copied: Vec<&str> = Vec::new();
    for filename in &["settings.json", "keybindings.json", "locale.json"] {
        let src = main_user.join(filename);
        if !src.exists() {
            continue;
        }
        let dst = target_user.join(filename);
        match std::fs::copy(&src, &dst) {
            Ok(_) => copied.push(filename),
            Err(e) => warn!(
                "[Profile] 复制主实例配置失败: file={}, src={}, dst={}, error={}",
                filename,
                src.display(),
                dst.display(),
                e
            ),
        }
    }

    let src_snippets = main_user.join("snippets");
    if src_snippets.exists() && src_snippets.is_dir() {
        let dst_snippets = target_user.join("snippets");
        match copy_dir_recursive(&src_snippets, &dst_snippets) {
            Ok(_) => copied.push("snippets/"),
            Err(e) => warn!(
                "[Profile] 复制 snippets 目录失败: src={}, dst={}, error={}",
                src_snippets.display(),
                dst_snippets.display(),
                e
            ),
        }
    }

    if copied.is_empty() {
        info!(
            "[Profile] 主实例无可复制配置,新分身使用 Windsurf 默认设置: main_user={}, target_user={}",
            main_user.display(),
            target_user.display()
        );
    } else {
        info!(
            "[Profile] 已从主实例复制配置到新分身: copied={:?}, main_user={}, target_user={}",
            copied,
            main_user.display(),
            target_user.display()
        );
    }
}

#[tauri::command]
pub async fn create_profile(
    name: String,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<ProfileRuntimeInfo, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("分身名称不能为空".to_string());
    }

    let id = Uuid::new_v4().to_string();
    let user_data_dir = profiles_root_dir()
        .map_err(|e| e.to_string())?
        .join(&id);
    info!(
        "[Profile] Creating profile: id={}, name={}, user_data_dir={}, os={}, arch={}",
        id,
        trimmed,
        user_data_dir.display(),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    std::fs::create_dir_all(user_data_dir.join("User").join("globalStorage"))
        .map_err(|e| format!("创建分身目录失败: {}", e))?;

    // 从主实例复制 settings/keybindings/locale/snippets,让新分身继承用户的编辑器偏好
    // (主题、禁用更新、Codeium 选项、快捷键、代码片段等)。
    // 失败不阻塞分身创建,仅 warn。
    copy_main_user_settings_to_profile(&user_data_dir);

    let profile = WindsurfProfile::new(id, trimmed.to_string(), user_data_dir);
    data_store
        .add_profile(profile.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(runtime_info(profile, &list_windsurf_process_command_lines()))
}

#[tauri::command]
pub async fn rename_profile(
    profile_id: String,
    name: String,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<ProfileRuntimeInfo, String> {
    if profile_id == MAIN_PROFILE_ID {
        return Err("主实例不能重命名".to_string());
    }
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("分身名称不能为空".to_string());
    }

    let store = data_store.inner().clone();
    let mut profile = store.get_profile(&profile_id).await.map_err(|e| e.to_string())?;
    profile.name = trimmed.to_string();
    store.update_profile(profile.clone()).await.map_err(|e| e.to_string())?;

    Ok(runtime_info(profile, &list_windsurf_process_command_lines()))
}

#[tauri::command]
pub async fn delete_profile(
    profile_id: String,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<Value, String> {
    if profile_id == MAIN_PROFILE_ID {
        return Err("主实例不能删除".to_string());
    }

    let store = data_store.inner().clone();
    let profile = store.get_profile(&profile_id).await.map_err(|e| e.to_string())?;
    let command_lines = list_windsurf_process_command_lines();
    if is_profile_running_from_cmds(&profile, &command_lines) {
        return Ok(json!({
            "success": false,
            "message": "分身正在运行，请先关闭对应 Windsurf 窗口"
        }));
    }

    store.delete_profile(&profile_id).await.map_err(|e| e.to_string())?;
    if profile.user_data_dir.exists() {
        std::fs::remove_dir_all(&profile.user_data_dir)
            .map_err(|e| format!("删除分身目录失败: {}", e))?;
    }

    Ok(json!({ "success": true }))
}

#[tauri::command]
pub async fn is_profile_running(
    profile_id: String,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<bool, String> {
    let store = data_store.inner().clone();
    let profile = resolve_profile(&store, &profile_id).await.map_err(|e| e.to_string())?;
    Ok(is_profile_running_from_cmds(
        &profile,
        &list_windsurf_process_command_lines(),
    ))
}

#[tauri::command]
pub async fn launch_profile(
    profile_id: String,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<Value, String> {
    let store = data_store.inner().clone();
    let profile = resolve_profile(&store, &profile_id).await.map_err(|e| e.to_string())?;
    let command_lines = list_windsurf_process_command_lines();
    if is_profile_running_from_cmds(&profile, &command_lines) {
        return Ok(json!({
            "success": true,
            "alreadyRunning": true,
            "profileId": profile.id
        }));
    }

    let exe_path = find_windsurf_exe()
        .ok_or_else(|| "找不到 Windsurf，请确认已安装 Windsurf".to_string())?;

    if !profile.is_main() {
        std::fs::create_dir_all(profile.user_data_dir.join("User").join("globalStorage"))
            .map_err(|e| format!("创建分身目录失败: {}", e))?;
    }

    spawn_profile_window(&profile, &exe_path)
        .map_err(|e| format!("启动 Windsurf 失败: {}", e))?;

    Ok(json!({
        "success": true,
        "alreadyRunning": false,
        "profileId": profile.id
    }))
}

#[tauri::command]
pub async fn stop_profile(
    profile_id: String,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<Value, String> {
    let store = data_store.inner().clone();
    let profile = resolve_profile(&store, &profile_id).await.map_err(|e| e.to_string())?;
    let processes = list_windsurf_processes();
    let pids = matching_profile_process_ids(&profile, &processes);

    if pids.is_empty() {
        return Ok(json!({
            "success": true,
            "alreadyStopped": true,
            "profileId": profile.id,
            "stopped": 0
        }));
    }

    let _ = stop_profile_processes_sync(&profile)?;

    tokio::time::sleep(tokio::time::Duration::from_millis(700)).await;

    // 兜底：重新扫描进程列表，确认是否真的关掉了
    let remaining = matching_profile_process_ids(&profile, &list_windsurf_processes());
    #[cfg(target_os = "macos")]
    if !remaining.is_empty() {
        warn!(
            "[Profile][macOS] Processes still alive after TERM, trying KILL: profile_id={}, remaining={:?}",
            profile.id,
            remaining
        );
        for pid in &remaining {
            match Command::new("kill").arg("-KILL").arg(pid.to_string()).output() {
                Err(e) => warn!("[Profile][macOS] Failed to run kill -KILL for PID {}: {}", pid, e),
                Ok(o) if !o.status.success() => warn!(
                    "[Profile][macOS] kill -KILL returned non-zero for PID {}, code={:?}",
                    pid,
                    o.status.code()
                ),
                Ok(_) => {}
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    let remaining = matching_profile_process_ids(&profile, &list_windsurf_processes());
    if !remaining.is_empty() {
        return Err(format!(
            "仍有 {} 个 Windsurf 进程未关闭 (PID: {:?})，请手动从系统进程管理器结束",
            remaining.len(),
            remaining
        ));
    }

    Ok(json!({
        "success": true,
        "alreadyStopped": false,
        "profileId": profile.id,
        "stopped": pids.len()
    }))
}

#[tauri::command]
pub async fn get_profile_current_info(
    profile_id: String,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<WindsurfCurrentInfo, String> {
    let store = data_store.inner().clone();
    let profile = resolve_profile(&store, &profile_id).await.map_err(|e| e.to_string())?;
    get_windsurf_info_from_dir(&profile.user_data_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn bind_account_to_profile(
    profile_id: String,
    account_id: Option<String>,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<WindsurfProfile, String> {
    if profile_id == MAIN_PROFILE_ID {
        return Err("主实例不支持绑定记录，请使用原切号入口".to_string());
    }

    let store = data_store.inner().clone();
    let email = match account_id.as_deref() {
        Some(id) if !id.is_empty() => {
            let uuid = Uuid::parse_str(id).map_err(|e| e.to_string())?;
            Some(store.get_account(uuid).await.map_err(|e| e.to_string())?.email)
        }
        _ => None,
    };

    store
        .update_profile_bound_account(&profile_id, account_id, email)
        .await
        .map_err(|e| e.to_string())?;
    store.get_profile(&profile_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_profile_auto_switch_config(
    profile_id: String,
    enabled: bool,
    group: String,
    threshold: i32,
    check_interval: Option<i32>,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<WindsurfProfile, String> {
    if profile_id == MAIN_PROFILE_ID {
        return Err("主实例自动换号配置请使用分身管理页主实例卡片".to_string());
    }

    let threshold = threshold.clamp(0, 100);
    let check_interval = check_interval.unwrap_or(300).clamp(10, 86_400);
    let cfg = ProfileAutoSwitch {
        enabled,
        group: if group.trim().is_empty() { "默认分组".to_string() } else { group.trim().to_string() },
        threshold,
        check_interval,
    };

    let store = data_store.inner().clone();
    store
        .update_profile_auto_switch(&profile_id, cfg)
        .await
        .map_err(|e| e.to_string())?;
    store.get_profile(&profile_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn switch_account_in_profile(
    app: tauri::AppHandle,
    profile_id: String,
    account_id: String,
    data_store: State<'_, Arc<DataStore>>,
    machine_id_store: State<'_, Arc<crate::commands::machine_id_commands::MachineIdStore>>,
) -> Result<Value, String> {
    if profile_id == MAIN_PROFILE_ID {
        return crate::commands::switch_account_commands::switch_account(
            app,
            account_id,
            data_store,
            machine_id_store,
        )
        .await;
    }

    let store = data_store.inner().clone();
    let profile = store.get_profile(&profile_id).await.map_err(|e| e.to_string())?;
    switch_profile_to_account(&app, &store, &profile, &account_id).await
}

#[tauri::command]
pub async fn check_profile_auto_switch(
    app: tauri::AppHandle,
    profile_id: String,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<Value, String> {
    if profile_id == MAIN_PROFILE_ID {
        return Ok(json!({
            "action": "skip",
            "reason": "主实例请使用 check_auto_switch"
        }));
    }

    let store = data_store.inner().clone();
    let profile = store.get_profile(&profile_id).await.map_err(|e| e.to_string())?;
    if !profile.auto_switch.enabled {
        return Ok(json!({
            "action": "skip",
            "reason": "分身自动换号未启用",
            "profile_id": profile_id
        }));
    }

    let settings = store.get_settings().await.map_err(|e| e.to_string())?;
    if !settings.seamless_switch_enabled {
        return Ok(json!({
            "action": "skip",
            "reason": "无感换号未启用",
            "profile_id": profile_id
        }));
    }

    let group = profile.auto_switch.group.clone();
    let threshold = profile.auto_switch.threshold;
    let windsurf_info = get_windsurf_info_from_dir(&profile.user_data_dir)
        .map_err(|e| format!("读取分身编辑器状态失败: {}", e))?;
    let editor_email = windsurf_info.email.clone();
    let all_accounts = store.get_all_accounts().await.map_err(|e| e.to_string())?;

    let current_account = editor_email
        .as_ref()
        .and_then(|email| {
            all_accounts
                .iter()
                .find(|a| a.email.eq_ignore_ascii_case(email) && a.group.as_deref() == Some(group.as_str()))
                .or_else(|| all_accounts.iter().find(|a| a.email.eq_ignore_ascii_case(email)))
                .cloned()
        });

    let current_account = match current_account {
        Some(account) => account,
        None => {
            let in_use = accounts_in_use_by_other_profiles(&store, &profile_id).await;
            let candidates: Vec<Account> = all_accounts
                .into_iter()
                .filter(|a| {
                    a.group.as_deref() == Some(group.as_str())
                        && !matches!(a.status, AccountStatus::Error(_))
                        && a.refresh_token.as_ref().map(|t| !t.is_empty()).unwrap_or(false)
                        && !in_use.contains(&a.email.to_ascii_lowercase())
                })
                .collect();

            if candidates.is_empty() {
                return Ok(json!({
                    "action": "no_candidate",
                    "reason": format!("分组 '{}' 中没有可用账号", group),
                    "profile_id": profile_id,
                    "editor_email": editor_email
                }));
            }

            let Some((target_id, target_email, target_daily, target_weekly, _)) = choose_best_candidate(&store, &candidates, threshold).await else {
                return Ok(json!({
                    "action": "no_candidate",
                    "reason": format!("分组 '{}' 中没有配额充足的账号", group),
                    "profile_id": profile_id,
                    "editor_email": editor_email
                }));
            };

            let switch_result = switch_profile_to_account(&app, &store, &profile, &target_id.to_string()).await?;
            if switch_result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                return Ok(json!({
                    "action": "switched",
                    "reason": "分身编辑器无已识别账号，首次自动切号",
                    "profile_id": profile_id,
                    "from_account": editor_email,
                    "to_account": target_email,
                    "to_account_id": target_id.to_string(),
                    "to_daily_remaining": target_daily,
                    "to_weekly_remaining": target_weekly,
                    "switch_result": switch_result
                }));
            }

            return Ok(json!({
                "action": "error",
                "reason": switch_result.get("error").and_then(|v| v.as_str()).unwrap_or("分身切号失败"),
                "profile_id": profile_id,
                "switch_result": switch_result
            }));
        }
    };

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
                let mut updated = current_account.clone();
                updated.daily_quota_remaining = Some(current_daily_remaining);
                updated.weekly_quota_remaining = Some(current_weekly_remaining);
                if let Some(v) = plan_status.get("daily_quota_reset").and_then(|v| v.as_i64()) {
                    updated.daily_quota_reset = Some(v);
                }
                if let Some(v) = plan_status.get("weekly_quota_reset").and_then(|v| v.as_i64()) {
                    updated.weekly_quota_reset = Some(v);
                }
                updated.last_quota_update = Some(Utc::now());
                let _ = store.update_account(updated).await;
            }
        }
    }

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
            "profile_id": profile_id,
            "current_account": current_account.email,
            "daily_remaining": current_daily_remaining,
            "weekly_remaining": current_weekly_remaining
        }));
    }

    let all_accounts = store.get_all_accounts().await.map_err(|e| e.to_string())?;
    let in_use = accounts_in_use_by_other_profiles(&store, &profile_id).await;
    let candidates: Vec<Account> = all_accounts
        .into_iter()
        .filter(|a| {
            a.group.as_deref() == Some(group.as_str())
                && a.id != current_account.id
                && !matches!(a.status, AccountStatus::Error(_))
                && a.refresh_token.as_ref().map(|t| !t.is_empty()).unwrap_or(false)
                && !in_use.contains(&a.email.to_ascii_lowercase())
        })
        .collect();

    if candidates.is_empty() {
        return Ok(json!({
            "action": "no_candidate",
            "reason": format!("分组 '{}' 中没有其他可用账号", group),
            "profile_id": profile_id,
            "current_account": current_account.email,
            "daily_remaining": current_daily_remaining,
            "weekly_remaining": current_weekly_remaining
        }));
    }

    let Some((target_id, target_email, target_daily, target_weekly, _)) = choose_best_candidate(&store, &candidates, threshold).await else {
        return Ok(json!({
            "action": "no_candidate",
            "reason": format!("分组 '{}' 中没有配额充足的账号 (需日配额>{}% 且 周配额>0%)", group, threshold),
            "profile_id": profile_id,
            "current_account": current_account.email,
            "daily_remaining": current_daily_remaining,
            "weekly_remaining": current_weekly_remaining
        }));
    };

    let switch_result = switch_profile_to_account(&app, &store, &profile, &target_id.to_string()).await?;
    if !switch_result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Ok(json!({
            "action": "error",
            "reason": switch_result.get("error").and_then(|v| v.as_str()).unwrap_or("分身切号失败"),
            "profile_id": profile_id,
            "switch_result": switch_result
        }));
    }

    Ok(json!({
        "action": "switched",
        "reason": switch_reason,
        "profile_id": profile_id,
        "from_account": current_account.email,
        "from_daily_remaining": current_daily_remaining,
        "from_weekly_remaining": current_weekly_remaining,
        "to_account": target_email,
        "to_account_id": target_id.to_string(),
        "to_daily_remaining": target_daily,
        "to_weekly_remaining": target_weekly,
        "switch_result": switch_result
    }))
}
