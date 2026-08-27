use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::Installation;

const PROCESS_NAME: &str = "WorkBuddy";
const EXECUTABLE_NAME: &str = "WorkBuddy.exe";

pub fn find_installation(configured_path: Option<&Path>) -> Option<Installation> {
    if let Some(path) = configured_path {
        return installation_from_path(path.to_path_buf());
    }
    if let Some(installation) = env::var_os("WORKBUDDY_PATH")
        .map(PathBuf::from)
        .and_then(installation_from_path)
    {
        return Some(installation);
    }
    if let Some(installation) = known_installation_paths()
        .into_iter()
        .find_map(installation_from_path)
    {
        return Some(installation);
    }
    for path in running_process_paths() {
        if let Some(installation) = installation_from_path(path) {
            return Some(installation);
        }
    }
    for path in registry_installation_paths() {
        if let Some(installation) = installation_from_path(path) {
            return Some(installation);
        }
    }
    None
}

pub fn expected_path_display() -> String {
    env::var_os("WORKBUDDY_PATH")
        .map(PathBuf::from)
        .or_else(default_local_app_path)
        .unwrap_or_else(|| PathBuf::from(EXECUTABLE_NAME))
        .to_string_lossy()
        .into_owned()
}

pub fn installed_version(installation: &Installation) -> Option<String> {
    let package_json = installation
        .executable
        .parent()?
        .join("resources/app/package.json");
    if let Ok(content) = fs::read_to_string(package_json) {
        if let Some(version) = serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|value| value.get("version")?.as_str().map(ToString::to_string))
        {
            return Some(version);
        }
    }
    powershell_lines(
        "$item = Get-Item -LiteralPath $env:WORKBUDDY_EXE -ErrorAction Stop; $item.VersionInfo.ProductVersion",
        &[("WORKBUDDY_EXE", &installation.executable)],
    )
    .into_iter()
    .next()
    .filter(|version| !version.is_empty())
}

pub fn running(installation: &Installation) -> bool {
    !process_pids(installation).is_empty()
}

pub fn cdp_process(installation: &Installation, port: u16) -> Option<u32> {
    let output = Command::new("netstat.exe")
        .args(["-ano", "-p", "tcp"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let pids = process_pids(installation)
        .into_iter()
        .collect::<HashSet<_>>();
    netstat_pid(&String::from_utf8_lossy(&output.stdout), port, &pids)
}

pub fn stop(installation: &Installation) -> Result<(), String> {
    request_close(installation);
    if wait_until(Duration::from_secs(15), || !running(installation)) {
        return Ok(());
    }
    Err(
        "WorkBuddy 未能正常退出。请保存工作并手动退出 WorkBuddy 后重试；Manager 不会强制终止它。"
            .to_string(),
    )
}

pub fn start_with_cdp(installation: &Installation, port: u16) -> Result<(), String> {
    Command::new(&installation.executable)
        .arg("--remote-debugging-address=127.0.0.1")
        .arg(format!("--remote-debugging-port={port}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法启动 WorkBuddy CDP 模式: {error}"))
}

pub fn start_normal(installation: &Installation) -> Result<(), String> {
    Command::new(&installation.executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("无法以普通模式启动 WorkBuddy: {error}"))?;
    if wait_until(Duration::from_secs(15), || running(installation)) {
        Ok(())
    } else {
        Err("等待 WorkBuddy 普通模式启动超时".to_string())
    }
}

pub fn node_command(installation: &Installation) -> Command {
    Command::new(&installation.executable)
}

fn known_installation_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = default_local_app_path() {
        candidates.push(path);
    }
    if let Some(path) = env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(path).join("WorkBuddy").join(EXECUTABLE_NAME));
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(path) = env::var_os(variable) {
            candidates.push(PathBuf::from(path).join("WorkBuddy").join(EXECUTABLE_NAME));
        }
    }

    candidates
}

fn default_local_app_path() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Programs/WorkBuddy").join(EXECUTABLE_NAME))
}

pub fn installation_from_path(path: PathBuf) -> Option<Installation> {
    let executable = if path.is_dir() {
        path.join(EXECUTABLE_NAME)
    } else {
        path
    };
    if !executable
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(EXECUTABLE_NAME))
    {
        return None;
    }
    executable.is_file().then(|| Installation {
        app_path: executable.clone(),
        executable,
    })
}

fn running_process_paths() -> Vec<PathBuf> {
    powershell_lines(
        &format!(
            "Get-Process -Name '{PROCESS_NAME}' -ErrorAction SilentlyContinue | ForEach-Object {{ try {{ $_.Path }} catch {{}} }}"
        ),
        &[],
    )
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

fn registry_installation_paths() -> Vec<PathBuf> {
    let script = r#"
$roots = @(
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
Get-ItemProperty -Path $roots -ErrorAction SilentlyContinue |
  Where-Object { $_.DisplayName -like '*WorkBuddy*' } |
  ForEach-Object {
    if ($_.DisplayIcon) { [Environment]::ExpandEnvironmentVariables([string]$_.DisplayIcon) }
    if ($_.InstallLocation) { Join-Path ([Environment]::ExpandEnvironmentVariables([string]$_.InstallLocation)) 'WorkBuddy.exe' }
  }
"#;
    powershell_lines(script, &[])
        .into_iter()
        .map(|path| {
            let path = path.trim().trim_matches('"');
            let path = path.strip_suffix(",0").unwrap_or(path).trim_matches('"');
            PathBuf::from(path)
        })
        .collect()
}

fn process_pids(installation: &Installation) -> Vec<u32> {
    let script = format!(
        "$target = [IO.Path]::GetFullPath($env:WORKBUDDY_EXE); Get-Process -Name '{PROCESS_NAME}' -ErrorAction SilentlyContinue | ForEach-Object {{ try {{ if ([IO.Path]::GetFullPath($_.Path) -ieq $target) {{ $_.Id }} }} catch {{}} }}"
    );
    powershell_lines(&script, &[("WORKBUDDY_EXE", &installation.executable)])
        .into_iter()
        .filter_map(|pid| pid.parse().ok())
        .collect()
}

fn netstat_pid(output: &str, port: u16, allowed_pids: &HashSet<u32>) -> Option<u32> {
    output.lines().find_map(|line| {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 5 || !columns[0].eq_ignore_ascii_case("TCP") {
            return None;
        }
        if !columns[3].eq_ignore_ascii_case("LISTENING") {
            return None;
        }
        let (host, raw_port) = columns[1].rsplit_once(':')?;
        if host != "127.0.0.1" {
            return None;
        }
        let local_port = raw_port.parse::<u16>().ok()?;
        let pid = columns.last()?.parse::<u32>().ok()?;
        (local_port == port && allowed_pids.contains(&pid)).then_some(pid)
    })
}

fn request_close(installation: &Installation) {
    let script = format!(
        "$target = [IO.Path]::GetFullPath($env:WORKBUDDY_EXE); Get-Process -Name '{PROCESS_NAME}' -ErrorAction SilentlyContinue | ForEach-Object {{ try {{ if ([IO.Path]::GetFullPath($_.Path) -ieq $target) {{ [void]$_.CloseMainWindow() }} }} catch {{}} }}"
    );
    let _ = powershell_lines(&script, &[("WORKBUDDY_EXE", &installation.executable)]);
}

fn powershell_lines(script: &str, path_env: &[(&str, &Path)]) -> Vec<String> {
    let script =
        format!("[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); {script}");
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &script,
    ]);
    for (name, value) in path_env {
        command.env(name, value);
    }
    let Ok(output) = command.output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
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
    use std::collections::HashSet;

    use std::path::Path;

    use super::{installation_from_path, netstat_pid};

    #[test]
    fn identifies_the_allowed_workbuddy_listener() {
        let output = "  TCP    127.0.0.1:49152      0.0.0.0:0      LISTENING       73\r\n\
                      TCP    127.0.0.1:9222       0.0.0.0:0      LISTENING       41\r\n\
                      TCP    0.0.0.0:49153        0.0.0.0:0      LISTENING       73\r\n\
                      TCP    [::1]:49154          [::]:0         LISTENING       73\r\n\
                      TCP    127.0.0.1:49155      127.0.0.1:123  ESTABLISHED     73\r\n";
        let allowed = HashSet::from([73]);
        assert_eq!(netstat_pid(output, 49152, &allowed), Some(73));
        assert_eq!(netstat_pid(output, 9222, &allowed), None);
        assert_eq!(netstat_pid(output, 49153, &allowed), None);
        assert_eq!(netstat_pid(output, 49154, &allowed), None);
        assert_eq!(netstat_pid(output, 49155, &allowed), None);
    }

    #[test]
    fn rejects_a_non_workbuddy_executable_path() {
        assert!(installation_from_path(Path::new(r"C:\Apps\Other.exe").to_path_buf()).is_none());
    }
}
