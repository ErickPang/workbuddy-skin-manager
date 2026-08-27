use std::{
    io::Read,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};

use crate::{
    models::{ManagerSettings, ManagerState, WorkBuddyStatus},
    theme_store::{self, load_settings, load_state, save_settings, save_state},
    workbuddy,
};

static RUNTIME_LOCK: Mutex<()> = Mutex::new(());
pub const RESTART_CONFIRMATION_REQUIRED: &str = "WORKBUDDY_RESTART_CONFIRMATION_REQUIRED";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaintenanceResult {
    Idle,
    Active,
    RestartRequired,
}

pub fn status(app: &AppHandle) -> Result<WorkBuddyStatus, String> {
    let _guard = RUNTIME_LOCK
        .lock()
        .map_err(|_| "主题运行锁异常".to_string())?;
    status_inner(app)
}

fn status_inner(app: &AppHandle) -> Result<WorkBuddyStatus, String> {
    let state = load_state(app)?;
    let settings = load_settings(app)?;
    let configured_path = settings.workbuddy_path.as_deref().map(Path::new);
    let installation = workbuddy::find_installation(configured_path);
    let version = installation.as_ref().and_then(workbuddy::installed_version);
    let manager_compatible = version
        .as_deref()
        .is_some_and(workbuddy::manager_supports_version);
    let running = installation.as_ref().is_some_and(workbuddy::running);
    let cdp_available = installation.as_ref().is_some_and(|installation| {
        state.cdp_port.is_some_and(|port| {
            workbuddy::cdp_available(port)
                && workbuddy::owns_cdp_session(installation, port, state.workbuddy_pid)
        })
    });
    let active_theme_id = cdp_available
        .then(|| state.active_theme_id.clone())
        .flatten();
    let restart_required = state.active_theme_id.is_some() && running && !cdp_available;

    Ok(WorkBuddyStatus {
        installed: installation.is_some(),
        running,
        app_path: installation
            .as_ref()
            .map(|installation| installation.app_path.to_string_lossy().into_owned())
            .or_else(|| settings.workbuddy_path.clone())
            .unwrap_or_else(workbuddy::expected_path_display),
        version,
        manager_compatible,
        cdp_available,
        cdp_port: state.cdp_port.filter(|_| cdp_available),
        active_theme_id,
        configured_theme_id: state.active_theme_id,
        restart_required,
        custom_path: settings.workbuddy_path.is_some(),
    })
}

pub fn set_workbuddy_path(app: &AppHandle, path: Option<&str>) -> Result<WorkBuddyStatus, String> {
    let _guard = RUNTIME_LOCK
        .lock()
        .map_err(|_| "主题运行锁异常".to_string())?;
    let state = load_state(app)?;
    let workbuddy_path = if let Some(path) = path {
        let canonical = std::fs::canonicalize(path)
            .map_err(|error| format!("无法读取所选 WorkBuddy: {error}"))?;
        let installation = workbuddy::installation_from_path(canonical)
            .ok_or_else(|| "所选位置不是有效的 WorkBuddy 应用".to_string())?;
        workbuddy::installed_version(&installation)
            .ok_or_else(|| "无法从所选位置读取 WorkBuddy 版本".to_string())?;
        if state.active_theme_id.is_some()
            && state.cdp_port.is_some_and(|port| {
                workbuddy::cdp_available(port)
                    && !workbuddy::owns_cdp_session(&installation, port, state.workbuddy_pid)
            })
        {
            return Err("所选 WorkBuddy 不是当前主题会话对应的应用，不能用于恢复".to_string());
        }
        Some(installation.app_path.to_string_lossy().into_owned())
    } else {
        if state.active_theme_id.is_some() {
            return Err("仍有活动主题，不能清除安装位置；请重新选择当前 WorkBuddy".to_string());
        }
        None
    };
    save_settings(
        app,
        &ManagerSettings {
            workbuddy_path,
            ..ManagerSettings::default()
        },
    )?;
    status_inner(app)
}

fn find_installation(app: &AppHandle) -> Result<Option<workbuddy::Installation>, String> {
    let settings = load_settings(app)?;
    Ok(workbuddy::find_installation(
        settings.workbuddy_path.as_deref().map(Path::new),
    ))
}

pub fn apply_theme(app: &AppHandle, id: &str, restart_confirmed: bool) -> Result<(), String> {
    let _guard = RUNTIME_LOCK
        .lock()
        .map_err(|_| "主题运行锁异常".to_string())?;
    apply_theme_inner(app, id, restart_confirmed)
}

fn apply_theme_inner(app: &AppHandle, id: &str, restart_confirmed: bool) -> Result<(), String> {
    let theme_dir = theme_store::theme_directory(app, id)?;
    if !theme_dir.join("manifest.json").exists() {
        return Err(format!("主题未安装: {id}"));
    }
    let installation = find_installation(app)?.ok_or_else(|| {
        format!(
            "没有检测到 WorkBuddy，请确认安装位置: {}",
            workbuddy::expected_path_display()
        )
    })?;
    let installed_theme = theme_store::read_installed_theme(&theme_dir)?;
    let version = workbuddy::installed_version(&installation)
        .ok_or_else(|| "无法读取 WorkBuddy 版本".to_string())?;
    if !workbuddy::manager_supports_version(&version) {
        return Err(format!(
            "Manager 尚未验证 WorkBuddy {version}，当前支持范围: {}",
            workbuddy::supported_versions_display()
        ));
    }
    if !workbuddy::matches_compatibility(
        &version,
        &installed_theme.manifest.compatibility.workbuddy,
    ) {
        return Err(format!(
            "主题不兼容当前 WorkBuddy {version}，支持范围: {}",
            installed_theme.manifest.compatibility.workbuddy.join(", ")
        ));
    }

    let was_running = workbuddy::running(&installation);
    require_restart_confirmation(was_running, restart_confirmed)?;
    let port = available_cdp_port()?;
    if was_running {
        workbuddy::stop(&installation)?;
    }
    let result = (|| {
        save_state(
            app,
            &ManagerState {
                active_theme_id: Some(id.to_string()),
                cdp_port: Some(port),
                workbuddy_pid: None,
                ..ManagerState::default()
            },
        )?;
        workbuddy::start_with_cdp(&installation, port)?;
        let pid = wait_for_cdp(&installation, port, Duration::from_secs(30))?;
        run_engine(app, &installation, "apply", Some(&theme_dir), port)?;
        save_state(
            app,
            &ManagerState {
                active_theme_id: Some(id.to_string()),
                cdp_port: Some(port),
                workbuddy_pid: Some(pid),
                ..ManagerState::default()
            },
        )
    })();

    if let Err(error) = result {
        let rollback = rollback_failed_apply(app, &installation, was_running);
        return Err(match rollback {
            Ok(()) => format!("{error}；已恢复 WorkBuddy 普通启动模式"),
            Err(rollback_error) => format!("{error}；自动恢复失败: {rollback_error}"),
        });
    }
    Ok(())
}

pub fn restore(app: &AppHandle, restart_confirmed: bool) -> Result<(), String> {
    let _guard = RUNTIME_LOCK
        .lock()
        .map_err(|_| "主题运行锁异常".to_string())?;
    restore_inner(app, restart_confirmed)
}

fn restore_inner(app: &AppHandle, restart_confirmed: bool) -> Result<(), String> {
    let state = load_state(app)?;
    let installation = find_installation(app)?;
    require_installation_for_active_theme(state.active_theme_id.is_some(), installation.is_some())?;
    let managed_cdp = installation.as_ref().is_some_and(|installation| {
        state.cdp_port.is_some_and(|port| {
            workbuddy::cdp_available(port)
                && workbuddy::owns_cdp_session(installation, port, state.workbuddy_pid)
        })
    });
    if state.active_theme_id.is_none() && !managed_cdp {
        return Ok(());
    }
    if let Some(installation) = installation.filter(workbuddy::running) {
        require_restart_confirmation(true, restart_confirmed)?;
        workbuddy::stop(&installation)?;
        workbuddy::start_normal(&installation)?;
    }
    save_state(app, &ManagerState::default())
}

pub fn maintain_active_theme(app: &AppHandle) -> Result<MaintenanceResult, String> {
    let _guard = RUNTIME_LOCK
        .lock()
        .map_err(|_| "主题运行锁异常".to_string())?;
    let state = load_state(app)?;
    let Some(id) = state.active_theme_id else {
        return Ok(MaintenanceResult::Idle);
    };
    let installation = find_installation(app)?;
    require_installation_for_active_theme(true, installation.is_some())?;
    let installation = installation.ok_or_else(active_installation_error)?;
    let theme_dir = theme_store::theme_directory(app, &id)?;
    theme_store::read_installed_theme(&theme_dir)?;
    if let Some(port) = state.cdp_port.filter(|port| {
        workbuddy::cdp_available(*port)
            && workbuddy::owns_cdp_session(&installation, *port, state.workbuddy_pid)
    }) {
        if run_engine(app, &installation, "check", Some(&theme_dir), port).is_err() {
            run_engine(app, &installation, "apply", Some(&theme_dir), port)?;
        }
        return Ok(MaintenanceResult::Active);
    }
    if workbuddy::running(&installation) {
        return Ok(MaintenanceResult::RestartRequired);
    }
    Ok(MaintenanceResult::Idle)
}

fn wait_for_cdp(
    installation: &workbuddy::Installation,
    port: u16,
    timeout: Duration,
) -> Result<u32, String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if workbuddy::cdp_available(port) {
            if let Some(pid) = workbuddy::cdp_process(installation, port) {
                return Ok(pid);
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err("等待 WorkBuddy CDP 就绪超时".to_string())
}

fn require_restart_confirmation(required: bool, confirmed: bool) -> Result<(), String> {
    if required && !confirmed {
        Err(RESTART_CONFIRMATION_REQUIRED.to_string())
    } else {
        Ok(())
    }
}

fn require_installation_for_active_theme(
    active: bool,
    installation_found: bool,
) -> Result<(), String> {
    if active && !installation_found {
        Err(active_installation_error())
    } else {
        Ok(())
    }
}

fn active_installation_error() -> String {
    "活动主题仍在记录中，但找不到对应的 WorkBuddy。请重新选择原 WorkBuddy 安装位置后再恢复，Manager 已保留恢复状态。".to_string()
}

fn run_engine(
    app: &AppHandle,
    installation: &workbuddy::Installation,
    command: &str,
    theme_dir: Option<&Path>,
    port: u16,
) -> Result<Output, String> {
    let engine_dir = engine_directory(app)?;
    let runner = engine_dir.join("runner.js");
    if !runner.exists() {
        return Err(format!("Manager CDP engine 不完整: {}", runner.display()));
    }

    let mut process = workbuddy::node_command(installation);
    process
        .env("ELECTRON_RUN_AS_NODE", "1")
        .arg(runner)
        .arg(command);
    if let Some(directory) = theme_dir {
        process.arg(directory);
    } else {
        process.arg("");
    }
    process.arg(port.to_string());
    let timeout = match command {
        "apply" => Duration::from_secs(60),
        "check" => Duration::from_secs(15),
        _ => Duration::from_secs(20),
    };
    let output = command_output_with_timeout(&mut process, timeout)?;
    if !output.status.success() {
        let detail = engine_error_detail(&output.stderr);
        return Err(if detail.is_empty() {
            format!("Manager CDP engine 执行失败: {}", output.status)
        } else {
            detail
        });
    }
    Ok(output)
}

fn command_output_with_timeout(process: &mut Command, timeout: Duration) -> Result<Output, String> {
    let mut child = process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法运行 Manager CDP engine: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取 Manager CDP engine 输出".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取 Manager CDP engine 错误输出".to_string())?;
    let stdout_reader = thread::spawn(move || read_process_output(stdout));
    let stderr_reader = thread::spawn(move || read_process_output(stderr));
    let deadline = Instant::now() + timeout;

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "Manager CDP engine 执行超时（{} 秒）",
                    timeout.as_secs()
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("无法等待 Manager CDP engine: {error}"));
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Manager CDP engine 输出读取任务异常结束".to_string())??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Manager CDP engine 错误读取任务异常结束".to_string())??;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_process_output(mut pipe: impl Read) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    pipe.read_to_end(&mut output)
        .map_err(|error| format!("无法读取 Manager CDP engine 输出: {error}"))?;
    Ok(output)
}

fn engine_error_detail(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter(|line| {
            !line.contains("electron/shell/common/mac/codesign_util.cc")
                && !line.contains("SecCodeCheckValidity")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn available_cdp_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("无法分配本机 CDP 端口: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("无法读取本机 CDP 端口: {error}"))
}

fn rollback_failed_apply(
    app: &AppHandle,
    installation: &workbuddy::Installation,
    was_running: bool,
) -> Result<(), String> {
    let runtime_result = (|| {
        if workbuddy::running(installation) {
            workbuddy::stop(installation)?;
        }
        if was_running {
            workbuddy::start_normal(installation)?;
        }
        Ok(())
    })();
    let state_result = save_state(app, &ManagerState::default());
    runtime_result.and(state_result)
}

fn engine_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法定位 Manager resources: {error}"))?;
    for bundled in [
        resource_dir.join("theme-engine"),
        resource_dir.join("resources/theme-engine"),
    ] {
        if bundled.exists() {
            return Ok(bundled);
        }
    }
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/theme-engine");
    if development.exists() {
        return Ok(development);
    }
    Err("找不到 Manager CDP engine".to_string())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{
        engine_error_detail, require_installation_for_active_theme, require_restart_confirmation,
        RESTART_CONFIRMATION_REQUIRED,
    };

    #[cfg(target_os = "macos")]
    #[test]
    fn terminates_a_timed_out_engine_process() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "exec /bin/sleep 5"]);
        let started = std::time::Instant::now();

        let error =
            super::command_output_with_timeout(&mut command, std::time::Duration::from_millis(100))
                .expect_err("process should time out");

        assert!(error.contains("执行超时"));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn requires_confirmation_before_restarting_workbuddy() {
        assert_eq!(
            require_restart_confirmation(true, false),
            Err(RESTART_CONFIRMATION_REQUIRED.to_string())
        );
        assert!(require_restart_confirmation(true, true).is_ok());
        assert!(require_restart_confirmation(false, false).is_ok());
    }

    #[test]
    fn preserves_active_state_when_the_installation_cannot_be_found() {
        let error = require_installation_for_active_theme(true, false)
            .expect_err("active theme without installation must not be treated as restored");
        assert!(error.contains("保留恢复状态"));
        assert!(require_installation_for_active_theme(false, false).is_ok());
    }

    #[test]
    fn embedded_engine_has_a_commonjs_package_scope() {
        let package_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/theme-engine/package.json");
        let package: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(package_path).expect("theme engine package.json should exist"),
        )
        .expect("theme engine package.json should be valid JSON");
        assert_eq!(
            package.get("type").and_then(|value| value.as_str()),
            Some("commonjs")
        );
    }

    #[test]
    fn removes_electron_codesign_noise_from_engine_errors() {
        let stderr = b"[0726/140942:ERROR:electron/shell/common/mac/codesign_util.cc:109] \
SecCodeCheckValidity: Error Domain=NSOSStatusErrorDomain Code=-67062\n\
theme component verification failed: sidebar\n";

        assert_eq!(
            engine_error_detail(stderr),
            "theme component verification failed: sidebar"
        );
    }
}
