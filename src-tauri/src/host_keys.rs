//! SSH host key TOFU (trust on first use).
//!
//! Fingerprints are stored in `host_keys.json` under the app data dir as
//! `{ "host:port": "sha256:BASE64" }` using the OpenSSH SHA-256 fingerprint format.

use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use russh::keys::ssh_key::{HashAlg, PublicKey};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

static STORE: OnceLock<Mutex<HostKeyStore>> = OnceLock::new();

/// Load or create the store. Must be called once during app setup.
pub fn init(data_dir: PathBuf) -> Result<()> {
    let store = HostKeyStore::load(data_dir)?;
    STORE
        .set(Mutex::new(store))
        .map_err(|_| anyhow::anyhow!("host key store already initialized"))?;
    Ok(())
}

fn store() -> &'static Mutex<HostKeyStore> {
    STORE.get().expect("host key store not initialized")
}

/// Verify the server key against stored fingerprints (TOFU on first connect).
pub fn verify(host: &str, port: u16, server_public_key: &PublicKey) -> Result<()> {
    let fp = fingerprint(server_public_key);
    let key = host_key(host, port);
    store().lock().verify(&key, &fp)
}

pub fn list() -> HashMap<String, String> {
    store().lock().entries.clone()
}

pub fn forget(host: &str, port: u16) -> Result<()> {
    let key = host_key(host, port);
    store().lock().forget(&key)
}

fn host_key(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

fn fingerprint(key: &PublicKey) -> String {
    let raw = format!("{}", key.fingerprint(HashAlg::Sha256));
    // OpenSSH uses `SHA256:` — normalize to lowercase for storage.
    raw.replacen("SHA256:", "sha256:", 1)
}

struct HostKeyStore {
    path: PathBuf,
    entries: HashMap<String, String>,
}

impl HostKeyStore {
    fn load(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating app data dir {}", dir.display()))?;
        let path = dir.join("host_keys.json");
        let entries = if path.exists() {
            let bytes = std::fs::read(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            serde_json::from_slice(&bytes).unwrap_or_default()
        } else {
            HashMap::new()
        };
        Ok(Self { path, entries })
    }

    fn verify(&mut self, key: &str, fp: &str) -> Result<()> {
        match self.entries.get(key) {
            None => {
                self.entries.insert(key.to_string(), fp.to_string());
                self.persist()?;
                Ok(())
            }
            Some(stored) if stored == fp => Ok(()),
            Some(stored) => bail!(
                "SSH host key mismatch for {key}: expected {stored}, got {fp}. \
                 If the server was reinstalled, forget the stored key and reconnect."
            ),
        }
    }

    fn forget(&mut self, key: &str) -> Result<()> {
        self.entries.remove(key);
        self.persist()
    }

    fn persist(&self) -> Result<()> {
        let json = serde_json::to_vec_pretty(&self.entries).context("serializing host_keys.json")?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).context("replacing host_keys.json")?;
        Ok(())
    }
}
