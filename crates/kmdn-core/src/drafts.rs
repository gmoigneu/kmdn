//! Unsaved editor text, mirrored to app-local SQLite so a crash never loses typing (D27).
//! Never committed, never shared. Deleting the database is always safe (D12).

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub root: String,
    pub slug: String,
    pub path: String,
    pub text: String,
    pub updated_at: i64,
}

pub struct DraftStore {
    conn: Connection,
}

impl DraftStore {
    pub fn open(db: &Path) -> Result<Self, DraftError> {
        if let Some(p) = db.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let conn = Connection::open(db)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS drafts (
               root TEXT NOT NULL,
               slug TEXT NOT NULL,
               path TEXT NOT NULL,
               text TEXT NOT NULL,
               updated_at INTEGER NOT NULL,
               PRIMARY KEY (root, slug, path)
             );",
        )?;
        Ok(Self { conn })
    }

    pub fn in_memory() -> Result<Self, DraftError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS drafts (root TEXT NOT NULL, slug TEXT NOT NULL, path TEXT NOT NULL, text TEXT NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY (root, slug, path));",
        )?;
        Ok(Self { conn })
    }

    pub fn put(&self, root: &str, slug: &str, path: &str, text: &str) -> Result<(), DraftError> {
        self.conn.execute(
            "INSERT INTO drafts (root, slug, path, text, updated_at) VALUES (?1, ?2, ?3, ?4, unixepoch())
             ON CONFLICT(root, slug, path) DO UPDATE SET text = excluded.text, updated_at = excluded.updated_at",
            params![root, slug, path, text],
        )?;
        Ok(())
    }

    pub fn get(&self, root: &str, slug: &str, path: &str) -> Result<Option<Draft>, DraftError> {
        Ok(self
            .conn
            .query_row(
                "SELECT root, slug, path, text, updated_at FROM drafts WHERE root = ?1 AND slug = ?2 AND path = ?3",
                params![root, slug, path],
                |r| Ok(Draft { root: r.get(0)?, slug: r.get(1)?, path: r.get(2)?, text: r.get(3)?, updated_at: r.get(4)? }),
            )
            .optional()?)
    }

    pub fn delete(&self, root: &str, slug: &str, path: &str) -> Result<(), DraftError> {
        self.conn.execute(
            "DELETE FROM drafts WHERE root = ?1 AND slug = ?2 AND path = ?3",
            params![root, slug, path],
        )?;
        Ok(())
    }

    /// Every draft for a thread, newest first.
    pub fn list(&self, root: &str, slug: &str) -> Result<Vec<Draft>, DraftError> {
        let mut st = self.conn.prepare("SELECT root, slug, path, text, updated_at FROM drafts WHERE root = ?1 AND slug = ?2 ORDER BY updated_at DESC")?;
        let rows = st.query_map(params![root, slug], |r| {
            Ok(Draft {
                root: r.get(0)?,
                slug: r.get(1)?,
                path: r.get(2)?,
                text: r.get(3)?,
                updated_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Drops drafts for threads that no longer exist.
    pub fn retain_slugs(&self, root: &str, slugs: &[String]) -> Result<usize, DraftError> {
        let mut removed = 0;
        let mut st = self
            .conn
            .prepare("SELECT DISTINCT slug FROM drafts WHERE root = ?1")?;
        let existing: Vec<String> = st
            .query_map(params![root], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for slug in existing {
            if !slugs.contains(&slug) {
                removed += self.conn.execute(
                    "DELETE FROM drafts WHERE root = ?1 AND slug = ?2",
                    params![root, slug],
                )?;
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_list_and_cleanup() {
        let s = DraftStore::in_memory().unwrap();
        assert!(s.get("/kb", "t", "a.md").unwrap().is_none());
        s.put("/kb", "t", "a.md", "# draft").unwrap();
        s.put("/kb", "t", "a.md", "# draft 2").unwrap();
        s.put("/kb", "gone", "b.md", "x").unwrap();
        assert_eq!(
            s.get("/kb", "t", "a.md").unwrap().unwrap().text,
            "# draft 2"
        );
        assert_eq!(s.list("/kb", "t").unwrap().len(), 1);
        assert_eq!(s.retain_slugs("/kb", &["t".into()]).unwrap(), 1);
        assert!(s.get("/kb", "gone", "b.md").unwrap().is_none());
        s.delete("/kb", "t", "a.md").unwrap();
        assert!(s.list("/kb", "t").unwrap().is_empty());
    }

    #[test]
    fn opens_a_file_backed_store() {
        let d = tempfile::tempdir().unwrap();
        let s = DraftStore::open(&d.path().join("kmdn/local.sqlite")).unwrap();
        s.put("/kb", "t", "a.md", "text").unwrap();
        drop(s);
        let s = DraftStore::open(&d.path().join("kmdn/local.sqlite")).unwrap();
        assert_eq!(s.get("/kb", "t", "a.md").unwrap().unwrap().text, "text");
    }
}
