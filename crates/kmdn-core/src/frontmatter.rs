//! YAML frontmatter (D11). All fields optional, unknown keys preserved.
//! Partial writes are text-level so untouched lines never change.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrontmatterError {
    #[error("frontmatter block is not closed")]
    Unclosed,
    #[error("invalid yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Status {
    #[default]
    Draft,
    Review,
    Published,
    Deprecated,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed: Option<String>,
    #[serde(flatten)]
    pub extra: serde_yaml::Mapping,
}

/// A document split into its raw frontmatter text (without fences) and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split<'a> {
    pub yaml: Option<&'a str>,
    pub body: &'a str,
    /// Byte offset where the body starts in the original text.
    pub body_start: usize,
}

/// Splits `---\n...\n---\n` off the top. Returns the whole text as body when absent.
pub fn split(text: &str) -> Result<Split<'_>, FrontmatterError> {
    let Some(rest) = text.strip_prefix("---") else {
        return Ok(Split {
            yaml: None,
            body: text,
            body_start: 0,
        });
    };
    let Some(rest) = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))
    else {
        return Ok(Split {
            yaml: None,
            body: text,
            body_start: 0,
        });
    };
    let yaml_start = text.len() - rest.len();
    let mut pos = yaml_start;
    for line in text[yaml_start..].split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" || trimmed == "..." {
            let yaml = &text[yaml_start..pos];
            let body_start = pos + line.len();
            return Ok(Split {
                yaml: Some(yaml),
                body: &text[body_start..],
                body_start,
            });
        }
        pos += line.len();
    }
    Err(FrontmatterError::Unclosed)
}

pub fn parse(text: &str) -> Result<(Frontmatter, &str), FrontmatterError> {
    let s = split(text)?;
    let fm = match s.yaml {
        Some(y) if !y.trim().is_empty() => serde_yaml::from_str(y)?,
        _ => Frontmatter::default(),
    };
    Ok((fm, s.body))
}

/// Sets or replaces one top-level scalar key without touching other lines.
/// Adds a frontmatter block when the document has none.
pub fn set_scalar(text: &str, key: &str, value: &str) -> Result<String, FrontmatterError> {
    let s = split(text)?;
    let rendered = serde_yaml::to_string(&serde_yaml::Value::String(value.to_string()))?;
    let rendered = rendered.trim_end();
    let new_line = format!("{key}: {rendered}");
    match s.yaml {
        None => Ok(format!("---\n{new_line}\n---\n{}", s.body)),
        Some(yaml) => {
            let mut lines: Vec<String> = yaml.lines().map(str::to_string).collect();
            let prefix = format!("{key}:");
            let mut replaced = false;
            for l in lines.iter_mut() {
                if l.starts_with(&prefix) && !l.starts_with(&format!("{prefix}:")) {
                    *l = new_line.clone();
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                lines.push(new_line);
            }
            Ok(format!("---\n{}\n---\n{}", lines.join("\n"), s.body))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "---\n# keep me\ntitle: Deploy runbook\ntags: [ops, deploy]\nstatus: published\ncustom: 42\n---\n# Body\n";

    #[test]
    fn parses_known_and_extra() {
        let (fm, body) = parse(DOC).unwrap();
        assert_eq!(fm.title.as_deref(), Some("Deploy runbook"));
        assert_eq!(fm.tags, vec!["ops", "deploy"]);
        assert_eq!(fm.status, Some(Status::Published));
        assert_eq!(fm.extra.get("custom").and_then(|v| v.as_i64()), Some(42));
        assert_eq!(body, "# Body\n");
    }

    #[test]
    fn no_frontmatter_is_fine() {
        let (fm, body) = parse("# Just a doc\n").unwrap();
        assert_eq!(fm, Frontmatter::default());
        assert_eq!(body, "# Just a doc\n");
    }

    #[test]
    fn unclosed_is_an_error_and_bad_yaml_too() {
        assert!(matches!(
            parse("---\ntitle: x\n"),
            Err(FrontmatterError::Unclosed)
        ));
        assert!(matches!(
            parse("---\ntitle: [\n---\n"),
            Err(FrontmatterError::Yaml(_))
        ));
    }

    #[test]
    fn set_scalar_preserves_comments_order_and_other_lines() {
        let out = set_scalar(DOC, "status", "deprecated").unwrap();
        assert!(out.contains("# keep me\ntitle: Deploy runbook\ntags: [ops, deploy]\nstatus: deprecated\ncustom: 42\n---\n# Body\n"));
        let added = set_scalar(DOC, "reviewed", "2026-09-19").unwrap();
        assert!(
            added.contains("custom: 42\nreviewed: '2026-09-19'\n---")
                || added.contains("custom: 42\nreviewed: 2026-09-19\n---")
        );
        let fresh = set_scalar("# Doc\n", "title", "Doc").unwrap();
        assert_eq!(fresh, "---\ntitle: Doc\n---\n# Doc\n");
    }
}
