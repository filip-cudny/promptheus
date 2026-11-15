use std::path::{Path, PathBuf};

use super::git::GitRunner;
use crate::services::change_log::CHANGE_LOG_DIR_NAME;

const ENTRIES: [&str; 3] = ["settings.json", "prompts", "skills"];

#[derive(Debug, Clone, PartialEq)]
pub struct SyncRepo {
    pub top_level: PathBuf,
    pub stage_paths: Vec<PathBuf>,
    pub branch: String,
    pub settings_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Detection {
    Off,
    Repo(SyncRepo),
    Error(String),
}

pub async fn detect(runner: &GitRunner, config_dir: &Path) -> Detection {
    let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut settings_target: Option<PathBuf> = None;
    for entry in ENTRIES {
        match locate(runner, &config_dir.join(entry)).await {
            Ok(Some(located)) => {
                if entry == "settings.json" {
                    settings_target = Some(located.1.clone());
                }
                found.push(located);
            }
            Ok(None) => {}
            Err(message) => return Detection::Error(message),
        }
    }

    let Some((top_level, _)) = found.first().cloned() else {
        return Detection::Off;
    };
    if found.iter().any(|(top, _)| *top != top_level) {
        return Detection::Error(
            "settings.json, prompts and skills point into different git repositories".to_string(),
        );
    }

    let change_log_dir = settings_target
        .as_ref()
        .and_then(|target| target.parent().map(|parent| parent.join(CHANGE_LOG_DIR_NAME)))
        .filter(|dir| dir.is_dir());
    let stage_paths = found
        .iter()
        .map(|(_, target)| target)
        .chain(change_log_dir.as_ref())
        .map(|target| {
            let relative = target.strip_prefix(&top_level).unwrap_or(target);
            if relative.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                relative.to_path_buf()
            }
        })
        .collect();

    match runner
        .run_ok(&top_level, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await
    {
        Ok(output) => Detection::Repo(SyncRepo {
            top_level,
            stage_paths,
            branch: output.stdout,
            settings_path: settings_target,
        }),
        Err(e) => Detection::Error(e.to_string()),
    }
}

async fn locate(
    runner: &GitRunner,
    entry: &Path,
) -> Result<Option<(PathBuf, PathBuf)>, String> {
    let is_symlink = std::fs::symlink_metadata(entry)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if !is_symlink {
        return Ok(None);
    }
    let Ok(target) = std::fs::canonicalize(entry) else {
        return Ok(None);
    };
    let git_dir = if target.is_dir() {
        target.clone()
    } else {
        match target.parent() {
            Some(parent) => parent.to_path_buf(),
            None => return Ok(None),
        }
    };
    let output = match runner
        .run(&git_dir, &["rev-parse", "--show-toplevel"])
        .await
    {
        Ok(output) if output.success => output,
        Ok(_) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let Ok(top_level) = std::fs::canonicalize(&output.stdout) else {
        return Ok(None);
    };
    Ok(Some((top_level, target)))
}
