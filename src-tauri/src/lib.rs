mod models;
mod repository;
mod services;
mod commands;
mod utils;

use repository::DataStore;
use commands::{AutoResetStore, ResetRecordStore};
use std::sync::Arc;
use tauri::Manager;

/// 获取日志目录（跨平台支持）
/// - Windows Debug: exe同级的logs目录
/// - Windows Release: exe同级的logs目录
/// - macOS: ~/Library/Logs/com.chao.windsurf-account-manager/
/// - Linux: ~/.local/share/com.chao.windsurf-account-manager/logs/
fn get_log_directory() -> Option<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    {
        // Debug模式：exe同级的logs目录
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("logs")))
    }
    
    #[cfg(not(debug_assertions))]
    {
        // Release模式：根据平台选择合适的日志目录
        #[cfg(target_os = "macos")]
        {
            // macOS: ~/Library/Logs/com.chao.windsurf-account-manager/
            std::env::var("HOME").ok()
                .map(|h| std::path::PathBuf::from(h).join("Library/Logs/com.chao.windsurf-account-manager"))
        }
        
        #[cfg(target_os = "linux")]
        {
            // Linux: ~/.local/share/com.chao.windsurf-account-manager/logs/
            std::env::var("HOME").ok()
                .map(|h| std::path::PathBuf::from(h).join(".local/share/com.chao.windsurf-account-manager/logs"))
        }
        
        #[cfg(target_os = "windows")]
        {
            // Windows: exe同级的logs目录
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("logs")))
        }
    }
}

/// 初始化日志：同时输出到控制台和日志文件
/// Debug和Release都会写入日志文件，便于问题排查
fn init_logging() {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::sync::Mutex;

    let log_dir = get_log_directory();

    let log_file: Option<Arc<Mutex<std::fs::File>>> = log_dir.and_then(|dir| {
        fs::create_dir_all(&dir).ok()?;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        #[cfg(debug_assertions)]
        let path = dir.join(format!("backend_{}.log", today));
        #[cfg(not(debug_assertions))]
        let path = dir.join(format!("app_{}.log", today));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()?;
        eprintln!("[init_logging] Log file: {}", path.display());
        Some(Arc::new(Mutex::new(file)))
    });

    let file_for_logger = log_file.clone();
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .format(move |buf, record| {
            use std::io::Write as _;
            let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let line = format!(
                "[{}] [{}] [{}] {}\n",
                ts,
                record.level(),
                record.target(),
                record.args()
            );
            // 写入文件
            if let Some(ref f) = file_for_logger {
                if let Ok(mut file) = f.lock() {
                    let _ = file.write_all(line.as_bytes());
                }
            }
            // 同时写入控制台
            buf.write_all(line.as_bytes())
        })
        .init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 初始化数据存储
            let store = DataStore::new(app.handle())
                .expect("Failed to initialize data store");
            let store = Arc::new(store);
            
            // 将数据存储注入到应用状态
            app.manage(store.clone());
            
            // 初始化机器设备码存储
            let machine_id_store = commands::MachineIdStore::new(app.handle())
                .expect("Failed to initialize machine ID store");
            app.manage(Arc::new(machine_id_store));
            
            // 初始化自动重置配置存储
            let auto_reset_store = AutoResetStore::new(app.handle())
                .expect("Failed to initialize auto reset store");
            app.manage(Arc::new(auto_reset_store));
            
            // 初始化重置记录存储
            let reset_record_store = ResetRecordStore::new(app.handle())
                .expect("Failed to initialize reset record store");
            app.manage(Arc::new(reset_record_store));
            
            // 初始化代理配置
            let store_for_proxy = store.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(settings) = store_for_proxy.get_settings().await {
                    if settings.proxy_enabled || settings.proxy_url.is_some() {
                        println!("[Init] Loading proxy config: enabled={}, url={:?}", 
                            settings.proxy_enabled, settings.proxy_url);
                        services::update_proxy_config(
                            settings.proxy_enabled,
                            settings.proxy_url
                        );
                    }
                }
            });
            
            // 获取版本号并设置窗口标题
            let version = app.package_info().version.to_string();
            if let Some(window) = app.get_webview_window("main") {
                let title = format!("windsurf-account-manager-simple v{}", version);
                window.set_title(&title).ok();
            }
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 账号管理命令
            commands::add_account,
            commands::add_account_by_refresh_token,
            commands::get_all_accounts,
            commands::get_account,
            commands::update_account,
            commands::delete_account,
            commands::delete_accounts_batch,
            commands::search_accounts,
            commands::filter_accounts_by_group,
            commands::filter_accounts_by_tags,
            
            // API操作命令
            commands::login_account,
            commands::refresh_token,
            commands::get_plan_status,
            commands::reset_credits,
            commands::update_seats,
            commands::get_billing,
            commands::update_plan,
            commands::cancel_subscription,
            commands::resume_subscription,
            commands::get_account_info,
            commands::get_current_user,
            commands::batch_reset_credits,
            commands::batch_refresh_tokens,
            commands::get_team_credit_entries,
            commands::get_trial_payment_link,
            commands::get_team_config,
            commands::update_team_config,
            commands::get_cascade_model_configs,
            commands::get_command_model_configs,
            commands::get_team_organizational_controls,
            commands::upsert_team_organizational_controls,
            commands::get_available_mcp_plugins,
            commands::delete_windsurf_user,
            
            // 支付相关命令
            commands::generate_virtual_card,
            commands::open_payment_window,
            commands::inject_card_info,
            commands::validate_card_number,
            commands::auto_fill_payment_form,
            commands::get_trial_payment_link_enhanced,
            commands::open_external_link,
            commands::open_external_link_incognito,
            commands::inject_auto_submit_script,
            commands::close_payment_window,
            commands::get_success_bins,
            commands::add_success_bin,
            commands::clear_success_bins,
            commands::get_random_success_bin,
            commands::reset_test_mode_progress,
            commands::get_test_mode_progress,
            
            // Protobuf解析API命令（返回解析后的数据）
            commands::get_current_user_parsed,
            commands::get_billing_parsed,
            commands::batch_get_users_parsed,

            // Analytics 分析命令
            commands::get_account_analytics,

            // 设置管理命令
            commands::get_settings,
            commands::update_settings,
            commands::get_groups,
            commands::add_group,
            commands::delete_group,
            commands::rename_group,
            commands::get_tags,
            commands::add_tag,
            commands::update_tag,
            commands::delete_tag,
            commands::batch_update_account_tags,
            commands::get_logs,
            commands::clear_logs,
            commands::get_stats,
            commands::export_data,
            
            // 切号相关命令
            commands::switch_account,
            commands::reset_machine_id,
            commands::check_admin_privileges,
            commands::check_auto_switch,
            
            // 机器设备码管理命令
            commands::get_current_machine_ids,
            commands::get_machine_id_records,
            commands::save_current_machine_id,
            commands::apply_machine_id,
            commands::update_machine_id_label,
            commands::delete_machine_id_record,
            commands::clear_all_machine_id_records,
            commands::toggle_machine_id_bookmark,
            
            // Windsurf信息命令
            commands::get_current_windsurf_info,
            
            // 应用信息命令
            commands::get_app_version,
            commands::get_app_title,
            commands::reset_http_client,
            
            // 无感换号补丁命令
            commands::get_windsurf_path,
            commands::apply_seamless_patch,
            commands::restore_seamless_patch,
            commands::check_patch_status,
            commands::validate_windsurf_path,

            // 数据备份命令
            commands::create_backup,
            commands::list_backups,
            commands::restore_backup,
            commands::export_data_to_file,
            commands::import_data_from_file,
            commands::get_data_directory,
            
            // 排序命令
            commands::get_sorted_accounts,
            commands::update_accounts_order,
            commands::update_sort_config,
            commands::get_sort_config,
            
            // 团队管理命令
            commands::get_team_members,
            commands::invite_team_members,
            commands::remove_team_member,
            commands::revoke_invitation,
            commands::get_pending_invitations,
            commands::get_my_pending_invitation,
            commands::accept_invitation,
            commands::reject_invitation,
            commands::request_team_access,
            commands::approve_team_join_request,
            // 自动充值管理
            commands::get_credit_top_up_settings,
            commands::update_credit_top_up_settings,
            // 成员权限管理
            commands::update_codeium_access,
            commands::add_user_role,
            commands::remove_user_role,
            commands::transfer_subscription,
            
            // 自动重置命令
            commands::get_auto_reset_configs,
            commands::add_auto_reset_config,
            commands::update_auto_reset_config,
            commands::delete_auto_reset_config,
            commands::check_and_auto_reset,
            commands::force_reset_config,
            commands::get_reset_records,
            commands::get_reset_stats,
            commands::clear_reset_records,
            
            // 日志命令
            commands::append_log_file,
            commands::get_log_directory,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
