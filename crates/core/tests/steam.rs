use newwin_core::steam::{game_dir, manifest_name};

const CS2: &str = "\"AppState\"\n{\n\t\"appid\"\t\t\"730\"\n\t\"LauncherPath\"\t\t\"C:\\\\Programs\\\\Steam\\\\steam.exe\"\n\t\"name\"\t\t\"Counter-Strike 2\"\n\t\"installdir\"\t\t\"Counter-Strike Global Offensive\"\n\t\"UserConfig\"\n\t{\n\t\t\"name\"\t\t\"Other\"\n\t}\n}\n";

#[test]
fn a_game_exe_names_its_library_and_install_folder() {
    let exe = r"d:\steamlibrary\steamapps\common\counter-strike global offensive\game\bin\win64\cs2.exe";
    assert_eq!(game_dir(exe), Some((r"d:\steamlibrary\steamapps", "counter-strike global offensive")));
    assert_eq!(game_dir(r"C:\Programs\Steam\SteamApps\Common\Terraria\Terraria.exe"), Some((r"C:\Programs\Steam\SteamApps", "Terraria")));
}

#[test]
fn other_exes_are_not_games() {
    assert_eq!(game_dir(r"C:\Programs\Steam\steam.exe"), None);
    assert_eq!(game_dir(r"C:\Programs\Steam\steamapps\common\loose.exe"), None);
    assert_eq!(game_dir(r"C:\Windows\System32\notepad.exe"), None);
}

#[test]
fn the_manifest_of_the_install_folder_gives_the_store_name() {
    assert_eq!(manifest_name(CS2, "counter-strike global offensive").as_deref(), Some("Counter-Strike 2"));
    assert_eq!(manifest_name(CS2, "Terraria"), None);
    assert_eq!(manifest_name("garbage", "Terraria"), None);
}

#[test]
fn escaped_characters_in_a_name_are_unescaped() {
    let acf = "\"AppState\"\n{\n\t\"name\"\t\t\"Say \\\"Hi\\\" \\\\o/\"\n\t\"installdir\"\t\t\"Hi\"\n}\n";
    assert_eq!(manifest_name(acf, "hi").as_deref(), Some(r#"Say "Hi" \o/"#));
}
