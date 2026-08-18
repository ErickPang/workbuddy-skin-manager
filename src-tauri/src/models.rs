use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compatibility {
    pub manager: String,
    pub workbuddy: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub preview: Option<String>,
    pub compatibility: Compatibility,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePalette {
    pub background: String,
    pub panel: String,
    pub panel_alt: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
    pub accent_text: String,
    pub border: String,
    pub hover: String,
    pub active: String,
    pub subtle: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ThemeBackground {
    pub image: String,
    #[serde(default = "default_background_position")]
    pub position: String,
    #[serde(default = "default_background_size")]
    pub size: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ThemeConfig {
    pub palette: ThemePalette,
    pub background: ThemeBackground,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledTheme {
    pub manifest: ThemeManifest,
    pub theme: ThemeConfig,
    pub preview_path: Option<String>,
    pub background_path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokenTheme {
    pub id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeLibrary {
    pub themes: Vec<InstalledTheme>,
    pub broken_themes: Vec<BrokenTheme>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeLibraryBackup {
    pub count: usize,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerState {
    #[serde(default = "current_state_version")]
    pub state_version: u32,
    #[serde(default)]
    pub active_theme_id: Option<String>,
    #[serde(default)]
    pub cdp_port: Option<u16>,
    #[serde(default)]
    pub workbuddy_pid: Option<u32>,
}

impl Default for ManagerState {
    fn default() -> Self {
        Self {
            state_version: current_state_version(),
            active_theme_id: None,
            cdp_port: None,
            workbuddy_pid: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerSettings {
    #[serde(default = "current_settings_version")]
    pub settings_version: u32,
    #[serde(default)]
    pub workbuddy_path: Option<String>,
}

impl Default for ManagerSettings {
    fn default() -> Self {
        Self {
            settings_version: current_settings_version(),
            workbuddy_path: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkBuddyStatus {
    pub installed: bool,
    pub running: bool,
    pub app_path: String,
    pub version: Option<String>,
    pub manager_compatible: bool,
    pub cdp_available: bool,
    pub cdp_port: Option<u16>,
    pub active_theme_id: Option<String>,
    pub configured_theme_id: Option<String>,
    pub restart_required: bool,
    pub custom_path: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticInfo {
    pub manager_version: String,
    pub platform: String,
    pub log_path: String,
    pub recent_errors: Vec<String>,
    pub workbuddy: Option<WorkBuddyStatus>,
    pub status_error: Option<String>,
}

fn default_background_position() -> String {
    "right center".to_string()
}

fn current_settings_version() -> u32 {
    1
}

fn current_state_version() -> u32 {
    1
}

fn default_background_size() -> String {
    "cover".to_string()
}
