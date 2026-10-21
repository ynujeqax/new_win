//! UI strings. The language follows the Windows display language; English is the fallback.

pub struct Strings {
    pub new_window: &'static str,
    pub close: &'static str,
    pub close_all_windows: &'static str,
    pub settings: &'static str,
    pub restart: &'static str,
    pub quit: &'static str,
}

static EN: Strings = Strings {
    new_window: "New window",
    close: "Close",
    close_all_windows: "Close all windows",
    settings: "Settings…",
    restart: "Restart",
    quit: "Quit NewWin",
};

static UK: Strings = Strings {
    new_window: "Нове вікно",
    close: "Закрити",
    close_all_windows: "Закрити всі вікна",
    settings: "Налаштування…",
    restart: "Перезапустити",
    quit: "Вийти з NewWin",
};

/// `lang_tag` is a BCP-47 tag such as `en-US` or `uk-UA`.
pub fn strings_for(lang_tag: &str) -> &'static Strings {
    let lang = lang_tag.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    match lang.as_str() {
        "uk" => &UK,
        _ => &EN,
    }
}
