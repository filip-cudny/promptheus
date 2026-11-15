use std::collections::HashMap;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tempfile::TempDir;
use tokio::sync::Notify;

use super::*;
use crate::services::change_log::change_log_dir;
use crate::services::config::ConfigService;
use crate::services::database::Database;
use crate::services::skill::SkillService;

const TIMEOUT: Duration = Duration::from_secs(20);

fn multiline_settings(theme: &str) -> String {
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
        let fixture = Self { tmp, env };
        fixture.build_repos();
        fixture
    }

    fn build_repos(&self) {
        let root = self.tmp.path();
        self.git(root, &["init", "--bare", "origin.git"]);
        self.git(root, &["clone", "origin.git", "a"]);
        let a = self.a();
        write(&a.join("tauri-app/settings.json"), &multiline_settings("first"));
        write(&a.join("tauri-app/prompts/p.md"), "prompt\n");
        write(&a.join("tauri-app/skills/s/SKILL.md"), "skill\n");
        write(&a.join("tauri-app/other.txt"), "other\n");
        self.git(&a, &["add", "-A"]);
        self.git(&a, &["commit", "-m", "init"]);
        self.git(&a, &["push", "-u", "origin", "main"]);
        self.git(root, &["clone", "origin.git", "b"]);
    }

    fn origin(&self) -> PathBuf {
        self.tmp.path().join("origin.git")
    }

    fn a(&self) -> PathBuf {
        self.tmp.path().join("a")
    }

    fn b(&self) -> PathBuf {
        self.tmp.path().join("b")
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

    fn config_dir_for(&self, clone: &Path, name: &str) -> PathBuf {
        let config_dir = self.tmp.path().join(name);
        std::fs::create_dir_all(&config_dir).unwrap();
        for entry in ["settings.json", "prompts", "skills"] {
            symlink(clone.join("tauri-app").join(entry), config_dir.join(entry)).unwrap();
        }
        config_dir
    }

    fn engine(&self, config_dir: PathBuf, env: HashMap<String, String>, timeout: Duration) -> SettingsSync {
        SettingsSync::new(
            config_dir,
            Arc::new(move || env.clone()),
            Some("testhost".to_string()),
            Some(timeout),
        )
    }

    fn engine_a(&self) -> SettingsSync {
        self.engine(self.config_dir_for(&self.a(), "config-a"), self.env.clone(), TIMEOUT)
    }

    fn runner(&self) -> GitRunner {
        GitRunner::new(self.env.clone(), TIMEOUT)
    }
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[derive(Default)]
struct RecordingHooks {
    reloads: Mutex<Vec<Vec<PathBuf>>>,
    statuses: Mutex<Vec<SyncState>>,
    locks: Mutex<usize>,
}

#[async_trait]
impl SyncHooks for RecordingHooks {
    type Guard = ();

    async fn lock_local(&self) -> Self::Guard {
        *self.locks.lock().unwrap() += 1;
    }

    async fn reload(&self, _guard: &mut Self::Guard, changed: &[PathBuf]) -> Result<(), String> {
        self.reloads.lock().unwrap().push(changed.to_vec());
        Ok(())
    }

    fn status_changed(&self, status: &SyncStatus) {
        self.statuses.lock().unwrap().push(status.state);
    }
}

fn completed(result: SyncResult) -> CycleReport {
    match result {
        SyncResult::Completed(report) => report,
        other => panic!("expected a completed cycle, got {:?}", other),
    }
}

#[tokio::test]
async fn local_change_is_committed_and_pushed() {
    let f = Fixture::new();
    let a = f.a();
    write(&a.join("tauri-app/settings.json"), &multiline_settings("changed"));
    write(&a.join("tauri-app/prompts/scratch.tmp"), "tmp\n");
    write(&a.join("tauri-app/extra.txt"), "staged elsewhere\n");
    f.git(&a, &["add", "tauri-app/extra.txt"]);

    let hooks = RecordingHooks::default();
    let report = completed(f.engine_a().sync(Trigger::Manual, &hooks).await);

    assert_eq!(report.status.state, SyncState::Synced);
    assert!(report.status.last_sync.is_some());
    assert_eq!(
        f.git(&f.origin(), &["log", "-1", "--format=%s", "main"]),
        "chore(sync): update from testhost"
    );
    let committed = f.git(&f.origin(), &["show", "--name-only", "--format=", "main"]);
    assert_eq!(committed, "tauri-app/settings.json");
    assert!(f
        .git(&a, &["status", "--porcelain"])
        .contains("A  tauri-app/extra.txt"));
}

#[tokio::test]
async fn invalid_settings_json_is_not_committed() {
    let f = Fixture::new();
    let a = f.a();
    let origin_log = f.git(&f.origin(), &["log", "--format=%H", "main"]);
    write(
        &a.join("tauri-app/settings.json"),
        &format!("<<<<<<< HEAD\n{}", multiline_settings("broken")),
    );

    let hooks = RecordingHooks::default();
    let report = completed(f.engine_a().sync(Trigger::Manual, &hooks).await);

    assert_eq!(report.status.state, SyncState::Error);
    assert!(report
        .status
        .message
        .unwrap()
        .contains("settings.json is invalid"));
    assert_eq!(f.git(&f.origin(), &["log", "--format=%H", "main"]), origin_log);
    assert_eq!(
        f.git(&a, &["status", "--porcelain", "--untracked-files=all"]),
        "M tauri-app/settings.json"
    );
}

#[tokio::test]
async fn modified_file_outside_targets_survives_sync() {
    let f = Fixture::new();
    write(&f.b().join("tauri-app/prompts/p.md"), "from b\n");
    f.git(&f.b(), &["commit", "-am", "b change"]);
    f.git(&f.b(), &["push"]);
    write(&f.a().join("tauri-app/other.txt"), "dirty\n");

    let hooks = RecordingHooks::default();
    let report = completed(f.engine_a().sync(Trigger::Manual, &hooks).await);

    assert_eq!(report.status.state, SyncState::Synced);
    assert_eq!(
        f.git(&f.a(), &["diff", "--name-only"]),
        "tauri-app/other.txt"
    );
}

#[tokio::test]
async fn remote_change_is_pulled_and_reported() {
    let f = Fixture::new();
    write(&f.b().join("tauri-app/prompts/p.md"), "from b\n");
    f.git(&f.b(), &["commit", "-am", "b change"]);
    f.git(&f.b(), &["push"]);

    let hooks = RecordingHooks::default();
    let report = completed(f.engine_a().sync(Trigger::Manual, &hooks).await);

    let expected = vec![f.a().canonicalize().unwrap().join("tauri-app/prompts/p.md")];
    assert_eq!(report.changed, expected);
    assert_eq!(*hooks.reloads.lock().unwrap(), vec![expected]);
    assert_eq!(
        std::fs::read_to_string(f.a().join("tauri-app/prompts/p.md")).unwrap(),
        "from b\n"
    );
}

#[tokio::test]
async fn conflict_aborts_rebase_and_pauses_periodic() {
    let f = Fixture::new();
    write(&f.b().join("tauri-app/settings.json"), &multiline_settings("from b"));
    f.git(&f.b(), &["commit", "-am", "b change"]);
    f.git(&f.b(), &["push"]);
    write(&f.a().join("tauri-app/settings.json"), &multiline_settings("from a"));

    let engine = f.engine_a();
    let hooks = RecordingHooks::default();
    let report = completed(engine.sync(Trigger::Startup, &hooks).await);

    assert_eq!(report.status.state, SyncState::Conflict);
    assert!(report.entered_new_state);
    for name in ["rebase-merge", "rebase-apply"] {
        let path = f.git(&f.a(), &["rev-parse", "--git-path", name]);
        assert!(!f.a().join(path).exists(), "{} still present", name);
    }
    assert_eq!(
        f.git(&f.a(), &["log", "-1", "--format=%s"]),
        "chore(sync): update from testhost"
    );

    let head = f.git(&f.a(), &["rev-parse", "HEAD"]);
    let reflog = f.git(&f.a(), &["reflog"]);
    assert_eq!(engine.sync(Trigger::Periodic, &hooks).await, SyncResult::Paused);
    assert_eq!(f.git(&f.a(), &["rev-parse", "HEAD"]), head);
    assert_eq!(f.git(&f.a(), &["reflog"]), reflog);
    assert_eq!(engine.status().state, SyncState::Conflict);

    let manual = completed(engine.sync(Trigger::Manual, &hooks).await);
    assert_eq!(manual.status.state, SyncState::Conflict);
    assert!(!manual.entered_new_state);
}

#[tokio::test]
async fn missing_origin_is_an_error() {
    let f = Fixture::new();
    f.git(&f.a(), &["remote", "remove", "origin"]);
    let hooks = RecordingHooks::default();

    let started = Instant::now();
    let report = completed(f.engine_a().sync(Trigger::Manual, &hooks).await);

    assert_eq!(report.status.state, SyncState::Error);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn origin_pointing_to_missing_path_is_an_error() {
    let f = Fixture::new();
    let missing = f.tmp.path().join("does-not-exist.git");
    f.git(&f.a(), &["remote", "set-url", "origin", missing.to_str().unwrap()]);
    let hooks = RecordingHooks::default();

    let started = Instant::now();
    let report = completed(f.engine_a().sync(Trigger::Manual, &hooks).await);

    assert_eq!(report.status.state, SyncState::Error);
    assert!(started.elapsed() < Duration::from_secs(10));
}

struct BlockingHooks {
    entered: Notify,
    release: Notify,
}

#[async_trait]
impl SyncHooks for BlockingHooks {
    type Guard = ();

    async fn lock_local(&self) -> Self::Guard {
        self.entered.notify_one();
        self.release.notified().await;
    }

    async fn reload(&self, _guard: &mut Self::Guard, _changed: &[PathBuf]) -> Result<(), String> {
        Ok(())
    }

    fn status_changed(&self, _status: &SyncStatus) {}
}

#[tokio::test]
async fn second_sync_while_running_returns_already_syncing() {
    let f = Fixture::new();
    let engine = Arc::new(f.engine_a());
    let hooks = Arc::new(BlockingHooks {
        entered: Notify::new(),
        release: Notify::new(),
    });

    let running = {
        let (engine, hooks) = (engine.clone(), hooks.clone());
        tokio::spawn(async move { engine.sync(Trigger::Manual, &*hooks).await })
    };
    hooks.entered.notified().await;

    assert_eq!(engine.status().state, SyncState::Syncing);
    assert_eq!(engine.sync(Trigger::Manual, &*hooks).await, SyncResult::AlreadySyncing);

    hooks.release.notify_one();
    let report = completed(running.await.unwrap());
    assert_eq!(report.status.state, SyncState::Synced);
}

#[tokio::test]
async fn ssh_remote_waiting_on_stdin_times_out() {
    for url in ["ssh://git@example.invalid/repo.git", "git@example.invalid:repo.git"] {
        let f = Fixture::new();
        let script = f.tmp.path().join("fake-ssh.sh");
        std::fs::write(&script, "#!/bin/sh\nread x\nsleep 30\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        f.git(&f.a(), &["remote", "set-url", "origin", url]);

        let mut env = f.env.clone();
        env.insert("GIT_SSH_COMMAND".to_string(), script.to_string_lossy().into_owned());
        let engine = f.engine(
            f.config_dir_for(&f.a(), "config-a"),
            env,
            Duration::from_secs(5),
        );
        let hooks = RecordingHooks::default();

        let started = Instant::now();
        let report = completed(engine.sync(Trigger::Manual, &hooks).await);

        assert_eq!(report.status.state, SyncState::Error, "{}", url);
        assert!(started.elapsed() < Duration::from_secs(10), "{}", url);
    }
}

#[tokio::test]
async fn detects_repo_from_symlinks() {
    let f = Fixture::new();
    let config_dir = f.config_dir_for(&f.a(), "config-a");

    let detection = detect(&f.runner(), &config_dir).await;

    assert_eq!(
        detection,
        Detection::Repo(SyncRepo {
            top_level: f.a().canonicalize().unwrap(),
            stage_paths: vec![
                PathBuf::from("tauri-app/settings.json"),
                PathBuf::from("tauri-app/prompts"),
                PathBuf::from("tauri-app/skills"),
            ],
            branch: "main".to_string(),
            settings_path: Some(f.a().canonicalize().unwrap().join("tauri-app/settings.json")),
        })
    );
}

#[tokio::test]
async fn change_log_dir_is_staged_only_when_it_exists() {
    let f = Fixture::new();
    let config_dir = f.config_dir_for(&f.a(), "config-a");

    let Detection::Repo(without) = detect(&f.runner(), &config_dir).await else {
        panic!("expected a repo");
    };
    assert!(!without.stage_paths.contains(&PathBuf::from("tauri-app/change-log")));

    write(&f.a().join("tauri-app/change-log/e.json"), "{}");
    let Detection::Repo(with) = detect(&f.runner(), &config_dir).await else {
        panic!("expected a repo");
    };
    assert!(with.stage_paths.contains(&PathBuf::from("tauri-app/change-log")));
}

#[tokio::test]
async fn regular_files_are_off() {
    let f = Fixture::new();
    let config_dir = f.tmp.path().join("plain-config");
    write(&config_dir.join("settings.json"), "{}");
    std::fs::create_dir_all(config_dir.join("prompts")).unwrap();
    std::fs::create_dir_all(config_dir.join("skills")).unwrap();

    assert_eq!(detect(&f.runner(), &config_dir).await, Detection::Off);
}

#[tokio::test]
async fn targets_in_different_repos_are_an_error() {
    let f = Fixture::new();
    let config_dir = f.tmp.path().join("mixed-config");
    std::fs::create_dir_all(&config_dir).unwrap();
    symlink(f.a().join("tauri-app/settings.json"), config_dir.join("settings.json")).unwrap();
    symlink(f.b().join("tauri-app/prompts"), config_dir.join("prompts")).unwrap();

    assert!(matches!(
        detect(&f.runner(), &config_dir).await,
        Detection::Error(_)
    ));
}

const SETTINGS_FIRST: &str = r#"{"theme":"first","models":[{"id":"m","type":"text","model":"gpt-4","display_name":"M","provider":"openai","api_key":"k"}]}"#;
const SETTINGS_SECOND: &str = r#"{"theme":"second","models":[{"id":"m","type":"text","model":"gpt-4","display_name":"M","provider":"openai","api_key":"k"}]}"#;

struct ReloadFixture {
    fixture: Fixture,
    config_dir: PathBuf,
    config: Arc<Mutex<ConfigService>>,
    skills: Arc<Mutex<SkillService>>,
    database: Arc<Mutex<Database>>,
    _db_dir: TempDir,
}

impl ReloadFixture {
    fn new() -> Self {
        let fixture = Fixture::new();
        write(&fixture.a().join("tauri-app/settings.json"), SETTINGS_FIRST);
        fixture.git(&fixture.a(), &["add", "-A"]);
        fixture.git(&fixture.a(), &["commit", "-m", "valid settings"]);
        fixture.git(&fixture.a(), &["push"]);
        fixture.git(&fixture.b(), &["pull"]);

        let config_dir = fixture.config_dir_for(&fixture.a(), "config-a");
        let config = ConfigService::load(&config_dir).unwrap();
        let skills = SkillService::load(&config_dir.join("skills"), &[]).unwrap();
        fixture.git(&fixture.a(), &["add", "-A"]);
        fixture.git(&fixture.a(), &["commit", "--allow-empty", "-m", "normalized by load"]);
        fixture.git(&fixture.a(), &["push"]);
        fixture.git(&fixture.b(), &["pull"]);
        let db_dir = TempDir::new().unwrap();
        let database = Database::open(db_dir.path()).unwrap();
        Self {
            fixture,
            config_dir,
            config: Arc::new(Mutex::new(config)),
            skills: Arc::new(Mutex::new(skills)),
            database: Arc::new(Mutex::new(database)),
            _db_dir: db_dir,
        }
    }

    fn push_from_b(&self, relative: &str, content: &str) {
        let b = self.fixture.b();
        write(&b.join("tauri-app").join(relative), content);
        self.fixture.git(&b, &["add", "-A"]);
        self.fixture.git(&b, &["commit", "-m", "change from b"]);
        self.fixture.git(&b, &["push"]);
    }

    fn engine_a(&self) -> SettingsSync {
        self.fixture.engine(
            self.config_dir.clone(),
            self.fixture.env.clone(),
            TIMEOUT,
        )
    }

    fn hooks(&self) -> ReloadingHooks {
        ReloadingHooks {
            config_dir: self.config_dir.clone(),
            config: self.config.clone(),
            skills: self.skills.clone(),
            database: self.database.clone(),
            reloaded: Mutex::new(Reloaded::default()),
        }
    }

    async fn sync_a(&self) -> Reloaded {
        let hooks = self.hooks();
        completed(self.engine_a().sync(Trigger::Manual, &hooks).await);
        let reloaded = *hooks.reloaded.lock().unwrap();
        reloaded
    }
}

struct ReloadingHooks {
    config_dir: PathBuf,
    config: Arc<Mutex<ConfigService>>,
    skills: Arc<Mutex<SkillService>>,
    database: Arc<Mutex<Database>>,
    reloaded: Mutex<Reloaded>,
}

#[async_trait]
impl SyncHooks for ReloadingHooks {
    type Guard = ();

    async fn lock_local(&self) -> Self::Guard {
        let reloaded = reload_if_changed(
            &mut self.config.lock().unwrap(),
            &mut self.skills.lock().unwrap(),
            self.database.lock().unwrap().conn(),
        )
        .unwrap();
        self.reloaded.lock().unwrap().merge(reloaded);
    }

    async fn reload(&self, _guard: &mut Self::Guard, changed: &[PathBuf]) -> Result<(), String> {
        let reloaded = reload_changed(
            changed,
            &self.config_dir.join("settings.json"),
            &self.config_dir.join("skills"),
            &change_log_dir(&self.config_dir),
            &mut self.config.lock().unwrap(),
            &mut self.skills.lock().unwrap(),
            self.database.lock().unwrap().conn(),
        )
        .map_err(|e| e.to_string())?;
        self.reloaded.lock().unwrap().merge(reloaded);
        Ok(())
    }

    fn status_changed(&self, _status: &SyncStatus) {}
}

#[tokio::test]
async fn pulled_settings_change_reloads_settings_and_skills() {
    let f = ReloadFixture::new();
    assert_eq!(f.config.lock().unwrap().settings().theme, "first");

    f.push_from_b("settings.json", SETTINGS_SECOND);
    let reloaded = f.sync_a().await;

    assert_eq!(f.config.lock().unwrap().settings().theme, "second");
    assert_eq!(reloaded, Reloaded { settings: true, skills: true, change_log: false });
}

#[tokio::test]
async fn pulled_skill_dir_reloads_skills_only() {
    let f = ReloadFixture::new();
    assert!(f.skills.lock().unwrap().get_skill("fresh").is_none());

    f.push_from_b(
        "skills/fresh/SKILL.md",
        "---\nname: fresh\ndescription: desc\n---\n\nbody\n",
    );
    let reloaded = f.sync_a().await;

    assert!(f.skills.lock().unwrap().get_skill("fresh").is_some());
    assert_eq!(reloaded, Reloaded { settings: false, skills: true, change_log: false });
}

#[tokio::test]
async fn pulled_prompt_change_reloads_nothing() {
    let f = ReloadFixture::new();

    f.push_from_b("prompts/p.md", "changed prompt\n");
    let reloaded = f.sync_a().await;

    assert_eq!(reloaded, Reloaded::default());
}

#[tokio::test]
async fn external_reset_is_reloaded_by_next_cycle() {
    let f = ReloadFixture::new();
    f.push_from_b("settings.json", SETTINGS_SECOND);
    let a = f.fixture.a();
    f.fixture.git(&a, &["fetch"]);
    f.fixture.git(&a, &["reset", "--hard", "origin/main"]);

    let reloaded = f.sync_a().await;

    let config = f.config.lock().unwrap();
    assert_eq!(config.settings().theme, "second");
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(f.config_dir.join("settings.json")).unwrap())
            .unwrap();
    let in_memory = serde_json::to_value(config.settings()).unwrap();
    assert_eq!(in_memory["theme"], on_disk["theme"]);
    assert_eq!(in_memory["models"][0]["id"], on_disk["models"][0]["id"]);
    assert!(reloaded.settings);
}

#[tokio::test]
async fn external_reset_is_reloaded_while_paused() {
    let f = ReloadFixture::new();
    f.push_from_b("settings.json", SETTINGS_SECOND);
    let a = f.fixture.a();
    write(
        &a.join("tauri-app/settings.json"),
        &SETTINGS_FIRST.replace("first", "third"),
    );

    let engine = f.engine_a();
    let report = completed(engine.sync(Trigger::Startup, &f.hooks()).await);
    assert_eq!(report.status.state, SyncState::Conflict);

    let rebase_dir = f.fixture.git(&a, &["rev-parse", "--git-path", "rebase-merge"]);
    if a.join(rebase_dir).exists() {
        f.fixture.git(&a, &["rebase", "--abort"]);
    }
    f.fixture.git(&a, &["reset", "--hard", "origin/main"]);

    let hooks = f.hooks();
    assert_eq!(engine.sync(Trigger::Periodic, &hooks).await, SyncResult::Paused);

    assert_eq!(f.config.lock().unwrap().settings().theme, "second");
    assert!(hooks.reloaded.lock().unwrap().settings);
}

#[tokio::test]
async fn pulled_change_log_entry_sets_change_log_only() {
    let f = ReloadFixture::new();
    f.push_from_b("change-log/first.json", "{}");
    f.fixture.git(&f.fixture.a(), &["pull"]);

    f.push_from_b("change-log/second.json", "{}");
    let reloaded = f.sync_a().await;

    assert_eq!(reloaded, Reloaded { settings: false, skills: false, change_log: true });
}

#[tokio::test]
async fn pulled_invalid_settings_json_is_a_failed_reload() {
    let f = ReloadFixture::new();
    f.push_from_b("settings.json", "<<<<<<< HEAD\n{}");

    let hooks = f.hooks();
    let report = completed(f.engine_a().sync(Trigger::Manual, &hooks).await);

    assert_eq!(report.status.state, SyncState::Error);
    assert!(report
        .status
        .message
        .unwrap()
        .contains("reload after sync failed"));
}
