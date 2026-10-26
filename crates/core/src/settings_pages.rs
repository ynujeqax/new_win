//! Windows Settings pages Spotlight can open (spec: 1.0.6 §2.1): `ms-settings:` URIs with names
//! in English and Ukrainian and the everyday words people search them by, in both languages.

use crate::spotlight::{Candidate, Kind};

pub struct Page {
    pub uri: &'static str,
    pub en: &'static str,
    pub uk: &'static str,
    /// More ways to find it, either language (lowercase).
    pub words: &'static [&'static str],
}

const fn p(uri: &'static str, en: &'static str, uk: &'static str, words: &'static [&'static str]) -> Page {
    Page { uri, en, uk, words }
}

pub const PAGES: &[Page] = &[
    // System
    p("ms-settings:display", "Display", "Дисплей", &["screen", "monitor", "brightness", "resolution", "scale", "hdr", "екран", "монітор", "яскравість", "роздільна здатність", "масштаб"]),
    p("ms-settings:nightlight", "Night light", "Нічне освітлення", &["blue light", "warm", "нічний режим", "синє світло"]),
    p("ms-settings:display-advancedgraphics", "Graphics", "Графіка", &["gpu", "graphics card", "відеокарта", "графічний процесор"]),
    p("ms-settings:sound", "Sound", "Звук", &["audio", "volume", "speakers", "microphone", "гучність", "динаміки", "мікрофон", "аудіо"]),
    p("ms-settings:sound-devices", "Sound devices", "Звукові пристрої", &["audio devices", "output", "input", "пристрої відтворення"]),
    p("ms-settings:apps-volume", "Volume mixer", "Мікшер гучності", &["mixer", "app volume", "мікшер"]),
    p("ms-settings:notifications", "Notifications", "Сповіщення", &["alerts", "toasts", "повідомлення"]),
    p("ms-settings:quiethours", "Focus", "Фокусування", &["do not disturb", "focus assist", "не турбувати"]),
    p("ms-settings:powersleep", "Power & sleep", "Живлення та сон", &["power", "sleep", "screen off", "живлення", "сон", "енергія"]),
    p("ms-settings:batterysaver", "Battery saver", "Економія заряду", &["battery", "акумулятор", "батарея"]),
    p("ms-settings:storagesense", "Storage", "Пам'ять", &["disk", "space", "storage sense", "cleanup", "диск", "місце", "сховище", "очищення"]),
    p("ms-settings:nearbysharing", "Nearby sharing", "Обмін поблизу", &["share", "надсилання поблизу"]),
    p("ms-settings:multitasking", "Multitasking", "Багатозадачність", &["snap", "alt tab", "virtual desktops", "прив'язка вікон", "віртуальні робочі столи"]),
    p("ms-settings:clipboard", "Clipboard", "Буфер обміну", &["copy", "paste", "history", "копіювання", "вставлення"]),
    p("ms-settings:remotedesktop", "Remote Desktop", "Віддалений робочий стіл", &["rdp", "remote", "віддалений доступ"]),
    p("ms-settings:about", "About", "Про систему", &["system info", "pc name", "specs", "windows version", "rename pc", "відомості", "характеристики", "версія windows", "ім'я пк"]),
    p("ms-settings:activation", "Activation", "Активація", &["product key", "license", "ключ продукту", "ліцензія"]),
    p("ms-settings:troubleshoot", "Troubleshoot", "Виправлення неполадок", &["fix", "problems", "неполадки", "проблеми"]),
    p("ms-settings:recovery", "Recovery", "Відновлення", &["reset", "reset this pc", "скидання", "повернути"]),
    p("ms-settings:projection", "Projecting to this PC", "Проєктування на цей ПК", &["miracast", "проєктор"]),
    // Bluetooth & devices
    p("ms-settings:bluetooth", "Bluetooth & devices", "Bluetooth і пристрої", &["bt", "bluetooth", "headphones", "pair", "навушники", "блютуз", "підключити"]),
    p("ms-settings:connecteddevices", "Devices", "Пристрої", &["connected devices", "add device", "додати пристрій"]),
    p("ms-settings:printers", "Printers & scanners", "Принтери та сканери", &["printer", "scanner", "print", "принтер", "сканер", "друк"]),
    p("ms-settings:mobile-devices", "Mobile devices", "Мобільні пристрої", &["phone", "phone link", "телефон", "смартфон"]),
    p("ms-settings:mousetouchpad", "Mouse", "Миша", &["pointer", "cursor speed", "scroll", "вказівник", "прокручування", "курсор"]),
    p("ms-settings:devices-touchpad", "Touchpad", "Сенсорна панель", &["trackpad", "gestures", "тачпад", "жести"]),
    p("ms-settings:pen", "Pen & Windows Ink", "Перо та Windows Ink", &["stylus", "стилус"]),
    p("ms-settings:autoplay", "AutoPlay", "Автовідтворення", &["autorun", "автозапуск носіїв"]),
    p("ms-settings:usb", "USB", "USB", &["usb notifications", "флешка"]),
    p("ms-settings:camera", "Cameras", "Камери", &["webcam", "camera", "вебкамера", "камера"]),
    // Network & internet
    p("ms-settings:network-status", "Network & internet", "Мережа й Інтернет", &["network", "internet", "connection", "мережа", "інтернет", "підключення"]),
    p("ms-settings:network-wifi", "Wi-Fi", "Wi-Fi", &["wifi", "wireless", "wlan", "вайфай", "бездротова мережа"]),
    p("ms-settings:network-ethernet", "Ethernet", "Ethernet", &["cable", "lan", "wired", "кабель", "дротова мережа"]),
    p("ms-settings:network-vpn", "VPN", "VPN", &["vpn", "впн"]),
    p("ms-settings:network-mobilehotspot", "Mobile hotspot", "Мобільна точка доступу", &["hotspot", "tethering", "роздати інтернет", "точка доступу"]),
    p("ms-settings:network-airplanemode", "Airplane mode", "Режим польоту", &["flight mode", "літак"]),
    p("ms-settings:network-proxy", "Proxy", "Проксі-сервер", &["proxy", "проксі"]),
    p("ms-settings:network-advancedsettings", "Advanced network settings", "Додаткові параметри мережі", &["adapters", "network reset", "адаптери", "скидання мережі"]),
    p("ms-settings:datausage", "Data usage", "Використання даних", &["traffic", "трафік"]),
    // Personalisation
    p("ms-settings:personalization", "Personalization", "Персоналізація", &["customize", "look", "вигляд", "оформлення"]),
    p("ms-settings:personalization-background", "Background", "Фон", &["wallpaper", "desktop background", "шпалери", "тло"]),
    p("ms-settings:colors", "Colors", "Кольори", &["dark mode", "light mode", "accent", "theme color", "темний режим", "світлий режим", "акцентний колір"]),
    p("ms-settings:themes", "Themes", "Теми", &["theme", "тема"]),
    p("ms-settings:lockscreen", "Lock screen", "Екран блокування", &["lock", "заблокувати", "блокування"]),
    p("ms-settings:personalization-start", "Start", "Пуск", &["start menu", "меню пуск"]),
    p("ms-settings:taskbar", "Taskbar", "Панель завдань", &["таскбар"]),
    p("ms-settings:fonts", "Fonts", "Шрифти", &["typeface", "шрифт"]),
    p("ms-settings:personalization-touchkeyboard", "Touch keyboard", "Сенсорна клавіатура", &["on-screen keyboard", "екранна клавіатура"]),
    // Apps
    p("ms-settings:appsfeatures", "Installed apps", "Інстальовані програми", &["apps", "programs", "uninstall", "remove", "програми", "видалити", "застосунки"]),
    p("ms-settings:defaultapps", "Default apps", "Програми за замовчуванням", &["default browser", "open with", "браузер за замовчуванням", "відкривати за допомогою"]),
    p("ms-settings:optionalfeatures", "Optional features", "Додаткові компоненти", &["features", "компоненти"]),
    p("ms-settings:startupapps", "Startup apps", "Автозавантаження", &["startup", "autostart", "run at login", "автозапуск"]),
    p("ms-settings:appsforwebsites", "Apps for websites", "Програми для веб-сайтів", &["links", "посилання"]),
    // Accounts
    p("ms-settings:yourinfo", "Your info", "Ваші відомості", &["account", "profile", "picture", "обліковий запис", "профіль", "аватар"]),
    p("ms-settings:signinoptions", "Sign-in options", "Параметри входу", &["password", "pin", "windows hello", "fingerprint", "face", "пароль", "пін", "відбиток", "вхід"]),
    p("ms-settings:emailandaccounts", "Email & accounts", "Електронна пошта й облікові записи", &["email", "mail accounts", "пошта"]),
    p("ms-settings:otherusers", "Other users", "Інші користувачі", &["family", "add user", "users", "користувачі", "додати користувача", "родина"]),
    p("ms-settings:sync", "Windows backup", "Резервне копіювання Windows", &["backup", "sync", "onedrive", "резервна копія", "синхронізація"]),
    // Time & language
    p("ms-settings:dateandtime", "Date & time", "Дата й час", &["time", "clock", "time zone", "date", "час", "годинник", "часовий пояс", "дата"]),
    p("ms-settings:regionlanguage", "Language & region", "Мова та регіон", &["language", "display language", "мова", "мова інтерфейсу"]),
    p("ms-settings:regionformatting", "Region", "Регіон", &["format", "region format", "country", "формат", "країна"]),
    p("ms-settings:keyboard", "Typing", "Введення", &["keyboard", "layout", "input", "autocorrect", "клавіатура", "розкладка", "автовиправлення"]),
    p("ms-settings:speech", "Speech", "Мовлення", &["voice", "dictation", "голос", "диктування"]),
    // Gaming
    p("ms-settings:gaming-gamebar", "Game Bar", "Ігрова панель", &["xbox", "game bar", "ігри"]),
    p("ms-settings:gaming-gamemode", "Game Mode", "Ігровий режим", &["gaming", "game", "games", "ігри", "гра"]),
    p("ms-settings:gaming-gamedvr", "Captures", "Записи", &["screen recording", "record", "запис екрана"]),
    // Accessibility
    p("ms-settings:easeofaccess-display", "Text size", "Розмір тексту", &["accessibility", "bigger text", "спеціальні можливості", "великий текст"]),
    p("ms-settings:easeofaccess-visualeffects", "Visual effects", "Візуальні ефекти", &["animations", "transparency", "анімації", "прозорість"]),
    p("ms-settings:easeofaccess-mousepointer", "Mouse pointer", "Вказівник миші", &["cursor size", "розмір курсора"]),
    p("ms-settings:easeofaccess-colorfilter", "Color filters", "Кольорові фільтри", &["color blind", "grayscale", "дальтонізм", "відтінки сірого"]),
    p("ms-settings:easeofaccess-highcontrast", "Contrast themes", "Контрастні теми", &["high contrast", "висока контрастність"]),
    p("ms-settings:easeofaccess-narrator", "Narrator", "Диктор", &["screen reader", "читання з екрана"]),
    p("ms-settings:easeofaccess-magnifier", "Magnifier", "Лупа", &["zoom", "збільшення"]),
    p("ms-settings:easeofaccess-keyboard", "Keyboard accessibility", "Спеціальні можливості клавіатури", &["sticky keys", "on-screen keyboard", "залипання клавіш"]),
    // Privacy & security
    p("ms-settings:privacy", "Privacy & security", "Конфіденційність і захист", &["privacy", "security", "permissions", "конфіденційність", "безпека", "дозволи"]),
    p("ms-settings:windowsdefender", "Windows Security", "Безпека Windows", &["antivirus", "defender", "firewall", "virus", "антивірус", "брандмауер", "віруси"]),
    p("ms-settings:privacy-location", "Location", "Розташування", &["gps", "геолокація", "місцезнаходження"]),
    p("ms-settings:privacy-webcam", "Camera permissions", "Дозволи камери", &["camera access", "доступ до камери"]),
    p("ms-settings:privacy-microphone", "Microphone permissions", "Дозволи мікрофона", &["microphone access", "mic", "доступ до мікрофона"]),
    p("ms-settings:findmydevice", "Find my device", "Пошук пристрою", &["lost", "загублений"]),
    p("ms-settings:developers", "For developers", "Для розробників", &["developer mode", "режим розробника", "sudo"]),
    // Windows Update
    p("ms-settings:windowsupdate", "Windows Update", "Windows Update", &["update", "updates", "patch", "оновлення", "оновити"]),
    p("ms-settings:windowsupdate-history", "Update history", "Журнал оновлень", &["installed updates", "встановлені оновлення"]),
    p("ms-settings:windowsupdate-options", "Advanced update options", "Додаткові параметри оновлення", &["active hours", "години активності"]),
    p("ms-settings:windowsinsider", "Windows Insider Program", "Програма оцінювання Windows", &["insider", "інсайдер"]),
];

/// Every page as a Spotlight entry, named in the UI language; the other language's name is one of
/// its search words.
pub fn candidates(uk: bool) -> Vec<Candidate> {
    PAGES
        .iter()
        .enumerate()
        .map(|(i, page)| {
            let (name, other) = if uk { (page.uk, page.en) } else { (page.en, page.uk) };
            let mut words: Vec<String> = Vec::with_capacity(page.words.len() + 1);
            if other != name {
                words.push(other.to_string());
            }
            words.extend(page.words.iter().map(|w| w.to_string()));
            let mut c = Candidate { name: name.to_string(), id: String::new(), kind: Kind::SettingsPage(i), file_path: None, packaged: false, words };
            c.id = c.usage_key();
            c
        })
        .collect()
}
