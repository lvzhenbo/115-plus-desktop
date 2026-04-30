//! 系统托盘。
//!
//! # 交互
//!
//! | 操作 | 行为 |
//! |------|------|
//! | 左键单击图标 | 显示并聚焦主窗口 |
//! | 右键菜单 — 显示主窗口 | 显示并聚焦主窗口 |
//! | 右键菜单 — 隐藏主窗口 | 隐藏到托盘，后台继续运行 |
//! | 右键菜单 — 退出 | 通知前端确认 → 暂停任务 → 退出进程 |
//!
//! 真正的退出逻辑在前端 `tray-quit` 事件监听器中完成（任务暂停、确认弹窗），
//! 最后由前端调用 `exit(0)`，Rust 侧 `ExitRequested` 放行。

use tauri::{
    AppHandle, Emitter, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};

use crate::show_window;

/// 在应用启动时创建系统托盘图标与菜单。
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let hide_item = MenuItem::with_id(app, "hide", "隐藏主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &hide_item, &quit_item])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().unwrap())
        .menu(&menu)
        .tooltip("115+")
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_tray_icon_event)
        .build(app)?;

    Ok(())
}

/// 托盘右键菜单回调。
fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id.as_ref() {
        "show" => {
            let _ = show_window(app);
        }
        "hide" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
        }
        "quit" => {
            // 先显示窗口，再通知前端走确认 → 暂停 → exit(0) 流程
            let _ = show_window(app);
            let _ = app.emit("tray-quit", ());
        }
        _ => {}
    }
}

/// 托盘图标鼠标事件回调：左键单击时切换窗口显隐。
fn handle_tray_icon_event(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        let _ = show_window(tray.app_handle());
    }
}
