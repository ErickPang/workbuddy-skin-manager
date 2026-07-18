use std::{
    net::{SocketAddr, TcpStream},
    path::Path,
    process::Command,
    time::Duration,
};

pub const WORKBUDDY_PATH: &str = "/Applications/WorkBuddy.app";
pub const WORKBUDDY_ELECTRON: &str = "/Applications/WorkBuddy.app/Contents/MacOS/Electron";

pub fn cdp_available(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok()
}

pub fn running() -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-f", "^/Applications/WorkBuddy.app/Contents/MacOS/Electron"])
        .status()
        .is_ok_and(|status| status.success())
}

pub fn cdp_process(port: u16) -> Option<u32> {
    let output = Command::new("/bin/ps")
        .args(["-ax", "-o", "pid=", "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    cdp_process_from_list(&String::from_utf8_lossy(&output.stdout), port)
}

pub fn owns_cdp_session(port: u16, expected_pid: Option<u32>) -> bool {
    cdp_process(port).is_some_and(|pid| expected_pid.is_none_or(|expected| expected == pid))
}

pub fn installed_version() -> Option<String> {
    read_version(Path::new(WORKBUDDY_PATH))
}

pub fn matches_compatibility(version: &str, patterns: &[String]) -> bool {
    let version_parts: Vec<&str> = version.split('.').collect();
    patterns.iter().any(|pattern| {
        let pattern = pattern.trim();
        if pattern == "*" {
            return true;
        }
        let pattern_parts: Vec<&str> = pattern.split('.').collect();
        pattern_parts.iter().enumerate().all(|(index, expected)| {
            matches!(*expected, "x" | "X" | "*")
                || version_parts
                    .get(index)
                    .is_some_and(|actual| actual == expected)
        }) && version_parts.len() >= pattern_parts.len()
    })
}

fn read_version(app_path: &Path) -> Option<String> {
    let plist_path = app_path.join("Contents/Info.plist");
    let value = plist::Value::from_file(plist_path).ok()?;
    value
        .as_dictionary()?
        .get("CFBundleShortVersionString")?
        .as_string()
        .map(ToString::to_string)
}

fn cdp_process_from_list(processes: &str, port: u16) -> Option<u32> {
    let port_argument = format!("--remote-debugging-port={port}");
    processes.lines().find_map(|line| {
        let line = line.trim();
        let split = line.find(char::is_whitespace)?;
        let pid = line[..split].parse::<u32>().ok()?;
        let command = line[split..].trim_start();
        (command.starts_with(WORKBUDDY_ELECTRON)
            && command.split_whitespace().any(|part| part == port_argument))
        .then_some(pid)
    })
}

#[cfg(test)]
mod tests {
    use super::{cdp_process_from_list, matches_compatibility};

    #[test]
    fn matches_workbuddy_wildcard_versions() {
        assert!(matches_compatibility("5.2.6", &["5.2.x".to_string()]));
        assert!(matches_compatibility("5.9.0", &["5.x".to_string()]));
        assert!(!matches_compatibility("6.0.0", &["5.2.x".to_string()]));
    }

    #[test]
    fn identifies_the_workbuddy_cdp_process() {
        let processes = "  41 /usr/bin/other --remote-debugging-port=49152\n\
                         73 /Applications/WorkBuddy.app/Contents/MacOS/Electron --remote-debugging-address=127.0.0.1 --remote-debugging-port=49152\n";
        assert_eq!(cdp_process_from_list(processes, 49152), Some(73));
        assert_eq!(cdp_process_from_list(processes, 9222), None);
    }
}
