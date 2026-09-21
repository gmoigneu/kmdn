//! Provider token storage (D10). Tokens live in the OS keychain (macOS Keychain, Windows
//! Credential Manager, Secret Service on Linux). Where no keychain is reachable, a 0600 file in
//! the app data directory is the fallback.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("corrupt secret store: {0}")]
    Corrupt(String),
    #[error("keychain: {0}")]
    Keychain(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredToken {
    pub host: String,
    pub login: String,
    pub token: String,
    /// "device_flow" or "pat"
    pub kind: String,
    /// Display name from the provider profile, used as the commit author name (05-git identity).
    #[serde(default)]
    pub name: Option<String>,
}

pub trait SecretStore: Send + Sync {
    fn get(&self, host: &str) -> Result<Option<StoredToken>, SecretError>;
    fn put(&self, token: &StoredToken) -> Result<(), SecretError>;
    fn delete(&self, host: &str) -> Result<(), SecretError>;
    fn hosts(&self) -> Result<Vec<String>, SecretError>;
}

/// JSON file, mode 0600 on unix. Interim backend.
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    fn read(&self) -> Result<BTreeMap<String, StoredToken>, SecretError> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) if s.trim().is_empty() => Ok(BTreeMap::new()),
            Ok(s) => serde_json::from_str(&s).map_err(|e| SecretError::Corrupt(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn write(&self, map: &BTreeMap<String, StoredToken>) -> Result<(), SecretError> {
        if let Some(p) = self.path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let tmp = self.path.with_extension("tmp");
        // Never follow a pre-existing file or symlink at the temp path, and create the file
        // with 0600 from the start so it is never world-readable, even briefly (review S9).
        let _ = std::fs::remove_file(&tmp);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        {
            use std::io::Write;
            let mut f = opts.open(&tmp)?;
            f.write_all(
                &serde_json::to_vec_pretty(map).map_err(|e| SecretError::Corrupt(e.to_string()))?,
            )?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

impl SecretStore for FileStore {
    fn get(&self, host: &str) -> Result<Option<StoredToken>, SecretError> {
        Ok(self.read()?.remove(host))
    }
    fn put(&self, token: &StoredToken) -> Result<(), SecretError> {
        let mut m = self.read()?;
        m.insert(token.host.clone(), token.clone());
        self.write(&m)
    }
    fn delete(&self, host: &str) -> Result<(), SecretError> {
        let mut m = self.read()?;
        m.remove(host);
        self.write(&m)
    }
    fn hosts(&self) -> Result<Vec<String>, SecretError> {
        Ok(self.read()?.into_keys().collect())
    }
}

const SERVICE: &str = "kmdn";

/// OS keychain backend. One entry per host holding the token record as JSON; the list of
/// hosts lives in a small non-secret index file because keychains cannot enumerate entries.
pub struct KeyringStore {
    index: PathBuf,
}

impl KeyringStore {
    pub fn new(index: impl AsRef<Path>) -> Self {
        Self {
            index: index.as_ref().to_path_buf(),
        }
    }

    fn entry(host: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SERVICE, host).map_err(|e| SecretError::Keychain(e.to_string()))
    }

    fn read_index(&self) -> Vec<String> {
        std::fs::read_to_string(&self.index)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn write_index(&self, hosts: &[String]) -> Result<(), SecretError> {
        if let Some(p) = self.index.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(
            &self.index,
            serde_json::to_vec(hosts).map_err(|e| SecretError::Corrupt(e.to_string()))?,
        )?;
        Ok(())
    }

    /// Round-trips a canary entry to find out whether a keychain is actually usable here.
    pub fn probe() -> bool {
        let Ok(e) = keyring::Entry::new(SERVICE, "kmdn-probe") else {
            return false;
        };
        let ok =
            e.set_password("ok").is_ok() && e.get_password().map(|v| v == "ok").unwrap_or(false);
        let _ = e.delete_credential();
        ok
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, host: &str) -> Result<Option<StoredToken>, SecretError> {
        match Self::entry(host)?.get_password() {
            Ok(json) => Ok(Some(
                serde_json::from_str(&json).map_err(|e| SecretError::Corrupt(e.to_string()))?,
            )),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Keychain(e.to_string())),
        }
    }
    fn put(&self, token: &StoredToken) -> Result<(), SecretError> {
        let json = serde_json::to_string(token).map_err(|e| SecretError::Corrupt(e.to_string()))?;
        Self::entry(&token.host)?
            .set_password(&json)
            .map_err(|e| SecretError::Keychain(e.to_string()))?;
        let mut hosts = self.read_index();
        if !hosts.contains(&token.host) {
            hosts.push(token.host.clone());
            hosts.sort();
            self.write_index(&hosts)?;
        }
        Ok(())
    }
    fn delete(&self, host: &str) -> Result<(), SecretError> {
        match Self::entry(host)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(SecretError::Keychain(e.to_string())),
        }
        let hosts: Vec<String> = self
            .read_index()
            .into_iter()
            .filter(|h| h != host)
            .collect();
        self.write_index(&hosts)
    }
    fn hosts(&self) -> Result<Vec<String>, SecretError> {
        Ok(self.read_index())
    }
}

/// The store to use on this machine: the keychain when it answers, else the 0600 file.
/// Tokens found in the file are moved into the keychain the first time it is available.
pub fn open_default(data_dir: &Path) -> Box<dyn SecretStore> {
    let file = FileStore::new(data_dir.join("secrets.json"));
    if !KeyringStore::probe() {
        return Box::new(file);
    }
    let ring = KeyringStore::new(data_dir.join("secrets-index.json"));
    if let Ok(hosts) = file.hosts() {
        let mut moved = 0;
        for h in hosts {
            if let Ok(Some(t)) = file.get(&h) {
                if ring.put(&t).is_ok() && file.delete(&h).is_ok() {
                    moved += 1;
                }
            }
        }
        if moved > 0 {
            let _ = std::fs::remove_file(data_dir.join("secrets.json"));
        }
    }
    Box::new(ring)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_permissions() {
        let d = tempfile::tempdir().unwrap();
        let store = FileStore::new(d.path().join("kmdn/secrets.json"));
        assert!(store.get("github.com").unwrap().is_none());
        store
            .put(&StoredToken {
                host: "github.com".into(),
                login: "alice".into(),
                token: "gho_x".into(),
                kind: "device_flow".into(),
                name: None,
            })
            .unwrap();
        store
            .put(&StoredToken {
                host: "gitlab.example.org".into(),
                login: "alice".into(),
                token: "glpat".into(),
                kind: "pat".into(),
                name: None,
            })
            .unwrap();
        assert_eq!(store.get("github.com").unwrap().unwrap().token, "gho_x");
        assert_eq!(
            store.hosts().unwrap(),
            vec!["github.com", "gitlab.example.org"]
        );
        store.delete("github.com").unwrap();
        assert!(store.get("github.com").unwrap().is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(d.path().join("kmdn/secrets.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn open_default_returns_a_working_store_on_any_machine() {
        let d = tempfile::tempdir().unwrap();
        let store = open_default(d.path());
        store
            .put(&StoredToken {
                host: "example.org".into(),
                login: "u".into(),
                token: "t".into(),
                kind: "pat".into(),
                name: None,
            })
            .unwrap();
        assert_eq!(store.get("example.org").unwrap().unwrap().token, "t");
        assert_eq!(store.hosts().unwrap(), vec!["example.org"]);
        store.delete("example.org").unwrap();
        assert!(store.get("example.org").unwrap().is_none());
    }
}
