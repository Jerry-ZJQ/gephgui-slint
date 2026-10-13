use std::{collections::HashMap};
use anyhow::Result;

use crate::user;

use crate::{Localization, L10n};
use crate::{GephWindow, GephTray};
use slint::{ComponentHandle};

// Use embedded localization struct mapping
// and embedded multi-language JSON resources
include!(concat!(env!("OUT_DIR"), "/l10n_generated.rs"));
include!(concat!(env!("OUT_DIR"), "/l10n_resources.rs"));

// load localization from embedded json resources
pub fn load_localization(language: &str) -> Result<Localization> {
    // en-US as default language and fallback
    let english = get_language_json("en-US")
        .expect("en-US.json must exist");

    let mut values: HashMap<String, String> =
        serde_json::from_str(english)?;

    // load target language and fallback to en-US if not supported
    if language != "en-US" {
        if let Some(content) = get_language_json(language) {
            let translated: HashMap<String, String> =
                serde_json::from_str(content)?;

            for (key, value) in translated {
                if values.contains_key(&key) {
                    values.insert(key, value);
                } else {
                    eprintln!("Unknown localization key: {key}");
                }
            }
        }
    }

    Ok(localization_from_map(&values))
}

// Apply app language changes to Geph window and tray
pub fn apply_language(
    language: &str,
    window: &GephWindow,
    tray: &GephTray,
) -> anyhow::Result<()> {
    // let language = language_label_mapping(label)
    //     .ok_or_else(|| anyhow::anyhow!("Unsupported language: {label}"))?;

    let localization = load_localization(language)?;

    // repsond to window and tray callbacks and set target language in UI
    window.global::<L10n>().set_text(localization.clone());
    tray.global::<L10n>().set_text(localization);

    // save user selected language to user-prefs.json
    user::save_user_prefs("language", language)?;

    Ok(())
}

// mapping language labels from Slint to Rust
// update this mapping if new languages added
pub fn label_to_language(label: &str) -> &'static str {
    match label {
        "English" => "en-US",
        "简体中文" => "zh-CN",
        "繁體中文" => "zh-TW",
        _ => "en-US",
    }
}

// mapping language labels from Rust to Slint
// update this mapping if new languages added
pub fn language_to_label(language: &str) -> &'static str {
    match language {
        "en-US" => "English",
        "zh-CN" => "简体中文",
        "zh-TW" => "繁體中文",
        _ => "English",
    }
}

// sys locale mapping
// update the match if needing more adaptive language display
pub fn sys_locale_mapping(locale: &str) -> &'static str {
    match locale {
        s if s.starts_with("zh-Hans") => "zh-CN",
        s if s.starts_with("zh-Hant") => "zh-TW",
        s if s == "zh" || s.starts_with("zh-") => "zh-CN",
        // s if s == "ja" || s.starts_with("ja-") => "ja-JP",
        _ => "en-US",
    }
}