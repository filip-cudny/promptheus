use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tempfile::TempDir;

use super::connect::{prepare, redact_url, ConnectEntries};
use super::*;

const TIMEOUT: Duration = Duration::from_secs(20);

fn settings_json(theme: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "theme": theme,
        "models": [{
            "id": "m",
            "type": "text",
            "model": "gpt-4",
            "display_name": "M",
            "provider": "openai",
            "api_key": "k"
        }]
    }))
    .unwrap()
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn all_entries() -> ConnectEntries {
    ConnectEntries {
        settings: true,
        prompts: true,
        skills: true,
    }
}

#[derive(Default)]
struct RecordingHooks {
    reloads: Mutex<Vec<Vec<PathBuf>>>,
}

#[async_trait]
impl SyncHooks for RecordingHooks {
    type Guard = ();

    async fn lock_local(&self) -> Self::Guard {}

    async fn reload(&self, _guard: &mut Self::Guard, changed: &[PathBuf]) -> Result<(), String> {
        self.reloads.lock().unwrap().push(changed.to_vec());
        Ok(())
    }

    fn status_changed(&self, _status: &SyncStatus) {}
}

struct Fixture {
    tmp: TempDir,
    env: HashMap<String, String>,
}

impl Fixture {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let gitconfig = tmp.path().join("gitconfig");
        std::fs::write(
            &gitconfig,
            "[user]\n\tname = Test\n\temail = test@example.com\n[init]\n\tdefaultBranch = main\n",
        )
        .unwrap();
        let env = HashMap::from([
            (
                "GIT_CONFIG_GLOBAL".to_string(),
                gitconfig.to_string_lossy().into_owned(),
            ),
            ("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string()),
        ]);
        Self { tmp, env }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .envs(&self.env)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn bare(&self, name: &str) -> PathBuf {
        let remote = self.path(name);
        self.git(self.tmp.path(), &["init", "--bare", name]);
        remote
    }

    fn remote_with(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let remote = self.bare(name);
        self.push_files(&remote, files);
        remote
    }

    fn push_files(&self, remote: &Path, files: &[(&str, &str)]) {
        let seed = self.path(&format!("seed-{}", self.tmp.path().read_dir().unwrap().count()));
        self.git(
            self.tmp.path(),
            &["clone", remote.to_str().unwrap(), seed.to_str().unwrap()],
        );
        for (name, content) in files {
            write(&seed.join(name), content);
        }
        self.git(&seed, &["add", "-A"]);
        self.git(&seed, &["commit", "-m", "seed"]);
        self.git(&seed, &["push", "-u", "origin", "main"]);
    }

    fn config_dir(&self) -> PathBuf {
        let config = self.path("config");
        write(&config.join("settings.json"), &settings_json("first"));
        write(&config.join("prompts/p.md"), "prompt\n");
        write(&config.join("skills/s/SKILL.md"), "skill\n");
        write(&config.join("change-log/id1.json"), "{\"id\":\"id1\"}");
        config
    }

    fn engine(&self, config_dir: &Path) -> SettingsSync {
        let env = self.env.clone();
        SettingsSync::new(
            config_dir.to_path_buf(),
            Arc::new(move || env.clone()),
            Some("testhost".to_string()),
            Some(TIMEOUT),
        )
    }

    fn runner(&self) -> GitRunner {
        GitRunner::new(self.env.clone(), TIMEOUT)
    }

    fn request(&self, prepared: &Prepared, entries: ConnectEntries) -> ConnectRequest {
        ConnectRequest {
            repo_path: PathBuf::from(&prepared.repo_path),
            folder: "promptheus".to_string(),
            entries,
            empty: prepared.empty,
        }
    }
}

fn url_of(remote: &Path) -> PrepareSource {
    PrepareSource::Url(remote.to_str().unwrap().to_string())
}

#[tokio::test]
async fn url_source_clones_and_links_entries() {
    let f = Fixture::new();
    let remote = f.remote_with("origin.git", &[("README.md", "readme\n")]);
    let config = f.config_dir();
    let clone = f.path("clone");
    let engine = f.engine(&config);

    let prepared = prepare(&url_of(&remote), &clone, &f.runner()).await.unwrap();
    assert!(clone.join(".git").exists());
    assert!(!prepared.empty);
    assert_eq!(prepared.suggested_folder, "promptheus");
    assert_eq!(prepared.found, ConnectEntries::default());

    let hooks = RecordingHooks::default();
    engine
        .connect(&f.request(&prepared, all_entries()), &hooks)
        .await
        .unwrap();

    let folder = clone.join("promptheus");
    assert_eq!(std::fs::read_link(config.join("settings.json")).unwrap(), folder.join("settings.json"));
    assert_eq!(std::fs::read_link(config.join("prompts")).unwrap(), folder.join("prompts"));
    assert_eq!(std::fs::read_link(config.join("skills")).unwrap(), folder.join("skills"));
    assert_eq!(read(&config.join("settings.json")), settings_json("first"));
    assert_eq!(read(&config.join("prompts/p.md")), "prompt\n");
    assert_eq!(read(&config.join("skills/s/SKILL.md")), "skill\n");
    assert_eq!(
        *hooks.reloads.lock().unwrap(),
        vec![vec![folder.join("settings.json"), folder.join("skills")]]
    );
}

#[tokio::test]
async fn empty_remote_gets_initial_commit_and_upstream() {
    let f = Fixture::new();
    let remote = f.bare("origin.git");
    let config = f.config_dir();
    let clone = f.path("clone");
    let engine = f.engine(&config);

    let prepared = prepare(&url_of(&remote), &clone, &f.runner()).await.unwrap();
    assert!(prepared.empty);

    engine
        .connect(&f.request(&prepared, all_entries()), &RecordingHooks::default())
        .await
        .unwrap();

    assert_eq!(f.git(&remote, &["rev-list", "--count", "main"]), "1");
    assert_eq!(
        f.git(&remote, &["log", "-1", "--format=%s", "main"]),
        "chore(sync): initial from testhost"
    );
    f.git(&clone, &["rev-parse", "@{u}"]);
}

#[tokio::test]
async fn joining_prefers_the_repository_version_and_backs_up_the_local_one() {
    let f = Fixture::new();
    let remote = f.remote_with(
        "origin.git",
        &[("promptheus/settings.json", &settings_json("second"))],
    );
    let config = f.config_dir();
    let clone = f.path("clone");
    let engine = f.engine(&config);

    let prepared = prepare(&url_of(&remote), &clone, &f.runner()).await.unwrap();
    assert_eq!(prepared.suggested_folder, "promptheus");
    assert_eq!(
        prepared.found,
        ConnectEntries {
            settings: true,
            prompts: false,
            skills: false
        }
    );

    engine
        .connect(&f.request(&prepared, all_entries()), &RecordingHooks::default())
        .await
        .unwrap();

    assert_eq!(read(&config.join("settings.json")), settings_json("second"));
    let backups: Vec<PathBuf> = std::fs::read_dir(&config)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("sync-backup-"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(read(&backups[0].join("settings.json")), settings_json("first"));
}

#[tokio::test]
async fn unchecked_entry_stays_a_regular_directory() {
    let f = Fixture::new();
    let remote = f.remote_with("origin.git", &[("README.md", "readme\n")]);
    let config = f.config_dir();
    let clone = f.path("clone");
    let engine = f.engine(&config);

    let prepared = prepare(&url_of(&remote), &clone, &f.runner()).await.unwrap();
    let entries = ConnectEntries {
        skills: false,
        ..all_entries()
    };
    let hooks = RecordingHooks::default();
    engine.connect(&f.request(&prepared, entries), &hooks).await.unwrap();

    let metadata = std::fs::symlink_metadata(config.join("skills")).unwrap();
    assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
    assert_eq!(read(&config.join("skills/s/SKILL.md")), "skill\n");
    assert!(std::fs::symlink_metadata(config.join("prompts")).unwrap().file_type().is_symlink());
    assert_eq!(
        *hooks.reloads.lock().unwrap(),
        vec![vec![clone.join("promptheus/settings.json")]]
    );
}

#[tokio::test]
async fn dir_source_without_upstream_is_refused() {
    let f = Fixture::new();
    let repo = f.path("local");
    f.git(f.tmp.path(), &["init", "local"]);
    write(&repo.join("README.md"), "readme\n");
    f.git(&repo, &["add", "-A"]);
    f.git(&repo, &["commit", "-m", "init"]);

    let error = prepare(&PrepareSource::Dir(repo), &f.path("unused"), &f.runner())
        .await
        .unwrap_err();

    assert!(error.contains("no upstream"), "{error}");
}

#[tokio::test]
async fn dir_source_with_upstream_is_prepared() {
    let f = Fixture::new();
    let remote = f.remote_with(
        "origin.git",
        &[("nested/settings.json", &settings_json("second"))],
    );
    let repo = f.path("local");
    f.git(f.tmp.path(), &["clone", remote.to_str().unwrap(), "local"]);

    let prepared = prepare(&PrepareSource::Dir(repo.clone()), &f.path("unused"), &f.runner())
        .await
        .unwrap();

    assert_eq!(
        std::fs::canonicalize(&prepared.repo_path).unwrap(),
        std::fs::canonicalize(&repo).unwrap()
    );
    assert_eq!(prepared.branch, "main");
    assert!(!prepared.empty);
    assert_eq!(prepared.suggested_folder, "nested");
    assert!(prepared.found.settings);
}

#[tokio::test]
async fn change_log_entries_move_without_overwriting() {
    let f = Fixture::new();
    let remote = f.remote_with(
        "origin.git",
        &[("promptheus/change-log/id2.json", "{\"id\":\"remote\"}")],
    );
    let config = f.config_dir();
    write(&config.join("change-log/id2.json"), "{\"id\":\"local\"}");
    let clone = f.path("clone");
    let engine = f.engine(&config);

    let prepared = prepare(&url_of(&remote), &clone, &f.runner()).await.unwrap();
    engine
        .connect(&f.request(&prepared, all_entries()), &RecordingHooks::default())
        .await
        .unwrap();

    let log = clone.join("promptheus/change-log");
    assert_eq!(read(&log.join("id1.json")), "{\"id\":\"id1\"}");
    assert_eq!(read(&log.join("id2.json")), "{\"id\":\"remote\"}");
    assert!(!config.join("change-log/id1.json").exists());
    assert_eq!(read(&config.join("change-log/id2.json")), "{\"id\":\"local\"}");
}

#[tokio::test]
async fn clone_dir_of_another_remote_is_refused() {
    let f = Fixture::new();
    let first = f.remote_with("first.git", &[("README.md", "one\n")]);
    let second = f.remote_with("second.git", &[("README.md", "two\n")]);
    let clone = f.path("clone");

    prepare(&url_of(&first), &clone, &f.runner()).await.unwrap();
    let error = prepare(&url_of(&second), &clone, &f.runner()).await.unwrap_err();

    assert!(error.contains(clone.to_str().unwrap()), "{error}");
}

#[tokio::test]
async fn clone_dir_of_the_same_remote_is_reused() {
    let f = Fixture::new();
    let remote = f.remote_with("origin.git", &[("README.md", "one\n")]);
    let clone = f.path("clone");

    prepare(&url_of(&remote), &clone, &f.runner()).await.unwrap();
    f.push_files(&remote, &[("promptheus/settings.json", &settings_json("second"))]);
    let prepared = prepare(&url_of(&remote), &clone, &f.runner()).await.unwrap();

    assert_eq!(prepared.repo_path, clone.to_string_lossy());
    f.git(&clone, &["rev-parse", "origin/main:promptheus/settings.json"]);
}

#[tokio::test]
async fn reused_clone_without_commits_tracks_the_remote_branch() {
    let f = Fixture::new();
    let remote = f.bare("origin.git");
    let clone = f.path("clone");

    assert!(prepare(&url_of(&remote), &clone, &f.runner()).await.unwrap().empty);
    f.push_files(&remote, &[("README.md", "one\n")]);
    let prepared = prepare(&url_of(&remote), &clone, &f.runner()).await.unwrap();

    assert!(!prepared.empty);
    f.git(&clone, &["rev-parse", "@{u}"]);
}

#[tokio::test]
async fn connect_refuses_while_a_cycle_runs() {
    let f = Fixture::new();
    let config = f.config_dir();
    let engine = f.engine(&config);
    engine.inner.lock().unwrap().running = true;
    let request = ConnectRequest {
        repo_path: f.path("clone"),
        folder: "promptheus".to_string(),
        entries: all_entries(),
        empty: false,
    };

    let error = engine.connect(&request, &RecordingHooks::default()).await.unwrap_err();

    assert_eq!(error, "already syncing");
}

#[tokio::test]
async fn connect_refuses_a_folder_outside_the_repository() {
    let f = Fixture::new();
    let engine = f.engine(&f.config_dir());
    let request = ConnectRequest {
        repo_path: f.path("clone"),
        folder: "../outside".to_string(),
        entries: all_entries(),
        empty: false,
    };

    assert!(engine.connect(&request, &RecordingHooks::default()).await.is_err());
}

#[test]
fn credentials_are_stripped_from_urls() {
    assert_eq!(
        redact_url("https://user:secret@example.com/team/repo.git"),
        "https://example.com/team/repo.git"
    );
    assert_eq!(
        redact_url("git@example.com:team/repo.git"),
        "git@example.com:team/repo.git"
    );
}
