use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

use super::Installation;

const DEFAULT_APP_PATH: &str = "/Applications/WorkBuddy.app";
const WORKBUDDY_BUNDLE_IDENTIFIER: &str = "com.workbuddy.workbuddy";

pub fn find_installation(configured_path: Option<&Path>) -> Option<Installation> {
    if let Some(path) = configured_path {
        return installation_from_path(path.to_path_buf());
    }
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
    let pid = process_list().and_then(|processes| {
        process_from_list(
            &String::from_utf8_lossy(&processes.stdout),
            &installation.executable,
            Some(&port_argument),
        )
    })?;
    owns_ipv4_listener(pid, port).then_some(pid)
}

pub fn stop(installation: &Installation) -> Result<(), String> {
    let _ = Command::new("/usr/bin/osascript")
        .args(["-e", "quit app \"WorkBuddy\""])
        .output();
    if wait_until(Duration::from_secs(15), || !running(installation)) {
        return Ok(());
    }
    Err(
        "WorkBuddy 未能正常退出。请保存工作并手动退出 WorkBuddy 后重试；Manager 不会强制终止它。"
            .to_string(),
    )
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

pub fn installation_from_path(app_path: PathBuf) -> Option<Installation> {
    if !app_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("WorkBuddy.app"))
    {
        return None;
    }
    let info = plist::Value::from_file(app_path.join("Contents/Info.plist")).ok()?;
    if !has_workbuddy_bundle_identifier(&info) {
        return None;
    }
    let executable_name = bundle_executable_name(&info)?;
    let executable = app_path.join("Contents/MacOS").join(executable_name);
    executable.is_file().then_some(Installation {
        app_path,
        executable,
    })
}

fn has_workbuddy_bundle_identifier(info: &plist::Value) -> bool {
    info.as_dictionary()
        .and_then(|dictionary| dictionary.get("CFBundleIdentifier"))
        .and_then(plist::Value::as_string)
        == Some(WORKBUDDY_BUNDLE_IDENTIFIER)
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
            let arguments = command.strip_prefix(executable.as_ref())?;
            ((arguments.is_empty() || arguments.starts_with(char::is_whitespace))
                && argument
                    .is_none_or(|expected| command.split_whitespace().any(|part| part == expected)))
            .then_some(pid)
        })
        .collect()
}

fn owns_ipv4_listener(pid: u32, port: u16) -> bool {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-a", "-p"])
        .arg(pid.to_string())
        .arg(format!("-iTCP@127.0.0.1:{port}"))
        .args(["-sTCP:LISTEN", "-t"])
        .output();
    output.is_ok_and(|output| {
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.trim().parse::<u32>() == Ok(pid))
    })
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

    use super::{
        bundle_executable_name, has_workbuddy_bundle_identifier, installation_from_path,
        process_from_list,
    };

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
        assert_eq!(
            process_from_list(
                "74 /Applications/WorkBuddy.app/Contents/MacOS/Electron-copy --remote-debugging-port=49152\n",
                executable,
                Some("--remote-debugging-port=49152")
            ),
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

    #[test]
    fn verifies_the_workbuddy_bundle_identifier() {
        let mut dictionary = plist::Dictionary::new();
        dictionary.insert(
            "CFBundleIdentifier".to_string(),
            plist::Value::String("com.workbuddy.workbuddy".to_string()),
        );
        assert!(has_workbuddy_bundle_identifier(&plist::Value::Dictionary(
            dictionary.clone()
        )));

        dictionary.insert(
            "CFBundleIdentifier".to_string(),
            plist::Value::String("com.example.other".to_string()),
        );
        assert!(!has_workbuddy_bundle_identifier(&plist::Value::Dictionary(
            dictionary
        )));
    }

    #[test]
    fn rejects_a_non_workbuddy_bundle_path_before_reading_metadata() {
        assert!(
            installation_from_path(Path::new("/Applications/Other.app").to_path_buf()).is_none()
        );
    }
}
