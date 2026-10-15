use std::{path::PathBuf, sync::OnceLock};

static PROFILE: OnceLock<&'static str> = OnceLock::new();

fn compiled_profile(identifier: &str, requested: Option<&str>) -> Result<&'static str, String> {
    let profile = if identifier.ends_with(".preview") {
        "preview"
    } else {
        "stable"
    };
    if requested.is_some_and(|requested| requested != profile) {
        return Err("The requested profile does not match this build. Use the preview build for isolated testing.".into());
    }
    Ok(profile)
}

pub fn initialize(identifier: &str) -> Result<(), String> {
    let profile = compiled_profile(identifier, std::env::var("CODE_UI_PROFILE").ok().as_deref())?;
    PROFILE
        .set(profile)
        .map_err(|_| "Application profile already initialized".into())
}

pub fn profile() -> &'static str {
    PROFILE.get().copied().unwrap_or("stable")
}

fn namespace(profile: Option<&str>) -> Result<&'static str, String> {
    match profile {
        None | Some("stable") => Ok("code-ui"),
        Some("preview") => Ok("code-ui-preview"),
        _ => Err("CODE_UI_PROFILE must be stable or preview.".into()),
    }
}

pub fn config_dir() -> Result<PathBuf, String> {
    Ok(dirs::config_dir()
        .ok_or("No config directory")?
        .join(namespace(Some(profile()))?))
}

pub fn data_dir() -> Result<PathBuf, String> {
    Ok(dirs::data_local_dir()
        .ok_or("No data directory")?
        .join(namespace(Some(profile()))?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_storage_follows_the_compiled_identity_even_without_an_environment_variable() {
        assert_eq!(
            compiled_profile("dev.hans.codeui.preview", None).unwrap(),
            "preview"
        );
        assert_eq!(compiled_profile("dev.hans.codeui", None).unwrap(), "stable");
        assert!(compiled_profile("dev.hans.codeui.preview", Some("stable")).is_err());
        assert!(compiled_profile("dev.hans.codeui", Some("preview")).is_err());
    }

    #[test]
    fn preview_has_a_separate_namespace_and_unknown_profiles_are_rejected() {
        assert_eq!(namespace(None).unwrap(), "code-ui");
        assert_eq!(namespace(Some("stable")).unwrap(), "code-ui");
        assert_eq!(namespace(Some("preview")).unwrap(), "code-ui-preview");
        assert!(namespace(Some("../code-ui")).is_err());
    }
}
