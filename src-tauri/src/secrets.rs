//! Secret storage backed by the OS keyring (Windows Credential Manager).
//!
//! Secrets are keyed by `<tunnel-id>:<kind>` so they never live in the
//! plaintext `tunnels.json` profile file.

use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE: &str = "tunnelo";

pub const KIND_PASSWORD: &str = "password";
pub const KIND_PASSPHRASE: &str = "passphrase";

fn entry(id: &str, kind: &str) -> Result<Entry> {
    Entry::new(SERVICE, &format!("{id}:{kind}")).context("opening keyring entry")
}

pub fn set(id: &str, kind: &str, value: &str) -> Result<()> {
    entry(id, kind)?
        .set_password(value)
        .context("storing secret in keyring")
}

pub fn get(id: &str, kind: &str) -> Option<String> {
    entry(id, kind).ok()?.get_password().ok()
}

pub fn delete(id: &str, kind: &str) -> Result<()> {
    if let Ok(e) = entry(id, kind) {
        // Missing entries are fine.
        let _ = e.delete_credential();
    }
    Ok(())
}

pub fn has(id: &str, kind: &str) -> bool {
    get(id, kind).is_some()
}

/// Remove every secret associated with a tunnel id.
pub fn delete_all(id: &str) {
    let _ = delete(id, KIND_PASSWORD);
    let _ = delete(id, KIND_PASSPHRASE);
}
