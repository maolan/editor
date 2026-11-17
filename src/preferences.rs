use std::path::PathBuf;

pub(crate) use maolan_engine::preferences::AudioDevicePreferences as EditorPreferences;

pub(crate) fn load() -> EditorPreferences {
    let Some(config_path) = edit_config_path() else {
        return EditorPreferences::default();
    };
    #[cfg(not(target_os = "freebsd"))]
    let preferences = EditorPreferences::load_from_path(&config_path);
    #[cfg(target_os = "freebsd")]
    let mut preferences = EditorPreferences::load_from_path(&config_path);
    #[cfg(target_os = "freebsd")]
    {
        let daw_preferences = daw_config_path()
            .map(|path| EditorPreferences::load_from_path(&path))
            .unwrap_or_default();
        let output = migrate_oss_device_id(
            preferences.default_output_device_id(),
            daw_preferences.default_output_device_id(),
        );
        let input = migrate_oss_device_id(
            preferences.default_input_device_id(),
            daw_preferences.default_input_device_id(),
        );
        if output.as_deref() != preferences.default_output_device_id()
            || input.as_deref() != preferences.default_input_device_id()
        {
            preferences.set_default_output_device_id(output);
            preferences.set_default_input_device_id(input);
            let _ = preferences.save_to_path(&config_path);
        }
    }
    preferences
}

#[cfg(target_os = "freebsd")]
pub(crate) fn save(preferences: &EditorPreferences) -> Result<(), String> {
    let Some(config_path) = edit_config_path() else {
        return Err(String::from("Could not determine config directory."));
    };
    preferences.save_to_path(&config_path)
}

fn edit_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("maolan")
            .join("editor")
            .join("config.toml"),
    )
}

#[cfg(target_os = "freebsd")]
fn daw_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("maolan")
            .join("daw")
            .join("config.toml"),
    )
}

#[cfg(target_os = "freebsd")]
fn migrate_oss_device_id(editor_id: Option<&str>, daw_id: Option<&str>) -> Option<String> {
    match editor_id {
        Some(id) if id.starts_with("/dev/dsp") => Some(id.to_string()),
        _ => daw_id
            .filter(|id| id.starts_with("/dev/dsp"))
            .map(ToOwned::to_owned),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "freebsd")]
    use super::*;

    #[test]
    #[cfg(target_os = "freebsd")]
    fn shared_preferences_load_and_save_preserves_other_settings() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-edit-prefs-{}",
            std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "existing_key = \"keep me\"\n").unwrap();
        let mut preferences = EditorPreferences::default();
        preferences.set_default_output_device_id(Some(String::from("/dev/dsp5")));
        preferences.set_default_input_device_id(Some(String::from("/dev/dsp5")));
        preferences.save_to_path(&path).unwrap();
        let loaded = EditorPreferences::load_from_path(&path);
        let saved = std::fs::read_to_string(&path).unwrap();
        assert_eq!(loaded, preferences);
        assert!(saved.contains("existing_key = \"keep me\""));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(target_os = "freebsd")]
    #[test]
    fn migration_uses_maolan_oss_device_id_for_legacy_editor_ids() {
        assert_eq!(
            migrate_oss_device_id(Some("in2"), Some("/dev/dsp5")),
            Some(String::from("/dev/dsp5"))
        );
        assert_eq!(
            migrate_oss_device_id(Some("/dev/dsp4"), Some("/dev/dsp5")),
            Some(String::from("/dev/dsp4"))
        );
    }
}
