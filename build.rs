use std::{
    io,
    env, fs,
    path::PathBuf,
};

// use serde_json;

#[cfg(windows)]
use winres::WindowsResource;

fn main() -> io::Result<()> {
    // generate l10n according to locale json files
    generate_l10n();

    // complie slint
    slint_build::compile("ui/geph_window.slint").unwrap();

    // The version shown in-app and used by the auto-updater is read via
    // option_env!("VERSION") at compile time. Track it so bumping VERSION forces
    // a rebuild instead of baking in a stale (or missing) value from the cache.
    println!("cargo:rerun-if-env-changed=VERSION");

    #[cfg(windows)]
    {
        WindowsResource::new()
            // This path can be absolute, or relative to your crate root.
            .set_icon("ui/resources/logos/logo-win.ico")
            // rfd's custom-text dialogs use TaskDialogIndirect, which requires
            // the version 6 Windows common controls activation context.
            .set_manifest(
                r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <assemblyIdentity
        type="win32"
        name="Geph.Geph"
        version="1.0.0.0"
        processorArchitecture="*"
    />
    <dependency>
        <dependentAssembly>
            <assemblyIdentity
                type="win32"
                name="Microsoft.Windows.Common-Controls"
                version="6.0.0.0"
                processorArchitecture="*"
                publicKeyToken="6595b64144ccf1df"
                language="*"
            />
        </dependentAssembly>
    </dependency>
    <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
        <security>
            <requestedPrivileges>
                <requestedExecutionLevel level="asInvoker" uiAccess="false" />
            </requestedPrivileges>
        </security>
    </trustInfo>
</assembly>
"#,
            )
            .compile()?;
    }

    Ok(())

}

// Establish l10n mapping with Slint Localization struct
// and embed multi-language json resources into binary build
fn generate_l10n() {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").unwrap()
    );

    let slint_path = manifest_dir.join("ui/l10n.slint");
    let english_path = manifest_dir.join("locales/en-US.json");

    println!("cargo:rerun-if-changed={}", slint_path.display());
    println!("cargo:rerun-if-changed={}", english_path.display());

    let slint = fs::read_to_string(&slint_path).unwrap();
    let english: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&english_path).unwrap()
    ).unwrap();

    // Traverse the locale folder to embed all language JSONs in binary build
    let locales_dir = manifest_dir.join("locales");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut resources = String::from(
        "pub fn get_language_json(language: &str) -> Option<&'static str> {\n    match language {\n"
    );

    for entry in fs::read_dir(&locales_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        println!("cargo:rerun-if-changed={}", path.display());

        let language = path.file_stem()
            .unwrap()
            .to_str()
            .unwrap();

        // check if every JSON is legal
        let content = fs::read_to_string(&path).unwrap();
        let _: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Invalid JSON {}: {e}", path.display()));

        resources.push_str(&format!(
            "        {language:?} => Some(include_str!({:?})),\n",
            path
        ));
    }

    resources.push_str("        _ => None,\n    }\n}\n");

    fs::write(
        out_dir.join("l10n_resources.rs"),
        resources,
    ).unwrap();

    // extract fields from the Slint Localization struct
    let mut fields = Vec::new();
    let mut in_struct = false;

    for line in slint.lines() {
        let line = line.trim();

        if line.starts_with("export struct Localization") {
            in_struct = true;
            continue;
        }

        if !in_struct {
            continue;
        }

        if line.starts_with('}') {
            break;
        }

        if let Some((name, ty)) = line.trim_end_matches(',')
            .split_once(':')
        {
            let name = name.trim();
            let ty = ty.trim();

            assert_eq!(
                ty, "string",
                "Unsupported Localization field type: {name}: {ty}"
            );

            fields.push(name.to_owned());
        }
    }

    assert!(!fields.is_empty(), "Localization fields not found");

    // Englist locale must cover all the fields of Slint Localization struct
    // and the values must be String
    for field in &fields {
        assert!(
            english.get(field).and_then(|v| v.as_str()).is_some(),
            "locales/en.json missing string field: {field}"
        );
    }

    // Automatically generate construction codes for Rust struct mapping
    let mut generated = String::from(
        "pub fn localization_from_map(
            values: &std::collections::HashMap<String, String>
        ) -> Localization {
            Localization {\n"
    );

    for field in &fields {
        let default = english[field].as_str().unwrap();

        generated.push_str(&format!(
            "    {field}: values.get({field:?})
                .cloned()
                .unwrap_or_else(|| {default:?}.to_owned())
                .into(),\n"
        ));
    }

    generated.push_str("    }\n}\n");

    fs::write(
        out_dir.join("l10n_generated.rs"),
        generated,
    ).unwrap();
}