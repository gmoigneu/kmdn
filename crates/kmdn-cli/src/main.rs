//! kmdn-cli: check | index | init. Used by CI and scripts (D59, D60).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use kmdn_core::checks;
use kmdn_core::index;

#[derive(Parser)]
#[command(name = "kmdn-cli", version, about = "kmdn knowledge base tools")]
struct Cli {
    /// Knowledge base root. Defaults to the current directory.
    #[arg(global = true, long, default_value = ".")]
    path: PathBuf,
    /// Machine-readable JSON output.
    #[arg(global = true, long)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Broken links, invalid frontmatter, oversize assets, stale AGENTS.md. Exit 1 on errors.
    Check {
        /// Asset size cap in bytes.
        #[arg(long, default_value_t = checks::DEFAULT_ASSET_CAP)]
        asset_cap: u64,
    },
    /// Regenerate AGENTS.md.
    Index,
    /// Write the new knowledge base template into an existing, empty-ish folder.
    Init {
        #[arg(long, default_value = "Knowledge base")]
        name: String,
    },
    /// Write the optional CI job that runs `kmdn-cli check` on pull requests.
    Ci {
        /// github or gitlab
        #[arg(long, default_value = "github")]
        provider: String,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let root = cli
        .path
        .canonicalize()
        .with_context(|| format!("path not found: {}", cli.path.display()))?;
    match cli.cmd {
        Cmd::Check { asset_cap } => {
            let findings = checks::run(&root, &checks::CheckOptions { asset_cap });
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&findings)?);
            } else if findings.is_empty() {
                println!("ok: no findings");
            } else {
                for f in &findings {
                    println!("{:?}\t{:?}\t{}\t{}", f.level, f.kind, f.path, f.message);
                }
                println!("{} finding(s)", findings.len());
            }
            Ok(if checks::has_errors(&findings) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        Cmd::Index => {
            let changed = index::write_agents_md(&root)?;
            if cli.json {
                println!("{}", serde_json::json!({ "changed": changed }));
            } else {
                println!(
                    "{}",
                    if changed {
                        "AGENTS.md updated"
                    } else {
                        "AGENTS.md already current"
                    }
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Ci { provider } => {
            let kind = if provider.eq_ignore_ascii_case("gitlab") {
                kmdn_core::repo::ProviderKind::GitLab {
                    host: "gitlab.com".into(),
                }
            } else {
                kmdn_core::repo::ProviderKind::GitHub
            };
            let written = kmdn_core::bootstrap::write_ci_check(&root, &kind)?;
            if cli.json {
                println!("{}", serde_json::json!({ "written": written }));
            } else {
                println!("wrote {}", written.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Init { name } => {
            if root.join(index::CONFIG_DIR).exists() {
                bail!("already a kmdn knowledge base: {}", root.display());
            }
            std::fs::create_dir_all(root.join(index::CONFIG_DIR))?;
            std::fs::write(
                root.join(index::CONFIG_DIR).join("config.yaml"),
                format!("version: 1\nname: {name}\ndescription: Describe what this knowledge base covers.\nbranch_prefix: kmdn/\nreview:\n  labels: [kmdn]\n  post_agent_log: true\nassets:\n  max_bytes: {}\nagents:\n  allowed_paths: [\"**/*.md\", \"**/assets/**\"]\n", checks::DEFAULT_ASSET_CAP),
            )?;
            if !root.join("README.md").exists() {
                std::fs::write(
                    root.join("README.md"),
                    format!(
                        "# {name}\n\nStart here. This knowledge base is maintained with kmdn.\n"
                    ),
                )?;
            }
            if !root.join("getting-started.md").exists() {
                std::fs::write(
                    root.join("getting-started.md"),
                    "---\ntitle: Getting started\ndescription: How this knowledge base is organized and how to contribute.\nstatus: published\norder: 1\n---\n# Getting started\n\nDocuments are markdown files. Folders are sections. Every change is a pull request.\n",
                )?;
            }
            index::write_agents_md(&root)?;
            if cli.json {
                println!("{}", serde_json::json!({ "initialized": root }));
            } else {
                println!("initialized {}", root.display());
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
