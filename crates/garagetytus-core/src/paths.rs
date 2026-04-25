//! Cross-platform path resolution. All callers go through these
//! helpers; raw `~`-expansion or `MAKAKOO_HOME` references are
//! forbidden inside garagetytus (LD#9 — Windows path expansion).
//!
//! Layout per OS:
//!
//! | OS      | data_dir                                          | config_dir                          | log_dir                            |
//! |---------|---------------------------------------------------|-------------------------------------|------------------------------------|
//! | macOS   | `~/Library/Application Support/garagetytus/`      | `~/Library/Application Support/...` | `~/Library/Logs/garagetytus/`      |
//! | Linux   | `$XDG_DATA_HOME/garagetytus/`                     | `$XDG_CONFIG_HOME/garagetytus/`     | `<data_dir>/logs/`                 |
//! | Windows | `%LOCALAPPDATA%\garagetytus\`                     | `%APPDATA%\garagetytus\`            | `<data_dir>\logs\`                 |
//!
//! Override: `GARAGETYTUS_HOME=<dir>` collapses every directory to
//! `<dir>/{data,config,logs}/`. Useful for tests + container
//! deployments that want a single mounted volume.

use std::path::PathBuf;

/// Override env var. When set, every path resolves under this root.
pub const GARAGETYTUS_HOME_ENV: &str = "GARAGETYTUS_HOME";
const APP_NAME: &str = "garagetytus";

/// Effective home dir — `GARAGETYTUS_HOME` if set, otherwise the
/// platform-default data dir. Equivalent to `data_dir()` on a fresh
/// install.
pub fn home_dir() -> PathBuf {
    if let Ok(p) = std::env::var(GARAGETYTUS_HOME_ENV) {
        return PathBuf::from(p);
    }
    data_dir()
}

/// Platform-appropriate data directory.
pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var(GARAGETYTUS_HOME_ENV) {
        return PathBuf::from(p).join("data");
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("./.garagetytus-data"))
        .join(APP_NAME)
}

/// Platform-appropriate config directory.
pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var(GARAGETYTUS_HOME_ENV) {
        return PathBuf::from(p).join("config");
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("./.garagetytus-config"))
        .join(APP_NAME)
}

/// Platform-appropriate log directory.
pub fn log_dir() -> PathBuf {
    if let Ok(p) = std::env::var(GARAGETYTUS_HOME_ENV) {
        return PathBuf::from(p).join("logs");
    }
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library/Logs")
            .join(APP_NAME)
    }
    #[cfg(not(target_os = "macos"))]
    {
        data_dir().join("logs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn override_collapses_all_paths_under_one_root() {
        let _g = LOCK.lock().unwrap();
        std::env::set_var(GARAGETYTUS_HOME_ENV, "/tmp/garagetytus-test");
        assert_eq!(home_dir(), PathBuf::from("/tmp/garagetytus-test"));
        assert_eq!(data_dir(), PathBuf::from("/tmp/garagetytus-test/data"));
        assert_eq!(config_dir(), PathBuf::from("/tmp/garagetytus-test/config"));
        assert_eq!(log_dir(), PathBuf::from("/tmp/garagetytus-test/logs"));
        std::env::remove_var(GARAGETYTUS_HOME_ENV);
    }

    #[test]
    fn defaults_route_through_dirs_crate() {
        let _g = LOCK.lock().unwrap();
        std::env::remove_var(GARAGETYTUS_HOME_ENV);
        let d = data_dir();
        let c = config_dir();
        let l = log_dir();
        assert!(d.ends_with(APP_NAME));
        assert!(c.ends_with(APP_NAME));
        // log_dir on Mac is under ~/Library/Logs/, not data dir.
        #[cfg(target_os = "macos")]
        assert!(l.ends_with(APP_NAME) && l.to_string_lossy().contains("Library/Logs"));
        #[cfg(not(target_os = "macos"))]
        assert!(l.starts_with(d.parent().unwrap()));
    }
}
