use serde::Deserialize;
use std::time::Duration;

use crate::{About, GephWindow};
use slint::{ComponentHandle};

const RELEASE_API: &str = "https://api.github.com/repos/Jerry-ZJQ/geph-slint-pkg/releases/latest";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub html_url: String,
}

// An update checker via github release API 
pub async fn check_update() -> anyhow::Result<Option<ReleaseInfo>> {
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

// App version init
pub fn init_version(window: &GephWindow) {
    window.global::<About>().set_app_version(APP_VERSION.into());
}