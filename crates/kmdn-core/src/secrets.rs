//! Provider token storage (D10). The OS keychain backend lands with packaging (#14); until
//! then a 0600 file in the app data directory keeps tokens out of the repo and out of logs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("corrupt secret store: {0}")]
    Corrupt(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredToken {
    pub host: String,
    pub login: String,
    pub token: String,
    /// "device_flow" or "pat"
    pub kind: String,
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
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(map).map_err(|e| SecretError::Corrupt(e.to_string()))?,
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
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
            })
            .unwrap();
        store
            .put(&StoredToken {
                host: "gitlab.example.org".into(),
                login: "alice".into(),
                token: "glpat".into(),
                kind: "pat".into(),
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
}
