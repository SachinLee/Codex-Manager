use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::service_runtime::stop_service;

use super::prompts::confirm_discard_unsaved_settings_for_app_exit;
use super::state::{
    has_unsaved_settings_draft_sections, mark_skip_next_unsaved_settings_exit_confirm,
    APP_EXIT_REQUESTED, KEEP_ALIVE_FOR_LIGHTWEIGHT_CLOSE, TRAY_AVAILABLE,
};
use super::window::{request_show_main_window, toggle_tray_preview_window};

const TRAY_MENU_SHOW_MAIN: &str = "tray_show_main";
const TRAY_MENU_PRIMARY_RESET: &str = "tray_primary_reset";
const TRAY_MENU_SECONDARY_RESET: &str = "tray_secondary_reset";
const TRAY_MENU_QUIT_APP: &str = "tray_quit_app";

/// 函数 `setup_tray`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn setup_tray(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    TRAY_AVAILABLE.store(false, std::sync::atomic::Ordering::Relaxed);
    let menu = build_tray_menu(app)?;
    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_SHOW_MAIN => {
                if let Err(err) = request_show_main_window(app) {
                    log::warn!("request show main window from tray failed: {}", err);
                }
            }
            TRAY_MENU_QUIT_APP => {
                if has_unsaved_settings_draft_sections() {
                    if !confirm_discard_unsaved_settings_for_app_exit() {
                        log::info!("tray exit canceled because settings drafts are still unsaved");
                        return;
                    }
                    mark_skip_next_unsaved_settings_exit_confirm();
                }
                APP_EXIT_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
                KEEP_ALIVE_FOR_LIGHTWEIGHT_CLOSE.store(false, std::sync::atomic::Ordering::Relaxed);
                stop_service();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Right,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Err(err) = refresh_tray_menu(tray.app_handle()) {
                    log::warn!("refresh tray menu failed: {}", err);
                }
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                rect,
                ..
            } = event
            {
                toggle_tray_preview_window(tray.app_handle(), position, rect);
            }
        });
    if let Ok(icon) =
        tauri::image::Image::from_bytes(include_bytes!("../../icons/tray-template.png"))
    {
        tray = tray.icon(icon).icon_as_template(true);
    } else if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    TRAY_AVAILABLE.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

fn build_tray_menu(app: &tauri::AppHandle) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let show_main = MenuItem::with_id(app, TRAY_MENU_SHOW_MAIN, "显示主窗口", true, None::<&str>)?;
    let summary = codexmanager_service::read_tray_usage_reset_summary();
    let primary = MenuItem::with_id(
        app,
        TRAY_MENU_PRIMARY_RESET,
        format!(
            "5小时重置：{}",
            format_tray_reset_time(summary.primary_resets_at)
        ),
        false,
        None::<&str>,
    )?;
    let secondary = MenuItem::with_id(
        app,
        TRAY_MENU_SECONDARY_RESET,
        format!(
            "7天重置：{}",
            format_tray_reset_time(summary.secondary_resets_at)
        ),
        false,
        None::<&str>,
    )?;
    let first_separator = PredefinedMenuItem::separator(app)?;
    let second_separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, TRAY_MENU_QUIT_APP, "退出", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &show_main,
            &first_separator,
            &primary,
            &secondary,
            &second_separator,
            &quit,
        ],
    )
}

fn refresh_tray_menu(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return Ok(());
    };
    let menu = build_tray_menu(app)?;
    tray.set_menu(Some(menu))
}

fn format_tray_reset_time(value: Option<i64>) -> String {
    let Some(value) = value.filter(|item| *item > 0) else {
        return "暂无".to_string();
    };
    let Some(datetime) = chrono::DateTime::from_timestamp(value, 0) else {
        return "暂无".to_string();
    };
    datetime
        .with_timezone(&chrono::Local)
        .format("%m-%d %H:%M")
        .to_string()
}
