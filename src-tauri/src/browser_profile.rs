use crate::config::BrowserProfileSettings;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub const PROFILE_MARKER_FILE_NAME: &str = ".pickgauge-profile.json";

const APP_IDENTIFIER: &str = "com.pickforge.pickgauge";
const MARKER_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserProfilePaths {
    pub root: PathBuf,
    pub codex: PathBuf,
    pub claude: PathBuf,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserProfileService {
    Codex,
    Claude,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserProfileMarker {
    schema_version: u32,
    app_identifier: String,
    service: BrowserProfileService,
    created_at: String,
}

pub fn should_prepare_browser_profiles(
    settings: &BrowserProfileSettings,
    web_enabled: bool,
) -> bool {
    web_enabled
        || configured_path(&settings.root_path).is_some()
        || configured_path(&settings.codex_path).is_some()
        || configured_path(&settings.claude_path).is_some()
}

pub fn prepare_browser_profiles(
    settings: &BrowserProfileSettings,
    app_data_dir: &Path,
) -> Result<BrowserProfilePaths, String> {
    let app_data_dir = prepare_app_data_dir(app_data_dir)?;
    let paths = resolve_browser_profile_paths(settings, &app_data_dir)?;

    ensure_profile_root_directory(&paths.root)?;
    ensure_profile_directory(&paths.codex, BrowserProfileService::Codex)?;
    ensure_profile_directory(&paths.claude, BrowserProfileService::Claude)?;

    Ok(paths)
}

pub fn clear_browser_profile(
    settings: &BrowserProfileSettings,
    app_data_dir: &Path,
    service: BrowserProfileService,
) -> Result<bool, String> {
    let paths = resolve_browser_profile_paths(settings, app_data_dir)?;
    let profile_path = paths.service_path(service);

    if !profile_path.exists() {
        return Ok(false);
    }

    reject_symlink_path(profile_path)?;
    reject_known_default_browser_profile(profile_path)?;

    if !profile_path.is_dir() {
        return Err("Browser profile path must be a directory".to_string());
    }

    let marker_path = profile_path.join(PROFILE_MARKER_FILE_NAME);
    let metadata = fs::symlink_metadata(&marker_path)
        .map_err(|_| "Browser profile directory is not app-owned".to_string())?;

    if metadata.file_type().is_symlink() {
        return Err("Browser profile marker must not be a symlink".to_string());
    }

    if !metadata.is_file() {
        return Err("Browser profile marker must be a file".to_string());
    }

    verify_marker(&marker_path, service)?;
    fs::remove_dir_all(profile_path)
        .map_err(|error| format!("Could not remove browser profile directory: {error}"))?;
    remove_empty_profile_root(&paths.root)?;

    Ok(true)
}

impl BrowserProfilePaths {
    fn service_path(&self, service: BrowserProfileService) -> &Path {
        match service {
            BrowserProfileService::Codex => &self.codex,
            BrowserProfileService::Claude => &self.claude,
        }
    }
}

fn prepare_app_data_dir(path: &Path) -> Result<PathBuf, String> {
    reject_symlink_path(path)?;
    fs::create_dir_all(path)
        .map_err(|error| format!("Could not create app data directory: {error}"))?;
    let path = canonicalize_existing_path(path)?;
    set_restrictive_directory_permissions(&path)?;
    Ok(path)
}

fn resolve_browser_profile_paths(
    settings: &BrowserProfileSettings,
    app_data_dir: &Path,
) -> Result<BrowserProfilePaths, String> {
    let root = match configured_path(&settings.root_path) {
        Some(path) => resolve_configured_path(path)?,
        None => app_data_dir.join("browser-profiles"),
    };
    let root = canonicalize_browser_profile_path(&root)?;

    let codex = match configured_path(&settings.codex_path) {
        Some(path) => resolve_configured_path(path)?,
        None => root.join("codex"),
    };
    let claude = match configured_path(&settings.claude_path) {
        Some(path) => resolve_configured_path(path)?,
        None => root.join("claude"),
    };

    let paths = BrowserProfilePaths {
        root,
        codex: canonicalize_browser_profile_path(&codex)?,
        claude: canonicalize_browser_profile_path(&claude)?,
    };
    reject_overlapping_profile_paths(&paths)?;

    Ok(paths)
}

fn ensure_profile_root_directory(path: &Path) -> Result<(), String> {
    reject_symlink_path(path)?;
    reject_known_default_browser_profile(path)?;

    if path.exists() && !path.is_dir() {
        return Err("Browser profile root path must be a directory".to_string());
    }

    fs::create_dir_all(path)
        .map_err(|error| format!("Could not create browser profile root directory: {error}"))?;
    set_restrictive_directory_permissions(path)
}

fn ensure_profile_directory(path: &Path, service: BrowserProfileService) -> Result<(), String> {
    reject_symlink_path(path)?;
    reject_known_default_browser_profile(path)?;

    if path.exists() && !path.is_dir() {
        return Err("Browser profile path must be a directory".to_string());
    }

    fs::create_dir_all(path)
        .map_err(|error| format!("Could not create browser profile directory: {error}"))?;

    let marker_path = path.join(PROFILE_MARKER_FILE_NAME);

    if let Ok(metadata) = fs::symlink_metadata(&marker_path) {
        if metadata.file_type().is_symlink() {
            return Err("Browser profile marker must not be a symlink".to_string());
        }

        if !metadata.is_file() {
            return Err("Browser profile marker must be a file".to_string());
        }

        verify_marker(&marker_path, service)?;
        set_restrictive_file_permissions(&marker_path)?;
        return set_restrictive_directory_permissions(path);
    }

    if directory_has_non_marker_contents(path)? {
        return Err("Browser profile directory is not app-owned or empty".to_string());
    }

    write_marker(&marker_path, service)?;
    set_restrictive_directory_permissions(path)
}

fn configured_path(value: &Option<String>) -> Option<&Path> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Path::new)
}

fn resolve_configured_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Browser profile paths must be absolute".to_string());
    }

    canonicalize_browser_profile_path(path)
}

fn canonicalize_browser_profile_path(path: &Path) -> Result<PathBuf, String> {
    let path = canonicalize_with_missing_tail(path)?;
    reject_known_default_browser_profile(&path)?;
    Ok(path)
}

fn canonicalize_existing_path(path: &Path) -> Result<PathBuf, String> {
    reject_symlink_path(path)?;
    fs::canonicalize(path).map_err(|error| format!("Could not canonicalize app data path: {error}"))
}

fn reject_overlapping_profile_paths(paths: &BrowserProfilePaths) -> Result<(), String> {
    if paths_overlap(&paths.codex, &paths.claude) {
        return Err("Browser profile paths must be separate per service".to_string());
    }

    if paths.root == paths.codex || paths.root == paths.claude {
        return Err("Browser profile root must not be a service profile path".to_string());
    }

    if paths.root.starts_with(&paths.codex) || paths.root.starts_with(&paths.claude) {
        return Err("Browser profile root must not be inside a service profile path".to_string());
    }

    Ok(())
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn canonicalize_with_missing_tail(path: &Path) -> Result<PathBuf, String> {
    reject_symlink_path(path)?;

    if path.exists() {
        return fs::canonicalize(path)
            .map_err(|error| format!("Could not canonicalize browser profile path: {error}"));
    }

    let mut missing_components = Vec::new();
    let mut existing = path;

    while !existing.exists() {
        let file_name = existing
            .file_name()
            .ok_or_else(|| "Browser profile path must have an existing parent".to_string())?;
        missing_components.push(file_name.to_owned());
        existing = existing
            .parent()
            .ok_or_else(|| "Browser profile path must have an existing parent".to_string())?;
    }

    reject_symlink_path(existing)?;
    let mut canonical = fs::canonicalize(existing)
        .map_err(|error| format!("Could not canonicalize browser profile path: {error}"))?;

    for component in missing_components.iter().rev() {
        canonical.push(component);
    }

    Ok(canonical)
}

fn reject_symlink_path(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();

    for component in path.components() {
        current.push(component);

        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                return Err("Browser profile paths must not contain symlinks".to_string());
            }
        }
    }

    Ok(())
}

fn reject_known_default_browser_profile(path: &Path) -> Result<(), String> {
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return Ok(());
    };
    let home = fs::canonicalize(&home).unwrap_or(home);
    let blocked_paths = [
        home.join(".config/google-chrome"),
        home.join(".config/chromium"),
        home.join(".config/BraveSoftware"),
        home.join(".config/microsoft-edge"),
        home.join(".config/vivaldi"),
        home.join(".config/opera"),
        home.join(".mozilla/firefox"),
        home.join(".var/app/com.google.Chrome"),
        home.join(".var/app/com.brave.Browser"),
        home.join(".var/app/org.chromium.Chromium"),
        home.join(".var/app/org.mozilla.firefox"),
    ];

    if blocked_paths
        .iter()
        .any(|blocked_path| path.starts_with(blocked_path))
    {
        return Err("Default browser profile paths are not allowed".to_string());
    }

    Ok(())
}

fn directory_has_non_marker_contents(path: &Path) -> Result<bool, String> {
    let entries = fs::read_dir(path)
        .map_err(|error| format!("Could not inspect profile directory: {error}"))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not inspect profile entry: {error}"))?;

        if entry.file_name() != PROFILE_MARKER_FILE_NAME {
            return Ok(true);
        }
    }

    Ok(false)
}

fn remove_empty_profile_root(path: &Path) -> Result<(), String> {
    if !path.exists() || !path.is_dir() {
        return Ok(());
    }

    if !directory_is_empty(path)? {
        return Ok(());
    }

    fs::remove_dir(path).map_err(|error| format!("Could not remove browser profile root: {error}"))
}

fn directory_is_empty(path: &Path) -> Result<bool, String> {
    fs::read_dir(path)
        .map_err(|error| format!("Could not inspect profile directory: {error}"))?
        .next()
        .transpose()
        .map(|entry| entry.is_none())
        .map_err(|error| format!("Could not inspect profile entry: {error}"))
}

fn verify_marker(path: &Path, service: BrowserProfileService) -> Result<(), String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("Could not read profile marker: {error}"))?;
    let marker = serde_json::from_str::<BrowserProfileMarker>(&raw)
        .map_err(|error| format!("Could not parse profile marker: {error}"))?;

    if marker.schema_version != MARKER_SCHEMA_VERSION
        || marker.app_identifier != APP_IDENTIFIER
        || marker.service != service
    {
        return Err("Browser profile marker does not match PickGauge ownership".to_string());
    }

    Ok(())
}

fn write_marker(path: &Path, service: BrowserProfileService) -> Result<(), String> {
    let marker = BrowserProfileMarker {
        schema_version: MARKER_SCHEMA_VERSION,
        app_identifier: APP_IDENTIFIER.to_string(),
        service,
        created_at: now_rfc3339(),
    };
    let raw = serde_json::to_string_pretty(&marker)
        .map_err(|error| format!("Could not serialize profile marker: {error}"))?;

    fs::write(path, raw).map_err(|error| format!("Could not write profile marker: {error}"))?;
    set_restrictive_file_permissions(path)
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(unix)]
fn set_restrictive_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Could not set profile marker permissions: {error}"))
}

#[cfg(not(unix))]
fn set_restrictive_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn set_restrictive_directory_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Could not set profile directory permissions: {error}"))
}

#[cfg(not(unix))]
fn set_restrictive_directory_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "pickgauge-browser-profile-test-{}-{id}",
                std::process::id()
            ));

            fs::create_dir_all(&path).expect("test directory is created");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn empty_settings() -> BrowserProfileSettings {
        BrowserProfileSettings {
            root_path: None,
            codex_path: None,
            claude_path: None,
        }
    }

    #[test]
    fn web_enabled_profiles_use_default_app_owned_paths() {
        let dir = TestDir::new();
        let paths =
            prepare_browser_profiles(&empty_settings(), &dir.path).expect("profiles prepare");

        assert_eq!(paths.root, dir.path.join("browser-profiles"));
        assert!(paths.codex.join(PROFILE_MARKER_FILE_NAME).exists());
        assert!(paths.claude.join(PROFILE_MARKER_FILE_NAME).exists());
    }

    #[test]
    fn missing_app_data_dir_is_created_before_default_profiles() {
        let dir = TestDir::new();
        let app_data_dir = dir.path.join("nested").join("app-data");

        let paths =
            prepare_browser_profiles(&empty_settings(), &app_data_dir).expect("profiles prepare");

        assert!(app_data_dir.exists());
        assert!(paths.codex.join(PROFILE_MARKER_FILE_NAME).exists());
        assert!(paths.claude.join(PROFILE_MARKER_FILE_NAME).exists());
    }

    #[test]
    fn configured_profile_paths_are_preserved_and_marked() {
        let dir = TestDir::new();
        let settings = BrowserProfileSettings {
            root_path: Some(dir.path.join("root").to_string_lossy().to_string()),
            codex_path: Some(dir.path.join("codex-custom").to_string_lossy().to_string()),
            claude_path: Some(dir.path.join("claude-custom").to_string_lossy().to_string()),
        };

        let paths = prepare_browser_profiles(&settings, &dir.path).expect("profiles prepare");

        assert_eq!(paths.root, dir.path.join("root"));
        assert_eq!(paths.codex, dir.path.join("codex-custom"));
        assert_eq!(paths.claude, dir.path.join("claude-custom"));
        assert!(paths.codex.join(PROFILE_MARKER_FILE_NAME).exists());
        assert!(paths.claude.join(PROFILE_MARKER_FILE_NAME).exists());
    }

    #[test]
    fn configured_service_profile_paths_must_be_distinct() {
        let dir = TestDir::new();
        let shared_path = dir.path.join("shared-profile");
        let settings = BrowserProfileSettings {
            codex_path: Some(shared_path.to_string_lossy().to_string()),
            claude_path: Some(shared_path.to_string_lossy().to_string()),
            ..empty_settings()
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("separate per service"));
        assert!(!shared_path.exists());
    }

    #[test]
    fn configured_service_profile_paths_must_not_be_nested() {
        let dir = TestDir::new();
        let codex_path = dir.path.join("codex-profile");
        let claude_path = codex_path.join("claude-profile");
        let settings = BrowserProfileSettings {
            codex_path: Some(codex_path.to_string_lossy().to_string()),
            claude_path: Some(claude_path.to_string_lossy().to_string()),
            ..empty_settings()
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("separate per service"));
        assert!(!codex_path.exists());
    }

    #[test]
    fn profile_root_must_not_overlap_service_profile_paths() {
        let dir = TestDir::new();
        let codex_path = dir.path.join("codex-profile");
        let settings = BrowserProfileSettings {
            root_path: Some(
                codex_path
                    .join("profile-root")
                    .to_string_lossy()
                    .to_string(),
            ),
            codex_path: Some(codex_path.to_string_lossy().to_string()),
            claude_path: Some(
                dir.path
                    .join("claude-profile")
                    .to_string_lossy()
                    .to_string(),
            ),
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("root must not be inside"));
        assert!(!codex_path.exists());
    }

    #[test]
    fn profile_root_must_not_equal_service_profile_path() {
        let dir = TestDir::new();
        let codex_path = dir.path.join("codex-profile");
        let settings = BrowserProfileSettings {
            root_path: Some(codex_path.to_string_lossy().to_string()),
            codex_path: Some(codex_path.to_string_lossy().to_string()),
            claude_path: Some(
                dir.path
                    .join("claude-profile")
                    .to_string_lossy()
                    .to_string(),
            ),
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("root must not be a service"));
        assert!(!codex_path.exists());
    }

    #[test]
    fn relative_configured_profile_paths_are_rejected() {
        let dir = TestDir::new();
        let settings = BrowserProfileSettings {
            root_path: Some("relative/path".to_string()),
            ..empty_settings()
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("absolute"));
    }

    #[test]
    fn non_empty_unmarked_directories_are_rejected() {
        let dir = TestDir::new();
        let codex_path = dir.path.join("codex");
        fs::create_dir_all(&codex_path).expect("profile dir is created");
        fs::write(codex_path.join("foreign-file"), "data").expect("foreign file is written");
        let settings = BrowserProfileSettings {
            codex_path: Some(codex_path.to_string_lossy().to_string()),
            ..empty_settings()
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("not app-owned or empty"));
    }

    #[cfg(unix)]
    #[test]
    fn non_empty_unmarked_directories_are_rejected_without_permission_changes() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new();
        let codex_path = dir.path.join("codex");
        fs::create_dir_all(&codex_path).expect("profile dir is created");
        fs::set_permissions(&codex_path, fs::Permissions::from_mode(0o755))
            .expect("permissions are set");
        fs::write(codex_path.join("foreign-file"), "data").expect("foreign file is written");
        let settings = BrowserProfileSettings {
            codex_path: Some(codex_path.to_string_lossy().to_string()),
            ..empty_settings()
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");
        let mode = fs::metadata(&codex_path)
            .expect("profile dir metadata")
            .permissions()
            .mode()
            & 0o777;

        assert!(error.contains("not app-owned or empty"));
        assert_eq!(mode, 0o755);
    }

    #[test]
    fn known_default_browser_profile_paths_are_rejected() {
        let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        let dir = TestDir::new();
        let default_browser_path = home
            .join(".config")
            .join("google-chrome")
            .join("PickGaugeTestProfile");
        let settings = BrowserProfileSettings {
            codex_path: Some(default_browser_path.to_string_lossy().to_string()),
            ..empty_settings()
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("Default browser profile"));
    }

    #[test]
    fn mismatched_ownership_marker_is_rejected() {
        let dir = TestDir::new();
        let codex_path = dir.path.join("codex");
        fs::create_dir_all(&codex_path).expect("profile dir is created");
        fs::write(
            codex_path.join(PROFILE_MARKER_FILE_NAME),
            r#"{"schemaVersion":1,"appIdentifier":"other","service":"codex","createdAt":"2026-06-03T00:00:00Z"}"#,
        )
        .expect("marker is written");
        let settings = BrowserProfileSettings {
            codex_path: Some(codex_path.to_string_lossy().to_string()),
            ..empty_settings()
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("does not match"));
    }

    #[cfg(unix)]
    #[test]
    fn marker_symlinks_are_rejected() {
        use std::os::unix::fs::symlink;

        let dir = TestDir::new();
        let codex_path = dir.path.join("codex");
        let target = dir.path.join("marker-target");
        fs::create_dir_all(&codex_path).expect("profile dir is created");
        fs::write(&target, "{}").expect("target is written");
        symlink(&target, codex_path.join(PROFILE_MARKER_FILE_NAME)).expect("symlink is created");
        let settings = BrowserProfileSettings {
            codex_path: Some(codex_path.to_string_lossy().to_string()),
            ..empty_settings()
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("marker must not be a symlink"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_profile_paths_are_rejected() {
        use std::os::unix::fs::symlink;

        let dir = TestDir::new();
        let target = dir.path.join("target");
        let link = dir.path.join("link");
        fs::create_dir_all(&target).expect("target dir is created");
        symlink(&target, &link).expect("symlink is created");
        let settings = BrowserProfileSettings {
            codex_path: Some(link.to_string_lossy().to_string()),
            ..empty_settings()
        };

        let error = prepare_browser_profiles(&settings, &dir.path).expect_err("path is rejected");

        assert!(error.contains("symlinks"));
    }

    #[cfg(unix)]
    #[test]
    fn profile_markers_and_directories_use_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new();
        let paths =
            prepare_browser_profiles(&empty_settings(), &dir.path).expect("profiles prepare");
        let marker_mode = fs::metadata(paths.codex.join(PROFILE_MARKER_FILE_NAME))
            .expect("marker metadata")
            .permissions()
            .mode()
            & 0o777;
        let root_mode = fs::metadata(&paths.root)
            .expect("profile root metadata")
            .permissions()
            .mode()
            & 0o777;
        let dir_mode = fs::metadata(&paths.codex)
            .expect("profile dir metadata")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(marker_mode, 0o600);
        assert_eq!(root_mode, 0o700);
        assert_eq!(dir_mode, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn existing_profile_marker_permissions_are_tightened() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new();
        let paths =
            prepare_browser_profiles(&empty_settings(), &dir.path).expect("profiles prepare");
        let marker_path = paths.codex.join(PROFILE_MARKER_FILE_NAME);
        fs::set_permissions(&marker_path, fs::Permissions::from_mode(0o644))
            .expect("permissions are widened");

        prepare_browser_profiles(&empty_settings(), &dir.path).expect("profiles prepare again");

        let marker_mode = fs::metadata(marker_path)
            .expect("marker metadata")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(marker_mode, 0o600);
    }

    #[test]
    fn clear_browser_profile_removes_only_marked_service_directory() {
        let dir = TestDir::new();
        let paths =
            prepare_browser_profiles(&empty_settings(), &dir.path).expect("profiles prepare");
        fs::write(paths.codex.join("browser-data"), "data").expect("profile data is written");
        fs::write(paths.claude.join("browser-data"), "data").expect("profile data is written");

        let cleared =
            clear_browser_profile(&empty_settings(), &dir.path, BrowserProfileService::Codex)
                .expect("profile clears");

        assert!(cleared);
        assert!(!paths.codex.exists());
        assert!(paths.claude.exists());
    }

    #[test]
    fn clear_browser_profile_returns_false_for_missing_profile() {
        let dir = TestDir::new();

        let cleared =
            clear_browser_profile(&empty_settings(), &dir.path, BrowserProfileService::Claude)
                .expect("missing profile is safe");

        assert!(!cleared);
    }

    #[test]
    fn clear_browser_profile_rejects_unmarked_directory() {
        let dir = TestDir::new();
        let codex_path = dir.path.join("browser-profiles").join("codex");
        fs::create_dir_all(&codex_path).expect("profile dir is created");

        let error =
            clear_browser_profile(&empty_settings(), &dir.path, BrowserProfileService::Codex)
                .expect_err("unmarked profile is rejected");

        assert!(error.contains("not app-owned"));
        assert!(codex_path.exists());
    }

    #[test]
    fn clear_browser_profile_rejects_mismatched_marker() {
        let dir = TestDir::new();
        let paths =
            prepare_browser_profiles(&empty_settings(), &dir.path).expect("profiles prepare");

        let error =
            clear_browser_profile(&empty_settings(), &dir.path, BrowserProfileService::Codex)
                .expect("codex profile clears");
        assert!(error);

        let error =
            clear_browser_profile(&empty_settings(), &dir.path, BrowserProfileService::Codex)
                .expect("missing cleared profile is safe");
        assert!(!error);

        fs::create_dir_all(&paths.codex).expect("profile dir is recreated");
        fs::write(
            paths.codex.join(PROFILE_MARKER_FILE_NAME),
            r#"{"schemaVersion":1,"appIdentifier":"com.pickforge.pickgauge","service":"claude","createdAt":"2026-06-03T00:00:00Z"}"#,
        )
        .expect("marker is written");

        let error =
            clear_browser_profile(&empty_settings(), &dir.path, BrowserProfileService::Codex)
                .expect_err("mismatched marker is rejected");

        assert!(error.contains("does not match"));
        assert!(paths.codex.exists());
    }
}
