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
    cdp_process, expected_path_display, find_installation, installation_from_path,
    installed_version, node_command, running, start_normal, start_with_cdp, stop,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installation {
    pub app_path: PathBuf,
    pub executable: PathBuf,
}

const SUPPORTED_WORKBUDDY_PATTERNS: &[&str] = &["5.2.x", "5.3.x"];

pub fn manager_supports_version(version: &str) -> bool {
    SUPPORTED_WORKBUDDY_PATTERNS
        .iter()
        .any(|pattern| matches_version_pattern(version, pattern))
}

pub fn supported_versions_display() -> String {
    SUPPORTED_WORKBUDDY_PATTERNS.join(", ")
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
    patterns
        .iter()
        .any(|pattern| matches_version_pattern(version, pattern))
}

fn matches_version_pattern(version: &str, pattern: &str) -> bool {
    let version_parts = version
        .trim()
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>();
    let Ok(version_parts) = version_parts else {
        return false;
    };
    if version_parts.len() < 3 {
        return false;
    }
    let pattern = pattern.trim();
    if pattern == "*" {
        return true;
    }
    let pattern_parts: Vec<&str> = pattern.split('.').collect();
    pattern_parts.iter().enumerate().all(|(index, expected)| {
        matches!(*expected, "x" | "X" | "*")
            || version_parts
                .get(index)
                .is_some_and(|actual| expected.parse::<u64>().ok().as_ref() == Some(actual))
    }) && version_parts.len() >= pattern_parts.len()
}

#[cfg(test)]
mod tests {
    use super::{manager_supports_version, matches_compatibility, supported_versions_display};

    #[test]
    fn matches_workbuddy_wildcard_versions() {
        assert!(matches_compatibility("5.2.6", &["5.2.x".to_string()]));
        assert!(matches_compatibility("5.9.0", &["5.x".to_string()]));
        assert!(matches_compatibility(
            "5.3.5",
            &["5.2.x".to_string(), "5.3.x".to_string()]
        ));
        assert!(matches_compatibility("5.3.5.12", &["5.3.x".to_string()]));
        assert!(!matches_compatibility("6.0.0", &["5.2.x".to_string()]));
        assert!(!matches_compatibility("5.3.beta", &["5.3.x".to_string()]));
        assert!(!matches_compatibility("5.3", &["*".to_string()]));
        assert!(!matches_compatibility("5.3.0-beta", &["*".to_string()]));
    }

    #[test]
    fn limits_the_manager_to_verified_workbuddy_versions() {
        assert!(manager_supports_version("5.2.6"));
        assert!(manager_supports_version("5.3.0"));
        assert!(manager_supports_version("5.3.5"));
        assert!(!manager_supports_version("5.4.0"));
        assert!(!manager_supports_version("6.0.0"));
        assert_eq!(supported_versions_display(), "5.2.x, 5.3.x");
    }
}
