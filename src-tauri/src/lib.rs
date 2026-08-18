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

use models::{DiagnosticInfo, InstalledTheme, ThemeLibrary, ThemeLibraryBackup, WorkBuddyStatus};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const MAX_RUNTIME_LOG_BYTES: u64 = 1024 * 1024;
const RECENT_RUNTIME_ERROR_LIMIT: usize = 20;

#[tauri::command]
async fn get_workbuddy_status(app: AppHandle) -> Result<WorkBuddyStatus, String> {
    tauri::async_runtime::spawn_blocking(move || runtime::status(&app))
        .await
        .map_err(|error| format!("状态检查任务异常结束: {error}"))?
}

#[tauri::command]
async fn set_workbuddy_path(
    app: AppHandle,
    path: Option<String>,
) -> Result<WorkBuddyStatus, String> {
    tauri::async_runtime::spawn_blocking(move || runtime::set_workbuddy_path(&app, path.as_deref()))
        .await
        .map_err(|error| format!("安装位置设置任务异常结束: {error}"))?
}

#[tauri::command]
async fn get_diagnostics(app: AppHandle) -> Result<DiagnosticInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let log_path = app
            .path()
            .app_log_dir()
            .map_err(|error| format!("无法定位日志目录: {error}"))?
            .join("runtime-errors.log");
        let (workbuddy, status_error) = match runtime::status(&app) {
            Ok(status) => (Some(status), None),
            Err(error) => (None, Some(error)),
        };
        Ok(DiagnosticInfo {
            manager_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
            recent_errors: read_recent_runtime_errors(&log_path, RECENT_RUNTIME_ERROR_LIMIT),
            log_path: log_path.to_string_lossy().into_owned(),
            workbuddy,
            status_error,
        })
    })
    .await
    .map_err(|error| format!("诊断任务异常结束: {error}"))?
}

#[tauri::command]
fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| format!("无法读取开机启动状态: {error}"))
}

#[tauri::command]
fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch()
            .enable()
            .map_err(|error| format!("无法启用开机启动: {error}"))
    } else {
        app.autolaunch()
            .disable()
            .map_err(|error| format!("无法关闭开机启动: {error}"))
    }
}

#[tauri::command]
async fn list_themes(app: AppHandle) -> Result<ThemeLibrary, String> {
    tauri::async_runtime::spawn_blocking(move || theme_store::list_installed_themes(&app))
        .await
        .map_err(|error| format!("主题库扫描任务异常结束: {error}"))?
}

#[tauri::command]
async fn list_preset_themes(app: AppHandle) -> Result<Vec<InstalledTheme>, String> {
    tauri::async_runtime::spawn_blocking(move || theme_store::list_preset_themes(&app))
        .await
        .map_err(|error| format!("预置主题扫描任务异常结束: {error}"))?
}

#[tauri::command]
async fn install_preset_theme(
    app: AppHandle,
    id: String,
    overwrite_confirmed: bool,
) -> Result<InstalledTheme, String> {
    tauri::async_runtime::spawn_blocking(move || {
        theme_store::install_preset_theme(&app, &id, overwrite_confirmed)
    })
    .await
    .map_err(|error| format!("预置主题安装任务异常结束: {error}"))?
}

#[tauri::command]
async fn import_theme_package(
    app: AppHandle,
    path: String,
    overwrite_confirmed: bool,
) -> Result<InstalledTheme, String> {
    tauri::async_runtime::spawn_blocking(move || {
        theme_store::import_package(&app, Path::new(&path), overwrite_confirmed)
    })
    .await
    .map_err(|error| format!("主题包导入任务异常结束: {error}"))?
}

#[tauri::command]
async fn export_theme_package(app: AppHandle, id: String, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        theme_store::export_package(&app, &id, Path::new(&path))
    })
    .await
    .map_err(|error| format!("主题包导出任务异常结束: {error}"))?
}

#[tauri::command]
async fn export_theme_library(
    app: AppHandle,
    directory: String,
) -> Result<ThemeLibraryBackup, String> {
    tauri::async_runtime::spawn_blocking(move || {
        theme_store::export_theme_library(&app, Path::new(&directory))
    })
    .await
    .map_err(|error| format!("主题库备份任务异常结束: {error}"))?
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
async fn delete_theme(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || theme_store::remove_theme(&app, &id))
        .await
        .map_err(|error| format!("主题删除任务异常结束: {error}"))?
}

#[tauri::command]
async fn apply_theme(app: AppHandle, id: String, restart_confirmed: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || runtime::apply_theme(&app, &id, restart_confirmed))
        .await
        .map_err(|error| format!("主题任务异常结束: {error}"))?
}

#[tauri::command]
async fn restore_workbuddy(app: AppHandle, restart_confirmed: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || runtime::restore(&app, restart_confirmed))
        .await
        .map_err(|error| format!("恢复任务异常结束: {error}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let start_hidden = std::env::args().any(|argument| argument == "--hidden");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|argument| argument == "--hidden") {
                show_main_window(app);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            app.handle().plugin(tauri_plugin_autostart::init(
                MacosLauncher::LaunchAgent,
                Some(vec!["--hidden"]),
            ))?;
            if let Err(error) = theme_store::recover_theme_transactions(app.handle()) {
                log_runtime_error(app.handle(), "theme-recovery", &error);
            }
            let show = MenuItem::with_id(app, "show", "显示 Manager", true, None::<&str>)?;
            let restore = MenuItem::with_id(
                app,
                "restore",
                "恢复官方外观（会重启 WorkBuddy）",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(
                app,
                "quit",
                "完全退出（会恢复并重启 WorkBuddy）",
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&show, &restore, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("app icon").clone())
                .icon_as_template(true)
                .tooltip("WorkBuddy Theme Manager")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "restore" => confirm_tray_action(app, false),
                    "quit" => confirm_tray_action(app, true),
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

            if !start_hidden {
                show_main_window(app.handle());
            }
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
            set_workbuddy_path,
            get_diagnostics,
            get_autostart_enabled,
            set_autostart_enabled,
            list_themes,
            list_preset_themes,
            install_preset_theme,
            import_theme_package,
            export_theme_package,
            export_theme_library,
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

fn confirm_tray_action(app: &AppHandle, quit: bool) {
    let app = app.clone();
    let message = if quit {
        "完全退出会停止主题守护；如有活动主题，还会关闭并重新启动 WorkBuddy。请先保存正在进行的工作。"
    } else {
        "恢复官方外观会关闭并重新启动 WorkBuddy。请先保存正在进行的工作。"
    };
    app.dialog()
        .message(message)
        .title(if quit {
            "确认完全退出"
        } else {
            "确认恢复"
        })
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "继续".to_string(),
            "取消".to_string(),
        ))
        .show(move |confirmed| {
            if !confirmed {
                return;
            }
            thread::spawn(move || match runtime::restore(&app, true) {
                Ok(()) if quit => app.exit(0),
                Ok(()) => {}
                Err(error) => report_runtime_error(&app, error),
            });
        });
}

pub(crate) fn log_runtime_error(app: &AppHandle, context: &str, error: &str) {
    let Ok(log_dir) = app.path().app_log_dir() else {
        return;
    };
    if fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let log_path = log_dir.join("runtime-errors.log");
    if rotate_runtime_log(&log_path, MAX_RUNTIME_LOG_BYTES).is_err() {
        return;
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) else {
        return;
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let sanitized = error.replace(['\r', '\n'], " ");
    let entry = serde_json::json!({
        "timestamp": timestamp,
        "level": "error",
        "context": context,
        "message": sanitized,
    });
    let _ = writeln!(file, "{entry}");
}

fn rotate_runtime_log(path: &Path, max_bytes: u64) -> Result<(), String> {
    if !path
        .metadata()
        .is_ok_and(|metadata| metadata.len() > max_bytes)
    {
        return Ok(());
    }
    let backup = path.with_extension("log.1");
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| format!("无法清理旧日志备份: {error}"))?;
    }
    fs::rename(path, backup).map_err(|error| format!("无法轮转运行日志: {error}"))
}

fn read_recent_runtime_errors(path: &Path, limit: usize) -> Vec<String> {
    let backup = path.with_extension("log.1");
    let mut lines = [backup.as_path(), path]
        .into_iter()
        .filter_map(|candidate| fs::read_to_string(candidate).ok())
        .flat_map(|content| {
            content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if lines.len() > limit {
        lines.drain(..lines.len() - limit);
    }
    lines
}

fn start_theme_monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut last_error: Option<String> = None;
        let mut restart_notice_sent = false;
        loop {
            let delay = match runtime::maintain_active_theme(&app) {
                Ok(runtime::MaintenanceResult::RestartRequired) => {
                    last_error = None;
                    if !restart_notice_sent {
                        let _ = app.emit("theme-restart-required", ());
                        restart_notice_sent = true;
                    }
                    Duration::from_secs(60)
                }
                Ok(runtime::MaintenanceResult::Active) => {
                    last_error = None;
                    restart_notice_sent = false;
                    Duration::from_secs(15)
                }
                Ok(runtime::MaintenanceResult::Idle) => {
                    last_error = None;
                    restart_notice_sent = false;
                    Duration::from_secs(60)
                }
                Err(error) if last_error.as_deref() != Some(&error) => {
                    log_runtime_error(&app, "theme-monitor", &error);
                    let _ = app.emit("runtime-error", error.clone());
                    last_error = Some(error);
                    Duration::from_secs(60)
                }
                Err(_) => Duration::from_secs(60),
            };
            thread::sleep(delay);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{read_recent_runtime_errors, rotate_runtime_log};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn rotates_runtime_logs_and_keeps_recent_entries() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("workbuddy-runtime-log-{nonce}"));
        fs::create_dir_all(&root).expect("create log test directory");
        let path = root.join("runtime-errors.log");
        fs::write(&path, "first\nsecond\n").expect("write runtime log");

        rotate_runtime_log(&path, 1).expect("rotate oversized log");
        fs::write(&path, "third\n").expect("write current runtime log");

        assert_eq!(
            read_recent_runtime_errors(&path, 2),
            vec!["second".to_string(), "third".to_string()]
        );
        fs::remove_dir_all(root).expect("remove log test directory");
    }

    #[test]
    fn asset_protocol_allows_packaged_preset_images() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("tauri.conf.json should be valid JSON");
        let scope = config
            .pointer("/app/security/assetProtocol/scope")
            .and_then(serde_json::Value::as_array)
            .expect("asset protocol scope should be configured");
        let allowed = scope
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();

        for path in [
            "$RESOURCE/preset-themes/**/*.png",
            "$RESOURCE/resources/preset-themes/**/*.png",
        ] {
            assert!(
                allowed.contains(&path),
                "missing preset asset scope: {path}"
            );
        }
    }
}
