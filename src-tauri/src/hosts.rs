//! Managed entries in the system hosts file (`# tunnelo-managed` marker).

use crate::elevation;
use anyhow::{Context, Result};
use std::collections::HashMap;
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;

pub const MARKER: &str = "# tunnelo-managed";

/// Path to the system hosts file (WOW64-safe for reads/writes from this process).
pub fn hosts_path() -> PathBuf {
    #[cfg(windows)]
    {
        hosts_path_elevated()
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

/// Real System32 hosts path — always use for elevated 64-bit PowerShell writes.
#[cfg(windows)]
pub fn hosts_path_elevated() -> PathBuf {
    // 32-bit processes on 64-bit Windows are redirected to SysWOW64\drivers\etc\hosts.
    if cfg!(target_pointer_width = "32") && Path::new(r"C:\Windows\Sysnative").exists() {
        return PathBuf::from(r"C:\Windows\Sysnative\drivers\etc\hosts");
    }
    PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
}

/// Managed tunnelo lines from hosts content, normalized for comparison.
pub fn managed_lines(content: &str) -> Vec<String> {
    let stripped = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines: Vec<String> = stripped
        .lines()
        .filter(|line| line.contains(MARKER))
        .map(|line| line.trim().to_string())
        .collect();
    lines.sort();
    lines
}

/// Whether the hosts file contains the expected tunnelo-managed entries.
#[cfg(windows)]
pub fn managed_entries_match(path: &Path, expected_content: &str) -> Result<bool> {
    let actual =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(managed_lines(&actual) == managed_lines(expected_content))
}

/// Remove all tunnelo-managed lines left from a previous crash.
pub fn cleanup_orphans() -> Result<()> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let filtered: Vec<&str> = content
        .lines()
        .filter(|line| !line.contains(MARKER))
        .collect();
    if filtered.len() == content.lines().count() {
        return Ok(());
    }
    let new_content = if filtered.is_empty() {
        String::new()
    } else {
        format!("{}\n", filtered.join("\n"))
    };
    elevation::write_hosts_file(&new_content)
        .with_context(|| format!("cleaning orphan hosts entries in {}", path.display()))
}

/// Sync the hosts file to match the desired domain → ref-count map.
pub fn sync_domains(domains: &HashMap<String, u32>) -> Result<()> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;

    let mut kept: Vec<String> = content
        .lines()
        .filter(|line| !line.contains(MARKER))
        .map(|s| s.to_string())
        .collect();

    let mut sorted: Vec<_> = domains.iter().filter(|(_, &c)| c > 0).collect();
    sorted.sort_by_key(|(h, _)| h.as_str());

    for (host, _) in sorted {
        kept.push(format!("127.0.0.1 {host} {MARKER}"));
    }

    let mut new_content = kept.join("\n");
    if !new_content.is_empty() {
        new_content.push('\n');
    }
    elevation::write_hosts_file(&new_content)
        .with_context(|| format!("writing {}", path.display()))
}
