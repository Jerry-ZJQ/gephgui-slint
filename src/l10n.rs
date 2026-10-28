use std::{collections::HashMap};
use anyhow::Result;
use sys_locale;

use crate::user;

use crate::{Localization, L10n};
use crate::{GephWindow, GephTray};
use slint::{ComponentHandle, ModelRc, VecModel};

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

// Available language list
pub const AVAILABLE_LANGUAGES: &[&str] = &[
    "English",
    "简体中文",
    "繁體中文",
];

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

// read user preferred language from user_prefs.json
// or match system locale language and then set the language at App startup
pub fn init_l10n(window: &GephWindow, tray: &GephTray) {
    // Set language options in Slint UI
    let language_list = ModelRc::new(VecModel::from(
        AVAILABLE_LANGUAGES.iter()
        .map(|label| (*label).into())
        .collect::<Vec<slint::SharedString>>(),
    ));
    window.global::<L10n>().set_available_languages(language_list);

    // Get user preferred language from user-prefs.json
    // if no preferred language, get system locale to set default language
    // Get system locale to set language for first startup
    // Fall back to en-US if preferred language is not available,
    // or sys_locale doesn't match with available languages
    let language = if let Some(value) = user::load_user_prefs("language") {
        println!("Loaded language is: {}", value);
        value
    } else {
        let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en-US"));
        println!("Current sys locale is: {}", locale);
        sys_locale_mapping(&locale).to_string()
    };
    // set language at App startup according to user prefs or system locale
    if let Err(e) = apply_language(&language, &window, &tray) {
        eprintln!("Failed to set UI language: {e:#}");
    }
    // sync selected language label to window UI
    let label= language_to_label(&language);
    window.global::<L10n>().set_selected_label(label.into());
}