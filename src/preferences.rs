use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub(crate) struct EditorPreferences {
    pub(crate) default_output_device_id: Option<String>,
    pub(crate) default_input_device_id: Option<String>,
}

impl EditorPreferences {
    pub(crate) fn load() -> Self {
        let Some(config_path) = edit_config_path() else {
            return Self::default();
        };
        Self::load_from_path(&config_path)
    }

    pub(crate) fn load_from_path(config_path: &Path) -> Self {
        let Ok(contents) = std::fs::read_to_string(config_path) else {
            return Self::default();
        };
        let Ok(value) = toml::from_str::<toml::Value>(&contents) else {
            return Self::default();
        };
        Self {
            default_output_device_id: preference_device_id(&value, "default_output_device_id"),
            default_input_device_id: preference_device_id(&value, "default_input_device_id"),
        }
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let Some(config_path) = edit_config_path() else {
            return Err(String::from("Could not determine config directory."));
        };
        self.save_to_path(&config_path)
    }

    pub(crate) fn save_to_path(&self, config_path: &Path) -> Result<(), String> {
        let mut lines: Vec<String> = if config_path.exists() {
            std::fs::read_to_string(config_path)
                .map_err(|err| err.to_string())?
                .lines()
                .map(ToOwned::to_owned)
                .collect()
        } else {
            Vec::new()
        };

        let mut output_set = false;
        let mut input_set = false;
        for line in &mut lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("default_output_device_id") {
                if let Some(id) = self.default_output_device_id.as_deref() {
                    *line = format!("default_output_device_id = \"{id}\"");
                } else {
                    *line = String::new();
                }
                output_set = true;
            } else if trimmed.starts_with("default_input_device_id") {
                if let Some(id) = self.default_input_device_id.as_deref() {
                    *line = format!("default_input_device_id = \"{id}\"");
                } else {
                    *line = String::new();
                }
                input_set = true;
            }
        }
        lines.retain(|line| !line.is_empty());

        if !output_set && let Some(id) = self.default_output_device_id.as_deref() {
            lines.push(format!("default_output_device_id = \"{id}\""));
        }
        if !input_set && let Some(id) = self.default_input_device_id.as_deref() {
            lines.push(format!("default_input_device_id = \"{id}\""));
        }

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let content = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        std::fs::write(config_path, content).map_err(|err| err.to_string())?;
        Ok(())
    }
}

fn preference_device_id(value: &toml::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(toml::Value::as_str)
        .filter(|id| !id.is_empty() && *id != "__auto__")
        .map(ToOwned::to_owned)
}

pub(crate) fn edit_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("maolan")
            .join("edit.toml"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_save_to_path_preserves_other_keys() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-edit-prefs-preserve-{}",
            std::time::UNIX_EPOCH.elapsed().unwrap().as_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let config_path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &config_path,
            "existing_key = \"keep me\"\ndefault_output_device_id = \"old\"\n",
        )
        .unwrap();

        let preferences = EditorPreferences {
            default_output_device_id: Some(String::from("new_out")),
            default_input_device_id: Some(String::from("new_in")),
        };
        preferences.save_to_path(&config_path).unwrap();

        let saved = std::fs::read_to_string(&config_path).unwrap();
        assert!(saved.contains("existing_key = \"keep me\""));
        assert!(saved.contains("default_output_device_id = \"new_out\""));
        assert!(saved.contains("default_input_device_id = \"new_in\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preferences_save_removes_empty_ids() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-edit-prefs-remove-{}",
            std::time::UNIX_EPOCH.elapsed().unwrap().as_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let config_path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &config_path,
            "default_output_device_id = \"old\"\ndefault_input_device_id = \"old\"\n",
        )
        .unwrap();

        let preferences = EditorPreferences {
            default_output_device_id: None,
            default_input_device_id: None,
        };
        preferences.save_to_path(&config_path).unwrap();

        let saved = std::fs::read_to_string(&config_path).unwrap();
        assert!(!saved.contains("default_output_device_id"));
        assert!(!saved.contains("default_input_device_id"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preferences_load_from_path_reads_device_ids() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-edit-prefs-load-{}",
            std::time::UNIX_EPOCH.elapsed().unwrap().as_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("edit.toml");
        let contents =
            "default_output_device_id = \"/dev/dsp2\"\ndefault_input_device_id = \"/dev/dsp3\"\n";
        std::fs::write(&config_path, contents).unwrap();

        let preferences = EditorPreferences::load_from_path(&config_path);

        assert_eq!(
            preferences.default_output_device_id.as_deref(),
            Some("/dev/dsp2")
        );
        assert_eq!(
            preferences.default_input_device_id.as_deref(),
            Some("/dev/dsp3")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
