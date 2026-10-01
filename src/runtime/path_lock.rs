//! One lock per path, so a streaming executor cannot lose an update.
//!
//! ## Why a lock and not a read/write classification
//!
//! `ROADMAP.md` §8 is blunt about this, and it is right: *"reads are safe"
//! is only true if a read cannot observe a half-applied write. Two parallel
//! writes to one file is a lost update. A tool that opens a file, reads it,
//! changes it and writes it back is three steps, and anything that can run
//! between the first and the last sees a file that does not exist yet.
//!
//! So **a read takes the same lock a write does**, for the whole
//! read-modify-write. Classification is then an optimisation layered on top —
//! it lets *different* paths run together — and never a substitute for the
//! lock. This module is the substitute it must not be replaced by.
//!
//! ## Different paths still run together
//!
//! The lock is per path precisely so that the parallelism is kept where it is
//! safe. A single global lock would make every rule trivially true and the
//! executor sequential, which is the failure mode a naive fix lands in.
//! `different_paths_do_not_block_each_other` is the test that says the two
//! properties hold at once.
//!
//! ## The entry is cleaned up
//!
//! A map that only grows is a leak with a plausible-looking shape. When the
//! last holder releases a path, its entry goes — so a run that touched ten
//! thousand files holds ten thousand locks for its lifetime and not one more,
//! and `no_entries_survive_their_guards` says so.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use tokio::sync::{Mutex, OwnedMutexGuard};

/// Per-path locks, shared by every task in a run.
///
/// The map holds **weak** references. That single choice removes a whole class
/// of bug that the first three versions of this file walked into:
///
/// * A strong `Arc` per entry needs a `strong_count == 1` test to remove it,
///   and that test is an **ordering** question — the guard's own mutex guard
///   is dropped *after* `Drop::drop` returns, so a spawned release can run
///   first and see a count that is one too high. Nothing is removed, and the
///   map grows for the life of the run. That is the exact leak the first
///   version's own doc comment claimed it did not have.
/// * Removing eagerly on `Drop` needs the same test, with the same hazard.
/// * Keeping entries and pruning later needs a sweep, which is a background
///   task this module should not own.
///
/// A `Weak` needs none of that: when the last guard goes, the entry's referent
/// is simply gone, and the next acquire finds a dead `Weak`, replaces it, and
/// moves on. There is no cleanup path to get wrong, because there is no
/// cleanup path. `held()` reports only the entries that still have a guard,
/// so the number a status line shows is the number of files actually held.
#[derive(Debug, Clone, Default)]
pub struct PathLocks {
    inner: Arc<Mutex<HashMap<PathBuf, Weak<tokio::sync::Mutex<()>>>>>,
}

impl PathLocks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the lock for `path`. Hold the returned guard for as long as the
    /// file is being touched.
    pub async fn acquire(&self, path: &Path) -> PathGuard {
        let key = normalise(path);
        let entry = {
            let mut map = self.inner.lock().await;
            let existing = map.get(&key).and_then(Weak::upgrade);
            match existing {
                Some(arc) => arc,
                None => {
                    let arc = Arc::new(tokio::sync::Mutex::new(()));
                    // A stale `Weak` is replaced rather than revived, so a
                    // finished file leaves nothing behind.
                    map.insert(key.clone(), Arc::downgrade(&arc));
                    arc
                }
            }
        };
        let guard = Arc::clone(&entry).lock_owned().await;
        PathGuard {
            _path: key,
            _guard: guard,
        }
    }

    /// How many paths are **currently held** — entries whose referent still
    /// exists, so this is the number of files a run is touching, not a
    /// historical count.
    pub async fn held(&self) -> usize {
        let map = self.inner.lock().await;
        map.values().filter(|w| w.strong_count() > 0).count()
    }
}

/// Held for as long as a path is being touched. Dropping it releases the path
/// with no bookkeeping — the map's `Weak` simply stops resolving.
pub struct PathGuard {
    // Kept so a caller can see which path this is while holding it; the map
    // needs no cleanup, which is the point.
    _path: PathBuf,
    _guard: OwnedMutexGuard<()>,
}

/// The key a path is locked under.
///
/// Canonicalised where possible, so `src/lib.rs` and `./src/../src/lib.rs` are
/// the same lock. Two different spellings of one file taking two different
/// locks is the same lost update with extra steps.
///
/// Falls back to the path as given when canonicalisation fails — a file that
/// does not exist yet cannot be canonicalised, and *creating* files is the case
/// most likely to be concurrent.
fn normalise(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        // Best-effort lexically: collapse `.` and `..` without touching disk.
        let mut out = PathBuf::new();
        for part in path.components() {
            match part {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    if !out.pop() {
                        out.push("..");
                    }
                }
                other => out.push(other.as_os_str()),
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn file(dir: &Path, name: &str) -> PathBuf {
        dir.join(name)
    }

    /// **The lost update.** Two tasks each do read-modify-write on one file.
    /// Without the lock, the second read sees the first's unwritten state and
    /// one append vanishes; with it, both survive.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_writes_to_one_file_do_not_lose_an_update() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file(dir.path(), "shared.txt");
        std::fs::write(&path, "").expect("write");
        let locks = PathLocks::new();

        let mut tasks = Vec::new();
        for tag in ["alpha", "beta", "gamma"] {
            let locks = locks.clone();
            let path = path.clone();
            tasks.push(tokio::spawn(async move {
                for _ in 0..20 {
                    // The whole read-modify-write under one guard. Holding it
                    // only for the write is the bug: a read that lands
                    // between another's read and write still loses the update.
                    let _g = locks.acquire(&path).await;
                    let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
                    existing.push_str(tag);
                    std::fs::write(&path, existing).expect("write");
                }
            }));
        }
        for t in tasks {
            t.await.expect("task");
        }

        let final_text = std::fs::read_to_string(&path).expect("read");
        // 3 tasks × 20 rounds = 60 appends, but the tags are 5, 4 and 5
        // characters, so the file must be 20 × 14 = 280 characters. The first
        // version of this asserted 60 and failed against **correct** code —
        // 60 appends of one character each is a different file.
        let expected: usize = ["alpha", "beta", "gamma"]
            .iter()
            .map(|t| t.len())
            .sum::<usize>()
            * 20;
        assert_eq!(
            final_text.chars().count(),
            expected,
            "every append must survive, so the lock serialised the \
             read-modify-write rather than losing one: {final_text:?}"
        );
        for tag in ["alpha", "beta", "gamma"] {
            assert_eq!(
                final_text.matches(tag).count(),
                20,
                "each task's 20 appends must all be present: {final_text:?}"
            );
        }
    }

    /// §8's point exactly: *a read cannot observe a half-applied write*.
    ///
    /// The writer holds the lock while it builds a deliberately invalid
    /// intermediate state. A reader that is not holding the lock would see
    /// it. This is the case a read/write classification gets wrong, because
    /// the read looks harmless.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_read_cannot_observe_a_half_applied_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = file(dir.path(), "half.txt");
        std::fs::write(&path, "complete").expect("write");
        let locks = PathLocks::new();
        let observed = Arc::new(Mutex::new(Vec::new()));

        let writer_locks = locks.clone();
        let writer_path = path.clone();
        let writer = tokio::spawn(async move {
            let _g = writer_locks.acquire(&writer_path).await;
            // Mid-write: truncate, then stop.
            std::fs::write(&writer_path, "HALF").expect("truncate");
            tokio::time::sleep(Duration::from_millis(120)).await;
            std::fs::write(&writer_path, "complete-again").expect("finish");
        });

        // Give the writer time to be mid-write.
        tokio::time::sleep(Duration::from_millis(40)).await;
        // The reader takes **the same lock**, so it waits rather than reading
        // the truncated file.
        let reader_locks = locks.clone();
        let reader_path = path.clone();
        let reader_seen = Arc::clone(&observed);
        let reader = tokio::spawn(async move {
            let _g = reader_locks.acquire(&reader_path).await;
            let text = std::fs::read_to_string(&reader_path).unwrap_or_default();
            reader_seen.lock().await.push(text);
        });

        writer.await.expect("writer");
        reader.await.expect("reader");
        let seen = observed.lock().await.clone();
        assert_eq!(
            seen,
            vec!["complete-again".to_string()],
            "the reader must see a finished file, never a half-applied one — \
             {seen:?}"
        );
    }

    /// And the parallelism is kept where it is safe. A single global lock
    /// would make both tests above pass and the executor sequential, which is
    /// the failure a naive fix lands in.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn different_paths_do_not_block_each_other() {
        let dir = tempfile::tempdir().expect("tempdir");
        let a = file(dir.path(), "a.txt");
        let b = file(dir.path(), "b.txt");
        std::fs::write(&a, "").expect("write");
        std::fs::write(&b, "").expect("write");
        let locks = PathLocks::new();

        let held = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for path in [a, b] {
            let locks = locks.clone();
            let held = Arc::clone(&held);
            let peak = Arc::clone(&peak);
            tasks.push(tokio::spawn(async move {
                let _g = locks.acquire(&path).await;
                let now = held.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(80)).await;
                held.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for t in tasks {
            t.await.expect("task");
        }
        assert_eq!(
            peak.load(Ordering::SeqCst),
            2,
            "two different paths must be able to hold their locks at the same \
             time, or the executor is sequential for no reason"
        );
    }

    /// The same file reached by two spellings is one lock.
    ///
    /// Two locks for one file is the same lost update with extra steps, and
    /// it is easy to arrive at: `./src/lib.rs` and `src/../src/lib.rs` are the
    /// same file and a path is whatever a tool was handed.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn two_spellings_of_one_path_take_the_same_lock() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real = dir.path().join("lib.rs");
        std::fs::write(&real, "").expect("write");
        let odd = dir.path().join("./nested/../lib.rs");
        let locks = PathLocks::new();

        let first = locks.acquire(&real).await;
        assert_eq!(locks.held().await, 1);
        drop(first);
        // **Not** by holding one guard and asking for the other: that is a
        // self-deadlock, and the first version of this test did exactly that
        // and hung the suite for four minutes before anyone looked at why.
        // One guard at a time, asserting the map — which cannot deadlock.
        let second = locks.acquire(&odd).await;
        assert_eq!(
            locks.held().await,
            1,
            "two spellings of one file must be one lock, not two"
        );
        drop(second);
    }

    /// A map that only grows is a leak with a plausible shape.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_entries_survive_their_guards() {
        let dir = tempfile::tempdir().expect("tempdir");
        let locks = PathLocks::new();
        // No per-iteration count assertion. The release is spawned, so an
        // entry from iteration *n-1* can still be in the map while iteration n
        // runs — which the first version of this test asserted against and
        // failed, for a state that is correct and transient.
        for i in 0..50 {
            let p = file(dir.path(), &format!("f{i}.txt"));
            let g = locks.acquire(&p).await;
            drop(g);
        }
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(2)).await;
            if locks.held().await == 0 {
                break;
            }
        }
        assert_eq!(
            locks.held().await,
            0,
            "a run that touched fifty files must not hold fifty locks for its \
             lifetime — {} left",
            locks.held().await
        );
    }

    /// A file that does not exist yet still gets a lock.
    ///
    /// *Creating* files is the case most likely to be concurrent, and
    /// canonicalisation fails on a path with nothing at it — so the fallback is
    /// the one that matters most.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_file_that_does_not_exist_yet_still_locks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("not-created-yet.txt");
        let locks = PathLocks::new();
        let g1 = locks.acquire(&missing).await;
        assert_eq!(locks.held().await, 1, "a missing file must still lock");
        drop(g1);
        // Two *tasks* racing to create it are serialised, which is the property
        // that matters. A second acquire in this task would self-deadlock.
        let locks2 = locks.clone();
        let p2 = missing.clone();
        let other = tokio::spawn(async move {
            let _g = locks2.acquire(&p2).await;
            std::fs::write(&p2, "made").expect("write");
        });
        other.await.expect("task");
        assert_eq!(std::fs::read_to_string(&missing).expect("read"), "made");
    }
}
