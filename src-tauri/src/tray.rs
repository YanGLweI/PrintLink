//! 系统托盘模块
//! 关闭窗口时最小化到任务栏右下角通知区域，后台驻留运行

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

/// 托盘刷新事件名（发送到前端触发列表刷新）
pub const TRAY_REFRESH_EVENT: &str = "tray-refresh";

/// 初始化系统托盘图标及菜单
pub fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
    let refresh_item = MenuItemBuilder::with_id("refresh", "刷新打印机列表").build(app)?;
    let separator = MenuItemBuilder::with_id("sep", "─────────")
        .enabled(false)
        .build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "退出 PrintLink").build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[&show_item, &refresh_item, &separator, &quit_item])
        .build()?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().unwrap())
        .tooltip("PrintLink - 共享打印机管理客户端")
        .title("PrintLink")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                show_main_window(app);
            }
            "refresh" => {
                // 通知前端刷新打印机列表
                let _ = app.emit(TRAY_REFRESH_EVENT, ());
                log::info!("托盘菜单：触发列表刷新");
            }
            "quit" => {
                log::info!("托盘菜单：退出程序");
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 单击/双击托盘图标恢复主窗口
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    log::info!("系统托盘初始化完成");
    Ok(())
}

/// 显示并聚焦主窗口
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        log::info!("主窗口已恢复显示");
    }
}
