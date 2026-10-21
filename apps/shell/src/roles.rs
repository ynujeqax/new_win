//! Which role this `NewWin.exe` process plays.

#[derive(Debug, PartialEq, Eq)]
pub enum Role {
    Watchdog,
    Shell,
}

/// Anything other than an explicit `--shell` is the watchdog, so a plain double-click
/// or the autostart entry always goes through crash supervision.
pub fn role_from_args(args: &[String]) -> Role {
    if args.iter().any(|a| a == "--shell") { Role::Shell } else { Role::Watchdog }
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
    }
}
