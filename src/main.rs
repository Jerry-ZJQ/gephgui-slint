// Disable Windows cmd during App startup
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

mod manager;
mod ui_service;
mod ui_states;
mod user;
mod window;
mod tray;
mod l10n;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod bootstrap;

use std::{sync::Arc};

use manager::manager_reachable;
use ui_service::{UiService};

use sys_locale::get_locale;

use slint::{ComponentHandle, SharedString};

use crate::l10n::{apply_language, label_to_language, language_to_label, sys_locale_mapping};

// Main entrance of the application
fn main() -> Result<(), slint::PlatformError> {

	// Create Slint Window
    let window = GephWindow::new()?;

    // prevent 2nd GUI instance from being created
    // by using a tiny TCP listener binding
    match start_single_instance_listener(window.as_weak()) {
        Ok(true) => {}
        Ok(false) => return Ok(()),
        Err(e) => {
            eprintln!("[Main] Single-instance listener failed: {e:?}");
        }
    }

    // Create Slint Tray
    let tray = GephTray::new()?;

    // Get user preferred language from user-prefs.json
    // if no preferred language, get system locale to set default language
    // Get system locale to set language for first startup
    // Fall back to en-US if preferred language is not available,
    // or sys_locale doesn't match with available languages
    let language = if let Some(value) = user::load_user_prefs("language") {
        println!("Loaded language is: {}", value);
        value
    } else {
        let locale = get_locale().unwrap_or_else(|| String::from("en-US"));
        println!("Current sys locale is: {}", locale);
        sys_locale_mapping(&locale).to_string()
    };
    // set language at App startup according to user prefs or system locale
    if let Err(e) = apply_language(&language, &window, &tray) {
        eprintln!("Failed to set UI language: {e:#}");
    }
    // sync select language label to window UI
    let label= language_to_label(&language);
    window.global::<L10n>().set_selected_label(label.into());

    // process Slint callback for language change
    let language_window = window.as_weak();
    let language_tray = tray.as_weak();

    window.on_language_changed(move |label: SharedString| {
        let (Some(window), Some(tray)) =
            (language_window.upgrade(), language_tray.upgrade())
        else {
            return;
        };

        let language = label_to_language(label.as_str());
        if let Err(e) = apply_language(language, &window, &tray) {
            eprintln!("Language change failed: {e:#}");
        }
        let language_label = language_to_label(language);
        window.global::<L10n>().set_selected_label(language_label.into());
    });

    // Show window by clicking Show Geph in Tray
    tray.on_show_window({
        let weak_window = window.as_weak();

        move || {
            if let Some(window) = weak_window.upgrade() {
                window.show().unwrap();
            }
        }
    });

    // Quit app by clicking Quit in Tray
    tray.on_quit(|| {
        println!("Quit from Tray");
        slint::quit_event_loop().unwrap();
    });

    // Click window's close button to hide
    window.window().on_close_requested(move || {
        println!("Hide window");
        slint::CloseRequestResponse::HideWindow
    });

    // Make sure the privileged host manager is installed, current, and answering
    // before we bring up the webview that talks to it. May show a native dialog and
    // elevate via pkexec (Linux) or UAC (Windows), or ask for a relaunch; returns
    // false if we should exit now.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    if !bootstrap::ensure_manager() {
        return Ok(());
    }

    // check if Daemon is available before start UiService
    let daemon_reachable = geph5_rt::block_on(manager_reachable());

    if daemon_reachable {
        println!("Daemon check OK, starting UI service");
        // load secret from local settings.json
        let secret = user::load_secret();
        // Initiate UiService
        let service = Arc::new(UiService::new(secret));

        // Register slint callbacks and sync for window and Tray
        window::window_callbacks::register_callbacks(
            &window,
            Arc::clone(&service),
        );

        window::window_sync::start_sync(
            &window,
            Arc::clone(&service),
        );

        tray::tray_callbacks::register_callbacks(
            &tray,
            Arc::clone(&service)
        );

        tray::tray_sync::start_sync(
            &tray,
            Arc::clone(&service),
        );

        // Start UiState polling
        service.clone().start_polling();
    } else {
        eprintln!("Geph Daemon is not reachable.");
    }

    window.run()
}


// An instance checker via tiny TCP listener
const SINGLE_INSTANCE_PORT: u16 = 8765;

fn start_single_instance_listener(
    window: slint::Weak<GephWindow>,
) -> anyhow::Result<bool> {
    let server = match tiny_http::Server::http(
        format!("127.0.0.1:{SINGLE_INSTANCE_PORT}")
    ) {
        Ok(server) => server,

        // Port already occupied → another Geph instance is running.
        Err(_) => {
            use std::io::Write;

            if let Ok(mut stream) =
                std::net::TcpStream::connect(
                    format!("127.0.0.1:{SINGLE_INSTANCE_PORT}")
                )
            {
                let _ = stream.write_all(
                    b"GET /__show HTTP/1.0\r\nHost: localhost\r\n\r\n"
                );
            }

            return Ok(false);
        }
    };

    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            if request.url() == "/__show" {
                let weak_window = window.clone();

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = weak_window.upgrade() {
                        window.show().ok();
                        // Can add set_focus or bring forward logic here as needed
                    }
                });

                let _ = request.respond(
                    tiny_http::Response::from_string("ok")
                );
            } else {
                let _ = request.respond(
                    tiny_http::Response::from_string("not found")
                        .with_status_code(404)
                );
            }
        }
    });

    Ok(true)
}