//! OS-specific path helpers for the desktop client.

use std::path::PathBuf;

/// Default OpenSSH private key path for this machine.
pub fn default_ssh_key_path() -> String {
    #[cfg(windows)]
    {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".into());
        format!(r"{home}\.ssh\id_ed25519")
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        format!("{home}/.ssh/id_ed25519")
    }
}

/// Expand `~` and normalize separators for the local OS.
pub fn resolve_key_path(path: &str) -> PathBuf {
    PathBuf::from(expand_home(path))
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return home_dir().unwrap_or_default();
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = home_dir() {
            return join_home(&home, rest);
        }
    }
    #[cfg(windows)]
    {
        return path.replace('/', "\\");
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

fn home_dir() -> Option<String> {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").ok()
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME").ok()
    }
}

fn join_home(home: &str, rest: &str) -> String {
    #[cfg(windows)]
    {
        format!(
            "{}\\{}",
            home.trim_end_matches('\\'),
            rest.replace('/', "\\")
        )
    }
    #[cfg(not(windows))]
    {
        format!("{}/{}", home.trim_end_matches('/'), rest)
    }
}
