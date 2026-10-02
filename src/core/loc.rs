


use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

pub type Table = HashMap<String, HashMap<String, String>>;

static STRINGS: OnceLock<Table> = OnceLock::new();
static LANG: RwLock<Lang> = RwLock::new(Lang::Ja);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Ja,
    En,
    Zh,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Ja => "ja",
            Lang::En => "en",
            Lang::Zh => "zh",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Ja => "日本語",
            Lang::En => "English",
            Lang::Zh => "简体中文",
        }
    }

    pub const ALL: [Lang; 3] = [Lang::Ja, Lang::En, Lang::Zh];

    
    pub fn detect_system() -> Lang {
        let l = std::env::var("LANG")
            .or_else(|_| std::env::var("LC_ALL"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if l.starts_with("ja") {
            Lang::Ja
        } else if l.starts_with("zh") {
            Lang::Zh
        } else if l.starts_with("en") {
            Lang::En
        } else {
            Lang::Ja
        }
    }
}


pub fn init() {
    let raw = include_str!("../../assets/strings.json");
    let table: Table = serde_json::from_str(raw).expect("assets/strings.json is invalid JSON");
    let _ = STRINGS.set(table);
}

pub fn set_lang(lang: Lang) {
    if let Ok(mut w) = LANG.write() {
        *w = lang;
    }
}

pub fn lang() -> Lang {
    LANG.read().map(|g| *g).unwrap_or(Lang::Ja)
}


pub fn t(key: &str) -> String {
    let code = lang().code();
    STRINGS
        .get()
        .and_then(|table| table.get(key))
        .and_then(|row| row.get(code).or_else(|| row.get("ja")))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

#[cfg(test)]
mod tests {
    use super::{init, STRINGS};

    #[test]
    fn every_translation_row_has_all_languages() {
        init();
        let table = STRINGS.get().expect("translation table is initialized");
        for (key, row) in table {
            for code in ["ja", "en", "zh"] {
                assert!(
                    row.get(code).is_some_and(|value| !value.trim().is_empty()),
                    "missing translation: {key}.{code}"
                );
            }
        }
    }
}
