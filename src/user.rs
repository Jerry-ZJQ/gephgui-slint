use::std::path::PathBuf;
use directories::ProjectDirs;

use anyhow::{Ok, Result};
use arboard::Clipboard;

use crate::manager::{self, AccountSecretCurrent, AccountSecretStatus, auth_secret};
use crate::{GephWindow, UserPrefs};
use slint::{ComponentHandle};

// Daemon manager settings.json paths
#[cfg(target_os = "macos")]
fn settings_path() -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(
        "/Library/Application Support/geph",
    ))
}

#[cfg(target_os = "windows")]
fn settings_path() -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(
        r"C:\ProgramData\geph",
    ))
}

#[cfg(target_os = "linux")]
fn settings_path() -> anyhow::Result<PathBuf> {
    // Linux path of Geph settings.json
}

// Load secret from local store settings.json
pub fn load_secret() -> Option<String> {
    let path = settings_path().ok()?.join("settings.json");

    let content = std::fs::read_to_string(path).ok()?;
    let settings: serde_json::Value = serde_json::from_str(&content).ok()?;

    settings
        .get("secret")
        .and_then(|v| v.as_str())
        .map(String::from)
}

// Login if no available secret from settings.json
pub async fn user_login(secret: String) -> anyhow::Result<AccountSecretCurrent> {
    // Remove non-effective chars from received secret before authentication
    let secret: String = secret.chars()
        .filter(|c| c.is_ascii_digit())
        .collect();

    anyhow::ensure!(!secret.is_empty(), "Secret cannot be empty");

    let status = auth_secret(secret.clone()).await?;
    match status {
        AccountSecretStatus::Current { current } => {
            // send secret to Daemon
            manager::secret_to_daemon(secret.clone()).await?;
            // Save user_id and invite_code to user_prefs.json for UI display
            save_user_prefs("user_id", &current.clone().user_id.to_string())?;
            save_user_prefs("invite_code", &current.clone().invite_code.unwrap())?;
            Ok(current)
        }
        AccountSecretStatus::Invalid(status) => {
            Err(anyhow::anyhow!("Account secret is invalid: {}", status))
        }
        AccountSecretStatus::Retired(status) => {
            Err(anyhow::anyhow!("Account secret is retired: {}", status))
        }
    }
}

// log out by clearing the secret from Daemon and settings.json
pub async fn user_logout() -> anyhow::Result<()> {
    manager::logout().await?;
    Ok(())
}

// user_prefs.json paths
// refer to crate::directories for detailed paths under different OS
fn user_prefs_path() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "Geph")
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory"))?;

    let config_dir = dirs.config_dir();
    std::fs::create_dir_all(config_dir)?;

    // println!("User prefs path is: {}", config_dir.display());

    Ok(config_dir.join("user_prefs.json"))
}

// load user prefs for string items
pub fn load_user_prefs(key: &str) -> Option<String> {
    let path = user_prefs_path().ok()?;

    let content = std::fs::read_to_string(path).ok()?;
    let prefs: serde_json::Value = serde_json::from_str(&content).ok()?;

    prefs
        .get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
}

// save user prefs for string items
pub fn save_user_prefs(key: &str, value: &str) -> anyhow::Result<()> {
    let path = user_prefs_path()?;

    let mut prefs: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::json!({})
    };

    prefs.as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid user_prefs.json format"))?
        .insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );

    std::fs::write(path, serde_json::to_string_pretty(&prefs)?)?;

    Ok(())
}

// load user prefs for bool items
pub fn load_user_prefs_bool(key: &str) -> Option<bool> {
    let path = user_prefs_path().ok()?;
    let content = std::fs::read_to_string(path).ok()?;
    let prefs: serde_json::Value = serde_json::from_str(&content).ok()?;

    prefs.get(key).and_then(|v| v.as_bool())
}

// save user prefs for bool items
pub fn save_user_prefs_bool(
    key: &str,
    value: bool,
) -> anyhow::Result<()> {
    let path = user_prefs_path()?;

    let mut prefs: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::json!({})
    };

    prefs.as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid user_prefs.json format"))?
        .insert(key.to_string(), serde_json::Value::Bool(value));

    std::fs::write(path, serde_json::to_string_pretty(&prefs)?)?;

    Ok(())
}

// initiate user_prefs (quit_on_close, disconnect_on_quit)
// can add other user_prefs items in future
pub fn init_user_prefs(window: &GephWindow) {
    let quit_on_close = load_user_prefs_bool("quit_on_close")
        .unwrap_or_else(|| {
            save_user_prefs_bool("quit_on_close", false)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to save quit_on_close: {e}");
                });
            false
        });
    window.global::<UserPrefs>().set_quit_on_close(quit_on_close);

    let disconnect_on_quit = load_user_prefs_bool("disconnect_on_quit")
        .unwrap_or_else(|| {
            save_user_prefs_bool("disconnect_on_quit", false)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to save disconnect_on_quit: {e}");
                });
            false
        });
    window.global::<UserPrefs>().set_disconnect_on_quit(disconnect_on_quit);
}

// Open system browser and direct to Geph official billing page with Account Secret
// Currently called by Manage Account, Plus Upgrade and Extend
pub fn open_billing_page(secret: &str) -> anyhow::Result<()> {
    let url = format!(
        "https://geph.io/billing/login_secret?secret={}",
        url::form_urlencoded::byte_serialize(secret.as_bytes())
            .collect::<String>()
    );

    webbrowser::open(&url)?;
    Ok(())
}

// in-App urls
pub const GITHUB_URL: &str = "https://github.com/Jerry-ZJQ/geph-slint-pkg";
pub const GEPH_WEBSITE_URL: &str = "https://geph.io/";
pub const GEPH_FORUM_URL: &str = "https://community.geph.io/t/topic/22395";

// Common function to open a url as args
pub fn open_url(url: &str) -> anyhow::Result<()> {
    webbrowser::open(url)?;
    Ok(())
}

// Copy texts to OS clipboard
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text.to_owned())?;
    Ok(())
}