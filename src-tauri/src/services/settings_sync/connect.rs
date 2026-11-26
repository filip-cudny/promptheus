use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::git::{GitError, GitRunner};
use super::{add_args, RunningGuard, SettingsSync, SyncHooks};
use crate::services::change_log::CHANGE_LOG_DIR_NAME;

pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(300);
const DEFAULT_FOLDER: &str = "promptheus";
const MAX_FOLDER_DEPTH: usize = 3;
const ALREADY_SYNCING: &str = "already syncing";
const SETTINGS: &str = "settings.json";
const PROMPTS: &str = "prompts";
const SKILLS: &str = "skills";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PrepareSource {
    Url(String),
    Dir(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ConnectEntries {
    pub settings: bool,
    pub prompts: bool,
    pub skills: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Prepared {
    pub repo_path: String,
    pub branch: String,
    pub empty: bool,
    pub suggested_folder: String,
    pub found: ConnectEntries,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ConnectRequest {
    pub repo_path: PathBuf,
    pub folder: String,
    pub entries: ConnectEntries,
    pub empty: bool,
}

pub fn redact_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    match rest[..authority_end].rfind('@') {
        Some(at) => format!("{scheme}://{}", &rest[at + 1..]),
        None => url.to_string(),
    }
}

fn git_message(error: GitError) -> String {
    match error {
        GitError::Failed { stderr, .. } => stderr,
        other => other.to_string(),
    }
}

fn git_message_for(error: GitError, url: &str) -> String {
    git_message(error).replace(url, &redact_url(url))
}

pub async fn prepare(
    source: &PrepareSource,
    clone_dir: &Path,
    runner: &GitRunner,
) -> Result<Prepared, String> {
    let (repo_path, empty) = match source {
        PrepareSource::Url(url) => prepare_url(url.trim(), clone_dir, runner).await?,
        PrepareSource::Dir(path) => prepare_dir(path, runner).await?,
    };
    let branch = runner
        .run_ok(&repo_path, &["symbolic-ref", "--short", "HEAD"])
        .await
        .map_err(git_message)?
        .stdout;
    let suggested_folder = suggest_folder(&repo_path);
    let folder = if suggested_folder == "." {
        repo_path.clone()
    } else {
        repo_path.join(&suggested_folder)
    };
    log::info!(
        "settings sync: prepared {} (branch {branch}, empty: {empty}, folder {suggested_folder})",
        repo_path.display()
    );
    Ok(Prepared {
        repo_path: repo_path.to_string_lossy().into_owned(),
        branch,
        empty,
        found: ConnectEntries {
            settings: folder.join(SETTINGS).is_file(),
            prompts: folder.join(PROMPTS).is_dir(),
            skills: folder.join(SKILLS).is_dir(),
        },
        suggested_folder,
    })
}

async fn prepare_url(
    url: &str,
    clone_dir: &Path,
    runner: &GitRunner,
) -> Result<(PathBuf, bool), String> {
    if url.is_empty() {
        return Err("the repository URL is empty".to_string());
    }
    let parent = clone_dir.parent().unwrap_or(clone_dir);
    fs::create_dir_all(parent)
        .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;

    log::info!("settings sync: checking access to {}", redact_url(url));
    let listing = runner
        .run_network(parent, &["ls-remote", "--", url])
        .await
        .map_err(|e| git_message_for(e, url))?;
    let empty = listing.stdout.is_empty();

    if clone_dir.join(".git").exists() {
        let existing = runner
            .run(clone_dir, &["config", "--local", "--get", "remote.origin.url"])
            .await
            .map_err(git_message)?;
        if !existing.success || existing.stdout != url {
            let remote = if existing.success {
                redact_url(&existing.stdout)
            } else {
                "no remote".to_string()
            };
            return Err(format!(
                "{} already holds a clone of another repository ({remote})",
                clone_dir.display()
            ));
        }
        log::info!("settings sync: reusing the clone in {}", clone_dir.display());
        runner
            .run_network(clone_dir, &["fetch"])
            .await
            .map_err(|e| git_message_for(e, url))?;
        if !empty {
            attach_unborn_branch(clone_dir, runner).await?;
        }
    } else {
        log::info!("settings sync: cloning {} into {}", redact_url(url), clone_dir.display());
        let target = clone_dir.as_os_str().to_owned();
        runner
            .run_network(parent, &[OsString::from("clone"), "--".into(), url.into(), target])
            .await
            .map_err(|e| git_message_for(e, url))?;
    }
    Ok((clone_dir.to_path_buf(), empty))
}

async fn attach_unborn_branch(clone_dir: &Path, runner: &GitRunner) -> Result<(), String> {
    let has_commits = runner
        .run(clone_dir, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .await
        .map_err(git_message)?
        .success;
    if has_commits {
        return Ok(());
    }
    let branch = runner
        .run_ok(clone_dir, &["symbolic-ref", "--short", "HEAD"])
        .await
        .map_err(git_message)?
        .stdout;
    log::info!("settings sync: the reused clone has no commits, tracking origin/{branch}");
    runner
        .run_ok(
            clone_dir,
            &["checkout", "-B", &branch, "--track", &format!("origin/{branch}")],
        )
        .await
        .map_err(git_message)?;
    Ok(())
}

async fn prepare_dir(path: &Path, runner: &GitRunner) -> Result<(PathBuf, bool), String> {
    let top = runner
        .run(path, &["rev-parse", "--show-toplevel"])
        .await
        .ok()
        .filter(|output| output.success)
        .ok_or_else(|| "not a git repository".to_string())?
        .stdout;
    let repo_path = PathBuf::from(top);
    let upstream = runner
        .run(&repo_path, &["rev-parse", "--verify", "--quiet", "@{u}"])
        .await
        .map_err(git_message)?;
    if !upstream.success {
        return Err("the branch has no upstream; push it with `git push -u` first".to_string());
    }
    log::info!("settings sync: checking access to the remote of {}", repo_path.display());
    let listing = runner
        .run_network(&repo_path, &["ls-remote"])
        .await
        .map_err(git_message)?;
    Ok((repo_path, listing.stdout.is_empty()))
}

fn suggest_folder(repo_path: &Path) -> String {
    let mut dirs = vec![PathBuf::new()];
    collect_dirs(repo_path, Path::new(""), 0, &mut dirs);
    dirs.sort();
    dirs.into_iter()
        .find(|relative| repo_path.join(relative).join(SETTINGS).is_file())
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                ".".to_string()
            } else {
                relative.to_string_lossy().into_owned()
            }
        })
        .unwrap_or_else(|| DEFAULT_FOLDER.to_string())
}

fn collect_dirs(root: &Path, relative: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == MAX_FOLDER_DEPTH {
        return;
    }
    let Ok(read) = fs::read_dir(root.join(relative)) else {
        return;
    };
    for entry in read.flatten() {
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir && entry.file_name() != ".git" {
            let child = relative.join(entry.file_name());
            out.push(child.clone());
            collect_dirs(root, &child, depth + 1, out);
        }
    }
}

fn validate_folder(folder: &str) -> Result<(), String> {
    let relative = Path::new(folder)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if relative {
        Ok(())
    } else {
        Err(format!("the folder must be a relative path without `..`: {folder}"))
    }
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

fn exists_or_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn remove_path(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn copy_path(from: &Path, to: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(from)?;
    if metadata.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_path(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else if metadata.file_type().is_symlink() {
        symlink(fs::read_link(from)?, to)?;
    } else {
        fs::copy(from, to)?;
    }
    Ok(())
}

// Simplification: a local entry that is a symlink to somewhere else moves as the link, not as its
// content: the repository then holds a symlink, not the files, so local edits never reach a
// commit and other machines get a dangling link. Stops being correct when a config entry
// (`settings.json`, `prompts`, `skills`) is a symlink, relative or absolute, to a path outside the
// repository before connecting. Upgrade path: resolve the link in `link_entry` and move the content.
fn move_path(from: &Path, to: &Path) -> Result<(), String> {
    let moved = (|| {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        match fs::rename(from, to) {
            Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
                copy_path(from, to)?;
                remove_path(from)
            }
            other => other,
        }
    })();
    moved.map_err(|e| format!("failed to move {} to {}: {e}", from.display(), to.display()))
}

fn link_entry(
    config_dir: &Path,
    folder_dir: &Path,
    entry: &str,
    backup_dir: &Path,
) -> Result<(), String> {
    let local = config_dir.join(entry);
    let target = folder_dir.join(entry);
    let linked_already = is_symlink(&local)
        && matches!(
            (fs::canonicalize(&local), fs::canonicalize(&target)),
            (Ok(a), Ok(b)) if a == b
        );
    if linked_already {
        log::info!("settings sync: {entry} is already linked into {}", folder_dir.display());
        return Ok(());
    }

    let local_exists = exists_or_link(&local);
    if exists_or_link(&target) {
        if local_exists {
            let backup = backup_dir.join(entry);
            log::info!("settings sync: {entry} exists in the repository, local copy moves to {}", backup.display());
            move_path(&local, &backup)?;
        }
    } else if local_exists {
        log::info!("settings sync: moving local {entry} into {}", folder_dir.display());
        move_path(&local, &target)?;
    } else if entry == SETTINGS {
        return Err(format!("{SETTINGS} exists neither locally nor in {}", folder_dir.display()));
    } else {
        fs::create_dir_all(&target)
            .map_err(|e| format!("failed to create {}: {e}", target.display()))?;
    }
    symlink(&target, &local)
        .map_err(|e| format!("failed to link {} to {}: {e}", local.display(), target.display()))?;
    log::info!("settings sync: linked {entry}");
    Ok(())
}

fn move_change_log(config_dir: &Path, folder_dir: &Path) -> Result<(), String> {
    let source = config_dir.join(CHANGE_LOG_DIR_NAME);
    let destination = folder_dir.join(CHANGE_LOG_DIR_NAME);
    let read = match fs::read_dir(&source) {
        Ok(read) => read,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("failed to read {}: {e}", source.display())),
    };
    let mut moved = 0;
    for entry in read {
        let entry =
            entry.map_err(|e| format!("failed to read an entry of {}: {e}", source.display()))?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if !from.is_file() || exists_or_link(&to) {
            continue;
        }
        move_path(&from, &to)?;
        moved += 1;
    }
    log::info!("settings sync: moved {moved} change-log entries into {}", destination.display());
    let ended_empty = fs::read_dir(&source)
        .map(|mut read| read.next().is_none())
        .unwrap_or(false);
    if ended_empty {
        fs::remove_dir(&source)
            .map_err(|e| format!("failed to remove {}: {e}", source.display()))?;
    }
    Ok(())
}

fn place_entries(
    config_dir: &Path,
    folder_dir: &Path,
    entries: ConnectEntries,
) -> Result<(), String> {
    fs::create_dir_all(folder_dir)
        .map_err(|e| format!("failed to create {}: {e}", folder_dir.display()))?;
    let backup_dir = config_dir.join(format!(
        "sync-backup-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    let checked = [
        (SETTINGS, entries.settings),
        (PROMPTS, entries.prompts),
        (SKILLS, entries.skills),
    ];
    for (entry, _) in checked.iter().filter(|(_, on)| *on) {
        link_entry(config_dir, folder_dir, entry, &backup_dir)?;
    }
    if entries.settings {
        move_change_log(config_dir, folder_dir)?;
    }
    Ok(())
}

impl SettingsSync {
    pub async fn prepare_source(
        &self,
        source: &PrepareSource,
        clone_dir: &Path,
    ) -> Result<Prepared, String> {
        let runner = self.runner(NETWORK_TIMEOUT).await;
        prepare(source, clone_dir, &runner).await
    }

    pub async fn connect<H: SyncHooks>(
        &self,
        request: &ConnectRequest,
        hooks: &H,
    ) -> Result<(), String> {
        validate_folder(&request.folder)?;
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.running {
                return Err(ALREADY_SYNCING.to_string());
            }
            inner.running = true;
        }
        let _running = RunningGuard(self.inner.clone());

        let mut guard = hooks.lock_local().await;
        let folder_dir = request.repo_path.join(&request.folder);
        log::info!(
            "settings sync: connecting {} (folder {}, entries {:?}, empty remote: {})",
            request.repo_path.display(),
            request.folder,
            request.entries,
            request.empty
        );

        let config_dir = self.config_dir.clone();
        let place_in = folder_dir.clone();
        let entries = request.entries;
        tokio::task::spawn_blocking(move || place_entries(&config_dir, &place_in, entries))
            .await
            .map_err(|e| format!("linking the entries failed: {e}"))??;

        let staged = self.stage_entries(request).await?;
        if request.empty {
            self.push_initial(request, &staged).await?;
        }

        let mut changed = Vec::new();
        if request.entries.settings {
            changed.push(folder_dir.join(SETTINGS));
        }
        if request.entries.skills {
            changed.push(folder_dir.join(SKILLS));
        }
        hooks
            .reload(&mut guard, &changed)
            .await
            .map_err(|message| format!("reload after connect failed: {message}"))?;
        log::info!("settings sync: connected {}", request.repo_path.display());
        Ok(())
    }

    async fn stage_entries(&self, request: &ConnectRequest) -> Result<Vec<PathBuf>, String> {
        let top = request.repo_path.as_path();
        let runner = self.runner(self.timeout).await;
        let relative = Path::new(&request.folder);
        let mut staged: Vec<PathBuf> = [
            (SETTINGS, request.entries.settings),
            (PROMPTS, request.entries.prompts),
            (SKILLS, request.entries.skills),
        ]
        .iter()
        .filter(|(_, on)| *on)
        .map(|(entry, _)| relative.join(entry))
        .collect();
        if request.entries.settings && top.join(relative).join(CHANGE_LOG_DIR_NAME).is_dir() {
            staged.push(relative.join(CHANGE_LOG_DIR_NAME));
        }
        runner.run_ok(top, &add_args(&staged)).await.map_err(git_message)?;
        log::info!("settings sync: staged {} paths in {}", staged.len(), top.display());
        Ok(staged)
    }

    async fn push_initial(&self, request: &ConnectRequest, paths: &[PathBuf]) -> Result<(), String> {
        let top = request.repo_path.as_path();
        let runner = self.runner(self.timeout).await;

        let mut staged = vec![OsString::from("diff"), "--cached".into(), "--quiet".into(), "--".into()];
        staged.extend(paths.iter().map(OsString::from));
        let diff = runner.run(top, &staged).await.map_err(git_message)?;
        match diff.code {
            Some(1) => {}
            Some(0) => return Err("nothing to commit: the checked entries hold no files".to_string()),
            _ => return Err(format!("git diff failed: {}", diff.stderr)),
        }

        let message = format!("chore(sync): initial from {}", self.host_name(&runner).await);
        let mut commit = vec![OsString::from("commit"), "-m".into(), message.into(), "--".into()];
        commit.extend(paths.iter().map(OsString::from));
        runner.run_ok(top, &commit).await.map_err(git_message)?;

        let branch = runner
            .run_ok(top, &["symbolic-ref", "--short", "HEAD"])
            .await
            .map_err(git_message)?
            .stdout;
        log::info!("settings sync: pushing the initial commit to origin/{branch}");
        self.runner(NETWORK_TIMEOUT)
            .await
            .run_network(top, &["push", "-u", "origin", &branch])
            .await
            .map_err(git_message)?;
        Ok(())
    }
}
