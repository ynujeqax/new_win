//! Which role this `NewWin.exe` process plays.

#[derive(Debug, PartialEq, Eq)]
pub enum Role {
    Watchdog,
    Shell,
    /// Shows the native taskbar again (after tests, from the uninstaller) and exits.
    RestoreTaskbar,
    /// Ends every NewWin process and shows the native taskbar (installer, uninstaller, Settings).
    Quit,
    /// Fetches shell icons for the shell over stdin/stdout (1.0.1); started and stopped by it.
    IconHelper,
}

/// Anything other than an explicit `--shell` is the watchdog, so a plain double-click
/// or the autostart entry always goes through crash supervision.
pub fn role_from_args(args: &[String]) -> Role {
    if args.iter().any(|a| a == "--shell") {
        Role::Shell
    } else if args.iter().any(|a| a == "--restore-taskbar") {
        Role::RestoreTaskbar
    } else if args.iter().any(|a| a == "--quit") {
        Role::Quit
    } else if args.iter().any(|a| a == "--icon-helper") {
        Role::IconHelper
    } else {
        Role::Watchdog
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn roles() {
        assert_eq!(role_from_args(&args(&[])), Role::Watchdog);
        assert_eq!(role_from_args(&args(&["--watchdog"])), Role::Watchdog);
        assert_eq!(role_from_args(&args(&["--shell"])), Role::Shell);
        assert_eq!(role_from_args(&args(&["--bogus"])), Role::Watchdog);
        assert_eq!(role_from_args(&args(&["--restore-taskbar"])), Role::RestoreTaskbar);
        assert_eq!(role_from_args(&args(&["--quit"])), Role::Quit);
        assert_eq!(role_from_args(&args(&["--icon-helper"])), Role::IconHelper);
    }
}
