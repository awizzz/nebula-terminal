use std::{collections::BTreeMap, fs};

use crate::{config::Config, platform};

const EN_US: &str = include_str!("../locales/en-US.toml");
const FR_FR: &str = include_str!("../locales/fr-FR.toml");

#[derive(Debug, Clone)]
pub struct Translator {
    language: String,
    values: BTreeMap<String, String>,
}

impl Translator {
    pub fn from_config(config: &Config) -> Self {
        let requested = if config.general.language.eq_ignore_ascii_case("auto") {
            platform::system_locale()
        } else {
            config.general.language.clone()
        };

        Self::load(&requested)
    }

    pub fn load(requested: &str) -> Self {
        let requested = normalize_locale(requested);
        let builtin = builtin_locale(&requested);
        let mut values = parse_locale(builtin.1).unwrap_or_default();
        let mut language = builtin.0.to_string();

        let custom_path = platform::data_dir()
            .join("locales")
            .join(format!("{requested}.toml"));

        if let Ok(raw) = fs::read_to_string(&custom_path) {
            if let Ok(custom) = parse_locale(&raw) {
                values.extend(custom);
                language = requested;
            }
        }

        Self { language, values }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn text(&self, key: &str) -> String {
        self.values
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    pub fn value(&self, key: &str, value: impl AsRef<str>) -> String {
        self.text(key).replace("{value}", value.as_ref())
    }
}

fn normalize_locale(locale: &str) -> String {
    let locale = locale.trim().replace('_', "-");
    let locale = locale.split('.').next().unwrap_or(&locale);
    if locale.is_empty() {
        "en-US".into()
    } else {
        locale.to_string()
    }
}

fn builtin_locale(requested: &str) -> (&'static str, &'static str) {
    let lower = requested.to_ascii_lowercase();
    if lower == "fr" || lower.starts_with("fr-") {
        ("fr-FR", FR_FR)
    } else {
        ("en-US", EN_US)
    }
}

fn parse_locale(raw: &str) -> Result<BTreeMap<String, String>, toml::de::Error> {
    toml::from_str(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_normalization_accepts_common_windows_forms() {
        assert_eq!(normalize_locale("fr_FR.UTF-8"), "fr-FR");
        assert_eq!(normalize_locale("en-US"), "en-US");
        assert_eq!(normalize_locale(""), "en-US");
    }

    #[test]
    fn french_locale_is_selected_for_french_variants() {
        assert_eq!(builtin_locale("fr-CA").0, "fr-FR");
        assert_eq!(builtin_locale("de-DE").0, "en-US");
    }

    #[test]
    fn built_in_locales_have_identical_keys() {
        let en = parse_locale(EN_US).expect("English locale must parse");
        let fr = parse_locale(FR_FR).expect("French locale must parse");
        let en_keys: Vec<_> = en.keys().collect();
        let fr_keys: Vec<_> = fr.keys().collect();
        assert_eq!(en_keys, fr_keys);
    }
}
