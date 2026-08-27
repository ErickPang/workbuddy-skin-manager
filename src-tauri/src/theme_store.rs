use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

use crate::{
    color_extractor,
    models::{
        BrokenTheme, InstalledTheme, ManagerSettings, ManagerState, ThemeConfig, ThemeLibrary,
        ThemeLibraryBackup, ThemeManifest,
    },
};

#[allow(dead_code)]
const MAX_ARCHIVE_BYTES: u64 = 20 * 1024 * 1024;
#[allow(dead_code)]
const MAX_UNCOMPRESSED_BYTES: u64 = 20 * 1024 * 1024;
#[allow(dead_code)]
const MAX_FILES: usize = 16;
#[allow(dead_code)]
const MAX_ARCHIVE_ENTRIES: usize = 32;
#[allow(dead_code)]
const MAX_JSON_BYTES: u64 = 256 * 1024;
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 8192;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const MIN_TEXT_CONTRAST: f64 = 4.5;
const IMAGE_VALIDATION_MARKER: &str = ".images-validated-v2";
pub const THEME_OVERWRITE_CONFIRMATION_REQUIRED: &str = "THEME_OVERWRITE_CONFIRMATION_REQUIRED";
static IMPORT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static STATE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static THEME_STORE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageValidationState {
    background: FileFingerprint,
    preview: Option<FileFingerprint>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileFingerprint {
    length: u64,
    modified_seconds: u64,
    modified_nanos: u32,
}

pub fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("无法确定 Manager 数据目录: {error}"))
}

fn themes_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("themes"))
}

fn preset_themes_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法定位预置主题目录: {error}"))?;
    for candidate in [
        resource_dir.join("preset-themes"),
        resource_dir.join("resources/preset-themes"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/preset-themes"),
    ] {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(resource_dir.join("preset-themes"))
}

pub fn theme_directory(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    validate_theme_id(id)?;
    Ok(themes_dir(app)?.join(id))
}

fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("manager-state.json"))
}

fn state_backup_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("manager-state.json.bak"))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("manager-settings.json"))
}

fn settings_backup_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("manager-settings.json.bak"))
}

pub fn load_settings(app: &AppHandle) -> Result<ManagerSettings, String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    let path = settings_path(app)?;
    let backup = settings_backup_path(app)?;
    match load_settings_files(&path, &backup) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            crate::log_runtime_error(app, "settings-recovery", &error);
            let settings = ManagerSettings::default();
            let content = serde_json::to_vec_pretty(&settings)
                .map_err(|serialize_error| format!("无法序列化默认设置: {serialize_error}"))?;
            atomic_write(&path, &content)?;
            atomic_write(&backup, &content)?;
            Ok(settings)
        }
    }
}

pub fn save_settings(app: &AppHandle, settings: &ManagerSettings) -> Result<(), String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    let mut settings = settings.clone();
    settings.settings_version = 1;
    let content = serde_json::to_vec_pretty(&settings)
        .map_err(|error| format!("无法序列化 Manager 设置: {error}"))?;
    atomic_write(&settings_path(app)?, &content)?;
    atomic_write(&settings_backup_path(app)?, &content)
}

fn load_settings_files(path: &Path, backup: &Path) -> Result<ManagerSettings, String> {
    cleanup_atomic_temps(path)?;
    cleanup_atomic_temps(backup)?;
    match read_valid_settings_file(path) {
        Ok(Some(settings)) => Ok(settings),
        Ok(None) => match read_valid_settings_file(backup)? {
            Some(settings) => restore_settings_file(path, settings),
            None => Ok(ManagerSettings::default()),
        },
        Err(primary_error) => {
            preserve_invalid_file(path)?;
            match read_valid_settings_file(backup) {
                Ok(Some(settings)) => restore_settings_file(path, settings),
                Ok(None) => Err(format!("{primary_error}；没有可用的设置备份")),
                Err(backup_error) => {
                    preserve_invalid_file(backup)?;
                    Err(format!("{primary_error}；设置备份也已损坏: {backup_error}"))
                }
            }
        }
    }
}

fn read_valid_settings_file(path: &Path) -> Result<Option<ManagerSettings>, String> {
    read_settings_file(path)?
        .map(validate_settings_version)
        .transpose()
}

fn read_settings_file(path: &Path) -> Result<Option<ManagerSettings>, String> {
    let content = match fs::read(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法读取 Manager 设置 {}: {error}", path.display())),
    };
    serde_json::from_slice(&content)
        .map(Some)
        .map_err(|error| format!("Manager 设置文件损坏 {}: {error}", path.display()))
}

fn restore_settings_file(
    path: &Path,
    settings: ManagerSettings,
) -> Result<ManagerSettings, String> {
    let settings = validate_settings_version(settings)?;
    let content = serde_json::to_vec_pretty(&settings)
        .map_err(|error| format!("无法序列化恢复设置: {error}"))?;
    atomic_write(path, &content)?;
    Ok(settings)
}

fn validate_settings_version(settings: ManagerSettings) -> Result<ManagerSettings, String> {
    if settings.settings_version == 1 {
        Ok(settings)
    } else {
        Err(format!(
            "不支持 Manager 设置版本 {}",
            settings.settings_version
        ))
    }
}

pub fn load_state(app: &AppHandle) -> Result<ManagerState, String> {
    load_state_files(&state_path(app)?, &state_backup_path(app)?)
}

pub fn save_state(app: &AppHandle, state: &ManagerState) -> Result<(), String> {
    let path = state_path(app)?;
    let backup = state_backup_path(app)?;
    let mut state = state.clone();
    state.state_version = 1;
    let content = serde_json::to_vec_pretty(&state)
        .map_err(|error| format!("无法序列化 Manager 状态: {error}"))?;
    atomic_write(&path, &content)?;
    atomic_write(&backup, &content)
}

fn load_state_files(path: &Path, backup: &Path) -> Result<ManagerState, String> {
    cleanup_atomic_temps(path)?;
    cleanup_atomic_temps(backup)?;
    match read_valid_state_file(path) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => match read_valid_state_file(backup)? {
            Some(state) => {
                let content = serde_json::to_vec_pretty(&state)
                    .map_err(|error| format!("无法序列化恢复状态: {error}"))?;
                atomic_write(path, &content)?;
                Ok(state)
            }
            None => Ok(ManagerState::default()),
        },
        Err(primary_error) => {
            preserve_invalid_file(path)?;
            match read_valid_state_file(backup) {
                Ok(Some(state)) => {
                    let content = serde_json::to_vec_pretty(&state)
                        .map_err(|error| format!("无法序列化恢复状态: {error}"))?;
                    atomic_write(path, &content)?;
                    Ok(state)
                }
                Ok(None) => Err(format!("{primary_error}；没有可用的状态备份")),
                Err(backup_error) => {
                    preserve_invalid_file(backup)?;
                    Err(format!("{primary_error}；状态备份也已损坏: {backup_error}"))
                }
            }
        }
    }
}

fn read_valid_state_file(path: &Path) -> Result<Option<ManagerState>, String> {
    read_state_file(path)?
        .map(validate_state_version)
        .transpose()
}

fn validate_state_version(state: ManagerState) -> Result<ManagerState, String> {
    if state.state_version == 1 {
        Ok(state)
    } else {
        Err(format!("不支持 Manager 状态版本 {}", state.state_version))
    }
}

fn preserve_invalid_file(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("无效配置路径: {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("manager-data");
    let preserved = parent.join(format!("{name}.invalid-{}", unique_import_suffix()?));
    fs::copy(path, preserved)
        .map(|_| ())
        .map_err(|error| format!("无法保留损坏配置 {}: {error}", path.display()))
}

fn cleanup_atomic_temps(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if !parent.exists() {
        return Ok(());
    }
    let prefix = format!(
        ".{}.",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("manager-state")
    );
    for entry in fs::read_dir(parent).map_err(|error| format!("无法检查状态临时文件: {error}"))?
    {
        let entry = entry.map_err(|error| format!("无法读取状态临时文件: {error}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".tmp") {
            fs::remove_file(entry.path())
                .map_err(|error| format!("无法清理状态临时文件: {error}"))?;
        }
    }
    Ok(())
}

fn read_state_file(path: &Path) -> Result<Option<ManagerState>, String> {
    let content = match fs::read(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法读取 Manager 状态 {}: {error}", path.display())),
    };
    serde_json::from_slice(&content)
        .map(Some)
        .map_err(|error| format!("Manager 状态文件损坏 {}: {error}", path.display()))
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("状态路径没有父目录: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建数据目录: {error}"))?;
    let sequence = STATE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("manager-state");
    let temporary = parent.join(format!(
        ".{file_name}.{}.{sequence}.tmp",
        std::process::id()
    ));
    let result = (|| {
        let mut file =
            File::create(&temporary).map_err(|error| format!("无法创建状态临时文件: {error}"))?;
        file.write_all(content)
            .map_err(|error| format!("无法写入状态临时文件: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("无法同步状态临时文件: {error}"))?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("无法切换 Manager 状态文件: {error}"))?;
        sync_parent_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_parent_directory(parent: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("无法同步 Manager 数据目录: {error}"))
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Ok(())
    }
}

pub fn recover_theme_transactions(app: &AppHandle) -> Result<(), String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    recover_theme_transactions_inner(&themes_dir(app)?)
}

fn recover_theme_transactions_inner(root: &Path) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(root)
        .map_err(|error| format!("无法检查主题事务残留: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("无法读取主题事务残留: {error}"))?;
    entries.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    entries.reverse();

    for entry in entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".import-") {
            fs::remove_dir_all(&path)
                .map_err(|error| format!("无法清理未完成的主题导入: {error}"))?;
            continue;
        }
        if !name.starts_with(".backup-") {
            continue;
        }
        let manifest: ThemeManifest = read_json(&path.join("manifest.json"))?;
        validate_theme_id(&manifest.id)?;
        let destination = root.join(&manifest.id);
        if destination.exists() {
            fs::remove_dir_all(&path).map_err(|error| format!("无法清理主题备份: {error}"))?;
        } else {
            fs::rename(&path, &destination)
                .map_err(|error| format!("无法恢复主题备份 {}: {error}", manifest.id))?;
        }
    }
    Ok(())
}

pub fn list_installed_themes(app: &AppHandle) -> Result<ThemeLibrary, String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    let root = themes_dir(app)?;
    list_installed_themes_inner(&root)
}

fn list_installed_themes_inner(root: &Path) -> Result<ThemeLibrary, String> {
    recover_theme_transactions_inner(root)?;
    if !root.exists() {
        return Ok(ThemeLibrary {
            themes: Vec::new(),
            broken_themes: Vec::new(),
        });
    }

    let mut themes = Vec::new();
    let mut broken_themes = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| format!("无法读取主题库: {error}"))? {
        let path = entry
            .map_err(|error| format!("无法读取主题项: {error}"))?
            .path();
        if !path.is_dir()
            || path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
        {
            continue;
        }
        match read_installed_theme_inner(&path) {
            Ok(theme) => themes.push(theme),
            Err(reason) => {
                if let Some(id) = path.file_name().and_then(|name| name.to_str()) {
                    if validate_theme_id(id).is_ok() {
                        broken_themes.push(BrokenTheme {
                            id: id.to_string(),
                            reason,
                        });
                    }
                }
            }
        }
    }
    themes.sort_by(|left, right| left.manifest.name.cmp(&right.manifest.name));
    broken_themes.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(ThemeLibrary {
        themes,
        broken_themes,
    })
}

pub fn list_preset_themes(app: &AppHandle) -> Result<Vec<InstalledTheme>, String> {
    let root = preset_themes_dir(app)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut themes = Vec::new();
    for entry in fs::read_dir(&root).map_err(|error| format!("无法读取预置主题目录: {error}"))?
    {
        let path = entry
            .map_err(|error| format!("无法读取预置主题项: {error}"))?
            .path();
        if path.is_dir() {
            match read_preset_theme(&path) {
                Ok(theme) => themes.push(theme),
                Err(error) => {
                    crate::log_runtime_error(
                        app,
                        "preset-theme",
                        &format!("无法加载预置主题 {}: {error}", path.display()),
                    );
                }
            }
        }
    }
    themes.sort_by(|left, right| left.manifest.name.cmp(&right.manifest.name));
    Ok(themes)
}

pub fn install_preset_theme(
    app: &AppHandle,
    id: &str,
    overwrite_confirmed: bool,
) -> Result<InstalledTheme, String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    validate_theme_id(id)?;
    let preset_dir = preset_themes_dir(app)?.join(id);
    let preset = read_preset_theme(&preset_dir)?;
    let root = themes_dir(app)?;
    recover_theme_transactions_inner(&root)?;
    require_overwrite_confirmation(&root, id, overwrite_confirmed)?;
    fs::create_dir_all(&root).map_err(|error| format!("无法创建主题库: {error}"))?;
    let staging = root.join(format!(".import-{}", unique_import_suffix()?));
    let result = (|| {
        fs::create_dir_all(&staging).map_err(|error| format!("无法创建主题目录: {error}"))?;
        copy_preset_file(&preset_dir, &staging, "manifest.json")?;
        copy_preset_file(&preset_dir, &staging, "theme.json")?;
        copy_preset_file(&preset_dir, &staging, &preset.theme.background.image)?;
        if let Some(preview) = preset.manifest.preview.as_deref() {
            if preview != preset.theme.background.image {
                copy_preset_file(&preset_dir, &staging, preview)?;
            }
        }
        fs::write(staging.join(IMAGE_VALIDATION_MARKER), [])
            .map_err(|error| format!("无法保存图片校验状态: {error}"))?;
        read_installed_theme_inner(&staging)?;
        commit_staged_theme(&root, &staging, id)
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    read_installed_theme_inner(&root.join(id))
}

pub fn read_installed_theme(theme_dir: &Path) -> Result<InstalledTheme, String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    let theme = read_installed_theme_inner(theme_dir)?;
    validate_image_file(Path::new(&theme.background_path))?;
    if let Some(preview) = theme.preview_path.as_deref() {
        if preview != theme.background_path {
            validate_image_file(Path::new(preview))?;
        }
    }
    Ok(theme)
}

fn read_installed_theme_inner(theme_dir: &Path) -> Result<InstalledTheme, String> {
    validate_theme_directory(theme_dir)?;
    validate_file_within_theme(theme_dir, &theme_dir.join("manifest.json"), "manifest.json")?;
    validate_file_within_theme(theme_dir, &theme_dir.join("theme.json"), "theme.json")?;
    let manifest: ThemeManifest = read_json(&theme_dir.join("manifest.json"))?;
    let theme: ThemeConfig = read_json(&theme_dir.join("theme.json"))?;
    validate_manifest(&manifest)?;
    validate_theme(&theme)?;

    let background_path = theme_dir.join(&theme.background.image);
    let preview_file = manifest
        .preview
        .as_deref()
        .map(|relative| theme_dir.join(relative));
    validate_file_within_theme(theme_dir, &background_path, "主题背景")?;
    if let Some(path) = preview_file.as_deref() {
        validate_file_within_theme(theme_dir, path, "主题预览图")?;
    }
    let validation_marker = theme_dir.join(IMAGE_VALIDATION_MARKER);
    validate_installed_images(
        &validation_marker,
        &background_path,
        preview_file.as_deref(),
    )?;
    let preview_path = manifest
        .preview
        .as_deref()
        .map(|relative| theme_dir.join(relative))
        .map(|path| path.to_string_lossy().into_owned());

    Ok(InstalledTheme {
        manifest,
        theme,
        preview_path,
        background_path: background_path.to_string_lossy().into_owned(),
    })
}

fn validate_installed_images(
    marker: &Path,
    background: &Path,
    preview: Option<&Path>,
) -> Result<(), String> {
    let before = image_validation_state(background, preview)?;
    let cached = validate_regular_file(marker, "图片校验状态")
        .ok()
        .and_then(|_| fs::read(marker).ok())
        .and_then(|content| serde_json::from_slice::<ImageValidationState>(&content).ok());
    if cached.as_ref() == Some(&before) {
        return Ok(());
    }

    validate_image_file(background)?;
    if let Some(path) = preview.filter(|path| *path != background) {
        validate_image_file(path)?;
    }
    let after = image_validation_state(background, preview)?;
    if before != after {
        return Err("主题图片在校验期间发生变化，请重试".to_string());
    }
    let content =
        serde_json::to_vec(&after).map_err(|error| format!("无法序列化图片校验状态: {error}"))?;
    atomic_write(marker, &content)
}

fn image_validation_state(
    background: &Path,
    preview: Option<&Path>,
) -> Result<ImageValidationState, String> {
    Ok(ImageValidationState {
        background: file_fingerprint(background, "主题图片")?,
        preview: preview
            .filter(|path| *path != background)
            .map(|path| file_fingerprint(path, "主题预览图"))
            .transpose()?,
    })
}

fn file_fingerprint(path: &Path, label: &str) -> Result<FileFingerprint, String> {
    let metadata = validate_regular_file(path, label)?;
    let modified = metadata
        .modified()
        .map_err(|error| format!("无法读取{label}修改时间 {}: {error}", path.display()))?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| format!("{label}修改时间无效: {}", path.display()))?;
    Ok(FileFingerprint {
        length: metadata.len(),
        modified_seconds: modified.as_secs(),
        modified_nanos: modified.subsec_nanos(),
    })
}

fn read_preset_theme(theme_dir: &Path) -> Result<InstalledTheme, String> {
    validate_theme_directory(theme_dir)?;
    validate_file_within_theme(theme_dir, &theme_dir.join("manifest.json"), "manifest.json")?;
    validate_file_within_theme(theme_dir, &theme_dir.join("theme.json"), "theme.json")?;
    let manifest: ThemeManifest = read_json(&theme_dir.join("manifest.json"))?;
    let theme: ThemeConfig = read_json(&theme_dir.join("theme.json"))?;
    validate_manifest(&manifest)?;
    validate_theme(&theme)?;
    let directory_id = theme_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "预置主题目录名无效".to_string())?;
    if manifest.id != directory_id {
        return Err(format!(
            "预置主题目录名 {directory_id} 与 manifest ID {} 不一致",
            manifest.id
        ));
    }
    let background_path = theme_dir.join(&theme.background.image);
    validate_file_within_theme(theme_dir, &background_path, "主题背景")?;
    validate_image_file(&background_path)?;
    let preview_path = if let Some(preview) = manifest.preview.as_deref() {
        let path = theme_dir.join(preview);
        validate_file_within_theme(theme_dir, &path, "主题预览图")?;
        validate_image_file(&path)?;
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    };
    Ok(InstalledTheme {
        manifest,
        theme,
        preview_path,
        background_path: background_path.to_string_lossy().into_owned(),
    })
}

fn copy_preset_file(
    source_root: &Path,
    destination_root: &Path,
    relative: &str,
) -> Result<(), String> {
    let source = source_root.join(relative);
    let destination = destination_root.join(relative);
    let parent = destination
        .parent()
        .ok_or_else(|| "预置主题文件路径无效".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建主题资源目录: {error}"))?;
    fs::copy(&source, &destination).map_err(|error| format!("无法安装预置主题文件: {error}"))?;
    Ok(())
}

pub fn import_package(
    app: &AppHandle,
    package_path: &Path,
    overwrite_confirmed: bool,
) -> Result<InstalledTheme, String> {
    import_package_into(&themes_dir(app)?, package_path, overwrite_confirmed)
}

pub fn export_package(app: &AppHandle, id: &str, destination: &Path) -> Result<(), String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    validate_theme_id(id)?;
    let root = themes_dir(app)?;
    let destination_parent = destination
        .parent()
        .ok_or_else(|| "导出位置的父目录不存在".to_string())?;
    if root.exists()
        && destination_parent.exists()
        && fs::canonicalize(destination_parent)
            .map_err(|error| format!("无法验证导出位置: {error}"))?
            .starts_with(
                fs::canonicalize(&root).map_err(|error| format!("无法验证主题库位置: {error}"))?,
            )
    {
        return Err("不能将主题包导出到 Manager 主题库内".to_string());
    }
    export_theme_into(&root.join(id), destination)
}

pub fn export_theme_library(
    app: &AppHandle,
    destination: &Path,
) -> Result<ThemeLibraryBackup, String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    export_theme_library_into(&themes_dir(app)?, destination)
}

fn export_theme_library_into(
    root: &Path,
    destination: &Path,
) -> Result<ThemeLibraryBackup, String> {
    if !destination.is_dir() {
        return Err("主题库备份位置必须是已有目录".to_string());
    }
    if root.exists()
        && fs::canonicalize(destination)
            .map_err(|error| format!("无法验证备份位置: {error}"))?
            .starts_with(
                fs::canonicalize(root).map_err(|error| format!("无法验证主题库位置: {error}"))?,
            )
    {
        return Err("不能将主题库备份到 Manager 主题库内".to_string());
    }

    let library = list_installed_themes_inner(root)?;
    if !library.broken_themes.is_empty() {
        return Err(format!(
            "主题库中有 {} 个无法读取的主题，请先修复或删除后再备份",
            library.broken_themes.len()
        ));
    }
    if library.themes.is_empty() {
        return Err("主题库中没有可备份的主题".to_string());
    }

    let suffix = unique_import_suffix()?;
    let staging = destination.join(format!(".workbuddy-theme-backup-{suffix}.tmp"));
    let final_directory = available_backup_directory(destination)?;
    let result = (|| {
        fs::create_dir(&staging).map_err(|error| format!("无法创建主题库备份临时目录: {error}"))?;
        for theme in &library.themes {
            export_theme_into(
                &root.join(&theme.manifest.id),
                &staging.join(format!("{}.wbskin", theme.manifest.id)),
            )?;
        }
        fs::rename(&staging, &final_directory)
            .map_err(|error| format!("无法完成主题库备份: {error}"))?;
        if let Err(error) = sync_parent_directory(destination) {
            let cleanup_error = fs::remove_dir_all(&final_directory).err();
            return Err(match cleanup_error {
                Some(cleanup) => format!("{error}；无法清理未同步的备份: {cleanup}"),
                None => error,
            });
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    Ok(ThemeLibraryBackup {
        count: library.themes.len(),
        path: final_directory.to_string_lossy().into_owned(),
    })
}

fn available_backup_directory(destination: &Path) -> Result<PathBuf, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("系统时间异常: {error}"))?
        .as_secs();
    for sequence in 0..1000 {
        let suffix = if sequence == 0 {
            String::new()
        } else {
            format!("-{sequence}")
        };
        let candidate = destination.join(format!("workbuddy-theme-backup-{seconds}{suffix}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("无法为主题库备份分配目录名".to_string())
}

fn export_theme_into(theme_dir: &Path, destination: &Path) -> Result<(), String> {
    let theme = read_installed_theme_inner(theme_dir)?;
    if theme.manifest.id
        != theme_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    {
        return Err("主题目录与 manifest ID 不一致".to_string());
    }
    validate_image_file(Path::new(&theme.background_path))?;
    if let Some(preview) = theme.preview_path.as_deref() {
        if preview != theme.background_path {
            validate_image_file(Path::new(preview))?;
        }
    }

    let parent = destination
        .parent()
        .filter(|path| path.is_dir())
        .ok_or_else(|| "导出位置的父目录不存在".to_string())?;
    if fs::canonicalize(parent)
        .map_err(|error| format!("无法验证导出位置: {error}"))?
        .starts_with(
            fs::canonicalize(theme_dir).map_err(|error| format!("无法验证主题目录: {error}"))?,
        )
    {
        return Err("不能用导出文件覆盖主题资源".to_string());
    }
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "导出文件名无效".to_string())?;
    if destination.exists() && !destination.is_file() {
        return Err("导出目标必须是文件".to_string());
    }
    let suffix = unique_import_suffix()?;
    let temporary = parent.join(format!(".{file_name}.{suffix}.tmp"));
    let backup = parent.join(format!(".{file_name}.{suffix}.bak"));

    let write_result = (|| {
        let file =
            File::create(&temporary).map_err(|error| format!("无法创建导出文件: {error}"))?;
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);
        add_export_file(&mut archive, theme_dir, "manifest.json", options)?;
        add_export_file(&mut archive, theme_dir, "theme.json", options)?;
        add_export_file(
            &mut archive,
            theme_dir,
            &theme.theme.background.image,
            options,
        )?;
        if let Some(preview) = theme.manifest.preview.as_deref() {
            if preview != theme.theme.background.image {
                add_export_file(&mut archive, theme_dir, preview, options)?;
            }
        }
        archive
            .finish()
            .map_err(|error| format!("无法完成主题包压缩: {error}"))?
            .sync_all()
            .map_err(|error| format!("无法同步导出文件: {error}"))?;
        Ok::<(), String>(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    let had_existing = destination.exists();
    if had_existing {
        if let Err(error) = fs::rename(destination, &backup) {
            let _ = fs::remove_file(&temporary);
            return Err(format!("无法暂存已有导出文件: {error}"));
        }
    }
    if let Err(error) = fs::rename(&temporary, destination) {
        let rollback_error = had_existing
            .then(|| fs::rename(&backup, destination).err())
            .flatten();
        let _ = fs::remove_file(&temporary);
        return Err(match rollback_error {
            Some(rollback) => format!("无法保存主题包: {error}；旧文件恢复失败: {rollback}"),
            None => format!("无法保存主题包: {error}"),
        });
    }
    if had_existing {
        let _ = fs::remove_file(backup);
    }
    sync_parent_directory(parent)
}

fn add_export_file(
    archive: &mut ZipWriter<File>,
    theme_dir: &Path,
    relative: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    validate_package_path(Path::new(relative))?;
    archive
        .start_file(relative, options)
        .map_err(|error| format!("无法写入主题包条目 {relative}: {error}"))?;
    let mut source = File::open(theme_dir.join(relative))
        .map_err(|error| format!("无法读取主题文件 {relative}: {error}"))?;
    std::io::copy(&mut source, archive)
        .map_err(|error| format!("无法压缩主题文件 {relative}: {error}"))?;
    Ok(())
}

pub fn create_theme_from_image(
    app: &AppHandle,
    image_path: &Path,
    name: String,
) -> Result<InstalledTheme, String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    let root = themes_dir(app)?;
    recover_theme_transactions_inner(&root)?;

    let source =
        fs::canonicalize(image_path).map_err(|error| format!("无法读取所选图片: {error}"))?;
    if !source.is_file() {
        return Err("所选路径不是图片文件".to_string());
    }
    validate_image_file(&source)?;

    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp") {
        return Err("只支持 PNG、JPEG 和 WebP 图片".to_string());
    }
    let palette = color_extractor::extract_theme_palette(&source)?;

    let display_name = normalized_display_name(&name)?;
    let id = next_available_theme_id(&root, &display_name)?;
    let manifest = ThemeManifest {
        schema_version: 1,
        id,
        name: display_name,
        version: "1.0.0".to_string(),
        author: "WorkBuddy Theme Manager".to_string(),
        description: "根据本地图片自动生成的主题".to_string(),
        preview: Some(format!("assets/background.{extension}")),
        compatibility: crate::models::Compatibility {
            manager: ">=1.0.0 <2.0.0".to_string(),
            workbuddy: vec!["5.2.x".to_string(), "5.3.x".to_string()],
        },
    };
    let theme = ThemeConfig {
        palette,
        background: crate::models::ThemeBackground {
            image: format!("assets/background.{extension}"),
            position: "right center".to_string(),
            size: "contain".to_string(),
        },
    };
    validate_manifest(&manifest)?;
    validate_theme(&theme)?;

    fs::create_dir_all(&root).map_err(|error| format!("无法创建主题库: {error}"))?;
    let staging = root.join(format!(".import-{}", unique_import_suffix()?));
    let result = (|| {
        fs::create_dir_all(staging.join("assets"))
            .map_err(|error| format!("无法创建主题目录: {error}"))?;
        let background = staging.join(&theme.background.image);
        fs::copy(&source, &background).map_err(|error| format!("无法复制主题图片: {error}"))?;
        validate_image_file(&background)?;
        write_json(&staging.join("manifest.json"), &manifest)?;
        write_json(&staging.join("theme.json"), &theme)?;
        fs::write(staging.join(IMAGE_VALIDATION_MARKER), [])
            .map_err(|error| format!("无法保存图片校验状态: {error}"))?;
        read_installed_theme_inner(&staging)?;
        commit_staged_theme(&root, &staging, &manifest.id)
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    read_installed_theme_inner(&root.join(&manifest.id))
}

#[allow(dead_code)]
fn import_package_into(
    root: &Path,
    package_path: &Path,
    overwrite_confirmed: bool,
) -> Result<InstalledTheme, String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    import_package_into_inner(root, package_path, overwrite_confirmed)
}

#[allow(dead_code)]
fn import_package_into_inner(
    root: &Path,
    package_path: &Path,
    overwrite_confirmed: bool,
) -> Result<InstalledTheme, String> {
    recover_theme_transactions_inner(root)?;
    let metadata =
        fs::metadata(package_path).map_err(|error| format!("无法读取主题包: {error}"))?;
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err("主题包超过 20 MB 限制".to_string());
    }

    let file = File::open(package_path).map_err(|error| format!("无法打开主题包: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("主题包不是有效 ZIP: {error}"))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!("主题包条目数量超过 {MAX_ARCHIVE_ENTRIES} 个限制"));
    }
    let mut names = HashSet::new();
    let mut normalized_names = HashSet::new();
    let mut file_count = 0usize;
    let mut total_size = 0u64;

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("无法读取 ZIP 条目: {error}"))?;
        if entry.encrypted() {
            return Err("主题包不能包含加密文件".to_string());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("主题包不能包含符号链接".to_string());
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err("主题包只支持 Stored 或 Deflate 压缩".to_string());
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| "主题包包含不安全路径".to_string())?;
        if entry.is_dir() {
            validate_package_directory(&enclosed)?;
            continue;
        }
        file_count += 1;
        total_size = total_size.saturating_add(entry.size());
        if file_count > MAX_FILES || total_size > MAX_UNCOMPRESSED_BYTES {
            return Err("主题包文件数量或解压体积超过限制".to_string());
        }
        validate_package_path(&enclosed)?;
        let name = path_key(&enclosed);
        if !normalized_names.insert(name.to_ascii_lowercase()) {
            return Err(format!("主题包包含重复文件: {name}"));
        }
        names.insert(name);
    }

    if !names.contains("manifest.json") || !names.contains("theme.json") {
        return Err("主题包根目录必须包含 manifest.json 和 theme.json".to_string());
    }

    let manifest: ThemeManifest = read_zip_json(&mut archive, "manifest.json")?;
    let theme: ThemeConfig = read_zip_json(&mut archive, "theme.json")?;
    validate_manifest(&manifest)?;
    validate_theme(&theme)?;
    if !names.contains(&theme.background.image) {
        return Err("theme.json 指定的背景图不存在".to_string());
    }
    if manifest
        .preview
        .as_ref()
        .is_some_and(|path| !names.contains(path))
    {
        return Err("manifest.json 指定的预览图不存在".to_string());
    }
    require_overwrite_confirmation(root, &manifest.id, overwrite_confirmed)?;

    fs::create_dir_all(root).map_err(|error| format!("无法创建主题库: {error}"))?;
    let nonce = unique_import_suffix()?;
    let staging = root.join(format!(".import-{nonce}"));
    fs::create_dir_all(&staging).map_err(|error| format!("无法创建导入临时目录: {error}"))?;

    let extraction_result = extract_archive(&mut archive, &staging);
    if let Err(error) = extraction_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    for name in &names {
        if Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "webp"
                )
            })
        {
            if let Err(error) = validate_image_file(&staging.join(name)) {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
        }
    }
    if let Err(error) = fs::write(staging.join(IMAGE_VALIDATION_MARKER), []) {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("无法保存图片校验状态: {error}"));
    }

    if let Err(error) = read_installed_theme_inner(&staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    commit_staged_theme(root, &staging, &manifest.id)?;
    read_installed_theme_inner(&root.join(&manifest.id))
}

fn require_overwrite_confirmation(root: &Path, id: &str, confirmed: bool) -> Result<(), String> {
    if root.join(id).exists() && !confirmed {
        Err(format!("{THEME_OVERWRITE_CONFIRMATION_REQUIRED}:{id}"))
    } else {
        Ok(())
    }
}

fn commit_staged_theme(root: &Path, staging: &Path, id: &str) -> Result<(), String> {
    let destination = root.join(id);
    let backup = root.join(format!(".backup-{id}-{}", unique_import_suffix()?));
    let had_existing = destination.exists();
    if had_existing {
        fs::rename(&destination, &backup).map_err(|error| format!("无法暂存已有主题: {error}"))?;
    }
    if let Err(error) = fs::rename(staging, &destination) {
        let rollback_error = had_existing
            .then(|| fs::rename(&backup, &destination).err())
            .flatten();
        return Err(match rollback_error {
            Some(rollback) => format!("无法完成主题导入: {error}；旧主题恢复失败: {rollback}"),
            None => format!("无法完成主题导入: {error}"),
        });
    }
    if had_existing {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| format!("无法生成主题配置: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("无法写入主题配置: {error}"))
}

fn normalized_display_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("主题名称长度必须为 1-80 个字符".to_string());
    }
    Ok(name.to_string())
}

fn next_available_theme_id(root: &Path, name: &str) -> Result<String, String> {
    let mut base = name
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    while base.contains("--") {
        base = base.replace("--", "-");
    }
    base = base.trim_matches('-').to_string();
    if base.is_empty() {
        base = "custom-theme".to_string();
    }
    base.truncate(72);
    base = base.trim_matches('-').to_string();
    for suffix in 0..10_000 {
        let id = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        if !root.join(&id).exists() {
            return Ok(id);
        }
    }
    Err("无法为主题生成可用 ID".to_string())
}

pub fn remove_theme(app: &AppHandle, id: &str) -> Result<(), String> {
    let _guard = THEME_STORE_LOCK
        .lock()
        .map_err(|_| "主题库运行锁异常".to_string())?;
    validate_theme_id(id)?;
    let root = themes_dir(app)?;
    recover_theme_transactions_inner(&root)?;
    if load_state(app)?.active_theme_id.as_deref() == Some(id) {
        return Err("当前使用中的主题不能删除，请先恢复官方外观".to_string());
    }
    let destination = root.join(id);
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|error| format!("无法删除主题: {error}"))?;
    }
    Ok(())
}

#[allow(dead_code)]
fn extract_archive(archive: &mut ZipArchive<File>, destination: &Path) -> Result<(), String> {
    let mut total_written = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("无法读取 ZIP 条目: {error}"))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| "主题包包含不安全路径".to_string())?
            .to_path_buf();
        let output = destination.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| format!("无法创建主题目录: {error}"))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("无法创建主题目录: {error}"))?;
        }
        let mut output_file =
            File::create(&output).map_err(|error| format!("无法写入主题文件: {error}"))?;
        let declared_size = entry.size();
        let remaining = MAX_UNCOMPRESSED_BYTES.saturating_sub(total_written);
        let copied = std::io::copy(&mut (&mut entry).take(remaining + 1), &mut output_file)
            .map_err(|error| format!("无法解压主题文件: {error}"))?;
        if copied > remaining {
            return Err("主题包实际解压体积超过 20 MB 限制".to_string());
        }
        if copied != declared_size {
            return Err(format!("ZIP 条目实际大小不一致: {}", output.display()));
        }
        total_written += copied;
        output_file
            .flush()
            .map_err(|error| format!("无法保存主题文件: {error}"))?;
    }
    Ok(())
}

#[allow(dead_code)]
fn read_zip_json<T: serde::de::DeserializeOwned>(
    archive: &mut ZipArchive<File>,
    name: &str,
) -> Result<T, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("主题包缺少 {name}"))?;
    let mut content = String::new();
    (&mut entry)
        .take(MAX_JSON_BYTES + 1)
        .read_to_string(&mut content)
        .map_err(|error| format!("无法读取 {name}: {error}"))?;
    if content.len() as u64 > MAX_JSON_BYTES {
        return Err(format!("{name} 超过 256 KB 限制"));
    }
    serde_json::from_str(&content).map_err(|error| format!("{name} 格式错误: {error}"))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    validate_regular_file(path, "JSON 文件")?;
    let content = fs::read_to_string(path)
        .map_err(|error| format!("无法读取 {}: {error}", path.display()))?;
    serde_json::from_str(&content).map_err(|error| format!("{} 格式错误: {error}", path.display()))
}

#[allow(dead_code)]
fn validate_package_path(path: &Path) -> Result<(), String> {
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("主题包包含不安全路径".to_string());
    }
    let key = path_key(path);
    if key == "manifest.json" || key == "theme.json" {
        return Ok(());
    }
    let is_asset = key.starts_with("assets/") || !key.contains('/');
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if is_asset && matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp") {
        return Ok(());
    }
    Err(format!("主题包包含不允许的文件: {key}"))
}

#[allow(dead_code)]
fn validate_package_directory(path: &Path) -> Result<(), String> {
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("主题包包含不安全目录".to_string());
    }
    let key = path_key(path);
    if key == "assets" || key.starts_with("assets/") {
        Ok(())
    } else {
        Err(format!("主题包包含不允许的目录: {key}"))
    }
}

fn validate_manifest(manifest: &ThemeManifest) -> Result<(), String> {
    if manifest.schema_version != 1 {
        return Err(format!("不支持 schemaVersion {}", manifest.schema_version));
    }
    validate_theme_id(&manifest.id)?;
    for (label, value) in [
        ("name", manifest.name.as_str()),
        ("version", manifest.version.as_str()),
        ("author", manifest.author.as_str()),
    ] {
        if value.trim().is_empty() || value.chars().count() > 80 {
            return Err(format!("manifest.json 的 {label} 无效"));
        }
    }
    if manifest.description.chars().count() > 240 {
        return Err("主题描述不能超过 240 个字符".to_string());
    }
    if manifest.compatibility.manager.trim().is_empty()
        || manifest.compatibility.workbuddy.is_empty()
    {
        return Err("主题必须声明 Manager 和 WorkBuddy 兼容范围".to_string());
    }
    validate_manager_compatibility(&manifest.compatibility.manager)?;
    validate_workbuddy_compatibility(&manifest.compatibility.workbuddy)?;
    if let Some(preview) = &manifest.preview {
        validate_relative_image_path(preview, false)?;
    }
    Ok(())
}

fn validate_workbuddy_compatibility(patterns: &[String]) -> Result<(), String> {
    for pattern in patterns {
        let pattern = pattern.trim();
        if pattern == "*" {
            continue;
        }
        let parts = pattern.split('.').collect::<Vec<_>>();
        if parts.is_empty() || parts.len() > 3 {
            return Err(format!("不支持的 WorkBuddy 兼容范围: {pattern}"));
        }
        let mut wildcard_seen = false;
        for part in parts {
            let wildcard = matches!(part, "x" | "X" | "*");
            if wildcard {
                wildcard_seen = true;
            } else if wildcard_seen
                || part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(format!("不支持的 WorkBuddy 兼容范围: {pattern}"));
            }
        }
    }
    Ok(())
}

fn validate_manager_compatibility(requirement: &str) -> Result<(), String> {
    let current = parse_semver(env!("CARGO_PKG_VERSION"))
        .ok_or_else(|| "Manager 自身版本格式无效".to_string())?;
    let requirement = requirement.trim();
    if requirement == "*" {
        return Ok(());
    }
    let mut matched = true;
    for condition in requirement.split_whitespace() {
        let (operator, raw_version) = [">=", "<=", ">", "<", "="]
            .into_iter()
            .find_map(|operator| {
                condition
                    .strip_prefix(operator)
                    .map(|value| (operator, value))
            })
            .unwrap_or(("=", condition));
        let expected = parse_semver(raw_version).ok_or_else(|| {
            format!("不支持的 Manager 兼容范围: {requirement}（请使用如 >=1.0.0 <2.0.0）")
        })?;
        matched &= match operator {
            ">=" => current >= expected,
            "<=" => current <= expected,
            ">" => current > expected,
            "<" => current < expected,
            _ => current == expected,
        };
    }
    if matched {
        Ok(())
    } else {
        Err(format!(
            "主题要求 Manager {requirement}，当前版本为 {}",
            env!("CARGO_PKG_VERSION")
        ))
    }
}

fn parse_semver(value: &str) -> Option<[u64; 3]> {
    let core = value
        .trim()
        .split_once('-')
        .map_or(value.trim(), |(core, _)| core);
    let mut parts = core.split('.');
    let version = [
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ];
    parts.next().is_none().then_some(version)
}

fn validate_theme(theme: &ThemeConfig) -> Result<(), String> {
    for color in [
        &theme.palette.background,
        &theme.palette.panel,
        &theme.palette.panel_alt,
        &theme.palette.text,
        &theme.palette.muted,
        &theme.palette.accent,
        &theme.palette.accent_text,
        &theme.palette.border,
        &theme.palette.hover,
        &theme.palette.active,
        &theme.palette.subtle,
    ] {
        if !is_hex_color(color) {
            return Err(format!("主题包含无效颜色: {color}"));
        }
    }
    let text_contrast = color_extractor::contrast(&theme.palette.text, &theme.palette.background);
    if text_contrast < MIN_TEXT_CONTRAST {
        return Err(format!(
            "主题文字与背景对比度不足（{text_contrast:.2}:1，需至少 {MIN_TEXT_CONTRAST}:1）"
        ));
    }
    validate_relative_image_path(&theme.background.image, true)?;
    if !matches!(theme.background.size.as_str(), "cover" | "contain") {
        return Err("背景 size 只能是 cover 或 contain".to_string());
    }
    if theme.background.position.trim().is_empty() || theme.background.position.len() > 40 {
        return Err("背景 position 无效".to_string());
    }
    Ok(())
}

fn validate_theme_id(id: &str) -> Result<(), String> {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 80 {
        return Err("主题 ID 长度必须为 1-80".to_string());
    }
    if !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        return Err("主题 ID 只能使用小写字母、数字和连字符".to_string());
    }
    Ok(())
}

fn validate_relative_image_path(value: &str, require_assets: bool) -> Result<(), String> {
    let path = Path::new(value);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("图片路径不安全".to_string());
    }
    if require_assets && !value.starts_with("assets/") {
        return Err("背景图必须位于 assets 目录".to_string());
    }
    let extension = path
        .extension()
        .and_then(|item| item.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp") {
        return Err("只支持 PNG、JPEG 和 WebP 图片".to_string());
    }
    Ok(())
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_image_file(path: &Path) -> Result<(), String> {
    let metadata = validate_regular_file(path, "图片")?;
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err(format!("图片超过 8 MB 限制: {}", path.display()));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("无法读取图片 {}: {error}", path.display()))?;
    let expected = match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => ImageKind::Png,
        "jpg" | "jpeg" => ImageKind::Jpeg,
        "webp" => ImageKind::Webp,
        _ => return Err("不支持的图片格式".to_string()),
    };
    let (actual, width, height) = image_metadata(&bytes)
        .ok_or_else(|| format!("图片内容损坏或格式不受支持: {}", path.display()))?;
    if actual != expected {
        return Err(format!("图片扩展名与真实格式不一致: {}", path.display()));
    }
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || pixels > MAX_IMAGE_PIXELS
    {
        return Err(format!(
            "图片尺寸超出限制: {}（{}x{}，单边最大 {}，总像素最大 {}）",
            path.display(),
            width,
            height,
            MAX_IMAGE_DIMENSION,
            MAX_IMAGE_PIXELS
        ));
    }
    let format = match actual {
        ImageKind::Png => image::ImageFormat::Png,
        ImageKind::Jpeg => image::ImageFormat::Jpeg,
        ImageKind::Webp => image::ImageFormat::WebP,
    };
    image::load_from_memory_with_format(&bytes, format)
        .map_err(|error| format!("图片无法完整解码 {}: {error}", path.display()))?;
    Ok(())
}

fn validate_regular_file(path: &Path, label: &str) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("无法读取{label} {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{label}不能是符号链接: {}", path.display()));
    }
    if !metadata.is_file() {
        return Err(format!("{label}不是普通文件: {}", path.display()));
    }
    Ok(metadata)
}

fn validate_theme_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("无法读取主题目录 {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("主题目录不能是符号链接: {}", path.display()));
    }
    if !metadata.is_dir() {
        return Err(format!("主题目录无效: {}", path.display()));
    }
    Ok(())
}

fn validate_file_within_theme(root: &Path, path: &Path, label: &str) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("{label}超出主题目录: {}", path.display()))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(format!("{label}路径无效: {}", path.display()));
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|error| format!("无法读取{label} {}: {error}", current.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "{label}路径不能包含符号链接: {}",
                current.display()
            ));
        }
    }
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("无法验证主题目录 {}: {error}", root.display()))?;
    let canonical_path = fs::canonicalize(path)
        .map_err(|error| format!("无法验证{label} {}: {error}", path.display()))?;
    if !canonical_path.starts_with(canonical_root) {
        return Err(format!("{label}超出主题目录: {}", path.display()));
    }
    validate_regular_file(path, label).map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImageKind {
    Png,
    Jpeg,
    Webp,
}

fn image_metadata(bytes: &[u8]) -> Option<(ImageKind, u32, u32)> {
    if bytes.len() >= 33
        && bytes[..8] == [137, 80, 78, 71, 13, 10, 26, 10]
        && bytes[8..12] == [0, 0, 0, 13]
        && &bytes[12..16] == b"IHDR"
    {
        return Some((
            ImageKind::Png,
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        ));
    }
    if bytes.starts_with(&[0xff, 0xd8]) && bytes.windows(2).any(|window| window == [0xff, 0xd9]) {
        let (width, height) = jpeg_dimensions(bytes)?;
        return Some((ImageKind::Jpeg, width, height));
    }
    if bytes.len() >= 30
        && &bytes[..4] == b"RIFF"
        && &bytes[8..12] == b"WEBP"
        && u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize + 8 <= bytes.len()
    {
        let (width, height) = webp_dimensions(bytes)?;
        return Some((ImageKind::Webp, width, height));
    }
    None
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut offset = 2usize;
    while offset + 3 < bytes.len() {
        while offset < bytes.len() && bytes[offset] == 0xff {
            offset += 1;
        }
        let marker = *bytes.get(offset)?;
        offset += 1;
        if marker == 0xd8 || marker == 0xd9 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = u16::from_be_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]) as usize;
        if length < 2 || offset + length > bytes.len() {
            return None;
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if length < 7 {
                return None;
            }
            let height = u16::from_be_bytes([*bytes.get(offset + 3)?, *bytes.get(offset + 4)?]);
            let width = u16::from_be_bytes([*bytes.get(offset + 5)?, *bytes.get(offset + 6)?]);
            return Some((u32::from(width), u32::from(height)));
        }
        offset += length;
    }
    None
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match &bytes[12..16] {
        b"VP8X" => {
            let width = 1 + u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]);
            let height = 1 + u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]);
            Some((width, height))
        }
        b"VP8L" if bytes[20] == 0x2f => {
            let width = 1 + u32::from(bytes[21]) + (u32::from(bytes[22] & 0x3f) << 8);
            let height = 1
                + (u32::from(bytes[22] >> 6))
                + (u32::from(bytes[23]) << 2)
                + (u32::from(bytes[24] & 0x0f) << 10);
            Some((width, height))
        }
        b"VP8 " if bytes[23..26] == [0x9d, 0x01, 0x2a] => {
            let width = u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3fff;
            let height = u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3fff;
            Some((u32::from(width), u32::from(height)))
        }
        _ => None,
    }
}

#[allow(dead_code)]
fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn unique_import_suffix() -> Result<String, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("系统时间异常: {error}"))?
        .as_nanos();
    let sequence = IMPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(format!("{}-{nanos}-{sequence}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Write,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{
        atomic_write, export_theme_into, export_theme_library_into, image_metadata,
        import_package_into, is_hex_color, list_installed_themes_inner, load_settings_files,
        load_state_files, next_available_theme_id, parse_semver, read_json, read_preset_theme,
        recover_theme_transactions_inner, validate_image_file, validate_manager_compatibility,
        validate_manifest, validate_theme, validate_theme_id, validate_workbuddy_compatibility,
        ImageKind, MAX_IMAGE_BYTES, THEME_OVERWRITE_CONFIRMATION_REQUIRED,
    };
    use crate::models::{
        ManagerSettings, ManagerState, ThemeBackground, ThemeConfig, ThemeManifest, ThemePalette,
    };

    #[test]
    fn validates_theme_ids() {
        assert!(validate_theme_id("hello-kitty").is_ok());
        assert!(validate_theme_id("Hello-Kitty").is_err());
        assert!(validate_theme_id("../hello").is_err());
        assert!(validate_theme_id("hello_").is_err());
    }

    #[test]
    fn generates_safe_unique_ids_for_image_themes() {
        let root = unique_test_directory("generated-id");
        fs::create_dir_all(root.join("sunset")).expect("create existing theme");

        assert_eq!(
            next_available_theme_id(&root, "Sunset").unwrap(),
            "sunset-1"
        );
        assert_eq!(
            next_available_theme_id(&root, "我的主题").unwrap(),
            "custom-theme"
        );

        fs::remove_dir_all(root).expect("remove generated id test directory");
    }

    #[test]
    fn validates_bundled_preset_theme_skeletons() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/preset-themes");
        let mut loaded = 0;
        for entry in fs::read_dir(&root).expect("bundled preset themes directory") {
            let path = entry.expect("preset theme entry").path();
            if !path.is_dir() {
                continue;
            }
            let id = path.file_name().and_then(|name| name.to_str()).unwrap();
            let manifest: ThemeManifest = read_json(&path.join("manifest.json"))
                .unwrap_or_else(|error| panic!("预设主题 {id} manifest 无效: {error}"));
            let theme: ThemeConfig = read_json(&path.join("theme.json"))
                .unwrap_or_else(|error| panic!("预设主题 {id} theme 无效: {error}"));
            validate_manifest(&manifest)
                .unwrap_or_else(|error| panic!("预设主题 {id} manifest 校验失败: {error}"));
            validate_theme(&theme)
                .unwrap_or_else(|error| panic!("预设主题 {id} theme 校验失败: {error}"));
            assert_eq!(manifest.id, id, "预设主题目录名与 manifest ID 不一致: {id}");
            if path.join(&theme.background.image).exists() {
                read_preset_theme(&path)
                    .unwrap_or_else(|error| panic!("预设主题 {id} 无法读取: {error}"));
            }
            loaded += 1;
        }
        assert!(
            loaded >= 6,
            "应至少发现 6 个内置预设主题骨架，实际 {loaded}"
        );
    }

    #[test]
    fn rejects_low_contrast_theme_text() {
        let theme = ThemeConfig {
            palette: ThemePalette {
                background: "#ffffff".to_string(),
                panel: "#ffffff".to_string(),
                panel_alt: "#ffffff".to_string(),
                text: "#ffffff".to_string(),
                muted: "#777777".to_string(),
                accent: "#888888".to_string(),
                accent_text: "#ffffff".to_string(),
                border: "#999999".to_string(),
                hover: "#aaaaaa".to_string(),
                active: "#bbbbbb".to_string(),
                subtle: "#cccccc".to_string(),
            },
            background: ThemeBackground {
                image: "assets/background.png".to_string(),
                position: "center center".to_string(),
                size: "cover".to_string(),
            },
        };
        assert!(validate_theme(&theme).is_err());
    }

    #[test]
    fn validates_six_digit_hex_colors() {
        assert!(is_hex_color("#d95f8d"));
        assert!(!is_hex_color("#fff"));
        assert!(!is_hex_color("rgba(0,0,0,.5)"));
    }

    #[test]
    fn validates_manager_compatibility_ranges() {
        assert!(validate_manager_compatibility(">=0.1.0 <2.0.0").is_ok());
        assert!(validate_manager_compatibility(">=2.0.0").is_err());
        assert!(validate_manager_compatibility("latest").is_err());
        assert_eq!(parse_semver("1.2.3-beta.1"), Some([1, 2, 3]));
    }

    #[test]
    fn validates_workbuddy_compatibility_patterns() {
        assert!(
            validate_workbuddy_compatibility(&["5.2.x".to_string(), "6.*".to_string()]).is_ok()
        );
        assert!(validate_workbuddy_compatibility(&["5.x.2".to_string()]).is_err());
        assert!(validate_workbuddy_compatibility(&["latest".to_string()]).is_err());
    }

    #[test]
    fn restores_a_corrupted_state_file_from_backup() {
        let root = unique_test_directory("state");
        fs::create_dir_all(&root).expect("create state test directory");
        let path = root.join("manager-state.json");
        let backup = root.join("manager-state.json.bak");
        let expected = ManagerState {
            active_theme_id: Some("hello-kitty".to_string()),
            cdp_port: Some(49152),
            workbuddy_pid: Some(73),
            ..ManagerState::default()
        };
        atomic_write(
            &backup,
            &serde_json::to_vec(&expected).expect("serialize state"),
        )
        .expect("write backup state");
        fs::write(&path, b"{broken").expect("write corrupted state");

        assert_eq!(
            load_state_files(&path, &backup).expect("recover state"),
            expected
        );
        assert_eq!(
            serde_json::from_slice::<ManagerState>(&fs::read(&path).expect("read restored state"))
                .expect("parse restored state"),
            expected
        );
        fs::remove_dir_all(root).expect("remove state test directory");
    }

    #[test]
    fn restores_corrupted_settings_from_backup_and_migrates_legacy_files() {
        let root = unique_test_directory("settings");
        fs::create_dir_all(&root).expect("create settings test directory");
        let path = root.join("manager-settings.json");
        let backup = root.join("manager-settings.json.bak");
        fs::write(
            &backup,
            br#"{"workbuddyPath":"/Applications/WorkBuddy.app"}"#,
        )
        .expect("write legacy settings backup");
        fs::write(&path, b"{broken").expect("write corrupted settings");

        let settings = load_settings_files(&path, &backup).expect("recover settings");

        assert_eq!(settings.settings_version, 1);
        assert_eq!(
            settings.workbuddy_path.as_deref(),
            Some("/Applications/WorkBuddy.app")
        );
        assert_eq!(
            serde_json::from_slice::<ManagerSettings>(
                &fs::read(&path).expect("read restored settings")
            )
            .expect("parse restored settings")
            .settings_version,
            1
        );
        assert!(has_preserved_invalid_file(
            &root,
            "manager-settings.json.invalid-"
        ));
        fs::remove_dir_all(root).expect("remove settings test directory");
    }

    #[test]
    fn preserves_unsupported_state_and_recovers_a_legacy_backup() {
        let root = unique_test_directory("unsupported-state");
        fs::create_dir_all(&root).expect("create state test directory");
        let path = root.join("manager-state.json");
        let backup = root.join("manager-state.json.bak");
        fs::write(
            &path,
            br#"{"stateVersion":2,"activeThemeId":"future-theme"}"#,
        )
        .expect("write unsupported state");
        fs::write(
            &backup,
            br#"{"activeThemeId":"hello-kitty","cdpPort":49152,"workbuddyPid":73}"#,
        )
        .expect("write legacy state backup");

        let state = load_state_files(&path, &backup).expect("recover legacy state");

        assert_eq!(state.state_version, 1);
        assert_eq!(state.active_theme_id.as_deref(), Some("hello-kitty"));
        assert!(has_preserved_invalid_file(
            &root,
            "manager-state.json.invalid-"
        ));
        fs::remove_dir_all(root).expect("remove state test directory");
    }

    #[test]
    fn preserves_both_unsupported_settings_files_before_falling_back() {
        let root = unique_test_directory("unsupported-settings");
        fs::create_dir_all(&root).expect("create settings test directory");
        let path = root.join("manager-settings.json");
        let backup = root.join("manager-settings.json.bak");
        fs::write(&path, br#"{"settingsVersion":2}"#).expect("write settings");
        fs::write(&backup, br#"{"settingsVersion":3}"#).expect("write settings backup");

        let error = load_settings_files(&path, &backup)
            .expect_err("unsupported settings files must not be accepted");

        assert!(error.contains("不支持 Manager 设置版本 2"));
        assert!(error.contains("设置备份也已损坏"));
        assert!(has_preserved_invalid_file(
            &root,
            "manager-settings.json.invalid-"
        ));
        assert!(has_preserved_invalid_file(
            &root,
            "manager-settings.json.bak.invalid-"
        ));
        fs::remove_dir_all(root).expect("remove settings test directory");
    }

    #[test]
    fn recovers_theme_backup_and_removes_stale_import() {
        let root = unique_test_directory("recovery");
        let backup = root.join(".backup-hello-kitty-fixture");
        let staging = root.join(".import-fixture");
        fs::create_dir_all(&backup).expect("create backup directory");
        fs::create_dir_all(&staging).expect("create staging directory");
        fs::write(
            backup.join("manifest.json"),
            br#"{
              "schemaVersion": 1,
              "id": "hello-kitty",
              "name": "Hello Kitty",
              "version": "1.0.0",
              "author": "Fixture",
              "compatibility": { "manager": ">=1.0.0", "workbuddy": ["5.2.x"] }
            }"#,
        )
        .expect("write backup manifest");

        recover_theme_transactions_inner(&root).expect("recover theme transaction");
        assert!(root.join("hello-kitty").is_dir());
        assert!(!backup.exists());
        assert!(!staging.exists());
        fs::remove_dir_all(root).expect("remove recovery test directory");
    }

    #[test]
    fn reads_supported_image_headers() {
        let mut png = vec![0; 33];
        png[..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        png[8..12].copy_from_slice(&13u32.to_be_bytes());
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&1920u32.to_be_bytes());
        png[20..24].copy_from_slice(&1080u32.to_be_bytes());
        assert_eq!(image_metadata(&png), Some((ImageKind::Png, 1920, 1080)));

        let mut jpeg = vec![0; 23];
        jpeg[..4].copy_from_slice(&[0xff, 0xd8, 0xff, 0xc0]);
        jpeg[4..6].copy_from_slice(&17u16.to_be_bytes());
        jpeg[6] = 8;
        jpeg[7..9].copy_from_slice(&1080u16.to_be_bytes());
        jpeg[9..11].copy_from_slice(&1920u16.to_be_bytes());
        jpeg[21..23].copy_from_slice(&[0xff, 0xd9]);
        assert_eq!(image_metadata(&jpeg), Some((ImageKind::Jpeg, 1920, 1080)));

        let mut webp = vec![0; 30];
        webp[..4].copy_from_slice(b"RIFF");
        webp[4..8].copy_from_slice(&22u32.to_le_bytes());
        webp[8..12].copy_from_slice(b"WEBP");
        webp[12..16].copy_from_slice(b"VP8X");
        webp[24..27].copy_from_slice(&[0x7f, 0x07, 0x00]);
        webp[27..30].copy_from_slice(&[0x37, 0x04, 0x00]);
        assert_eq!(image_metadata(&webp), Some((ImageKind::Webp, 1920, 1080)));
    }

    #[test]
    fn rejects_images_larger_than_the_runtime_limit() {
        let root = unique_test_directory("large-image");
        fs::create_dir_all(&root).expect("create image test directory");
        let image = root.join("large.png");
        let file = fs::File::create(&image).expect("create oversized image");
        file.set_len(MAX_IMAGE_BYTES + 1)
            .expect("set oversized image length");

        let error = validate_image_file(&image).expect_err("oversized image must be rejected");
        assert!(error.contains("8 MB"));
        fs::remove_dir_all(root).expect("remove image test directory");
    }

    #[test]
    fn rejects_an_image_with_only_a_valid_header() {
        let root = unique_test_directory("truncated-image");
        fs::create_dir_all(&root).expect("create truncated image directory");
        let image = root.join("truncated.png");
        let mut png = vec![0; 33];
        png[..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        png[8..12].copy_from_slice(&13u32.to_be_bytes());
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&1u32.to_be_bytes());
        png[20..24].copy_from_slice(&1u32.to_be_bytes());
        fs::write(&image, png).expect("write truncated png");

        let error = validate_image_file(&image).expect_err("truncated image must be rejected");

        assert!(error.contains("完整解码"));
        fs::remove_dir_all(root).expect("remove truncated image directory");
    }

    #[test]
    fn rejects_case_colliding_zip_entries() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("wbskin-duplicate-test-{nonce}"));
        fs::create_dir_all(&root).expect("create duplicate test directory");
        let package = root.join("duplicate.wbskin");
        let file = fs::File::create(&package).expect("create duplicate package");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        archive
            .start_file("assets/Background.png", options)
            .expect("start first entry");
        archive.write_all(b"first").expect("write first entry");
        archive
            .start_file("assets/background.png", options)
            .expect("start colliding entry");
        archive.write_all(b"second").expect("write second entry");
        archive.finish().expect("finish duplicate package");

        let error = import_package_into(&root.join("themes"), &package, false)
            .expect_err("case-colliding files must be rejected");
        assert!(error.contains("重复文件"));
        fs::remove_dir_all(root).expect("remove duplicate test directory");
    }

    #[test]
    fn rejects_too_many_archive_entries() {
        let root = unique_test_directory("entries");
        fs::create_dir_all(&root).expect("create entry test directory");
        let package = root.join("too-many.wbskin");
        let file = fs::File::create(&package).expect("create entry package");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for index in 0..33 {
            archive
                .add_directory(format!("assets/{index}/"), options)
                .expect("add directory entry");
        }
        archive.finish().expect("finish entry package");

        let error = import_package_into(&root.join("themes"), &package, false)
            .expect_err("oversized entry list must be rejected");
        assert!(error.contains("条目数量"));
        fs::remove_dir_all(root).expect("remove entry test directory");
    }

    #[test]
    fn imports_the_studio_hello_kitty_fixture() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_millis();
        let root = std::env::temp_dir().join(format!("wbskin-manager-test-{nonce}"));
        let package =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/Hello-Kitty.wbskin");

        let imported = import_package_into(&root, &package, false).expect("fixture should import");

        assert_eq!(imported.manifest.id, "hello-kitty");
        assert_eq!(
            imported.manifest.compatibility.workbuddy,
            ["5.2.x".to_string(), "5.3.x".to_string()]
        );
        assert_eq!(imported.manifest.compatibility.manager, ">=1.0.0 <2.0.0");
        assert_eq!(imported.theme.palette.accent, "#a63d68");
        assert_eq!(imported.theme.background.size, "contain");
        assert!(imported.background_path.ends_with("assets/background.png"));
        assert!(PathBuf::from(&imported.background_path).is_file());
        assert!(imported
            .preview_path
            .as_deref()
            .is_some_and(|path| PathBuf::from(path).is_file()));
        assert!(!imported.background_path.contains(".import-"));
        let background = PathBuf::from(&imported.background_path);
        let original_background =
            fs::read(&background).expect("existing background should remain readable");
        let conflict = import_package_into(&root, &package, false)
            .expect_err("existing theme should require overwrite confirmation");
        assert!(conflict.contains(THEME_OVERWRITE_CONFIRMATION_REQUIRED));
        assert_eq!(
            fs::read(&background).expect("existing background should remain readable"),
            original_background
        );
        let updated = import_package_into(&root, &package, true)
            .expect("confirmed fixture should update atomically");
        assert_eq!(updated.manifest.id, "hello-kitty");
        let background = PathBuf::from(&updated.background_path);
        fs::OpenOptions::new()
            .write(true)
            .open(&background)
            .expect("open installed background")
            .set_len(MAX_IMAGE_BYTES + 1)
            .expect("replace installed background with oversized content");
        let error = super::read_installed_theme(
            background
                .parent()
                .and_then(|path| path.parent())
                .expect("installed theme directory"),
        )
        .expect_err("application read must revalidate replaced images");
        assert!(error.contains("8 MB"));
        fs::remove_dir_all(root).expect("remove test theme library");
    }

    #[test]
    fn exports_an_importable_theme_package_atomically() {
        let root = unique_test_directory("export");
        let installed_root = root.join("themes");
        let roundtrip_root = root.join("roundtrip");
        let package =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/Hello-Kitty.wbskin");
        let imported =
            import_package_into(&installed_root, &package, false).expect("fixture should import");
        let exported = root.join("hello-kitty.wbskin");
        fs::create_dir_all(&root).expect("create export directory");
        fs::write(&exported, b"previous package").expect("write previous export");

        export_theme_into(&installed_root.join("hello-kitty"), &exported)
            .expect("theme should export");
        let roundtrip = import_package_into(&roundtrip_root, &exported, false)
            .expect("export should re-import");

        assert_eq!(roundtrip.manifest.id, imported.manifest.id);
        assert_eq!(roundtrip.manifest.name, imported.manifest.name);
        assert_eq!(
            roundtrip.theme.background.image,
            imported.theme.background.image
        );
        assert_eq!(
            roundtrip.theme.palette.accent,
            imported.theme.palette.accent
        );
        assert!(PathBuf::from(roundtrip.background_path).is_file());
        fs::remove_dir_all(root).expect("remove export test directory");
    }

    #[test]
    fn backs_up_the_installed_theme_library_as_importable_packages() {
        let root = unique_test_directory("library-backup");
        let installed_root = root.join("themes");
        let destination = root.join("backup");
        let restore_root = root.join("restored");
        let package =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/Hello-Kitty.wbskin");
        import_package_into(&installed_root, &package, false).expect("fixture should import");
        fs::create_dir_all(&destination).expect("create library backup directory");
        fs::write(
            destination.join("hello-kitty.wbskin"),
            b"existing user file",
        )
        .expect("write existing destination file");

        let result = export_theme_library_into(&installed_root, &destination)
            .expect("theme library should export");
        let backup_directory = PathBuf::from(&result.path);
        let backup = backup_directory.join("hello-kitty.wbskin");
        let restored = import_package_into(&restore_root, &backup, false)
            .expect("library backup should re-import");

        assert_eq!(result.count, 1);
        assert!(backup_directory.starts_with(&destination));
        assert_eq!(restored.manifest.id, "hello-kitty");
        assert!(backup.is_file());
        assert_eq!(
            fs::read(destination.join("hello-kitty.wbskin"))
                .expect("read existing destination file"),
            b"existing user file"
        );
        fs::remove_dir_all(root).expect("remove library backup test directory");
    }

    #[test]
    fn reports_a_corrupted_installed_theme() {
        let root = unique_test_directory("broken-theme");
        let package =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/Hello-Kitty.wbskin");
        import_package_into(&root, &package, false).expect("fixture should import");
        fs::write(root.join("hello-kitty/theme.json"), b"{broken")
            .expect("corrupt installed theme");

        let library = list_installed_themes_inner(&root).expect("scan theme library");

        assert!(library.themes.is_empty());
        assert_eq!(library.broken_themes.len(), 1);
        assert_eq!(library.broken_themes[0].id, "hello-kitty");
        assert!(library.broken_themes[0].reason.contains("格式错误"));
        fs::remove_dir_all(root).expect("remove broken theme test directory");
    }

    #[test]
    fn revalidates_an_installed_image_after_its_fingerprint_changes() {
        let root = unique_test_directory("changed-image");
        let package =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/Hello-Kitty.wbskin");
        let imported = import_package_into(&root, &package, false).expect("fixture should import");
        list_installed_themes_inner(&root).expect("prime validation marker");
        fs::write(&imported.background_path, b"not an image").expect("replace installed image");

        let library = list_installed_themes_inner(&root).expect("scan changed theme");

        assert!(library.themes.is_empty());
        assert_eq!(library.broken_themes.len(), 1);
        assert_eq!(library.broken_themes[0].id, "hello-kitty");
        assert!(library.broken_themes[0].reason.contains("图片内容损坏"));
        fs::remove_dir_all(root).expect("remove changed image test directory");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_an_installed_image_replaced_by_a_symlink() {
        use std::os::unix::fs::symlink;

        let root = unique_test_directory("installed-symlink");
        let package =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/Hello-Kitty.wbskin");
        let imported = import_package_into(&root, &package, false).expect("fixture should import");
        let background = PathBuf::from(imported.background_path);
        let external = root.join("external.png");
        fs::copy(&background, &external).expect("copy external image");
        fs::remove_file(&background).expect("remove installed image");
        symlink(&external, &background).expect("replace image with symlink");

        let error = super::read_installed_theme_inner(&root.join("hello-kitty"))
            .expect_err("installed symlink must be rejected");

        assert!(error.contains("符号链接"));
        fs::remove_dir_all(root).expect("remove symlink test directory");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_an_installed_image_beneath_a_symlinked_directory() {
        use std::os::unix::fs::symlink;

        let root = unique_test_directory("installed-parent-symlink");
        let package =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/Hello-Kitty.wbskin");
        import_package_into(&root, &package, false).expect("fixture should import");
        let theme_dir = root.join("hello-kitty");
        let assets = theme_dir.join("assets");
        let external = root.join("external-assets");
        fs::rename(&assets, &external).expect("move assets outside theme directory");
        symlink(&external, &assets).expect("replace assets directory with symlink");

        let error = super::read_installed_theme_inner(&theme_dir)
            .expect_err("parent directory symlink must be rejected");

        assert!(error.contains("符号链接"));
        fs::remove_dir_all(root).expect("remove parent symlink test directory");
    }

    fn unique_test_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("wbskin-{label}-test-{nonce}"))
    }

    fn has_preserved_invalid_file(root: &std::path::Path, prefix: &str) -> bool {
        fs::read_dir(root)
            .expect("read test directory")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
    }
}
