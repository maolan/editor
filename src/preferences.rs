use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct EditorPreferences {
    default_output_device_id: Option<String>,
    default_input_device_id: Option<String>,
}

impl EditorPreferences {
    pub(crate) fn from_toml_str(contents: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(contents)
    }

    pub(crate) fn default_output_device_id(&self) -> Option<&str> {
        self.default_output_device_id.as_deref()
    }

    pub(crate) fn default_input_device_id(&self) -> Option<&str> {
        self.default_input_device_id.as_deref()
    }

    #[cfg(target_os = "freebsd")]
    pub(crate) fn set_default_output_device_id(&mut self, id: Option<String>) {
        self.default_output_device_id = id;
    }

    #[cfg(target_os = "freebsd")]
    pub(crate) fn set_default_input_device_id(&mut self, id: Option<String>) {
        self.default_input_device_id = id;
    }

    #[cfg(target_os = "freebsd")]
    fn update_toml_str(&self, contents: &str) -> Result<String, String> {
        let mut value = if contents.trim().is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            toml::from_str::<toml::Value>(contents).map_err(|error| error.to_string())?
        };
        let table = value
            .as_table_mut()
            .ok_or_else(|| String::from("Audio device settings require a TOML table."))?;
        update_id(
            table,
            "default_output_device_id",
            &self.default_output_device_id,
        );
        update_id(
            table,
            "default_input_device_id",
            &self.default_input_device_id,
        );
        toml::to_string_pretty(&value).map_err(|error| error.to_string())
    }

    fn load_from_path(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| Self::from_toml_str(&contents).ok())
            .unwrap_or_default()
    }

    #[cfg(target_os = "freebsd")]
    fn save_to_path(&self, path: &std::path::Path) -> Result<(), String> {
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.to_string()),
        };
        let updated = self.update_toml_str(&contents)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, updated).map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "freebsd")]
fn update_id(table: &mut toml::map::Map<String, toml::Value>, key: &str, id: &Option<String>) {
    if let Some(id) = id
        .as_deref()
        .filter(|id| !id.is_empty() && *id != "__auto__")
    {
        table.insert(key.to_string(), toml::Value::String(id.to_string()));
    } else {
        table.remove(key);
    }
}

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
