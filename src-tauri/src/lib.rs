//! PrintLink - 共享打印机自动连接客户端
//! 模块导出 + Tauri 指令注册 + 窗口关闭拦截（最小化到托盘）

use tauri::Manager;

mod config;
mod credential;
mod printer_api;
mod shared_drive;
mod smb_scan;
mod tray;
mod utils;

// ===== 类型定义（用于 Tauri Events） =====

/// Server scan result event
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerScanResult {
    pub server_addr: String,
    pub count: u32,
    pub printers: Vec<smb_scan::PrinterItem>,
}

/// Server scan error event
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerScanError {
    pub server_addr: String,
    pub error: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger system
    utils::init_logger();

    tauri::Builder::default()
        // Single instance protection: focus existing window on second launch
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            log::info!("检测到重复启动请求，已聚焦现有窗口");
        }))
        // Register opener plugin (for opening external browser links)
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Initialize system tray
            tray::setup_tray(app.handle())?;
            log::info!("PrintLink 应用启动完成");
            Ok(())
        })
        // Intercept window close events: hide to tray instead of exiting
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                log::info!("窗口关闭请求已拦截，最小化到系统托盘");
            }
        })
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::save_config_command,
            config::reset_config,
            credential::init_print_credential,
            smb_scan::get_server_printer_list,
            smb_scan::fetch_driver_info_async,
            smb_scan::get_printer_cache,
            smb_scan::save_printer_cache,
            // Multi-server management commands
            printer_api::add_extra_server,
            printer_api::list_extra_servers,
            printer_api::remove_extra_server,
            printer_api::scan_cached_server,
            printer_api::get_printer_cache_multi,
            shared_drive::connect_shared_drive,
            shared_drive::get_shared_drive_folders,
            shared_drive::open_shared_folder,
            shared_drive::disconnect_shared_drive,
            shared_drive::get_shared_drive_config,
            printer_api::connect_printer,
            printer_api::get_local_printer_list,
            printer_api::set_default_printer,
            printer_api::remove_printer,
            printer_api::open_printer_property,
            printer_api::open_printer_preference,
            printer_api::get_default_printer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
