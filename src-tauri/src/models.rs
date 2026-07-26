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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerState {
    pub active_theme_id: Option<String>,
    #[serde(default)]
    pub cdp_port: Option<u16>,
    #[serde(default)]
    pub workbuddy_pid: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkBuddyStatus {
    pub installed: bool,
    pub app_path: String,
    pub version: Option<String>,
    pub manager_compatible: bool,
    pub cdp_available: bool,
    pub cdp_port: Option<u16>,
    pub active_theme_id: Option<String>,
    pub configured_theme_id: Option<String>,
}

fn default_background_position() -> String {
    "right center".to_string()
}

fn default_background_size() -> String {
    "cover".to_string()
}
