use tauri::command;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use regex::bytes::Regex;
use chrono::Local;
use std::sync::Arc;
use tauri::State;
use crate::repository::DataStore;
use crate::commands::auto_continue_commands::AUTO_CONTINUE_BRIDGE_PORT;
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};

const SEAMLESS_PATCH_MARKER: &[u8] = b"WindsurfAccountManagerSeamlessOAuthPatchV2";
const SEAMLESS_OAUTH_ERROR_MARKER: &[u8] = b"Failed to handle OAuth callback";
const SEAMLESS_ACCOUNT_MANAGER_ERROR_MARKER: &[u8] = b"Failed to handle account-manager OAuth callback";
const AUTO_CONTINUE_WORKBENCH_MARKER: &[u8] = b"WindsurfAccountManagerAutoContinueBridge";
const AUTO_CONTINUE_LEGACY_SENDER_MARKER: &[u8] = b"WindsurfAccountManagerAutoContinueSenderBridge";
const AUTO_CONTINUE_EXTENSION_MARKER: &[u8] = b"WindsurfAccountManagerAutoContinueExtensionBridgeV2";

/// 获取 extension.js 相对路径（跨平台）
fn get_extension_js_relative_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        // macOS: Windsurf.app/Contents/Resources/app/extensions/windsurf/dist/extension.js
        PathBuf::from("Contents")
            .join("Resources")
            .join("app")
            .join("extensions")
            .join("windsurf")
            .join("dist")
            .join("extension.js")
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows/Linux: resources/app/extensions/windsurf/dist/extension.js
        PathBuf::from("resources")
            .join("app")
            .join("extensions")
            .join("windsurf")
            .join("dist")
            .join("extension.js")
    }
}

fn get_workbench_js_relative_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("Contents")
            .join("Resources")
            .join("app")
            .join("out")
            .join("vs")
            .join("workbench")
            .join("workbench.desktop.main.js")
    }
    #[cfg(not(target_os = "macos"))]
    {
        PathBuf::from("resources")
            .join("app")
            .join("out")
            .join("vs")
            .join("workbench")
            .join("workbench.desktop.main.js")
    }
}

fn get_product_json_relative_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("Contents")
            .join("Resources")
            .join("app")
            .join("product.json")
    }
    #[cfg(not(target_os = "macos"))]
    {
        PathBuf::from("resources").join("app").join("product.json")
    }
}

/// 获取Windsurf的安装路径
#[command]
pub async fn get_windsurf_path() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        // Windows: 首先尝试从开始菜单快捷方式获取
        let start_menu_path = std::env::var("APPDATA")
            .map(|p| PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs\\Windsurf"))
            .ok();
        
        if let Some(start_menu) = start_menu_path {
            if let Ok(entries) = fs::read_dir(&start_menu) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("lnk") {
                        if let Ok(target) = resolve_shortcut(&path) {
                            if let Some(parent) = target.parent() {
                                let windsurf_root = parent.to_path_buf();
                                let extension_file = windsurf_root.join(get_extension_js_relative_path());
                                
                                if extension_file.exists() {
                                    return Ok(windsurf_root.to_string_lossy().to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Windows: 尝试常见的安装位置
        let possible_locations = vec![
            std::env::var("LOCALAPPDATA").ok().map(|p| PathBuf::from(p).join("Programs\\Windsurf")),
            Some(PathBuf::from("C:\\Program Files\\Windsurf")),
            Some(PathBuf::from("C:\\Program Files (x86)\\Windsurf")),
            Some(PathBuf::from("D:\\Program\\Windsurf")),
        ];
        
        for location in possible_locations.into_iter().flatten() {
            let extension_file = location.join(get_extension_js_relative_path());
            if extension_file.exists() {
                return Ok(location.to_string_lossy().to_string());
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // macOS: 检查 /Applications/Windsurf.app
        let possible_locations = vec![
            PathBuf::from("/Applications/Windsurf.app"),
            std::env::var("HOME").ok().map(|h| PathBuf::from(h).join("Applications/Windsurf.app")).unwrap_or_default(),
        ];
        
        for location in possible_locations {
            let extension_file = location.join(get_extension_js_relative_path());
            if extension_file.exists() {
                return Ok(location.to_string_lossy().to_string());
            }
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        // Linux: 检查常见安装位置
        let possible_locations = vec![
            PathBuf::from("/opt/Windsurf"),
            PathBuf::from("/usr/share/windsurf"),
            std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".local/share/Windsurf")).unwrap_or_default(),
        ];
        
        for location in possible_locations {
            let extension_file = location.join(get_extension_js_relative_path());
            if extension_file.exists() {
                return Ok(location.to_string_lossy().to_string());
            }
        }
    }
    
    Err("未找到Windsurf安装路径".to_string())
}

/// 解析Windows快捷方式
#[cfg(target_os = "windows")]
fn resolve_shortcut(lnk_path: &Path) -> Result<PathBuf, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    
    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(&[
            "-NoProfile",
            "-Command",
            &format!(
                "$sh = New-Object -ComObject WScript.Shell; $sh.CreateShortcut('{}').TargetPath",
                lnk_path.display()
            )
        ])
        .output()
        .map_err(|e| e.to_string())?;
    
    if output.status.success() {
        let target = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();
        
        if !target.is_empty() {
            Ok(PathBuf::from(target))
        } else {
            Err("快捷方式目标为空".to_string())
        }
    } else {
        Err("解析快捷方式失败".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn resolve_shortcut(_lnk_path: &Path) -> Result<PathBuf, String> {
    Err("不支持的操作系统".to_string())
}

/// 应用无感换号补丁
///
/// 参数：
/// - `windsurf_path`：Windsurf 安装目录
/// - `force`：是否强制重新打补丁。为 true 时会先尝试用最干净的备份还原 extension.js，再重新应用补丁，
///   用于覆盖已损坏/旧版本补丁的场景。
#[command]
pub async fn apply_seamless_patch(
    windsurf_path: String,
    force: Option<bool>,
    data_store: State<'_, Arc<DataStore>>,
) -> Result<serde_json::Value, String> {
    let extension_file = PathBuf::from(&windsurf_path).join(get_extension_js_relative_path());
    
    if !extension_file.exists() {
        return Err(format!("extension.js 文件不存在: {:?}", extension_file));
    }
    
    let force = force.unwrap_or(false);
    let mut restored_from_backup: Option<String> = None;

    // force 模式：先用最干净的备份覆盖当前文件，再走正常的打补丁流程
    if force && is_file_patched(&extension_file) {
        let extension_dir = extension_file.parent()
            .ok_or("无法获取扩展目录")?
            .to_path_buf();
        let saved_backup = data_store
            .get_settings()
            .await
            .map_err(|e| e.to_string())?
            .patch_backup_path
            .clone();
        let backup_path = find_latest_backup(&extension_dir, &saved_backup)?;
        fs::copy(&backup_path, &extension_file)
            .map_err(|e| format!("还原备份失败: {} (备份文件: {:?})", e, backup_path))?;
        restored_from_backup = Some(backup_path.to_string_lossy().to_string());
    }
    
    // 1. 先读取文件内容，检查是否已打补丁
    //    注意：必须按字节读取，extension.js 是大型 webpack bundle，
    //    个别 Windsurf 版本 / 用户机器上文件中可能含有非 UTF-8 字节
    //    （比如被其他工具改写过、自动更新被截断等）。
    //    用 fs::read_to_string 会立即报 "stream did not contain valid UTF-8" 而失败。
    let content: Vec<u8> = fs::read(&extension_file)
        .map_err(|e| format!("读取文件失败: {}", e))?;
    
    let mut modified_content: Vec<u8> = content.clone();
    let mut modifications = vec![];
    
    // 2. 应用修改1: 添加全局 OAuth 回调处理器
    // 新版 Windsurf（三元表达式 + maybeHandleUriWithToken 分支）：
    //   this._uriHandler.event(A=>{"/refresh-authentication-session"===A.path?(0,m.refreshAuthenticationSession)():this._loginInProgress||this.maybeHandleUriWithToken(A)})
    let pattern1_new_str = r#"this\._uriHandler\.event\((\w+)=>\{"/refresh-authentication-session"===(\w+)\.path\?\(0,(\w+)\.refreshAuthenticationSession\)\(\):this\._loginInProgress\|\|this\.maybeHandleUriWithToken\((\w+)\)\}\)"#;
    // 旧版 Windsurf（&& 短路写法）：
    //   this._uriHandler.event(A=>{"/refresh-authentication-session"===A.path&&(0,m.refreshAuthenticationSession)()})
    let pattern1_old_str = r#"this\._uriHandler\.event\((\w+)=>\{"/refresh-authentication-session"===(\w+)\.path&&\(0,(\w+)\.refreshAuthenticationSession\)\(\)\}\)"#;

    let pattern1_new = Regex::new(pattern1_new_str)
        .map_err(|e| format!("正则表达式错误(新): {}", e))?;
    let pattern1_old = Regex::new(pattern1_old_str)
        .map_err(|e| format!("正则表达式错误(旧): {}", e))?;

    // 先尝试新版格式，再回退到旧版
    let pattern1_match = pattern1_new
        .captures(&modified_content)
        .map(|c| ("new", c))
        .or_else(|| pattern1_old.captures(&modified_content).map(|c| ("old", c)));

    if let Some((variant, captures)) = pattern1_match {
        // 注意：变量名按 \w+ 捕获，必然是 ASCII（合法 JS 标识符），
        // 因此从字节切片转 str 一定成功，这里用 from_utf8 严格转换更安全。
        let var_name1 = captures.get(1)
            .and_then(|m| std::str::from_utf8(m.as_bytes()).ok())
            .unwrap_or("")
            .to_string();
        let var_name2 = captures.get(2)
            .and_then(|m| std::str::from_utf8(m.as_bytes()).ok())
            .unwrap_or("")
            .to_string();
        let module_name = captures.get(3)
            .and_then(|m| std::str::from_utf8(m.as_bytes()).ok())
            .unwrap_or("")
            .to_string();
        // 新版有第4个捕获组（maybeHandleUriWithToken 的参数名），旧版无
        let var_name4 = captures.get(4)
            .and_then(|m| std::str::from_utf8(m.as_bytes()).ok())
            .map(|s| s.to_string());

        // 所有捕获到的变量名必须一致，避免误替换
        let vars_consistent = var_name1 == var_name2
            && var_name4.as_deref().map(|v| v == var_name1).unwrap_or(true);

        if vars_consistent && !var_name1.is_empty() && !module_name.is_empty() {
            let replacement = format!(
                r#"this._uriHandler.event(async {}=>{{if("/refresh-authentication-session"==={}.path){{(0,{}.refreshAuthenticationSession)()}}else{{try{{const t=new URLSearchParams({}.fragment).get("access_token");if(null===t)throw new Error("No token");await this.handleAuthToken(t)}}catch(e){{console.error("[Windsurf] Failed to handle OAuth callback:",e)}}}}}})"#,
                var_name1, var_name1, module_name, var_name1
            );

            // 字节级替换：取整段匹配的字节切片，构造新的 Vec<u8>
            let full_match: Vec<u8> = captures.get(0).unwrap().as_bytes().to_vec();
            modified_content = replace_bytes(&modified_content, &full_match, replacement.as_bytes());
            modifications.push(if variant == "new" {
                "OAuth回调处理器(新版格式)"
            } else {
                "OAuth回调处理器"
            });
        }
    }
    
    // 3. 应用修改2: 移除180秒超时限制
    let pattern2_str = r#",new Promise\(\((\w+),(\w+)\)=>setTimeout\(\(\)=>\{(\w+)\(new (\w+)\)\},18e4\)\)"#;
    let pattern2 = Regex::new(pattern2_str)
        .map_err(|e| format!("正则表达式错误2: {}", e))?;
    
    if let Some(captures) = pattern2.captures(&modified_content) {
        // 第二个参数 vs setTimeout 中的变量，都是 ASCII 标识符
        let reject_var1 = captures[2].to_vec();
        let reject_var2 = captures[3].to_vec();
        
        // 检查是否是同一个reject变量
        if reject_var1 == reject_var2 {
            let full_match: Vec<u8> = captures.get(0).unwrap().as_bytes().to_vec();
            modified_content = replace_bytes(&modified_content, &full_match, b"");
            modifications.push("移除超时限制");
        }
    }
    
    // 4. 验证是否需要修改
    // 如果内容没变化，要进一步区分两种情况：
    //   a) 文件确实已经打过补丁（包含补丁特征 "Failed to handle OAuth callback"）
    //   b) 正则表达式未能匹配当前 Windsurf 版本（常见于首次安装最新版 Windsurf 的新用户，
    //      之前这里被错误地当作 "已打过补丁" 从而陷入死循环）
    if modified_content == content {
        if has_any_seamless_patch(&content) {
            return Ok(serde_json::json!({
                "success": true,
                "already_patched": true,
                "seamless_patch_marker": has_current_seamless_patch(&content),
                "message": "补丁已经应用过了"
            }));
        } else {
            return Err(
                "补丁规则未能匹配当前 Windsurf 版本的 extension.js（首次使用/Windsurf 升级后常见）。\
                请确认 Windsurf 版本，或点击\"重新打补丁\"按钮尝试从备份还原后再应用。"
                    .to_string(),
            );
        }
    }
    
    // 5. 确认需要打补丁后，才管理和创建备份文件
    let parent_dir = extension_file.parent()
        .ok_or("无法获取父目录")?;
    
    // 查找所有现有备份文件
    let mut backup_files: Vec<PathBuf> = fs::read_dir(parent_dir)
        .map_err(|e| format!("读取目录失败: {}", e))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("extension.js.backup."))
                .unwrap_or(false)
        })
        .collect();
    
    // 按修改时间排序（最早的在前）
    backup_files.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    });
    
    // 如果备份文件数量达到3个或更多，删除最早的备份
    while backup_files.len() >= 3 {
        if let Some(oldest) = backup_files.first() {
            fs::remove_file(oldest)
                .map_err(|e| format!("删除旧备份失败: {}", e))?;
            println!("删除旧备份文件: {:?}", oldest);
            backup_files.remove(0);
        } else {
            break;
        }
    }
    
    // 创建新的备份文件（此时 extension_file 是原始未打补丁的文件）
    let backup_file = extension_file.with_extension(&format!(
        "js.backup.{}",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    
    fs::copy(&extension_file, &backup_file)
        .map_err(|e| format!("备份失败: {}", e))?;
    
    // 6. 写入修改后的文件
    fs::write(&extension_file, &modified_content)
        .map_err(|e| format!("写入文件失败: {}", e))?;
    
    // 7. 保存补丁状态到设置
    let mut settings = data_store.get_settings().await.map_err(|e| e.to_string())?;
    settings.seamless_switch_enabled = true;
    settings.windsurf_path = Some(windsurf_path.clone());
    settings.patch_backup_path = Some(backup_file.to_string_lossy().to_string());
    data_store.update_settings(settings).await.map_err(|e| e.to_string())?;
    
    // 8. 重启Windsurf
    restart_windsurf(Some(&windsurf_path)).await?;
    
    Ok(serde_json::json!({
        "success": true,
        "modifications": modifications,
        "backup_file": backup_file.to_string_lossy().to_string(),
        "restored_from_backup": restored_from_backup,
        "forced": force,
        "message": if force {
            "补丁已重新应用，Windsurf正在重启"
        } else {
            "补丁应用成功，Windsurf正在重启"
        }
    }))
}

/// 还原无感换号补丁
#[command]
pub async fn restore_seamless_patch(
    data_store: State<'_, Arc<DataStore>>,
) -> Result<serde_json::Value, String> {
    let settings = data_store.get_settings().await.map_err(|e| e.to_string())?;
    
    let windsurf_path = settings.windsurf_path
        .ok_or_else(|| "未找到Windsurf路径".to_string())?;
    
    let extension_file = PathBuf::from(&windsurf_path).join(get_extension_js_relative_path());
    let extension_dir = extension_file.parent()
        .ok_or("无法获取扩展目录")?
        .to_path_buf();
    
    // 尝试找到可用的备份文件
    let backup_path = find_latest_backup(&extension_dir, &settings.patch_backup_path)?;
    
    println!("使用备份文件还原: {:?}", backup_path);
    
    // 还原备份文件
    fs::copy(&backup_path, &extension_file)
        .map_err(|e| format!("还原失败: {} (备份文件: {:?})", e, backup_path))?;
    
    // 更新设置
    let mut settings = data_store.get_settings().await.map_err(|e| e.to_string())?;
    settings.seamless_switch_enabled = false;
    data_store.update_settings(settings).await.map_err(|e| e.to_string())?;
    
    // 重启Windsurf
    restart_windsurf(Some(&windsurf_path)).await?;
    
    Ok(serde_json::json!({
        "success": true,
        "message": "补丁已还原，Windsurf正在重启",
        "backup_used": backup_path.to_string_lossy().to_string()
    }))
}

/// 字节级 contains：在 haystack 中查找 needle 子序列
fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn has_any_seamless_patch(content: &[u8]) -> bool {
    bytes_contains(content, SEAMLESS_PATCH_MARKER)
        || bytes_contains(content, SEAMLESS_OAUTH_ERROR_MARKER)
        || bytes_contains(content, SEAMLESS_ACCOUNT_MANAGER_ERROR_MARKER)
}

fn has_current_seamless_patch(content: &[u8]) -> bool {
    bytes_contains(content, SEAMLESS_PATCH_MARKER)
}

/// 字节级 replace：把 haystack 中第一次出现的 needle 替换为 replacement
fn replace_bytes(haystack: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return haystack.to_vec();
    }
    if let Some(pos) = haystack
        .windows(needle.len())
        .position(|w| w == needle)
    {
        let mut out = Vec::with_capacity(haystack.len() - needle.len() + replacement.len());
        out.extend_from_slice(&haystack[..pos]);
        out.extend_from_slice(replacement);
        out.extend_from_slice(&haystack[pos + needle.len()..]);
        out
    } else {
        haystack.to_vec()
    }
}

/// 检查文件是否包含补丁特征（是否已打过补丁）
fn is_file_patched(file_path: &Path) -> bool {
    // 按字节读取，避免 UTF-8 校验失败导致这里直接判定为"未打补丁"，
    // 进而错误地把一个其实已经打过补丁的文件当成"干净的备份"返回。
    if let Ok(content) = fs::read(file_path) {
        has_any_seamless_patch(&content)
    } else {
        false
    }
}

/// 给定 Windsurf 安装根目录，检测无感补丁是否已生效。
/// 返回 None 表示路径无效或 extension.js 不存在；Some(true/false) 为是否已打补丁。
pub fn is_seamless_patch_active(windsurf_path: &str) -> Option<bool> {
    let extension_file = PathBuf::from(windsurf_path).join(get_extension_js_relative_path());
    if !extension_file.exists() {
        return None;
    }
    Some(is_file_patched(&extension_file))
}

fn rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).rposition(|window| window == needle)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn truncate_appended_auto_continue_block(content: &[u8], marker: &[u8]) -> Option<Vec<u8>> {
    let marker_pos = find_bytes(content, marker)?;
    let before_marker = &content[..marker_pos];
    let script_start = rfind_bytes(before_marker, b"\n;(() => {")
        .or_else(|| rfind_bytes(before_marker, b"\r\n;(() => {"))
        .or_else(|| rfind_bytes(before_marker, b";(() => {"))?;
    let mut stripped = content[..script_start].to_vec();
    while matches!(stripped.last(), Some(b'\n' | b'\r' | b' ' | b'\t')) {
        stripped.pop();
    }
    stripped.extend_from_slice(b"\n");
    Some(stripped)
}

fn strip_appended_auto_continue_extension_blocks(content: &[u8]) -> Vec<u8> {
    let mut stripped = content.to_vec();
    loop {
        if let Some(next) = truncate_appended_auto_continue_block(&stripped, AUTO_CONTINUE_EXTENSION_MARKER) {
            stripped = next;
            continue;
        }
        if let Some(next) = truncate_appended_auto_continue_block(&stripped, AUTO_CONTINUE_LEGACY_SENDER_MARKER) {
            stripped = next;
            continue;
        }
        break;
    }
    stripped
}

fn strip_appended_auto_continue_workbench_blocks(content: &[u8]) -> Vec<u8> {
    let mut stripped = content.to_vec();
    loop {
        if let Some(next) = truncate_appended_auto_continue_block(&stripped, AUTO_CONTINUE_WORKBENCH_MARKER) {
            stripped = next;
            continue;
        }
        break;
    }
    stripped
}

fn build_auto_continue_extension_script() -> Vec<u8> {
    let script = r#";(() => {
  try {
    if (globalThis.__wamAutoContinueExtensionBridgeV2Installed) return;
    Object.defineProperty(globalThis, "__wamAutoContinueExtensionBridgeV2Installed", { value: true, configurable: false });
    console.info("[WindsurfAccountManagerAutoContinueExtensionBridgeV2] installed as workbench-send fallback marker");
  } catch (error) {
    console.error("[WindsurfAccountManagerAutoContinueExtensionBridgeV2] failed", error);
  }
})();
"#;
    script.as_bytes().to_vec()
}

fn build_auto_continue_workbench_script() -> Vec<u8> {
    let script = format!(
        r#";(() => {{
  try {{
    if (globalThis.__wamAutoContinueBridgeInstalled) return;
    Object.defineProperty(globalThis, "__wamAutoContinueBridgeInstalled", {{ value: true, configurable: false }});
    const base = "http://127.0.0.1:{port}/wam-auto-continue";
    let config = {{
      enabled: false,
      continueText: "继续工作",
      markers: [
        "third-party model provider is experiencing issues",
        "included daily usage quota is exhausted",
        "all API providers are over their global rate limit for trial users",
        "daily usage quota",
        "usage quota is exhausted",
        "quota is exhausted",
        "global rate limit",
        "rate limit for trial users",
        "purchase extra usage",
        "premium models"
      ]
    }};
    const textOf = (value) => {{
      try {{
        if (value == null) return "";
        if (typeof value === "string") return value;
        if (value instanceof Error) return value.stack || value.message || String(value);
        return JSON.stringify(value);
      }} catch {{
        return String(value ?? "");
      }}
    }};
    const hasMarker = (text) => {{
      const lower = String(text || "").toLowerCase();
      return (config.markers || []).some(marker => lower.includes(String(marker).toLowerCase()));
    }};
    const isBridgeUrl = (url) => String(url || "").startsWith(base);
    const post = (event) => {{
      try {{
        fetch(base + "/event", {{
          method: "POST",
          headers: {{ "content-type": "application/json" }},
          body: JSON.stringify(event),
          keepalive: true
        }}).catch(() => {{}});
      }} catch {{}}
    }};
    const report = async (eventType, value, source, url, payload) => {{
      if (isBridgeUrl(url)) return;
      const message = textOf(value);
      if (!message) return;
      if (eventType !== "auto_continue_test" && !hasMarker(message)) return;
      post({{
        eventType,
        source,
        url,
        location: String(location.href),
        message: message.slice(0, 6000),
        payload: payload || undefined
      }});
    }};
    const refreshConfig = () => fetch(base + "/config", {{ cache: "no-store" }})
      .then(response => response.json())
      .then(result => {{ if (result && result.config) config = {{ ...config, ...result.config }}; }})
      .catch(() => {{}});
    refreshConfig();
    setInterval(refreshConfig, 5000);
    const requestJson = (method, path, body) => fetch(base + path, {{
      method,
      headers: body ? {{ "content-type": "application/json" }} : undefined,
      body: body ? JSON.stringify(body) : undefined,
      cache: "no-store"
    }}).then(response => response.text()).then(text => text ? JSON.parse(text) : {{}});
    const reportResult = (actionId, success, error, method) => requestJson("POST", "/action-result", {{
      actionId,
      success,
      method: method || null,
      error: error ? String(error && error.message ? error.message : error) : null
    }}).catch(() => {{}});
    const isVisible = (element) => {{
      try {{
        if (!element || !(element instanceof Element)) return false;
        const style = getComputedStyle(element);
        if (style.display === "none" || style.visibility === "hidden" || style.opacity === "0") return false;
        const rect = element.getBoundingClientRect();
        return rect.width > 8 && rect.height > 8 && rect.bottom > 0 && rect.right > 0 && rect.top < innerHeight && rect.left < innerWidth;
      }} catch {{
        return false;
      }}
    }};
    const isEditable = (element) => {{
      if (!isVisible(element)) return false;
      if (element.closest?.("[aria-hidden='true']")) return false;
      if (element.matches?.("textarea,input")) return !element.disabled && !element.readOnly && element.type !== "hidden";
      return element.isContentEditable || element.getAttribute?.("role") === "textbox";
    }};
    const visibleText = (element) => String(element?.innerText || element?.textContent || "");
    const queuedMessageText = (element) => String([
      element?.innerText,
      element?.textContent,
      element?.getAttribute?.("aria-label"),
      element?.getAttribute?.("title"),
      element?.getAttribute?.("placeholder")
    ].filter(Boolean).join(" "));
    const hasQueuedMessages = () => {{
      try {{
        const pattern = /\b\d+\s+messages?\s+queued\b|enter\s+to\s+send\s+queued\s+message|queued\s+message|messages?\s+queued|消息.*排队|排队.*消息/;
        return Array.from(document.querySelectorAll("div,span,p,label,input,textarea,[aria-label],[placeholder]"))
          .filter(isVisible)
          .some(element => {{
            const text = queuedMessageText(element).trim().toLowerCase();
            return text.length <= 220 && pattern.test(text);
          }});
      }} catch {{
        return false;
      }}
    }};
    const markerElementIds = new WeakMap();
    let markerElementSeq = 0;
    const markerElementId = (element) => {{
      if (!markerElementIds.has(element)) markerElementIds.set(element, ++markerElementSeq);
      return markerElementIds.get(element);
    }};
    const findVisibleMarkerCandidate = () => {{
      const selectors = "[role='alert'],[aria-live],div,span,p,li";
      const elements = Array.from(document.querySelectorAll(selectors)).filter(isVisible);
      const candidates = [];
      let order = 0;
      for (const element of elements) {{
        try {{
          if (element.matches?.("textarea,input,[contenteditable='true'],[role='textbox']")) continue;
          if (element.querySelector?.("textarea,input,[contenteditable='true'],[role='textbox']")) continue;
          const text = visibleText(element).trim();
          if (!text || text.length > 1800 || !hasMarker(text)) continue;
          const lowerText = text.toLowerCase();
          if (text.includes(config.continueText || "继续工作")) continue;
          if (/ask anything|enter\s+to\s+send\s+queued\s+message|\b\d+\s+messages?\s+queued\b|messages?\s+queued|command awaiting approval/.test(lowerText)) continue;
          const markerChild = Array.from(element.children || []).some(child => {{
            try {{
              const childText = visibleText(child).trim();
              return isVisible(child) && childText && childText.length < text.length && hasMarker(childText);
            }} catch {{
              return false;
            }}
          }});
          if (markerChild) continue;
          const attr = String((element.getAttribute("role") || "") + " " + (element.getAttribute("class") || "") + " " + (element.getAttribute("aria-label") || "")).toLowerCase();
          const rect = element.getBoundingClientRect();
          candidates.push({{ element, text, attr, rect, order: order++ }});
        }} catch {{}}
      }}
      let best = null;
      let bestScore = Number.NEGATIVE_INFINITY;
      for (const candidate of candidates) {{
        let score = candidate.rect.bottom * 4 + candidate.order;
        if (/alert|error|warning|toast|notification|quota/.test(candidate.attr)) score += 1200;
        if (candidate.text.length <= 360) score += 300;
        if (candidate.rect.bottom > innerHeight * 0.35) score += 300;
        if (score > bestScore) {{
            bestScore = score;
            best = candidate;
        }}
      }}
      if (!best) return null;
      const keyText = best.text.toLowerCase().replace(/\s+/g, " ").slice(0, 500);
      return {{ element: best.element, text: best.text, key: keyText + ":" + markerElementId(best.element) }};
    }};
    const candidateEditors = () => {{
      const selector = "textarea,input[type='text'],input:not([type]),[contenteditable='true'],[role='textbox']";
      return Array.from(document.querySelectorAll(selector))
        .filter(isEditable)
        .map(element => {{
          const rect = element.getBoundingClientRect();
          const meta = String([
            element.getAttribute("aria-label"),
            element.getAttribute("placeholder"),
            element.getAttribute("data-testid"),
            element.getAttribute("class"),
            element.id
          ].filter(Boolean).join(" ")).toLowerCase();
          let score = Math.min(rect.width, 900) + Math.min(rect.height, 220);
          if (/chat|message|prompt|composer|cascade|ask|input|textarea/.test(meta)) score += 800;
          if (element === document.activeElement || element.contains(document.activeElement)) score += 500;
          if (rect.width < 160) score -= 600;
          if (/search|filter|find/.test(meta)) score -= 1000;
          return {{ element, score }};
        }})
        .sort((a, b) => b.score - a.score)
        .map(item => item.element);
    }};
    const setNativeValue = (element, value) => {{
      const proto = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
      if (setter) setter.call(element, value);
      else element.value = value;
    }};
    const readEditorText = (editor) => {{
      try {{
        if (editor.matches?.("textarea,input")) return String(editor.value || "");
        return visibleText(editor);
      }} catch {{
        return "";
      }}
    }};
    const clearEditor = (editor) => {{
      editor.focus?.();
      if (editor.matches?.("textarea,input")) {{
        setNativeValue(editor, "");
        editor.dispatchEvent(new InputEvent("input", {{ bubbles: true, inputType: "deleteContentBackward", data: null }}));
        editor.dispatchEvent(new Event("change", {{ bubbles: true }}));
        return;
      }}
      try {{
        const selection = getSelection();
        const range = document.createRange();
        range.selectNodeContents(editor);
        selection.removeAllRanges();
        selection.addRange(range);
        document.execCommand?.("delete", false);
      }} catch {{}}
      if (readEditorText(editor).trim()) {{
        editor.textContent = "";
      }}
      editor.dispatchEvent(new InputEvent("input", {{ bubbles: true, inputType: "deleteContentBackward", data: null }}));
    }};
    const fillEditor = (editor, text) => {{
      editor.scrollIntoView?.({{ block: "center", inline: "nearest" }});
      editor.focus?.();
      clearEditor(editor);
      if (editor.matches?.("textarea,input")) {{
        setNativeValue(editor, text);
        editor.dispatchEvent(new InputEvent("input", {{ bubbles: true, inputType: "insertText", data: text }}));
        editor.dispatchEvent(new Event("change", {{ bubbles: true }}));
        return;
      }}
      const selection = getSelection();
      const range = document.createRange();
      range.selectNodeContents(editor);
      selection.removeAllRanges();
      selection.addRange(range);
      const inserted = document.execCommand?.("insertText", false, text);
      if (!inserted || !visibleText(editor).includes(text)) {{
        editor.textContent = text;
      }}
      if (readEditorText(editor).trim() !== text) {{
        editor.textContent = text;
      }}
      editor.dispatchEvent(new InputEvent("input", {{ bubbles: true, inputType: "insertText", data: text }}));
    }};
    const buttonLabel = (button) => String([
      button.getAttribute?.("aria-label"),
      button.getAttribute?.("title"),
      button.getAttribute?.("data-testid"),
      button.getAttribute?.("class"),
      button.querySelector?.("[aria-label]")?.getAttribute("aria-label"),
      button.querySelector?.("[class*='codicon']")?.getAttribute("class"),
      button.textContent
    ].filter(Boolean).join(" ")).toLowerCase();
    const clickableAncestor = (element) => element?.closest?.("button,[role='button'],a,[tabindex]") || element;
    const findSubmitCandidates = (editor) => {{
      const containers = [];
      let node = editor;
      for (let index = 0; node && index < 8; index += 1, node = node.parentElement) containers.push(node);
      const buttons = [];
      for (const container of containers) {{
        buttons.push(...Array.from(container.querySelectorAll?.("button,[role='button'],a,[tabindex],[class*='send'],[class*='arrow-up'],[class*='codicon-send'],[class*='codicon-arrow']") || []));
      }}
      buttons.push(...Array.from(document.querySelectorAll("button,[role='button'],a,[tabindex],[class*='send'],[class*='arrow-up'],[class*='codicon-send'],[class*='codicon-arrow']")));
      const editorRect = editor.getBoundingClientRect();
      const container = containers.find(item => {{
        const rect = item.getBoundingClientRect?.();
        return rect && rect.width >= editorRect.width && rect.right >= editorRect.right;
      }}) || editor.parentElement || editor;
      const containerRect = container.getBoundingClientRect();
      const pointCandidates = [];
      for (const point of [
        [containerRect.right - 18, editorRect.top + editorRect.height / 2],
        [containerRect.right - 18, containerRect.bottom - 18],
        [editorRect.right + 24, editorRect.top + editorRect.height / 2]
      ]) {{
        const hit = document.elementFromPoint(point[0], point[1]);
        const clickable = clickableAncestor(hit);
        if (clickable) pointCandidates.push(clickable);
      }}
      buttons.push(...pointCandidates);
      const unique = Array.from(new Set(buttons.map(clickableAncestor))).filter(button =>
        button &&
        button !== editor &&
        !editor.contains(button) &&
        !button.contains?.(editor) &&
        !button.matches?.("textarea,input,[contenteditable='true'],[role='textbox']") &&
        isVisible(button) &&
        !button.disabled &&
        button.getAttribute?.("aria-disabled") !== "true"
      );
      const scored = unique.map(button => {{
        const rect = button.getBoundingClientRect();
        const label = buttonLabel(button);
        let score = 0;
        if (/send|submit|发送|提交/.test(label)) score += 1800;
        if (/arrow.?up|paper|plane|codicon-arrow|codicon-send/.test(label)) score += 1200;
        if (/mic|microphone|voice|audio|record|plus|add|attach|context|code|model|gpt|thinking|settings|more|menu|copy|thumb/.test(label)) score -= 1800;
        const centerY = rect.top + rect.height / 2;
        const editorCenterY = editorRect.top + editorRect.height / 2;
        const verticalDistance = Math.abs(centerY - editorCenterY);
        score -= verticalDistance * 4;
        if (verticalDistance < 36) score += 650;
        if (rect.left >= editorRect.left - 20 && rect.top >= editorRect.top - 50 && rect.bottom <= editorRect.bottom + 90) score += 350;
        if (rect.left >= editorRect.left + editorRect.width * 0.6) score += 550;
        if (containerRect.right - rect.right < 90) score += 900;
        score += Math.max(0, rect.left - containerRect.left) / 4;
        if (rect.width >= 18 && rect.width <= 64 && rect.height >= 18 && rect.height <= 64) score += 250;
        return {{ button, score }};
      }}).sort((a, b) => b.score - a.score);
      return scored.filter(item => item.score > -300).map(item => item.button);
    }};
    const clickControl = (element) => {{
      element.scrollIntoView?.({{ block: "center", inline: "nearest" }});
      const rect = element.getBoundingClientRect();
      const options = {{ bubbles: true, cancelable: true, view: window, clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 }};
      try {{ element.dispatchEvent(new PointerEvent("pointerover", options)); }} catch {{}}
      try {{ element.dispatchEvent(new PointerEvent("pointerenter", options)); }} catch {{}}
      try {{ element.dispatchEvent(new MouseEvent("mouseover", options)); }} catch {{}}
      try {{ element.dispatchEvent(new PointerEvent("pointerdown", {{ ...options, button: 0, buttons: 1 }})); }} catch {{}}
      try {{ element.dispatchEvent(new MouseEvent("mousedown", {{ ...options, button: 0, buttons: 1 }})); }} catch {{}}
      try {{ element.dispatchEvent(new PointerEvent("pointerup", {{ ...options, button: 0, buttons: 0 }})); }} catch {{}}
      try {{ element.dispatchEvent(new MouseEvent("mouseup", {{ ...options, button: 0, buttons: 0 }})); }} catch {{}}
      try {{ element.dispatchEvent(new MouseEvent("click", {{ ...options, button: 0, buttons: 0 }})); }} catch {{}}
      try {{ element.click?.(); }} catch {{}}
    }};
    const pressEnter = (editor) => {{
      for (const type of ["keydown", "keypress", "keyup"]) {{
        editor.dispatchEvent(new KeyboardEvent(type, {{ key: "Enter", code: "Enter", which: 13, keyCode: 13, bubbles: true, cancelable: true }}));
      }}
    }};
    const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));
    const waitForSubmission = async (editor, text, timeoutMs = 2600) => {{
      const deadline = Date.now() + timeoutMs;
      while (Date.now() < deadline) {{
        await sleep(250);
        const editorText = readEditorText(editor);
        if (hasQueuedMessages()) {{
          if (editorText.includes(text)) clearEditor(editor);
          return true;
        }}
        if (!editorText.includes(text)) return true;
      }}
      return false;
    }};
    const cleanupResidualText = (editor, text) => {{
      try {{
        if (readEditorText(editor).includes(text)) clearEditor(editor);
      }} catch {{}}
    }};
    const cleanupResidualAutoText = (text) => {{
      try {{
        for (const editor of candidateEditors().slice(0, 4)) cleanupResidualText(editor, text);
      }} catch {{}}
    }};
    const sendTextViaDom = async (text) => {{
      if (hasQueuedMessages()) throw new Error("Cascade 已有排队消息，暂停自动继续");
      const editors = candidateEditors();
      if (!editors.length) throw new Error("未找到可见的 Cascade 输入框");
      const errors = [];
      for (const editor of editors.slice(0, 4)) {{
        let touchedEditor = false;
        try {{
          if (hasQueuedMessages()) return "queued_existing";
          fillEditor(editor, text);
          touchedEditor = true;
          await sleep(350);
          const buttons = findSubmitCandidates(editor);
          const button = buttons[0];
          if (button) {{
            clickControl(button);
            if (await waitForSubmission(editor, text)) {{
              return "dom_button";
            }}
            cleanupResidualText(editor, text);
            throw new Error("已点击发送按钮但未确认提交，已清理输入框并暂停避免重复排队");
          }}
          pressEnter(editor);
          if (await waitForSubmission(editor, text)) {{
            return "dom_enter";
          }}
          cleanupResidualText(editor, text);
          throw new Error("已填入文本但未确认提交，已清理输入框并暂停避免重复排队");
        }} catch (error) {{
          errors.push(String(error && error.message ? error.message : error));
          if (touchedEditor) break;
        }}
      }}
      throw new Error(errors.join(" | ") || "输入框填充失败");
    }};
    let actionRunning = false;
    let lastHandledMarkerElement = null;
    let lastHandledMarkerKey = "";
    const pollActions = async () => {{
      if (actionRunning) return;
      actionRunning = true;
      try {{
        const markerCandidate = findVisibleMarkerCandidate();
        if (!markerCandidate) {{
          lastHandledMarkerElement = null;
          lastHandledMarkerKey = "";
          return;
        }}
        if (hasQueuedMessages()) {{
          cleanupResidualAutoText(config.continueText || "继续工作");
          return;
        }}
        const result = await requestJson("GET", "/actions");
        const sameMarkerAlreadyHandled = lastHandledMarkerElement === markerCandidate.element && lastHandledMarkerKey === markerCandidate.key;
        let sentOne = false;
        for (const action of result.actions || []) {{
          if (sentOne) {{
            await reportResult(action.id, false, "skipped: another action already sent in this poll", null);
            continue;
          }}
          try {{
            if (hasQueuedMessages()) {{
              await reportResult(action.id, false, "skipped: cascade has queued messages", null);
              continue;
            }}
            if (sameMarkerAlreadyHandled) {{
              await reportResult(action.id, false, "skipped: marker already handled", null);
              continue;
            }}
            const method = await sendTextViaDom(action.text || config.continueText || "继续工作");
            lastHandledMarkerElement = markerCandidate.element;
            lastHandledMarkerKey = markerCandidate.key || "";
            sentOne = true;
            console.info("[WindsurfAccountManagerAutoContinueBridge] action sent via " + method);
            await reportResult(action.id, true, null, method);
          }} catch (error) {{
            await reportResult(action.id, false, error, null);
          }}
        }}
      }} catch {{}}
      finally {{
        actionRunning = false;
      }}
    }};
    setInterval(pollActions, 1500);
    setTimeout(pollActions, 1200);
    let lastMessage = "";
    let markerMissingSince = 0;
    let scanTimer = 0;
    const scanVisibleText = () => {{
      scanTimer = 0;
      try {{
        if (hasQueuedMessages()) {{
          cleanupResidualAutoText(config.continueText || "继续工作");
          return;
        }}
        const now = Date.now();
        const candidate = findVisibleMarkerCandidate();
        if (!candidate) {{
          if (!markerMissingSince) markerMissingSince = now;
          if (now - markerMissingSince > 1500) lastMessage = "";
          return;
        }}
        markerMissingSince = 0;
        const text = candidate.text;
        const marker = (config.markers || []).find(item => text.toLowerCase().includes(String(item).toLowerCase()));
        const index = marker ? text.toLowerCase().indexOf(String(marker).toLowerCase()) : 0;
        const start = Math.max(0, index - 240);
        const message = text.slice(start, Math.min(text.length, index + 1200));
        const messageKey = candidate.key || message;
        if (messageKey === lastMessage) return;
        lastMessage = messageKey;
        report("dom_text", message, "workbench-dom", String(location.href), {{ markerKey: messageKey }});
      }} catch {{}}
    }};
    const scheduleScan = () => {{
      if (scanTimer) return;
      scanTimer = setTimeout(scanVisibleText, 800);
    }};
    const startObserver = () => {{
      try {{
        if (!document.body || globalThis.__wamAutoContinueDomObserver) return;
        const observer = new MutationObserver(scheduleScan);
        observer.observe(document.body, {{ childList: true, subtree: true, characterData: true }});
        Object.defineProperty(globalThis, "__wamAutoContinueDomObserver", {{ value: observer, configurable: false }});
        scheduleScan();
      }} catch {{}}
    }};
    if (document.readyState === "loading") {{
      document.addEventListener("DOMContentLoaded", startObserver, {{ once: true }});
    }} else {{
      startObserver();
    }}
    setInterval(scheduleScan, 5000);
    globalThis.addEventListener?.("error", event => report("window_error", event?.message || event?.error, "window_error", String(location.href)));
    globalThis.addEventListener?.("unhandledrejection", event => report("unhandledrejection", event?.reason, "unhandledrejection", String(location.href)));
    console.info("[WindsurfAccountManagerAutoContinueBridge] installed");
  }} catch (error) {{
    console.error("[WindsurfAccountManagerAutoContinueBridge] failed", error);
  }}
}})();
"#,
        port = AUTO_CONTINUE_BRIDGE_PORT
    );
    script.into_bytes()
}

fn compute_sha256_base64_no_pad(content: &[u8]) -> String {
    let digest = Sha256::digest(content);
    general_purpose::STANDARD_NO_PAD.encode(digest)
}

fn sync_workbench_product_checksum(windsurf_path: &str, workbench_content: &[u8]) -> Result<Option<String>, String> {
    let product_file = PathBuf::from(windsurf_path).join(get_product_json_relative_path());
    if !product_file.exists() {
        return Ok(None);
    }
    let product_content = fs::read(&product_file)
        .map_err(|e| format!("读取 product.json 失败: {}", e))?;
    let mut product_json: serde_json::Value = serde_json::from_slice(&product_content)
        .map_err(|e| format!("解析 product.json 失败: {}", e))?;
    let checksums = product_json
        .get_mut("checksums")
        .and_then(|value| value.as_object_mut())
        .ok_or_else(|| "product.json 中未找到 checksums".to_string())?;
    let key = "vs/workbench/workbench.desktop.main.js";
    let new_checksum = compute_sha256_base64_no_pad(workbench_content);
    let old_checksum = checksums
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    if old_checksum.as_deref() == Some(new_checksum.as_str()) {
        return Ok(None);
    }
    let backup_file = product_file.with_extension(&format!(
        "json.backup.auto_continue_checksum.{}",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    fs::copy(&product_file, &backup_file)
        .map_err(|e| format!("备份 product.json 失败: {}", e))?;
    checksums.insert(key.to_string(), serde_json::Value::String(new_checksum.clone()));
    let mut serialized = serde_json::to_vec_pretty(&product_json)
        .map_err(|e| format!("序列化 product.json 失败: {}", e))?;
    serialized.push(b'\n');
    fs::write(&product_file, serialized)
        .map_err(|e| format!("写入 product.json 失败: {}", e))?;
    Ok(Some(format!(
        "{} -> {}",
        old_checksum.unwrap_or_else(|| "<missing>".to_string()),
        new_checksum
    )))
}

fn is_workbench_product_checksum_current(windsurf_path: &str, workbench_content: &[u8]) -> Result<Option<bool>, String> {
    let product_file = PathBuf::from(windsurf_path).join(get_product_json_relative_path());
    if !product_file.exists() {
        return Ok(None);
    }
    let product_content = fs::read(&product_file)
        .map_err(|e| format!("读取 product.json 失败: {}", e))?;
    let product_json: serde_json::Value = serde_json::from_slice(&product_content)
        .map_err(|e| format!("解析 product.json 失败: {}", e))?;
    let expected = product_json
        .get("checksums")
        .and_then(|value| value.get("vs/workbench/workbench.desktop.main.js"))
        .and_then(|value| value.as_str());
    Ok(expected.map(|value| value == compute_sha256_base64_no_pad(workbench_content)))
}

fn apply_auto_continue_bridge_to_workbench(windsurf_path: &str) -> Result<Option<String>, String> {
    let workbench_file = PathBuf::from(windsurf_path).join(get_workbench_js_relative_path());
    if !workbench_file.exists() {
        return Err(format!("workbench.desktop.main.js 文件不存在: {:?}", workbench_file));
    }
    let content = fs::read(&workbench_file)
        .map_err(|e| format!("读取 workbench 文件失败: {}", e))?;
    let cleaned_content = strip_appended_auto_continue_workbench_blocks(&content);
    let mut modified_content = cleaned_content;
    modified_content.extend_from_slice(b"\n");
    modified_content.extend_from_slice(&build_auto_continue_workbench_script());
    modified_content.extend_from_slice(b"\n");
    if modified_content == content {
        sync_workbench_product_checksum(windsurf_path, &content)?;
        return Ok(None);
    }
    let backup_file = workbench_file.with_extension(&format!(
        "js.backup.auto_continue.{}",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    fs::copy(&workbench_file, &backup_file)
        .map_err(|e| format!("备份 workbench 文件失败: {}", e))?;
    fs::write(&workbench_file, &modified_content)
        .map_err(|e| format!("写入 workbench 文件失败: {}", e))?;
    sync_workbench_product_checksum(windsurf_path, &modified_content)?;
    Ok(Some(backup_file.to_string_lossy().to_string()))
}

fn apply_auto_continue_sender_to_extension(windsurf_path: &str) -> Result<Option<String>, String> {
    let extension_file = PathBuf::from(windsurf_path).join(get_extension_js_relative_path());
    if !extension_file.exists() {
        return Err(format!("extension.js 文件不存在: {:?}", extension_file));
    }
    let content = fs::read(&extension_file)
        .map_err(|e| format!("读取 extension.js 文件失败: {}", e))?;
    let cleaned_content = strip_appended_auto_continue_extension_blocks(&content);
    if cleaned_content == content && bytes_contains(&content, AUTO_CONTINUE_EXTENSION_MARKER) {
        return Ok(None);
    }
    let backup_file = extension_file.with_extension(&format!(
        "js.backup.auto_continue_extension.{}",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    fs::copy(&extension_file, &backup_file)
        .map_err(|e| format!("备份 extension.js 文件失败: {}", e))?;
    let mut modified_content = cleaned_content;
    modified_content.extend_from_slice(b"\n");
    modified_content.extend_from_slice(&build_auto_continue_extension_script());
    modified_content.extend_from_slice(b"\n");
    fs::write(&extension_file, &modified_content)
        .map_err(|e| format!("写入 extension.js 文件失败: {}", e))?;
    Ok(Some(backup_file.to_string_lossy().to_string()))
}

#[command]
pub async fn apply_auto_continue_bridge_patch(
    windsurf_path: String,
) -> Result<serde_json::Value, String> {
    let workbench_backup_file = apply_auto_continue_bridge_to_workbench(&windsurf_path)?;
    let extension_backup_file = apply_auto_continue_sender_to_extension(&windsurf_path)?;
    let already_patched = workbench_backup_file.is_none() && extension_backup_file.is_none();
    let workbench_changed = workbench_backup_file.is_some();
    let extension_changed = extension_backup_file.is_some();
    if !already_patched {
        restart_windsurf(Some(&windsurf_path)).await?;
    }

    Ok(serde_json::json!({
        "success": true,
        "already_patched": already_patched,
        "backup_file": extension_backup_file.clone().or_else(|| workbench_backup_file.clone()),
        "workbench_backup_file": workbench_backup_file,
        "extension_backup_file": extension_backup_file,
        "message": if already_patched {
            "自动继续 Bridge 补丁已安装，workbench 校验已同步"
        } else if workbench_changed && extension_changed {
            "自动继续 Bridge 检测/发送补丁已安装，workbench 校验已同步，Windsurf 正在重启"
        } else if workbench_changed {
            "自动继续 Bridge 检测补丁已安装，workbench 校验已同步，Windsurf 正在重启"
        } else {
            "自动继续 Bridge 扩展补丁已安装，Windsurf 正在重启"
        }
    }))
}

/// 查找最新的可用且干净的备份文件
fn find_latest_backup(extension_dir: &Path, saved_backup_path: &Option<String>) -> Result<PathBuf, String> {
    // 1. 首先尝试使用设置中保存的备份路径
    if let Some(ref saved_path) = saved_backup_path {
        let saved = PathBuf::from(saved_path);
        if saved.exists() {
            // 验证备份文件是否是干净的
            if !is_file_patched(&saved) {
                return Ok(saved);
            }
            println!("设置中保存的备份文件已被污染（包含补丁特征）: {:?}", saved);
        } else {
            println!("设置中保存的备份文件不存在: {:?}", saved);
        }
    }
    
    // 2. 查找目录中所有备份文件
    let mut backup_files: Vec<PathBuf> = fs::read_dir(extension_dir)
        .map_err(|e| format!("读取目录失败: {}", e))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("extension.js.backup."))
                .unwrap_or(false)
        })
        .collect();
    
    if backup_files.is_empty() {
        return Err("未找到任何备份文件，无法还原。请手动重新安装 Windsurf 或从官方下载 extension.js 文件".to_string());
    }
    
    // 按修改时间排序（最旧的在前，因为最早的备份最可能是干净的）
    backup_files.sort_by(|a, b| {
        let time_a = fs::metadata(a).and_then(|m| m.modified()).ok();
        let time_b = fs::metadata(b).and_then(|m| m.modified()).ok();
        time_a.cmp(&time_b)
    });
    
    // 3. 查找第一个干净的备份文件（从最旧的开始）
    for backup in &backup_files {
        if !is_file_patched(backup) {
            return Ok(backup.clone());
        }
        println!("备份文件已被污染，跳过: {:?}", backup);
    }
    
    // 所有备份都被污染了
    Err("所有备份文件都已被污染（包含补丁特征）。请手动重新安装 Windsurf 获取原始 extension.js 文件".to_string())
}

/// 检查补丁状态
#[command]
pub async fn check_patch_status(
    windsurf_path: String,
) -> Result<serde_json::Value, String> {
    let extension_file = PathBuf::from(&windsurf_path).join(get_extension_js_relative_path());
    let workbench_file = PathBuf::from(&windsurf_path).join(get_workbench_js_relative_path());
    
    if !extension_file.exists() {
        return Ok(serde_json::json!({
            "installed": false,
            "error": "extension.js文件不存在"
        }));
    }
    
    // 按字节读取，避免 extension.js 含有非 UTF-8 字节时整个状态检查接口直接报错
    let content = fs::read(&extension_file)
        .map_err(|e| format!("读取文件失败: {}", e))?;
    
    // 检查是否包含补丁标识（字节级 contains）
    let has_oauth_handler = has_any_seamless_patch(&content);
    let has_timeout_removed = !bytes_contains(&content, b"18e4");
    let has_current_seamless_marker = has_current_seamless_patch(&content);
    let has_auto_continue_extension = bytes_contains(&content, AUTO_CONTINUE_EXTENSION_MARKER);
    let workbench_content = if workbench_file.exists() {
        Some(fs::read(&workbench_file).map_err(|e| format!("读取 workbench 文件失败: {}", e))?)
    } else {
        None
    };
    let has_auto_continue_workbench = workbench_content
        .as_ref()
        .map(|content| bytes_contains(content, AUTO_CONTINUE_WORKBENCH_MARKER))
        .unwrap_or(false);
    let workbench_checksum_current = if let Some(content) = workbench_content.as_ref() {
        is_workbench_product_checksum_current(&windsurf_path, content)?
    } else {
        None
    };
    
    Ok(serde_json::json!({
        "installed": has_oauth_handler,
        "oauth_handler": has_oauth_handler,
        "seamless_patch_marker": has_current_seamless_marker,
        "timeout_removed": has_timeout_removed,
        "auto_continue_bridge": has_auto_continue_extension && has_auto_continue_workbench && workbench_checksum_current.unwrap_or(true),
        "auto_continue_detector": has_auto_continue_workbench,
        "auto_continue_sender": has_auto_continue_extension,
        "auto_continue_workbench_dirty": has_auto_continue_workbench && !workbench_checksum_current.unwrap_or(true),
        "auto_continue_workbench_checksum_current": workbench_checksum_current
    }))
}

/// 重启Windsurf
/// windsurf_path: 可选的Windsurf安装路径，优先使用此路径直接启动
async fn restart_windsurf(windsurf_path: Option<&str>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        // 1. 关闭Windsurf
        Command::new("taskkill")
            .creation_flags(CREATE_NO_WINDOW)
            .args(&["/F", "/IM", "Windsurf.exe"])
            .output()
            .map_err(|e| format!("关闭Windsurf失败: {}", e))?;
        
        // 等待进程完全结束
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        // 2. 优先尝试使用已知路径直接启动 Windsurf.exe
        if let Some(path) = windsurf_path {
            let exe_path = PathBuf::from(path).join("Windsurf.exe");
            if exe_path.exists() {
                match Command::new(&exe_path)
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn() {
                    Ok(_) => {
                        println!("通过已知路径启动Windsurf: {:?}", exe_path);
                        return Ok(());
                    }
                    Err(e) => {
                        println!("直接启动失败，尝试快捷方式: {}", e);
                    }
                }
            }
        }
        
        // 3. 回退：搜索快捷方式启动
        let shortcut_dirs = get_shortcut_search_dirs();
        
        for dir in shortcut_dirs {
            if let Ok(shortcut) = find_windsurf_shortcut(&dir) {
                Command::new("cmd")
                    .creation_flags(CREATE_NO_WINDOW)
                    .args(&["/C", "start", "", &shortcut.to_string_lossy()])
                    .spawn()
                    .map_err(|e| format!("启动Windsurf失败: {}", e))?;
                
                println!("通过快捷方式启动Windsurf: {:?}", shortcut);
                return Ok(());
            }
        }
        
        return Err("未找到Windsurf可执行文件或快捷方式".to_string());
    }
    
    #[cfg(target_os = "macos")]
    {
        // 1. 关闭Windsurf
        Command::new("pkill")
            .args(&["-f", "Windsurf"])
            .output()
            .map_err(|e| format!("关闭Windsurf失败: {}", e))?;
        
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        // 2. 优先使用已知路径启动
        if let Some(path) = windsurf_path {
            let app_path = PathBuf::from(path);
            if app_path.exists() {
                match Command::new("open")
                    .args(&["-a", &app_path.to_string_lossy()])
                    .spawn() {
                    Ok(_) => {
                        println!("通过已知路径启动Windsurf: {:?}", app_path);
                        return Ok(());
                    }
                    Err(e) => {
                        println!("直接启动失败，尝试默认方式: {}", e);
                    }
                }
            }
        }
        
        // 3. 回退：使用默认方式启动
        Command::new("open")
            .args(&["-a", "Windsurf"])
            .spawn()
            .map_err(|e| format!("启动Windsurf失败: {}", e))?;
        
        return Ok(());
    }
    
    #[cfg(target_os = "linux")]
    {
        // 1. 关闭Windsurf
        Command::new("pkill")
            .args(&["-f", "windsurf"])
            .output()
            .map_err(|e| format!("关闭Windsurf失败: {}", e))?;
        
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        // 2. 优先使用已知路径启动
        if let Some(path) = windsurf_path {
            let exe_path = PathBuf::from(path).join("windsurf");
            if exe_path.exists() {
                match Command::new(&exe_path).spawn() {
                    Ok(_) => {
                        println!("通过已知路径启动Windsurf: {:?}", exe_path);
                        return Ok(());
                    }
                    Err(e) => {
                        println!("直接启动失败，尝试默认方式: {}", e);
                    }
                }
            }
        }
        
        // 3. 回退：使用默认方式启动
        Command::new("windsurf")
            .spawn()
            .map_err(|e| format!("启动Windsurf失败: {}", e))?;
        
        return Ok(());
    }
    
    #[allow(unreachable_code)]
    Err("不支持的操作系统".to_string())
}

/// 获取快捷方式搜索目录列表 (Windows)
#[cfg(target_os = "windows")]
fn get_shortcut_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    
    // 用户桌面
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        dirs.push(PathBuf::from(&userprofile).join("Desktop"));
    }
    
    // 公共桌面
    if let Ok(public) = std::env::var("PUBLIC") {
        dirs.push(PathBuf::from(&public).join("Desktop"));
    }
    
    // 用户开始菜单
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(&appdata).join("Microsoft\\Windows\\Start Menu\\Programs"));
        dirs.push(PathBuf::from(&appdata).join("Microsoft\\Windows\\Start Menu\\Programs\\Windsurf"));
    }
    
    // 公共开始菜单
    if let Ok(programdata) = std::env::var("PROGRAMDATA") {
        dirs.push(PathBuf::from(&programdata).join("Microsoft\\Windows\\Start Menu\\Programs"));
        dirs.push(PathBuf::from(&programdata).join("Microsoft\\Windows\\Start Menu\\Programs\\Windsurf"));
    }
    
    dirs
}

/// 在指定目录中查找 Windsurf 快捷方式 (Windows)
#[cfg(target_os = "windows")]
fn find_windsurf_shortcut(dir: &Path) -> Result<PathBuf, String> {
    if !dir.exists() {
        return Err(format!("目录不存在: {:?}", dir));
    }
    
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_windsurf = path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase().contains("windsurf"))
                .unwrap_or(false);
            let is_lnk = path.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase() == "lnk")
                .unwrap_or(false);
            
            if is_windsurf && is_lnk {
                return Ok(path);
            }
        }
    }
    
    Err(format!("在 {:?} 中未找到 Windsurf 快捷方式", dir))
}
