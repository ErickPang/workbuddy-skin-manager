use std::{
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    time::Duration,
};

#[cfg(target_os = "macos")]
#[path = "workbuddy/macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "workbuddy/windows.rs"]
mod platform;

pub use platform::{
    cdp_process, expected_path_display, find_installation, installed_version, node_command,
    running, start_normal, start_with_cdp, stop,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installation {
    pub app_path: PathBuf,
    pub executable: PathBuf,
}

pub fn cdp_available(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok()
}

pub fn owns_cdp_session(installation: &Installation, port: u16, expected_pid: Option<u32>) -> bool {
    cdp_process(installation, port)
        .is_some_and(|pid| expected_pid.is_none_or(|expected| expected == pid))
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

#[cfg(test)]
mod tests {
    use super::matches_compatibility;

    #[test]
    fn matches_workbuddy_wildcard_versions() {
        assert!(matches_compatibility("5.2.6", &["5.2.x".to_string()]));
        assert!(matches_compatibility("5.9.0", &["5.x".to_string()]));
        assert!(!matches_compatibility("6.0.0", &["5.2.x".to_string()]));
    }
}
