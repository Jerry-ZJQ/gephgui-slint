// Disable Windows cmd on App startup
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

mod manager;
mod ui_service;
mod ui_states;
mod user;
mod window;
mod tray;
mod l10n;
mod updater;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod bootstrap;

use std::{sync::Arc};

use manager::manager_reachable;
use ui_service::{UiService};

use slint::{ComponentHandle, SharedString};

use crate::l10n::{apply_language, init_l10n, label_to_language, language_to_label};
use crate::user::{init_user_prefs};
use crate::updater::{init_version, check_update};

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

    // initiate UI language
    init_l10n(&window, &tray);
    init_version(&window);

    // process Slint callback for language change
    // keep this callback process in main() for now
    // if more callbacks for both window & tray operations
    // introduced in future, consider moving such logics to a new module
    let language_window = window.as_weak();
    let language_tray = tray.as_weak();

    // initiate user_prefs: quit_on_close and disconnect_on_quit settings in UI
    init_user_prefs(&window);

    // Check App update
    let update_window = window.as_weak();
    geph5_rt::spawn(async move {
        match check_update().await {
            Ok(Some(release)) => {
                let window_weak = update_window.clone();

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = window_weak.upgrade() {
                        let about = window.global::<About>();

                        about.set_update_available(true);
                        about.set_latest_version(release.tag_name.into());
                        about.set_release_url(release.html_url.into());
                    }
                });
            }
            Ok(None) => {
                println!("Application is up to date");
            }
            Err(e) => {
                eprintln!("Update check failed: {e}");
            }
        }
    }).detach();

    // callback process for language change
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
    let window_quit = window.as_weak();
    tray.on_quit(move || {
        println!("Quit from Tray");
        if let Some(window) = window_quit.upgrade() {
            let disconnect_on_quit = window.global::<UserPrefs>().get_disconnect_on_quit();
            process_app_quit(disconnect_on_quit);
        } else {
            // backup procedure: quit app with disconnect_on_quit as false
            // in case acquiring disconnect_on_quit not successful
            process_app_quit(false);
        }
    });

    // Click window's close button to hide or quit
    let window_close = window.as_weak();
    window.window().on_close_requested(move || {
        let Some(window) = window_close.upgrade() else {
            return slint::CloseRequestResponse::HideWindow;
        };

        let quit_on_close = window.global::<UserPrefs>().get_quit_on_close();
        if quit_on_close {
            let disconnect_on_quit = window.global::<UserPrefs>().get_disconnect_on_quit();
            process_app_quit(disconnect_on_quit);
            println!("Quit on window close");
        } else {
            println!("Hide window");
        }
        slint::CloseRequestResponse::HideWindow 
    });

    // Make sure the privileged host manager is installed, current, and answering
    // before we bring up the Slint UI that talks to it. May show a native dialog and
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

// The quit app function
fn process_app_quit(disconnect_on_quit: bool) {
    if disconnect_on_quit {
        geph5_rt::spawn(async move {
            println!("Stop daemon on quit");
            if let Err(e) = manager::stop_daemon().await {
                eprintln!("Failed to stop daemon: {e}");
            }
            // Make sure quit event loop in Slint UI thread
            slint::invoke_from_event_loop(|| {
                slint::quit_event_loop().unwrap();
            })
            .unwrap();
        }).detach();
    } else {
        slint::quit_event_loop().unwrap();
    }
}