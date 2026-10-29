//! The single-flight lease (D-11): one training job at a time on a deployment.
//!
//! The lock is one record in the same [`TaskBackend`] the task store uses, so a
//! DynamoDB deployment needs no second table. It lives under the printable
//! owner [`LOCK_OWNER`] and task id [`LOCK_TASK_ID`], reachable from the aws CLI.
//! `#` cannot occur in a Cognito subject, an M2M client id or `local`, so no
//! caller's task can ever share the lock's key (and `submit` refuses an owner
//! equal to it anyway).
//!
//! # No backend TTL, on purpose
//!
//! The record's `expires_at_secs` is ALWAYS `None`. `TaskBackend::get` hides an
//! expired row, but DynamoDB's conditional `put_new` (`attribute_not_exists`)
//! still sees it until DynamoDB's asynchronous TTL deletion removes it, so a
//! backend TTL would make `put_new` conflict while `get` says "absent" and wedge
//! the deployment. Expiry is the lease instead: `lease_expires_at_secs` in the
//! envelope, judged by [`acquire`] in code.
//!
//! # Who is "live"
//!
//! A holder is live when the lock is not released, its lease has not passed and
//! the holder's own task record (found through the stored `holder_owner`) still
//! exists. A record that is released, past its lease, or whose holder task is
//! gone is taken over by compare-and-swap, so a lost task record cannot wedge the
//! deployment.
//!
//! A holder whose task is TERMINAL is still live until the lock is RELEASED
//! (plan 14-05). A client `tasks/cancel` makes the task `Cancelled` immediately,
//! while its training job or pack task may still be running; the finalizer
//! releases the lock only when compute is terminal. Reading "terminal" as "free"
//! would let a replacement `train` start a second billed run beside the first.
//! A finalizer that crashed before releasing is resumed by its scheduled
//! reconcile sweep, and the lease bounds the worst case.

use std::time::{SystemTime, UNIX_EPOCH};

use aprender_mcp_setfit_train::{BackendError, StoredTask, TaskBackend};
use pmcp::types::tasks::Task;
use pmcp::types::TaskStatus;
use serde::{Deserialize, Serialize};

use crate::layout::LOCK_LEASE_SECS;

/// The owner the lock record lives under. Printable so the record is addressable
/// from the aws CLI, and containing `#`, which no auth subject can.
pub const LOCK_OWNER: &str = "laya-train#single-flight";

/// The lock record's task id.
pub const LOCK_TASK_ID: &str = "lock";

/// Current epoch seconds.
#[must_use]
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A live job holds the lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// The holder's task id.
    pub holder: String,
}

impl std::fmt::Display for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "task {} holds the single-flight lock", self.holder)
    }
}

/// Why [`acquire`] did not take the lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquireError {
    /// A live job holds it.
    Held(Held),
    /// The backend failed or contention outlasted the retries.
    Backend(String),
}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Held(held) => held.fmt(f),
            Self::Backend(message) => write!(f, "lock backend: {message}"),
        }
    }
}

impl std::error::Error for AcquireError {}

/// The lock's envelope. `holder_owner` lets the finalizer (14-05) tell an
/// expired record from an owner mismatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LockEnvelope {
    holder: String,
    holder_owner: String,
    acquired_at_secs: u64,
    lease_expires_at_secs: u64,
}

/// A decoded lock record, for the finalizer's reconcile sweep (14-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockState {
    /// The holder's task id.
    pub holder: String,
    /// The holder's owner (auth subject).
    pub holder_owner: String,
    /// When the lock was taken.
    pub acquired_at_secs: u64,
    /// When the lease runs out.
    pub lease_expires_at_secs: u64,
    /// True once the holder (or a takeover's predecessor) released it.
    pub released: bool,
}

fn decode(record: &StoredTask) -> Option<LockState> {
    let envelope: LockEnvelope = serde_json::from_value(record.envelope.clone()?).ok()?;
    Some(LockState {
        holder: envelope.holder,
        holder_owner: envelope.holder_owner,
        acquired_at_secs: envelope.acquired_at_secs,
        lease_expires_at_secs: envelope.lease_expires_at_secs,
        released: record.task.status.is_terminal(),
    })
}

fn encode(
    holder: &str,
    holder_owner: &str,
    status: TaskStatus,
    acquired_at: u64,
    lease_expires_at: u64,
    version: u64,
) -> StoredTask {
    let envelope = LockEnvelope {
        holder: holder.to_string(),
        holder_owner: holder_owner.to_string(),
        acquired_at_secs: acquired_at,
        lease_expires_at_secs: lease_expires_at,
    };
    StoredTask {
        task: Task::new(LOCK_TASK_ID, status),
        result: None,
        envelope: serde_json::to_value(envelope).ok(),
        version,
        // ALWAYS None: see the module doc.
        expires_at_secs: None,
    }
}

fn backend_error(e: &BackendError) -> AcquireError {
    AcquireError::Backend(e.to_string())
}

/// Read the lock record, if one exists.
pub async fn read(backend: &dyn TaskBackend) -> Option<LockState> {
    let record = backend.get(LOCK_OWNER, LOCK_TASK_ID).await.ok().flatten()?;
    decode(&record)
}

/// Whether the holder of `state` is live at `now`.
async fn is_live(backend: &dyn TaskBackend, state: &LockState, now: u64) -> bool {
    if state.released || now >= state.lease_expires_at_secs {
        return false;
    }
    match backend.get(&state.holder_owner, &state.holder).await {
        // The holder's task is gone (expired or lost): nothing will ever release the lock.
        Ok(None) => false,
        // Present, terminal or not: the lock is the finalizer's to release (module doc).
        Ok(Some(_)) => true,
        // Cannot tell: fail closed, treat it as live.
        Err(_) => true,
    }
}

/// The live holder's task id, or `None`, judged at `now_secs`. Backend errors read
/// as `None`: the pre-check is advisory, and [`acquire`] is the authority.
pub async fn peek_at(backend: &dyn TaskBackend, now: u64) -> Option<String> {
    let state = read(backend).await?;
    is_live(backend, &state, now).await.then_some(state.holder)
}

/// [`peek_at`] at the current time.
pub async fn peek(backend: &dyn TaskBackend) -> Option<String> {
    peek_at(backend, now_secs()).await
}

/// Take the lock for `holder_task_id`, or say who holds it.
///
/// Empty -> `put_new`; released, past its lease or held by a terminal task ->
/// compare-and-swap takeover; otherwise [`AcquireError::Held`]. A lost race
/// re-reads and decides again, so of two racing acquires exactly one wins.
///
/// # Errors
///
/// [`AcquireError::Held`] with the live holder, or [`AcquireError::Backend`].
pub async fn acquire(
    backend: &dyn TaskBackend,
    holder_task_id: &str,
    holder_owner: &str,
    now: u64,
) -> Result<(), AcquireError> {
    for _ in 0..4 {
        let existing = backend
            .get(LOCK_OWNER, LOCK_TASK_ID)
            .await
            .map_err(|e| backend_error(&e))?;
        let lease_end = now + LOCK_LEASE_SECS;
        match existing {
            None => {
                let record = encode(
                    holder_task_id,
                    holder_owner,
                    TaskStatus::Working,
                    now,
                    lease_end,
                    1,
                );
                match backend.put_new(LOCK_OWNER, record).await {
                    Ok(()) => return Ok(()),
                    Err(BackendError::Conflict) => continue,
                    Err(e) => return Err(backend_error(&e)),
                }
            }
            Some(record) => {
                let Some(state) = decode(&record) else {
                    return Err(AcquireError::Backend(
                        "the lock record is unreadable".to_string(),
                    ));
                };
                if is_live(backend, &state, now).await {
                    return Err(AcquireError::Held(Held {
                        holder: state.holder,
                    }));
                }
                let taken = encode(
                    holder_task_id,
                    holder_owner,
                    TaskStatus::Working,
                    now,
                    lease_end,
                    record.version + 1,
                );
                match backend
                    .cas(LOCK_OWNER, LOCK_TASK_ID, record.version, taken)
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(BackendError::Conflict) => continue,
                    Err(e) => return Err(backend_error(&e)),
                }
            }
        }
    }
    Err(AcquireError::Backend(
        "lock contention outlasted the retries".to_string(),
    ))
}

/// Release the lock, only when `holder_task_id` is the holder. Anything else
/// (another task, an absent or already released lock, a lost CAS) is a no-op;
/// returns whether this call released it.
pub async fn release(backend: &dyn TaskBackend, holder_task_id: &str) -> bool {
    for _ in 0..2 {
        let Ok(Some(record)) = backend.get(LOCK_OWNER, LOCK_TASK_ID).await else {
            return false;
        };
        let Some(state) = decode(&record) else {
            return false;
        };
        if state.holder != holder_task_id || state.released {
            return false;
        }
        let released = encode(
            &state.holder,
            &state.holder_owner,
            TaskStatus::Completed,
            state.acquired_at_secs,
            state.lease_expires_at_secs,
            record.version + 1,
        );
        match backend
            .cas(LOCK_OWNER, LOCK_TASK_ID, record.version, released)
            .await
        {
            Ok(()) => return true,
            Err(BackendError::Conflict) => {}
            Err(_) => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aprender_mcp_setfit_train::InMemoryTaskBackend;

    use super::*;

    const OWNER: &str = "owner-1";

    fn backend() -> Arc<InMemoryTaskBackend> {
        Arc::new(InMemoryTaskBackend::new())
    }

    /// Put a Working task record, as `mint_for_request` would before an acquire.
    async fn mint(backend: &dyn TaskBackend, id: &str) {
        let record = StoredTask {
            task: Task::new(id, TaskStatus::Working),
            result: None,
            envelope: None,
            version: 1,
            expires_at_secs: None,
        };
        backend.put_new(OWNER, record).await.expect("mint");
    }

    async fn finish(backend: &dyn TaskBackend, id: &str) {
        let mut record = backend.get(OWNER, id).await.expect("get").expect("record");
        let expected = record.version;
        record.task.status = TaskStatus::Completed;
        record.version += 1;
        backend.cas(OWNER, id, expected, record).await.expect("cas");
    }

    #[test]
    fn lock_owner_is_printable_and_unforgeable() {
        assert_eq!(LOCK_OWNER, "laya-train#single-flight");
        assert!(LOCK_OWNER.contains('#'));
        assert!(LOCK_OWNER.bytes().all(|b| b.is_ascii_graphic()));
    }

    #[tokio::test]
    async fn lock_acquire_on_empty_then_held_by_second() {
        let b = backend();
        mint(b.as_ref(), "T1").await;
        mint(b.as_ref(), "T2").await;
        acquire(b.as_ref(), "T1", OWNER, 1_000)
            .await
            .expect("T1 takes it");
        assert_eq!(peek_at(b.as_ref(), 1_001).await.as_deref(), Some("T1"));
        let err = acquire(b.as_ref(), "T2", OWNER, 1_001)
            .await
            .expect_err("held");
        assert_eq!(
            err,
            AcquireError::Held(Held {
                holder: "T1".into()
            })
        );
    }

    #[tokio::test]
    async fn lock_holder_owner_is_recorded() {
        let b = backend();
        mint(b.as_ref(), "T1").await;
        acquire(b.as_ref(), "T1", OWNER, 1_000)
            .await
            .expect("acquire");
        let state = read(b.as_ref()).await.expect("state");
        assert_eq!(state.holder, "T1");
        assert_eq!(state.holder_owner, OWNER);
        assert_eq!(state.acquired_at_secs, 1_000);
        assert_eq!(state.lease_expires_at_secs, 1_000 + LOCK_LEASE_SECS);
        assert!(!state.released);
    }

    #[tokio::test]
    async fn lock_expired_lease_is_taken_over() {
        let b = backend();
        mint(b.as_ref(), "T1").await;
        mint(b.as_ref(), "T2").await;
        acquire(b.as_ref(), "T1", OWNER, 1_000).await.expect("T1");
        let past_lease = 1_000 + LOCK_LEASE_SECS;
        assert_eq!(peek_at(b.as_ref(), past_lease).await, None);
        acquire(b.as_ref(), "T2", OWNER, past_lease)
            .await
            .expect("takeover");
        assert_eq!(read(b.as_ref()).await.expect("state").holder, "T2");
    }

    #[tokio::test]
    async fn lock_released_then_acquirable() {
        let b = backend();
        mint(b.as_ref(), "T1").await;
        mint(b.as_ref(), "T2").await;
        acquire(b.as_ref(), "T1", OWNER, 1_000).await.expect("T1");
        assert!(release(b.as_ref(), "T1").await);
        assert_eq!(peek_at(b.as_ref(), 1_001).await, None);
        acquire(b.as_ref(), "T2", OWNER, 1_001)
            .await
            .expect("T2 after release");
    }

    #[tokio::test]
    async fn lock_terminal_holder_keeps_the_lock_until_it_is_released() {
        // A cancelled (terminal) task's compute may still be running: the lock is
        // the finalizer's to release, not something a terminal status frees.
        let b = backend();
        mint(b.as_ref(), "T1").await;
        mint(b.as_ref(), "T2").await;
        acquire(b.as_ref(), "T1", OWNER, 1_000).await.expect("T1");
        finish(b.as_ref(), "T1").await;
        assert_eq!(peek_at(b.as_ref(), 1_001).await.as_deref(), Some("T1"));
        let err = acquire(b.as_ref(), "T2", OWNER, 1_001)
            .await
            .expect_err("a terminal holder still holds the lock");
        assert!(matches!(err, AcquireError::Held(_)), "{err:?}");
        assert!(release(b.as_ref(), "T1").await);
        acquire(b.as_ref(), "T2", OWNER, 1_002)
            .await
            .expect("T2 after the release");
    }

    #[tokio::test]
    async fn lock_holder_with_a_gone_record_is_taken_over() {
        let b = backend();
        mint(b.as_ref(), "T2").await;
        // T1 never had a task record (expired or lost): nothing can release it.
        acquire(b.as_ref(), "T1", OWNER, 1_000).await.expect("T1");
        assert_eq!(peek_at(b.as_ref(), 1_001).await, None);
        acquire(b.as_ref(), "T2", OWNER, 1_001)
            .await
            .expect("takeover");
    }

    #[tokio::test]
    async fn lock_release_by_non_holder_is_a_noop() {
        let b = backend();
        mint(b.as_ref(), "T1").await;
        acquire(b.as_ref(), "T1", OWNER, 1_000).await.expect("T1");
        assert!(!release(b.as_ref(), "T2").await);
        assert_eq!(peek_at(b.as_ref(), 1_001).await.as_deref(), Some("T1"));
        assert!(
            release(b.as_ref(), "T1").await,
            "the holder can still release"
        );
        assert_eq!(peek_at(b.as_ref(), 1_001).await, None);
    }

    #[tokio::test]
    async fn lock_racing_acquires_have_exactly_one_winner() {
        for _ in 0..20 {
            let b = backend();
            mint(b.as_ref(), "T1").await;
            mint(b.as_ref(), "T2").await;
            let (b1, b2) = (Arc::clone(&b), Arc::clone(&b));
            let a = tokio::spawn(async move { acquire(b1.as_ref(), "T1", OWNER, 1_000).await });
            let c = tokio::spawn(async move { acquire(b2.as_ref(), "T2", OWNER, 1_000).await });
            let (ra, rc) = (a.await.expect("join"), c.await.expect("join"));
            assert_eq!(
                u8::from(ra.is_ok()) + u8::from(rc.is_ok()),
                1,
                "exactly one winner: {ra:?} {rc:?}"
            );
            let loser = if ra.is_ok() { rc } else { ra };
            assert!(matches!(loser, Err(AcquireError::Held(_))), "{loser:?}");
        }
    }

    #[tokio::test]
    async fn lock_record_never_carries_a_backend_ttl() {
        let b = backend();
        mint(b.as_ref(), "T1").await;
        mint(b.as_ref(), "T2").await;
        let ttl = |b: &Arc<InMemoryTaskBackend>| {
            let b = Arc::clone(b);
            async move {
                b.get(LOCK_OWNER, LOCK_TASK_ID)
                    .await
                    .expect("get")
                    .expect("record")
                    .expires_at_secs
            }
        };
        acquire(b.as_ref(), "T1", OWNER, 1_000).await.expect("T1");
        assert_eq!(ttl(&b).await, None, "after acquire");
        acquire(b.as_ref(), "T2", OWNER, 1_000 + LOCK_LEASE_SECS)
            .await
            .expect("takeover");
        assert_eq!(ttl(&b).await, None, "after takeover");
        assert!(release(b.as_ref(), "T2").await);
        assert_eq!(ttl(&b).await, None, "after release");
    }
}
