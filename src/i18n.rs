use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Turkish,
    English,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preference {
    System,
    Turkish,
    English,
}
impl Language {
    pub fn from_locale(locale: &str) -> Self {
        let code = locale.split(['_', '-', '.', '@', ':']).next().unwrap_or("");
        if code.eq_ignore_ascii_case("tr") {
            Self::Turkish
        } else {
            Self::English
        }
    }
    pub fn tr(self, text: &str) -> &str {
        if self == Self::Turkish {
            return text;
        }
        static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();
        CATALOG
            .get_or_init(|| {
                serde_json::from_str(include_str!("translations.json"))
                    .expect("valid translation catalog")
            })
            .get(text)
            .map(String::as_str)
            .unwrap_or(text)
    }
    pub fn error(self, text: &str) -> String {
        let translated = self.tr(text);
        if translated != text {
            return translated.into();
        }
        for prefix in [
            "Paket içindeki servis kurulumu başlatılamadı: ",
            "Paket içindeki servis hazırlanamadı: ",
        ] {
            if let Some(detail) = text.strip_prefix(prefix) {
                return format!("{}{detail}", self.tr(prefix));
            }
        }
        text.into()
    }
}
impl Preference {
    fn path() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
            .map(|p| p.join("cepdesk/applet-language"))
    }
    pub fn load() -> Self {
        Self::path()
            .map(|path| Self::load_from(&path))
            .unwrap_or(Self::System)
    }
    fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| match s.trim() {
                "tr" => Self::Turkish,
                "en" => Self::English,
                _ => Self::System,
            })
            .unwrap_or(Self::System)
    }
    pub fn save(self) -> std::io::Result<()> {
        let path = Self::path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "XDG_CONFIG_HOME / HOME")
        })?;
        self.save_to(&path)
    }
    fn save_to(self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        // Rename publishes the whole preference to other applet/window instances.
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(
            &temporary,
            match self {
                Self::System => "system",
                Self::Turkish => "tr",
                Self::English => "en",
            },
        )?;
        std::fs::rename(temporary, path)
    }
    pub fn resolve(self) -> Language {
        match self {
            Self::Turkish => Language::Turkish,
            Self::English => Language::English,
            Self::System => Language::from_locale(
                &["LC_ALL", "LC_MESSAGES", "LANG"]
                    .into_iter()
                    .filter_map(|key| std::env::var(key).ok())
                    .find(|v| !v.is_empty())
                    .unwrap_or_default(),
            ),
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::System => 0,
            Self::Turkish => 1,
            Self::English => 2,
        }
    }
    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Turkish,
            2 => Self::English,
            _ => Self::System,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locale_variants_and_fallback() {
        for locale in ["tr_TR.UTF-8", "tr-TR", "tr", "TR_tr", "tr_TR@variant"] {
            assert_eq!(Language::from_locale(locale), Language::Turkish);
        }
        for locale in ["en_US.UTF-8", "de_DE", "C.UTF-8", "", "tRicky"] {
            assert_eq!(Language::from_locale(locale), Language::English);
        }
    }
    #[test]
    fn catalog_and_service_errors() {
        let catalog: HashMap<String, String> =
            serde_json::from_str(include_str!("translations.json")).unwrap();
        assert!(catalog.len() > 90);
        for (tr, en) in &catalog {
            assert!(!en.is_empty());
            assert_eq!(Language::Turkish.tr(tr), tr);
            assert_eq!(Language::English.tr(tr), en);
        }
        assert_eq!(
            Language::English.error("Servis yanıt vermedi."),
            "Service did not respond."
        );
        assert_eq!(
            Language::English.error("Paket içindeki servis hazırlanamadı: detail"),
            "Could not prepare bundled service: detail"
        );
    }
    #[test]
    fn preference_persists_and_invalid_values_use_system() {
        let directory =
            std::env::temp_dir().join(format!("cepdesk-language-test-{}", std::process::id()));
        let path = directory.join("applet-language");
        assert_eq!(Preference::load_from(&path), Preference::System);
        for preference in [Preference::Turkish, Preference::English, Preference::System] {
            preference.save_to(&path).unwrap();
            assert_eq!(Preference::load_from(&path), preference);
        }
        std::fs::write(&path, "invalid").unwrap();
        assert_eq!(Preference::load_from(&path), Preference::System);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn preference_choices() {
        for (i, p) in [Preference::System, Preference::Turkish, Preference::English]
            .into_iter()
            .enumerate()
        {
            assert_eq!(Preference::from_index(i), p);
            assert_eq!(p.index(), i);
        }
        assert_eq!(Preference::Turkish.resolve(), Language::Turkish);
        assert_eq!(Preference::English.resolve(), Language::English);
    }
}
