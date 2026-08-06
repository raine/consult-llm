use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

use consult_llm_core::jsonl::read_jsonl_from_offset;
use consult_llm_core::monitoring::{
    ActiveSnapshot, HISTORY_FILE, HistoryRecord, RunEvent, RunEventKind, RunMeta, is_pid_alive,
    runs_dir,
};

use crate::meta::load_run_meta;

pub(crate) enum PollUpdate {
    ActiveRunAdded(ActiveSnapshot),
    ActiveRunUpdated(ActiveSnapshot),
    ActiveRunRemoved(String),
    OrphanDetected(String),
    HistoryRecords(Vec<HistoryRecord>),
    DetailMetadata(RunMeta),
    DetailEvents {
        run_id: String,
        events: Vec<RunEvent>,
    },
}

pub(crate) enum PollCommand {
    EnterDetail { run_id: String, file_offset: u64 },
    ExitDetail,
    ResetHistory,
    Shutdown,
}

struct DetailPollState {
    run_id: String,
    file_offset: u64,
}

pub(crate) fn spawn(
    dir: PathBuf,
    poll_interval: Duration,
) -> (
    mpsc::Receiver<PollUpdate>,
    mpsc::Sender<PollCommand>,
    JoinHandle<()>,
) {
    let (update_tx, update_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();

    let handle = thread::Builder::new()
        .name("poller".into())
        .spawn(move || run(dir, poll_interval, update_tx, cmd_rx))
        .expect("failed to spawn poller thread");

    (update_rx, cmd_tx, handle)
}

fn run(
    dir: PathBuf,
    reconcile_interval: Duration,
    tx: mpsc::Sender<PollUpdate>,
    cmd_rx: mpsc::Receiver<PollCommand>,
) {
    let active_dir = dir.join("active");
    let (watch_tx, watch_rx) = mpsc::channel::<()>();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<notify::Event>| {
            if result.is_ok() {
                let _ = watch_tx.send(());
            }
        },
        Config::default(),
    )
    .ok();

    if let Some(watcher) = watcher.as_mut() {
        let _ = watcher.watch(&active_dir, RecursiveMode::NonRecursive);
        let _ = watcher.watch(&dir, RecursiveMode::NonRecursive);
    }

    cleanup_startup_orphans(&active_dir);

    let mut watched_detail_path: Option<PathBuf> = None;
    let mut active_snapshots = HashMap::new();
    let mut reported_orphans = HashSet::new();
    let mut history_offset = 0u64;
    let mut detail: Option<DetailPollState> = None;
    let mut next_reconcile = Instant::now();
    let mut next_orphan_check = Instant::now();

    loop {
        while let Ok(cmd) = cmd_rx.try_recv() {
            if apply_command(
                cmd,
                &dir,
                &tx,
                &mut detail,
                &mut history_offset,
                watcher.as_mut(),
                &mut watched_detail_path,
            ) {
                return;
            }
        }

        let mut saw_notify = false;
        while watch_rx.try_recv().is_ok() {
            saw_notify = true;
        }

        let now = Instant::now();
        if saw_notify || now >= next_reconcile {
            let next_snapshots = load_active_snapshots(&active_dir);
            if !diff_snapshots(&active_snapshots, &next_snapshots, &tx) {
                return;
            }
            active_snapshots = next_snapshots;
            reported_orphans.retain(|run_id| active_snapshots.contains_key(run_id));

            let records = poll_history(&dir, &mut history_offset);
            if !records.is_empty() && tx.send(PollUpdate::HistoryRecords(records)).is_err() {
                return;
            }

            if let Some(detail) = detail.as_mut() {
                let events = poll_detail_events(&dir, &detail.run_id, &mut detail.file_offset);
                if !events.is_empty()
                    && tx
                        .send(PollUpdate::DetailEvents {
                            run_id: detail.run_id.clone(),
                            events,
                        })
                        .is_err()
                {
                    return;
                }
            }

            next_reconcile = Instant::now() + reconcile_interval;
        }

        if now >= next_orphan_check {
            let orphans = collect_orphans(&active_snapshots, &reported_orphans);
            for run_id in orphans {
                reported_orphans.insert(run_id.clone());
                if tx.send(PollUpdate::OrphanDetected(run_id)).is_err() {
                    return;
                }
            }
            next_orphan_check = Instant::now() + Duration::from_secs(5);
        }

        let until_reconcile = next_reconcile.saturating_duration_since(Instant::now());
        let until_orphans = next_orphan_check.saturating_duration_since(Instant::now());
        let wait = until_reconcile
            .min(until_orphans)
            .min(Duration::from_millis(100));
        let _ = watch_rx.recv_timeout(wait);
    }
}

fn apply_command(
    cmd: PollCommand,
    dir: &Path,
    tx: &mpsc::Sender<PollUpdate>,
    detail: &mut Option<DetailPollState>,
    history_offset: &mut u64,
    watcher: Option<&mut RecommendedWatcher>,
    watched_detail_path: &mut Option<PathBuf>,
) -> bool {
    match cmd {
        PollCommand::Shutdown => return true,
        PollCommand::EnterDetail {
            run_id,
            file_offset,
        } => {
            *detail = Some(DetailPollState {
                run_id: run_id.clone(),
                file_offset,
            });

            update_detail_watch(
                watcher,
                watched_detail_path,
                Some(detail_events_path(dir, &run_id)),
            );

            if let Some(meta) = load_run_meta(&run_id)
                && tx.send(PollUpdate::DetailMetadata(meta)).is_err()
            {
                return true;
            }

            if let Some(detail) = detail.as_mut() {
                let events = poll_detail_events(dir, &detail.run_id, &mut detail.file_offset);
                if !events.is_empty()
                    && tx
                        .send(PollUpdate::DetailEvents {
                            run_id: detail.run_id.clone(),
                            events,
                        })
                        .is_err()
                {
                    return true;
                }
            }
        }
        PollCommand::ExitDetail => {
            *detail = None;
            update_detail_watch(watcher, watched_detail_path, None);
        }
        PollCommand::ResetHistory => {
            *history_offset = 0;
        }
    }
    false
}

fn update_detail_watch(
    watcher: Option<&mut RecommendedWatcher>,
    watched_detail_path: &mut Option<PathBuf>,
    next_detail_path: Option<PathBuf>,
) {
    let Some(watcher) = watcher else {
        *watched_detail_path = next_detail_path;
        return;
    };

    if let Some(path) = watched_detail_path.take() {
        let _ = watcher.unwatch(&path);
    }

    if let Some(path) = next_detail_path {
        let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
        *watched_detail_path = Some(path);
    }
}

fn detail_events_path(dir: &Path, run_id: &str) -> PathBuf {
    dir.join("runs").join(format!("{run_id}.events.jsonl"))
}

fn load_active_snapshots(dir: &Path) -> HashMap<String, ActiveSnapshot> {
    let mut out = HashMap::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Ok(bytes) = fs::read(&path)
            && let Ok(snapshot) = serde_json::from_slice::<ActiveSnapshot>(&bytes)
        {
            out.insert(snapshot.run_id.clone(), snapshot);
        }
    }

    out
}

fn diff_snapshots(
    prev: &HashMap<String, ActiveSnapshot>,
    next: &HashMap<String, ActiveSnapshot>,
    tx: &mpsc::Sender<PollUpdate>,
) -> bool {
    for (run_id, snapshot) in next {
        match prev.get(run_id) {
            None => {
                if tx
                    .send(PollUpdate::ActiveRunAdded(snapshot.clone()))
                    .is_err()
                {
                    return false;
                }
            }
            Some(previous)
                if previous.last_seq != snapshot.last_seq
                    && tx
                        .send(PollUpdate::ActiveRunUpdated(snapshot.clone()))
                        .is_err() =>
            {
                return false;
            }
            _ => {}
        }
    }

    for run_id in prev.keys() {
        if !next.contains_key(run_id)
            && tx
                .send(PollUpdate::ActiveRunRemoved(run_id.clone()))
                .is_err()
        {
            return false;
        }
    }

    true
}

fn poll_history(dir: &Path, history_offset: &mut u64) -> Vec<HistoryRecord> {
    let path = dir.join(HISTORY_FILE);
    let mut records: Vec<HistoryRecord> = read_jsonl_from_offset(&path, history_offset);
    for record in &mut records {
        record.parsed_ts = chrono::DateTime::parse_from_rfc3339(&record.ts)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc));
    }
    records
}

fn poll_detail_events(dir: &Path, run_id: &str, file_offset: &mut u64) -> Vec<RunEvent> {
    read_jsonl_from_offset(&detail_events_path(dir, run_id), file_offset)
}

fn cleanup_startup_orphans(active_dir: &Path) {
    for (run_id, snapshot) in load_active_snapshots(active_dir) {
        if !is_pid_alive(snapshot.pid) {
            let _ = fs::remove_file(active_dir.join(format!("{run_id}.json")));
        }
    }
}

fn collect_orphans(
    active: &HashMap<String, ActiveSnapshot>,
    reported_orphans: &HashSet<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (run_id, snapshot) in active {
        if reported_orphans.contains(run_id) || is_pid_alive(snapshot.pid) {
            continue;
        }
        if events_file_has_finish(run_id) {
            continue;
        }
        out.push(run_id.clone());
    }
    out
}

fn events_file_has_finish(run_id: &str) -> bool {
    let path = runs_dir().join(format!("{run_id}.events.jsonl"));
    let Ok(file) = fs::File::open(path) else {
        return false;
    };

    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .any(|line| match serde_json::from_str::<RunEvent>(&line) {
            Ok(event) => matches!(event.kind, RunEventKind::RunFinished { .. }),
            Err(_) => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(run_id: &str, seq: u64) -> ActiveSnapshot {
        ActiveSnapshot {
            v: 1,
            run_id: run_id.into(),
            pid: 1,
            started_at: "2026-04-24T00:00:00.000Z".into(),
            model: "gpt-5".into(),
            backend: "api".into(),
            project: "proj".into(),
            thread_id: None,
            task_mode: None,
            reasoning_effort: None,
            last_seq: seq,
            last_event_at: "2026-04-24T00:00:00.000Z".into(),
            stage: None,
        }
    }

    #[test]
    fn diff_snapshots_emits_add_update_and_remove() {
        let (tx, rx) = mpsc::channel();

        let prev = HashMap::from([
            ("run-a".to_string(), snapshot("run-a", 1)),
            ("run-b".to_string(), snapshot("run-b", 1)),
        ]);
        let next = HashMap::from([
            ("run-a".to_string(), snapshot("run-a", 2)),
            ("run-c".to_string(), snapshot("run-c", 1)),
        ]);

        assert!(diff_snapshots(&prev, &next, &tx));

        let updates: Vec<_> = rx.try_iter().collect();
        assert_eq!(updates.len(), 3);
        assert!(updates.iter().any(
            |update| matches!(update, PollUpdate::ActiveRunUpdated(s) if s.run_id == "run-a")
        ));
        assert!(
            updates.iter().any(
                |update| matches!(update, PollUpdate::ActiveRunAdded(s) if s.run_id == "run-c")
            )
        );
        assert!(updates.iter().any(
            |update| matches!(update, PollUpdate::ActiveRunRemoved(run_id) if run_id == "run-b")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_startup_orphans_removes_dead_runs_without_creating_history() {
        let temp = tempfile::tempdir().unwrap();
        let active_dir = temp.path().join("active");
        fs::create_dir(&active_dir).unwrap();

        let dead = ActiveSnapshot {
            pid: i32::MAX as u32,
            ..snapshot("dead", 1)
        };
        fs::write(
            active_dir.join("dead.json"),
            serde_json::to_vec(&dead).unwrap(),
        )
        .unwrap();

        cleanup_startup_orphans(&active_dir);

        assert!(!active_dir.join("dead.json").exists());
        assert!(!temp.path().join(HISTORY_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_startup_orphans_retains_live_runs() {
        let temp = tempfile::tempdir().unwrap();
        let active_dir = temp.path().join("active");
        fs::create_dir(&active_dir).unwrap();

        let live = ActiveSnapshot {
            pid: std::process::id(),
            ..snapshot("live", 1)
        };
        fs::write(
            active_dir.join("live.json"),
            serde_json::to_vec(&live).unwrap(),
        )
        .unwrap();

        cleanup_startup_orphans(&active_dir);

        assert!(active_dir.join("live.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn collect_orphans_ignores_finished_runs() {
        use std::io::Write;

        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("XDG_STATE_HOME", temp.path());
        }

        let dead_pid = i32::MAX as u32;
        let active = HashMap::from([
            (
                "unfinished".to_string(),
                ActiveSnapshot {
                    pid: dead_pid,
                    ..snapshot("unfinished", 1)
                },
            ),
            (
                "finished".to_string(),
                ActiveSnapshot {
                    pid: dead_pid,
                    ..snapshot("finished", 1)
                },
            ),
        ]);

        let finished_path = runs_dir().join("finished.events.jsonl");
        let mut file = fs::File::create(finished_path).unwrap();
        writeln!(
            file,
            "{}",
            serde_json::to_string(&RunEvent {
                v: 1,
                run_id: "finished".into(),
                seq: 1,
                ts: "2026-04-24T00:00:00.000Z".into(),
                kind: RunEventKind::RunFinished {
                    duration_ms: 1,
                    success: true,
                    error: None,
                },
            })
            .unwrap()
        )
        .unwrap();

        let orphans = collect_orphans(&active, &HashSet::new());
        assert_eq!(orphans, vec!["unfinished".to_string()]);
    }
}
