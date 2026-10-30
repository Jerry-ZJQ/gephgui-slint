use serde::Deserialize;
use std::time::Duration;

use crate::{About, GephWindow};
use slint::{ComponentHandle};

const RELEASE_API: &str = "https://api.github.com/repos/Jerry-ZJQ/geph-slint-pkg/releases/latest";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// App version init
pub fn init_version(window: &GephWindow) {
    window.global::<About>().set_app_version(APP_VERSION.into());
}

#[derive(Debug, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub html_url: String,
}

// An update checker via github release API 
async fn check_update() -> anyhow::Result<Option<ReleaseInfo>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent(format!(
            "geph-slint-pkg/{}", APP_VERSION))
        .build()?;

    let release: ReleaseInfo = client
        .get(RELEASE_API)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let latest = release.tag_name.strip_prefix('v')
        .unwrap_or(&release.tag_name);

    let latest = semver::Version::parse(latest)?;
    let current = semver::Version::parse(APP_VERSION)?;

    if latest > current {
        Ok(Some(release))
    } else {
        Ok(None)
    }
}

// Periodic update checker - triggers an update check at first call and every 6 hours
pub fn start_periodic_update_check(window: &GephWindow) {
    let window_weak = window.as_weak();

    geph5_rt::spawn(async move {
        loop {
            // Check immediately at startup, then every 6 hours.
            match check_update().await {
                Ok(Some(release)) => {
                    let window_weak = window_weak.clone();

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
                    eprintln!("Application is up to date");
                }
                Err(e) => {
                    eprintln!("Update check failed: {e}");
                }
            }

            tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
        }
    }).detach();
}