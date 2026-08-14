mod color_extractor;
mod models;
mod runtime;
mod theme_store;
mod workbuddy;

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use models::{InstalledTheme, WorkBuddyStatus};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};

#[tauri::command]
async fn get_workbuddy_status(app: AppHandle) -> Result<WorkBuddyStatus, String> {
    tauri::async_runtime::spawn_blocking(move || runtime::status(&app))
        .await
        .map_err(|error| format!("状态检查任务异常结束: {error}"))?
}

#[tauri::command]
fn list_themes(app: AppHandle) -> Result<Vec<InstalledTheme>, String> {
    theme_store::list_installed_themes(&app)
}

#[tauri::command]
fn list_preset_themes(app: AppHandle) -> Result<Vec<InstalledTheme>, String> {
    theme_store::list_preset_themes(&app)
}

#[tauri::command]
async fn install_preset_theme(app: AppHandle, id: String) -> Result<InstalledTheme, String> {
    tauri::async_runtime::spawn_blocking(move || theme_store::install_preset_theme(&app, &id))
        .await
        .map_err(|error| format!("预置主题安装任务异常结束: {error}"))?
}

#[tauri::command]
async fn create_theme_from_image(
    app: AppHandle,
    path: String,
    name: String,
) -> Result<InstalledTheme, String> {
    tauri::async_runtime::spawn_blocking(move || {
        theme_store::create_theme_from_image(&app, Path::new(&path), name)
    })
    .await
    .map_err(|error| format!("图片主题生成任务异常结束: {error}"))?
}

#[tauri::command]
fn delete_theme(app: AppHandle, id: String) -> Result<(), String> {
    theme_store::remove_theme(&app, &id)
}

#[tauri::command]
async fn apply_theme(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || runtime::apply_theme(&app, &id))
        .await
        .map_err(|error| format!("主题任务异常结束: {error}"))?
}

#[tauri::command]
async fn restore_workbuddy(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || runtime::restore(&app))
        .await
        .map_err(|error| format!("恢复任务异常结束: {error}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Err(error) = theme_store::recover_theme_transactions(app.handle()) {
                log_runtime_error(app.handle(), "theme-recovery", &error);
            }
            let show = MenuItem::with_id(app, "show", "显示 Manager", true, None::<&str>)?;
            let restore = MenuItem::with_id(app, "restore", "恢复官方外观", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "完全退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &restore, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("app icon").clone())
                .icon_as_template(true)
                .tooltip("WorkBuddy Skin Manager")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "restore" => {
                        let app = app.clone();
                        thread::spawn(move || {
                            if let Err(error) = runtime::restore(&app) {
                                report_runtime_error(&app, error);
                            }
                        });
                    }
                    "quit" => {
                        let app = app.clone();
                        thread::spawn(move || match runtime::restore(&app) {
                            Ok(()) => app.exit(0),
                            Err(error) => report_runtime_error(&app, error),
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            show_main_window(app.handle());
            start_theme_monitor(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_workbuddy_status,
            list_themes,
            list_preset_themes,
            install_preset_theme,
            create_theme_from_image,
            delete_theme,
            apply_theme,
            restore_workbuddy
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn report_runtime_error(app: &AppHandle, error: String) {
    log_runtime_error(app, "user-action", &error);
    let _ = app.emit("runtime-error", error);
    show_main_window(app);
}

fn log_runtime_error(app: &AppHandle, context: &str, error: &str) {
    let Ok(log_dir) = app.path().app_log_dir() else {
        return;
    };
    if fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("runtime-errors.log"))
    else {
        return;
    };
    if file
        .metadata()
        .is_ok_and(|metadata| metadata.len() > 1024 * 1024)
    {
        let _ = file.set_len(0);
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let sanitized = error.replace(['\r', '\n'], " ");
    let _ = writeln!(file, "{timestamp}\t{context}\t{sanitized}");
}

fn start_theme_monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut last_error: Option<String> = None;
        loop {
            match runtime::maintain_active_theme(&app) {
                Ok(_) => last_error = None,
                Err(error) if last_error.as_deref() != Some(&error) => {
                    log_runtime_error(&app, "theme-monitor", &error);
                    let _ = app.emit("runtime-error", error.clone());
                    last_error = Some(error);
                }
                Err(_) => {}
            }
            thread::sleep(Duration::from_secs(60));
        }
    });
}
