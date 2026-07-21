//! PrintLink - 共享打印机自动连接客户端
//! 模块导出 + Tauri 指令注册 + 窗口关闭拦截（最小化到托盘）

use tauri::Manager;

mod config;
mod credential;
mod printer_api;
mod smb_scan;
mod tray;
mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志系统
    utils::init_logger();

    tauri::Builder::default()
        // 单实例保护：第二次启动时聚焦已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            log::info!("检测到重复启动请求，已聚焦现有窗口");
        }))
        // 注册 opener 插件（用于打开外部浏览器链接）
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 初始化系统托盘
            tray::setup_tray(app.handle())?;
            log::info!("PrintLink 应用启动完成");
            Ok(())
        })
        // 拦截窗口关闭事件：隐藏到托盘而非退出
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
