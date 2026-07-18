use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};

use crate::{
    models::{ManagerState, WorkBuddyStatus},
    theme_store::{self, load_state, save_state},
    workbuddy,
};

static RUNTIME_LOCK: Mutex<()> = Mutex::new(());

pub fn status(app: &AppHandle) -> Result<WorkBuddyStatus, String> {
    let _guard = RUNTIME_LOCK
        .lock()
        .map_err(|_| "主题运行锁异常".to_string())?;
    let state = load_state(app)?;
    let cdp_available = state.cdp_port.is_some_and(|port| {
        workbuddy::cdp_available(port) && workbuddy::owns_cdp_session(port, state.workbuddy_pid)
    });
    let active_theme_id = match (&state.active_theme_id, state.cdp_port) {
        (Some(id), Some(port)) if cdp_available => {
            let theme_dir = theme_store::theme_directory(app, id)?;
            run_engine(app, "check", Some(&theme_dir), port)
                .is_ok()
                .then(|| id.clone())
        }
        _ => None,
    };

    Ok(WorkBuddyStatus {
        installed: Path::new(workbuddy::WORKBUDDY_PATH).exists(),
        app_path: workbuddy::WORKBUDDY_PATH.to_string(),
        version: workbuddy::installed_version(),
        cdp_available,
        cdp_port: state.cdp_port.filter(|_| cdp_available),
        active_theme_id,
        configured_theme_id: state.active_theme_id,
    })
}

pub fn apply_theme(app: &AppHandle, id: &str) -> Result<(), String> {
    let _guard = RUNTIME_LOCK
        .lock()
        .map_err(|_| "主题运行锁异常".to_string())?;
    apply_theme_inner(app, id)
}

fn apply_theme_inner(app: &AppHandle, id: &str) -> Result<(), String> {
    let theme_dir = theme_store::theme_directory(app, id)?;
    if !theme_dir.join("manifest.json").exists() {
        return Err(format!("主题未安装: {id}"));
    }
    if !Path::new(workbuddy::WORKBUDDY_ELECTRON).exists() {
        return Err("没有检测到 /Applications/WorkBuddy.app".to_string());
    }
    let installed_theme = theme_store::read_installed_theme(&theme_dir)?;
    let version =
        workbuddy::installed_version().ok_or_else(|| "无法读取 WorkBuddy 版本".to_string())?;
    if !workbuddy::matches_compatibility(
        &version,
        &installed_theme.manifest.compatibility.workbuddy,
    ) {
        return Err(format!(
            "主题不兼容当前 WorkBuddy {version}，支持范围: {}",
            installed_theme.manifest.compatibility.workbuddy.join(", ")
        ));
    }

    let was_running = workbuddy::running();
    let port = available_cdp_port()?;
    let result = (|| {
        save_state(
            app,
            &ManagerState {
                active_theme_id: Some(id.to_string()),
                cdp_port: Some(port),
                workbuddy_pid: None,
            },
        )?;
        if was_running {
            stop_workbuddy()?;
        }
        start_workbuddy_with_cdp(port)?;
        let pid = wait_for_cdp(port, Duration::from_secs(30))?;
        run_engine(app, "apply", Some(&theme_dir), port)?;
        save_state(
            app,
            &ManagerState {
                active_theme_id: Some(id.to_string()),
                cdp_port: Some(port),
                workbuddy_pid: Some(pid),
            },
        )
    })();

    if let Err(error) = result {
        let rollback = rollback_failed_apply(app, was_running);
        return Err(match rollback {
            Ok(()) => format!("{error}；已恢复 WorkBuddy 普通启动模式"),
            Err(rollback_error) => format!("{error}；自动恢复失败: {rollback_error}"),
        });
    }
    Ok(())
}

pub fn restore(app: &AppHandle) -> Result<(), String> {
    let _guard = RUNTIME_LOCK
        .lock()
        .map_err(|_| "主题运行锁异常".to_string())?;
    restore_inner(app)
}

fn restore_inner(app: &AppHandle) -> Result<(), String> {
    let state = load_state(app)?;
    let managed_cdp = state.cdp_port.is_some_and(|port| {
        workbuddy::cdp_available(port) && workbuddy::owns_cdp_session(port, state.workbuddy_pid)
    });
    if state.active_theme_id.is_none() && !managed_cdp {
        return Ok(());
    }
    let was_running = workbuddy::running();
    if was_running {
        stop_workbuddy()?;
        start_workbuddy_normal()?;
    }
    save_state(app, &ManagerState::default())
}

pub fn maintain_active_theme(app: &AppHandle) -> Result<bool, String> {
    let _guard = RUNTIME_LOCK
        .lock()
        .map_err(|_| "主题运行锁异常".to_string())?;
    let state = load_state(app)?;
    let Some(id) = state.active_theme_id else {
        return Ok(false);
    };
    let theme_dir = theme_store::theme_directory(app, &id)?;
    if let Some(port) = state.cdp_port.filter(|port| {
        workbuddy::cdp_available(*port) && workbuddy::owns_cdp_session(*port, state.workbuddy_pid)
    }) {
        if run_engine(app, "check", Some(&theme_dir), port).is_err() {
            run_engine(app, "apply", Some(&theme_dir), port)?;
        }
        return Ok(true);
    }
    if workbuddy::running() {
        apply_theme_inner(app, &id)?;
        return Ok(true);
    }
    Ok(false)
}

fn stop_workbuddy() -> Result<(), String> {
    let _ = Command::new("/usr/bin/osascript")
        .args(["-e", "quit app \"WorkBuddy\""])
        .output();
    if wait_until(Duration::from_secs(3), || !workbuddy::running()) {
        return Ok(());
    }
    let _ = Command::new("/usr/bin/pkill")
        .args(["-f", "WorkBuddy.app/Contents/MacOS/Electron"])
        .output();
    if wait_until(Duration::from_secs(5), || !workbuddy::running()) {
        Ok(())
    } else {
        Err("无法停止正在运行的 WorkBuddy".to_string())
    }
}

fn start_workbuddy_with_cdp(port: u16) -> Result<(), String> {
    let output = Command::new("/usr/bin/open")
        .args([
            "-na",
            workbuddy::WORKBUDDY_PATH,
            "--args",
            "--remote-debugging-address=127.0.0.1",
        ])
        .arg(format!("--remote-debugging-port={port}"))
        .output()
        .map_err(|error| format!("无法启动 WorkBuddy CDP 模式: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "无法启动 WorkBuddy CDP 模式: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn wait_for_cdp(port: u16, timeout: Duration) -> Result<u32, String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if workbuddy::cdp_available(port) {
            if let Some(pid) = workbuddy::cdp_process(port) {
                return Ok(pid);
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err("等待 WorkBuddy CDP 就绪超时".to_string())
}

fn run_engine(
    app: &AppHandle,
    command: &str,
    theme_dir: Option<&Path>,
    port: u16,
) -> Result<Output, String> {
    let engine_dir = engine_directory(app)?;
    let runner = engine_dir.join("runner.js");
    if !runner.exists() {
        return Err(format!("Manager CDP engine 不完整: {}", runner.display()));
    }

    let mut process = Command::new(workbuddy::WORKBUDDY_ELECTRON);
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
    let output = process
        .output()
        .map_err(|error| format!("无法运行 Manager CDP engine: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("Manager CDP engine 执行失败: {}", output.status)
        } else {
            detail
        });
    }
    Ok(output)
}

fn available_cdp_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("无法分配本机 CDP 端口: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("无法读取本机 CDP 端口: {error}"))
}

fn start_workbuddy_normal() -> Result<(), String> {
    let output = Command::new("/usr/bin/open")
        .arg(workbuddy::WORKBUDDY_PATH)
        .output()
        .map_err(|error| format!("无法以普通模式启动 WorkBuddy: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "无法以普通模式启动 WorkBuddy: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if wait_until(Duration::from_secs(15), workbuddy::running) {
        Ok(())
    } else {
        Err("等待 WorkBuddy 普通模式启动超时".to_string())
    }
}

fn rollback_failed_apply(app: &AppHandle, was_running: bool) -> Result<(), String> {
    let runtime_result = (|| {
        if workbuddy::running() {
            stop_workbuddy()?;
        }
        if was_running {
            start_workbuddy_normal()?;
        }
        Ok(())
    })();
    let state_result = save_state(app, &ManagerState::default());
    runtime_result.and(state_result)
}

fn wait_until(timeout: Duration, condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(250));
    }
    condition()
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
}
