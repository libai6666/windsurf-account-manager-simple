use tauri::command;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use regex::bytes::Regex;
use chrono::Local;
use std::sync::Arc;
use tauri::State;
use crate::repository::DataStore;

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
        if is_full_patch_installed(&extension_file) {
            return Ok(serde_json::json!({
                "success": true,
                "already_patched": true,
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
        bytes_contains(&content, b"Failed to handle OAuth callback")
    } else {
        false
    }
}

fn is_full_patch_installed(file_path: &Path) -> bool {
    if let Ok(content) = fs::read(file_path) {
        bytes_contains(&content, b"Failed to handle OAuth callback")
    } else {
        false
    }
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
    let has_oauth_handler = bytes_contains(&content, b"Failed to handle OAuth callback");
    let has_extension_login = bytes_contains(&content, b"WindsurfAccountManager v2] Profile login applied")
        || bytes_contains(&content, b"WindsurfAccountManager] Profile login applied");
    let has_timeout_removed = !bytes_contains(&content, b"18e4");
    
    Ok(serde_json::json!({
        "installed": has_oauth_handler,
        "oauth_handler": has_oauth_handler,
        "extension_login": has_extension_login,
        "timeout_removed": has_timeout_removed
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
