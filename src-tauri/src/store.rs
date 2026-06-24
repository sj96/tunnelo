//! Persistence of tunnel profiles to a JSON file in the app data dir.

use crate::model::TunnelProfile;
use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::path::PathBuf;

/// Thread-safe, file-backed collection of tunnel profiles.
pub struct ProfileStore {
    path: PathBuf,
    profiles: RwLock<Vec<TunnelProfile>>,
}

impl ProfileStore {
    /// Load the store from `dir/tunnels.json`, creating the dir if needed.
    pub fn load(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating app data dir {}", dir.display()))?;
        let path = dir.join("tunnels.json");
        let profiles: Vec<TunnelProfile> = if path.exists() {
            let bytes = std::fs::read(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            serde_json::from_slice(&bytes).context("parsing tunnels.json")?
        } else {
            Vec::new()
        };
        let profiles: Vec<TunnelProfile> = profiles.into_iter().map(TunnelProfile::normalize).collect();
        Ok(Self {
            path,
            profiles: RwLock::new(profiles),
        })
    }

    fn persist(&self, profiles: &[TunnelProfile]) -> Result<()> {
        let json = serde_json::to_vec_pretty(profiles).context("serializing profiles")?;
        // Write atomically via a temp file + rename.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).context("replacing tunnels.json")?;
        Ok(())
    }

    pub fn list(&self) -> Vec<TunnelProfile> {
        self.profiles.read().clone()
    }

    pub fn get(&self, id: &str) -> Option<TunnelProfile> {
        self.profiles.read().iter().find(|p| p.id == id).cloned()
    }

    /// Insert or update by id. Returns the stored profile.
    pub fn upsert(&self, mut profile: TunnelProfile) -> Result<TunnelProfile> {
        profile = profile.normalize();
        if profile.id.is_empty() {
            profile.id = TunnelProfile::new_id();
        }
        for m in &mut profile.mappings {
            if m.id.is_empty() {
                m.id = crate::model::ForwardMapping::new_id();
            }
        }
        {
            let mut guard = self.profiles.write();
            match guard.iter_mut().find(|p| p.id == profile.id) {
                Some(existing) => *existing = profile.clone(),
                None => guard.push(profile.clone()),
            }
            self.persist(&guard)?;
        }
        Ok(profile)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut guard = self.profiles.write();
        guard.retain(|p| p.id != id);
        self.persist(&guard)?;
        Ok(())
    }
}
