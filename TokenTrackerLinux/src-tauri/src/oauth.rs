use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, Url};

#[derive(Default)]
pub struct PendingAuthCode(Mutex<Option<String>>);

#[derive(Default)]
pub struct DashboardBaseUrl(Mutex<Option<String>>);

impl DashboardBaseUrl {
    pub fn store(&self, url: String) {
        if let Ok(mut dashboard_url) = self.0.lock() {
            *dashboard_url = Some(url);
        }
    }

    fn get(&self) -> Option<String> {
        self.0.lock().ok()?.clone()
    }
}

impl PendingAuthCode {
    pub fn store(&self, code: String) {
        if let Ok(mut pending) = self.0.lock() {
            *pending = Some(code);
        }
    }

    pub fn take(&self) -> Option<String> {
        self.0.lock().ok()?.take()
    }
}

pub fn parse_auth_callback(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    if url.scheme() != "tokentracker"
        || url.host_str() != Some("auth")
        || url.path() != "/callback"
        || url.fragment().is_some()
    {
        return None;
    }

    let mut codes = url
        .query_pairs()
        .filter(|(key, _)| key == "insforge_code")
        .map(|(_, value)| value.into_owned());
    let code = codes.next()?;
    if code.is_empty() || codes.next().is_some() {
        return None;
    }
    Some(code)
}

pub fn is_allowed_oauth_url(raw: &str) -> bool {
    Url::parse(raw)
        .map(|url| url.scheme() == "https" && url.host_str().is_some())
        .unwrap_or(false)
}

fn desktop_exec_quote(path: &Path) -> Option<String> {
    let raw = path.to_str()?;
    if raw.is_empty() || raw.chars().any(|ch| matches!(ch, '\n' | '\r' | '\0')) {
        return None;
    }
    // Desktop Entry Exec quoting: backslash-escape characters that retain a
    // special meaning inside double quotes. `%` must also be doubled or it is
    // interpreted as a field code before the real `%u` argument.
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    Some(format!("\"{escaped}\""))
}

pub fn appimage_desktop_entry(appimage: &Path) -> Option<String> {
    let executable = desktop_exec_quote(appimage)?;
    Some(format!(
        "[Desktop Entry]\nType=Application\nName=TokenTracker\nComment=Local AI token usage tracker\nExec={executable} %u\nIcon=tokentracker-linux\nTerminal=false\nCategories=Development;Utility;\nStartupNotify=true\nMimeType=x-scheme-handler/tokentracker;\nX-AppImage-Integrate=false\n"
    ))
}

fn user_applications_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(value).join("applications"));
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".local/share/applications"))
}

/// Register the custom callback scheme for a directly launched AppImage.
///
/// Arch installs the repository's desktop file system-wide. A bare AppImage
/// has no installer, so without this per-user entry the browser cannot return
/// the PKCE code to the already-running process. Registration happens before
/// the dashboard is shown and is refreshed on every launch in case the user
/// moved or renamed the AppImage.
pub fn ensure_appimage_protocol_registration() -> Result<bool, String> {
    let Some(appimage_raw) = std::env::var_os("APPIMAGE").filter(|value| !value.is_empty()) else {
        return Ok(false);
    };
    let appimage = PathBuf::from(appimage_raw);
    if !appimage.is_absolute() || !appimage.is_file() {
        return Err("APPIMAGE must point to an existing absolute file".to_string());
    }
    let applications_dir = user_applications_dir()
        .ok_or_else(|| "HOME/XDG_DATA_HOME is unavailable for protocol registration".to_string())?;
    fs::create_dir_all(&applications_dir)
        .map_err(|error| format!("failed to create {}: {error}", applications_dir.display()))?;
    let desktop_name = "tokentracker-appimage.desktop";
    let desktop_path = applications_dir.join(desktop_name);
    let content = appimage_desktop_entry(&appimage)
        .ok_or_else(|| "AppImage path cannot be represented in a desktop entry".to_string())?;
    let temporary = desktop_path.with_extension(format!("desktop.{}.tmp", std::process::id()));
    fs::write(&temporary, content)
        .and_then(|()| fs::rename(&temporary, &desktop_path))
        .map_err(|error| format!("failed to write {}: {error}", desktop_path.display()))?;

    let status = Command::new("xdg-mime")
        .args(["default", desktop_name, "x-scheme-handler/tokentracker"])
        .status()
        .map_err(|error| format!("failed to run xdg-mime: {error}"))?;
    if !status.success() {
        return Err(format!("xdg-mime exited with {status}"));
    }

    let query = Command::new("xdg-mime")
        .args(["query", "default", "x-scheme-handler/tokentracker"])
        .output()
        .map_err(|error| format!("failed to verify xdg-mime registration: {error}"))?;
    let registered = String::from_utf8_lossy(&query.stdout).trim().to_string();
    if !query.status.success() || registered != desktop_name {
        return Err(format!(
            "protocol registration did not read back (expected {desktop_name}, got {registered:?})"
        ));
    }
    Ok(true)
}

pub fn callback_url(base: &str, code: &str) -> Option<String> {
    let mut url = Url::parse(base).ok()?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }

    url.set_path("/auth/callback");
    url.query_pairs_mut()
        .append_pair("insforge_code", code)
        .append_pair("app", "1");
    Some(url.into())
}

pub fn handle_callback(app: &AppHandle, raw: &str) -> bool {
    let Some(code) = parse_auth_callback(raw) else {
        return false;
    };

    let pending = app.state::<PendingAuthCode>();
    pending.store(code);
    deliver_pending_callback(app)
}

pub fn deliver_pending_callback(app: &AppHandle) -> bool {
    let pending = app.state::<PendingAuthCode>();
    let Some(code) = pending.take() else {
        return false;
    };
    let Some(window) = app.get_webview_window("main") else {
        pending.store(code);
        return false;
    };
    let dashboard_url = app.state::<DashboardBaseUrl>();
    let Some(base) = dashboard_url.get() else {
        pending.store(code);
        return false;
    };
    let Some(url) = callback_url(&base, &code).and_then(|value| Url::parse(&value).ok()) else {
        pending.store(code);
        return false;
    };

    if window.navigate(url).is_err() {
        pending.store(code);
        return false;
    }
    let _ = window.show();
    let _ = window.set_focus();
    true
}

#[tauri::command]
pub fn open_oauth(url: String) -> Result<(), String> {
    if !is_allowed_oauth_url(&url) {
        return Err("OAuth URL must be an absolute HTTPS URL".to_string());
    }

    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to open the system browser: {error}"))
}
