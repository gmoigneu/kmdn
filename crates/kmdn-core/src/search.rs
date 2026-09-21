//! Full-text search over document titles and bodies (D12, D33). Lives in the app-local
//! SQLite file next to the drafts; rebuilt from a scan in the background and never committed.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::index::Scan;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    pub path: String,
    pub title: String,
    /// Matching excerpt with the terms wrapped in `[` `]`.
    pub snippet: String,
}

pub struct SearchIndex {
    conn: Connection,
}

impl SearchIndex {
    pub fn open(db: &Path) -> Result<Self, SearchError> {
        if let Some(p) = db.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let conn = Connection::open(db)?;
        Self::init(conn)
    }

    pub fn in_memory() -> Result<Self, SearchError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, SearchError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
               root UNINDEXED, path UNINDEXED, title, body, tokenize = 'porter unicode61'
             );",
        )?;
        Ok(Self { conn })
    }

    /// Replaces the index for `root` with the documents in `scan`.
    pub fn reindex(&mut self, root: &str, scan: &Scan) -> Result<usize, SearchError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM docs_fts WHERE root = ?1", params![root])?;
        {
            let mut ins = tx.prepare(
                "INSERT INTO docs_fts (root, path, title, body) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (doc, body) in scan.docs.iter().zip(scan.bodies.iter()) {
                ins.execute(params![root, doc.path, doc.title, body])?;
            }
        }
        tx.commit()?;
        Ok(scan.docs.len())
    }

    /// Ranked hits for a free-text query. Each word becomes a prefix term so partial typing works.
    pub fn search(&self, root: &str, query: &str, limit: usize) -> Result<Vec<Hit>, SearchError> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|w| w.replace('"', ""))
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{w}\"*"))
            .collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let expr = terms.join(" ");
        let mut st = self.conn.prepare(
            "SELECT path, title, snippet(docs_fts, 3, '[', ']', '…', 14)
             FROM docs_fts WHERE root = ?1 AND docs_fts MATCH ?2
             ORDER BY bm25(docs_fts, 0, 0, 4.0, 1.0) LIMIT ?3",
        )?;
        let rows = st.query_map(params![root, expr, limit as i64], |r| {
            Ok(Hit {
                path: r.get(0)?,
                title: r.get(1)?,
                snippet: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn count(&self, root: &str) -> Result<usize, SearchError> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM docs_fts WHERE root = ?1",
            params![root],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::scan_full;

    #[test]
    fn indexes_and_searches_bodies_with_prefixes() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("deploy.md"), "---\ntitle: Deploy runbook\n---\n# Deploy\n\nRun the rollout, then watch the dashboard for errors.\n").unwrap();
        std::fs::write(
            d.path().join("onboarding.md"),
            "# Onboarding\n\nNew teammates get a laptop and a dashboard account.\n",
        )
        .unwrap();
        let scan = scan_full(d.path());
        let mut idx = SearchIndex::in_memory().unwrap();
        assert_eq!(idx.reindex("/kb", &scan).unwrap(), 2);
        let hits = idx.search("/kb", "dashb", 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.snippet.contains("[dashboard]")));
        let hits = idx.search("/kb", "rollout errors", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Deploy runbook");
        assert!(idx.search("/kb", "   ", 10).unwrap().is_empty());
        assert!(idx.search("/other", "dashboard", 10).unwrap().is_empty());
        // reindex replaces, never accumulates
        idx.reindex("/kb", &scan).unwrap();
        assert_eq!(idx.count("/kb").unwrap(), 2);
    }
}
