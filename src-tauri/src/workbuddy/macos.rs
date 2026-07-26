use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

use super::Installation;

const DEFAULT_APP_PATH: &str = "/Applications/WorkBuddy.app";

pub fn find_installation() -> Option<Installation> {
    env::var_os("WORKBUDDY_PATH")
        .map(PathBuf::from)
        .and_then(installation_from_path)
        .or_else(|| installation_from_path(PathBuf::from(DEFAULT_APP_PATH)))
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join("Applications/WorkBuddy.app"))
                .and_then(installation_from_path)
        })
}

pub fn expected_path_display() -> String {
    env::var_os("WORKBUDDY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_APP_PATH))
        .to_string_lossy()
        .into_owned()
}

pub fn installed_version(installation: &Installation) -> Option<String> {
    let plist_path = installation.app_path.join("Contents/Info.plist");
    let value = plist::Value::from_file(plist_path).ok()?;
    value
        .as_dictionary()?
        .get("CFBundleShortVersionString")?
        .as_string()
        .map(ToString::to_string)
}

pub fn running(installation: &Installation) -> bool {
    !process_pids(installation).is_empty()
}

pub fn cdp_process(installation: &Installation, port: u16) -> Option<u32> {
    let port_argument = format!("--remote-debugging-port={port}");
    process_list().and_then(|processes| {
        process_from_list(
            &String::from_utf8_lossy(&processes.stdout),
            &installation.executable,
            Some(&port_argument),
        )
    })
}

pub fn stop(installation: &Installation) -> Result<(), String> {
    let _ = Command::new("/usr/bin/osascript")
        .args(["-e", "quit app \"WorkBuddy\""])
        .output();
    if wait_until(Duration::from_secs(3), || !running(installation)) {
        return Ok(());
    }
    signal_processes(installation, "-TERM");
    if wait_until(Duration::from_secs(3), || !running(installation)) {
        return Ok(());
    }
    signal_processes(installation, "-KILL");
    if wait_until(Duration::from_secs(2), || !running(installation)) {
        Ok(())
    } else {
        Err("无法停止正在运行的 WorkBuddy".to_string())
    }
}

pub fn start_with_cdp(installation: &Installation, port: u16) -> Result<(), String> {
    let output = Command::new("/usr/bin/open")
        .args([
            "-na",
            installation.app_path.to_string_lossy().as_ref(),
            "--args",
            "--remote-debugging-address=127.0.0.1",
        ])
        .arg(format!("--remote-debugging-port={port}"))
        .output()
        .map_err(|error| format!("无法启动 WorkBuddy CDP 模式: {error}"))?;
    command_result(output, "无法启动 WorkBuddy CDP 模式")
}

pub fn start_normal(installation: &Installation) -> Result<(), String> {
    let output = Command::new("/usr/bin/open")
        .arg(&installation.app_path)
        .output()
        .map_err(|error| format!("无法以普通模式启动 WorkBuddy: {error}"))?;
    command_result(output, "无法以普通模式启动 WorkBuddy")?;
    if wait_until(Duration::from_secs(15), || running(installation)) {
        Ok(())
    } else {
        Err("等待 WorkBuddy 普通模式启动超时".to_string())
    }
}

pub fn node_command(installation: &Installation) -> Command {
    Command::new(&installation.executable)
}

fn installation_from_path(app_path: PathBuf) -> Option<Installation> {
    let info = plist::Value::from_file(app_path.join("Contents/Info.plist")).ok()?;
    let executable_name = bundle_executable_name(&info)?;
    let executable = app_path.join("Contents/MacOS").join(executable_name);
    executable.is_file().then_some(Installation {
        app_path,
        executable,
    })
}

fn bundle_executable_name(info: &plist::Value) -> Option<&str> {
    info.as_dictionary()?.get("CFBundleExecutable")?.as_string()
}

fn process_list() -> Option<Output> {
    let output = Command::new("/bin/ps")
        .args(["-ax", "-o", "pid=", "-o", "command="])
        .output()
        .ok()?;
    output.status.success().then_some(output)
}

fn process_pids(installation: &Installation) -> Vec<u32> {
    let Some(output) = process_list() else {
        return Vec::new();
    };
    processes_from_list(
        &String::from_utf8_lossy(&output.stdout),
        &installation.executable,
        None,
    )
}

fn process_from_list(processes: &str, executable: &Path, argument: Option<&str>) -> Option<u32> {
    processes_from_list(processes, executable, argument)
        .into_iter()
        .next()
}

fn processes_from_list(processes: &str, executable: &Path, argument: Option<&str>) -> Vec<u32> {
    let executable = executable.to_string_lossy();
    processes
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let split = line.find(char::is_whitespace)?;
            let pid = line[..split].parse::<u32>().ok()?;
            let command = line[split..].trim_start();
            (command.starts_with(executable.as_ref())
                && argument
                    .is_none_or(|expected| command.split_whitespace().any(|part| part == expected)))
            .then_some(pid)
        })
        .collect()
}

fn signal_processes(installation: &Installation, signal: &str) {
    for pid in process_pids(installation) {
        let _ = Command::new("/bin/kill")
            .args([signal, &pid.to_string()])
            .output();
    }
}

fn command_result(output: Output, context: &str) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{bundle_executable_name, process_from_list};

    #[test]
    fn identifies_the_workbuddy_cdp_process() {
        let processes = "  41 /usr/bin/other --remote-debugging-port=49152\n\
                         73 /Applications/WorkBuddy.app/Contents/MacOS/Electron --remote-debugging-address=127.0.0.1 --remote-debugging-port=49152\n";
        let executable = Path::new("/Applications/WorkBuddy.app/Contents/MacOS/Electron");
        assert_eq!(
            process_from_list(processes, executable, Some("--remote-debugging-port=49152")),
            Some(73)
        );
        assert_eq!(
            process_from_list(processes, executable, Some("--remote-debugging-port=9222")),
            None
        );
    }

    #[test]
    fn reads_the_bundle_executable_from_info_plist() {
        let mut dictionary = plist::Dictionary::new();
        dictionary.insert(
            "CFBundleExecutable".to_string(),
            plist::Value::String("WorkBuddyLauncher".to_string()),
        );
        let info = plist::Value::Dictionary(dictionary);

        assert_eq!(bundle_executable_name(&info), Some("WorkBuddyLauncher"));
    }
}
