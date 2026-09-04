//! [`DynamoDbTaskBackend`] — task records that outlive a container.
//!
//! The whole point of the `TaskBackend` seam: `AprenderTaskStore`, the mint
//! handoff, the envelope side channel and the guarded terminal write are
//! already written and already tested against `InMemoryTaskBackend`. This file
//! only has to answer five questions durably.
//!
//! # Records are stored as JSON, not as attribute maps
//!
//! `Task` is `#[non_exhaustive]` and the SDK says so explicitly — it has grown
//! `diagnostic_detail` already and documents where the next field will go. A
//! hand-written attribute-map mapping would silently drop every field added
//! after it was written, and the loss would show up as a task that came back
//! from DynamoDB missing its status message rather than as a compile error.
//! Serializing the record with serde makes the SDK's own derives the mapping,
//! so a new field round-trips the day it appears.
//!
//! The cost is that nothing but the key attributes is queryable. That is the
//! right trade here: every access is by `(owner_id, task_id)` or a query of one
//! owner's partition, and there is no access pattern that wants to filter on a
//! task's interior.
//!
//! # Reads are CONSISTENT
//!
//! The worker's first act is to read an envelope the request function wrote
//! seconds earlier, from a different host. An eventually-consistent read is
//! allowed to miss that write, which would surface as a training run that
//! failed because its config "did not exist". `consistent_read(true)` on every
//! `GetItem` costs twice the read units on an on-demand table where a task
//! record is read a handful of times, and removes the whole class.
//!
//! # Expiry is enforced twice on purpose
//!
//! DynamoDB's TTL deletes items on its own schedule, typically within 48 hours,
//! so an expired record really can come back from a read. `is_live` filters
//! here, and `AprenderTaskStore::load` filters again on top. Neither is
//! redundant: the store's filter is what makes the SDK-facing contract exact,
//! and this one keeps an expired record from being handed to a CAS that would
//! resurrect it.

use std::collections::HashMap;

use aprender_mcp_setfit_train::{BackendError, StoredTask, TaskBackend};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use pmcp::async_trait;
use pmcp::server::task_store::TaskStoreError;

/// The largest record this backend will write.
///
/// DynamoDB's hard item limit is 400 KB; this leaves room for the key and
/// version attributes and for JSON growing slightly under serde. It matters
/// because the training server's transport bound on a client config is 1 MiB —
/// larger than DynamoDB's item limit — so a config between roughly 380 KB and
/// 1 MiB passes the MCP boundary and can only be refused here. Refusing it with
/// a message that says WHY beats a raw `ValidationException`.
///
/// In practice a SetFit training config is a twelve-knob object of a few
/// hundred bytes, so this ceiling is three orders of magnitude clear of real
/// traffic and no client should ever meet it.
pub const MAX_ITEM_BYTES: usize = 380_000;

const ATTR_OWNER: &str = "owner_id";
const ATTR_TASK_ID: &str = "task_id";
const ATTR_VERSION: &str = "version";
const ATTR_EXPIRES: &str = "expires_at";
const ATTR_TASK: &str = "task";
const ATTR_RESULT: &str = "result";
const ATTR_ENVELOPE: &str = "envelope";

/// `owner_id`, `task_id` and `version` all appear in condition expressions, and
/// `VERSION` is on DynamoDB's reserved-word list. Aliasing all three rather
/// than only the one that bites today means a later expression cannot
/// reintroduce the problem by naming a word that becomes reserved.
fn attr_names() -> HashMap<String, String> {
    [
        ("#owner", ATTR_OWNER),
        ("#tid", ATTR_TASK_ID),
        ("#ver", ATTR_VERSION),
    ]
    .into_iter()
    .map(|(alias, name)| (alias.to_string(), name.to_string()))
    .collect()
}

fn internal(message: impl Into<String>) -> BackendError {
    BackendError::Store(TaskStoreError::Internal {
        message: message.into(),
    })
}

/// A record is live when it has no expiry or its expiry is still ahead.
fn is_live(record: &StoredTask, now_secs: u64) -> bool {
    record.expires_at_secs.is_none_or(|e| now_secs <= e)
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Render a record as a DynamoDB item, refusing one too large to store.
fn to_item(
    owner_id: &str,
    record: &StoredTask,
) -> Result<HashMap<String, AttributeValue>, BackendError> {
    let task = serde_json::to_string(&record.task)
        .map_err(|e| internal(format!("cannot serialize task: {e}")))?;
    let mut item: HashMap<String, AttributeValue> = HashMap::new();
    let mut total = task.len();
    item.insert(
        ATTR_OWNER.to_string(),
        AttributeValue::S(owner_id.to_string()),
    );
    item.insert(
        ATTR_TASK_ID.to_string(),
        AttributeValue::S(record.task.task_id.clone()),
    );
    item.insert(
        ATTR_VERSION.to_string(),
        AttributeValue::N(record.version.to_string()),
    );
    item.insert(ATTR_TASK.to_string(), AttributeValue::S(task));
    if let Some(result) = record.result.as_ref() {
        let json = serde_json::to_string(result)
            .map_err(|e| internal(format!("cannot serialize task result: {e}")))?;
        total += json.len();
        item.insert(ATTR_RESULT.to_string(), AttributeValue::S(json));
    }
    if let Some(envelope) = record.envelope.as_ref() {
        let json = serde_json::to_string(envelope)
            .map_err(|e| internal(format!("cannot serialize run envelope: {e}")))?;
        total += json.len();
        item.insert(ATTR_ENVELOPE.to_string(), AttributeValue::S(json));
    }
    if let Some(expires) = record.expires_at_secs {
        item.insert(
            ATTR_EXPIRES.to_string(),
            AttributeValue::N(expires.to_string()),
        );
    }
    if total > MAX_ITEM_BYTES {
        return Err(internal(format!(
            "this task record is {total} bytes, over the {MAX_ITEM_BYTES}-byte ceiling a \
             DynamoDB item allows; the training config is the only part a client controls, \
             so send a smaller one"
        )));
    }
    Ok(item)
}

fn read_str(item: &HashMap<String, AttributeValue>, key: &str) -> Option<String> {
    item.get(key).and_then(|v| v.as_s().ok()).cloned()
}

/// Parse a DynamoDB item back into a record.
fn from_item(item: &HashMap<String, AttributeValue>) -> Result<StoredTask, BackendError> {
    let task_json = read_str(item, ATTR_TASK)
        .ok_or_else(|| internal(format!("task record has no `{ATTR_TASK}` attribute")))?;
    let task = serde_json::from_str(&task_json)
        .map_err(|e| internal(format!("cannot parse stored task: {e}")))?;
    let result = read_str(item, ATTR_RESULT)
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|e| internal(format!("cannot parse stored task result: {e}")))?;
    let envelope = read_str(item, ATTR_ENVELOPE)
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|e| internal(format!("cannot parse stored run envelope: {e}")))?;
    let number = |key: &str| -> Option<u64> {
        item.get(key)
            .and_then(|v| v.as_n().ok())
            .and_then(|n| n.parse().ok())
    };
    Ok(StoredTask {
        task,
        result,
        envelope,
        // A record with no readable version cannot be safely compare-and-swapped
        // against, so refuse rather than guess a number that would make the next
        // write unconditional.
        version: number(ATTR_VERSION)
            .ok_or_else(|| internal(format!("task record has no `{ATTR_VERSION}` attribute")))?,
        expires_at_secs: number(ATTR_EXPIRES),
    })
}

/// Task records in DynamoDB, keyed `(owner_id, task_id)`.
#[derive(Debug, Clone)]
pub struct DynamoDbTaskBackend {
    client: Client,
    table: String,
}

impl DynamoDbTaskBackend {
    /// Bind to a table. Does not check that it exists — a Lambda's cold start
    /// is not the place for a `DescribeTable` round trip, and the first real
    /// operation reports a missing table clearly enough.
    #[must_use]
    pub fn new(client: Client, table: impl Into<String>) -> Self {
        Self {
            client,
            table: table.into(),
        }
    }

    /// Build one from the ambient AWS configuration.
    ///
    /// # Errors
    ///
    /// When [`crate::ENV_TASKS_TABLE`] is unset.
    pub async fn from_env() -> Result<Self, String> {
        let table = crate::require_env(crate::ENV_TASKS_TABLE)?;
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        Ok(Self::new(Client::new(&config), table))
    }

    fn key(owner_id: &str, task_id: &str) -> [(String, AttributeValue); 2] {
        [
            (
                ATTR_OWNER.to_string(),
                AttributeValue::S(owner_id.to_string()),
            ),
            (
                ATTR_TASK_ID.to_string(),
                AttributeValue::S(task_id.to_string()),
            ),
        ]
    }
}

#[async_trait]
impl TaskBackend for DynamoDbTaskBackend {
    async fn put_new(&self, owner_id: &str, record: StoredTask) -> Result<(), BackendError> {
        let item = to_item(owner_id, &record)?;
        self.client
            .put_item()
            .table_name(&self.table)
            .set_item(Some(item))
            // Minting generates a fresh id, so an existing item here is an
            // id-collision bug rather than a normal race — but it must never be
            // an overwrite, which is what an unconditional PutItem would be.
            .condition_expression("attribute_not_exists(#owner) AND attribute_not_exists(#tid)")
            .set_expression_attribute_names(Some(attr_names()))
            .send()
            .await
            .map_err(|e| {
                if e.as_service_error()
                    .is_some_and(aws_sdk_dynamodb::operation::put_item::PutItemError::is_conditional_check_failed_exception)
                {
                    BackendError::Conflict
                } else {
                    internal(format!("DynamoDB PutItem failed: {e}"))
                }
            })?;
        Ok(())
    }

    async fn get(&self, owner_id: &str, task_id: &str) -> Result<Option<StoredTask>, BackendError> {
        let mut request = self
            .client
            .get_item()
            .table_name(&self.table)
            // See the module doc: the worker reads an envelope written seconds
            // earlier on another host, and a stale read there is a training run
            // that fails for a config that does exist.
            .consistent_read(true);
        for (name, value) in Self::key(owner_id, task_id) {
            request = request.key(name, value);
        }
        let output = request
            .send()
            .await
            .map_err(|e| internal(format!("DynamoDB GetItem failed: {e}")))?;
        let Some(item) = output.item else {
            return Ok(None);
        };
        let record = from_item(&item)?;
        Ok(is_live(&record, now_epoch_secs()).then_some(record))
    }

    async fn cas(
        &self,
        owner_id: &str,
        task_id: &str,
        expected_version: u64,
        record: StoredTask,
    ) -> Result<(), BackendError> {
        // The item carries its own key, so a record whose id disagrees with the
        // one being CAS'd would condition on one item and write another. It
        // cannot happen through `AprenderTaskStore`, and that is exactly why it
        // would be invisible if it ever started to.
        if record.task.task_id != task_id {
            return Err(internal(format!(
                "refusing to compare-and-swap {task_id} with a record carrying id {}",
                record.task.task_id
            )));
        }
        let item = to_item(owner_id, &record)?;
        self.client
            .put_item()
            .table_name(&self.table)
            .set_item(Some(item))
            .condition_expression("#ver = :expected")
            .set_expression_attribute_names(Some(attr_names()))
            .expression_attribute_values(
                ":expected",
                AttributeValue::N(expected_version.to_string()),
            )
            .send()
            .await
            .map_err(|e| {
                // A vanished item fails this condition too, which is correct:
                // the store retries once and then reads NotFound, rather than
                // resurrecting a record TTL already reclaimed.
                if e.as_service_error()
                    .is_some_and(aws_sdk_dynamodb::operation::put_item::PutItemError::is_conditional_check_failed_exception)
                {
                    BackendError::Conflict
                } else {
                    internal(format!("DynamoDB conditional PutItem failed: {e}"))
                }
            })?;
        Ok(())
    }

    async fn list(
        &self,
        owner_id: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<StoredTask>, Option<String>), BackendError> {
        let mut request = self
            .client
            .query()
            .table_name(&self.table)
            .key_condition_expression("#owner = :owner")
            .set_expression_attribute_names(Some(attr_names()))
            .expression_attribute_values(":owner", AttributeValue::S(owner_id.to_string()))
            // Newest first, matching the in-memory backend and the SDK's own
            // store. Task ids are random, so this orders by id, not by time —
            // stated rather than implied, because the in-memory backend sorts by
            // `created_at` and a caller must not read ordering into either.
            .scan_index_forward(false);
        if let Some(cursor) = cursor {
            for (name, value) in Self::key(owner_id, cursor) {
                request = request.exclusive_start_key(name, value);
            }
        }
        let output = request
            .send()
            .await
            .map_err(|e| internal(format!("DynamoDB Query failed: {e}")))?;
        let now = now_epoch_secs();
        let mut records = Vec::new();
        for item in output.items.unwrap_or_default() {
            let record = from_item(&item)?;
            if is_live(&record, now) {
                records.push(record);
            }
        }
        // The cursor is just the last key's task id: the partition is fixed by
        // the owner, so nothing else in `LastEvaluatedKey` is information, and
        // an opaque blob would only be a base64 round trip over one string.
        let next = output
            .last_evaluated_key
            .as_ref()
            .and_then(|key| read_str(key, ATTR_TASK_ID));
        Ok((records, next))
    }

    async fn sweep_expired(&self) -> Result<usize, BackendError> {
        // DynamoDB's own TTL reclaims these, and a Scan-and-delete sweep from a
        // request-path Lambda would cost far more than the storage it frees.
        // Correctness does not depend on it: expired records are filtered on
        // read, here and again in the store.
        Ok(0)
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod tests {
    use super::*;
    use pmcp::types::tasks::Task;
    use pmcp::types::{CallToolResult, Content, TaskStatus};

    fn record(version: u64, expires: Option<u64>) -> StoredTask {
        StoredTask {
            task: Task::new("t-1", TaskStatus::Working)
                .with_ttl(1000)
                .with_timestamps("2026-09-03T00:00:00Z", "2026-09-03T00:00:00Z")
                .with_status_message("running"),
            result: Some(CallToolResult::new(vec![Content::Text {
                text: "{\"phase\":\"completed\"}".to_string(),
            }])),
            envelope: Some(serde_json::json!({ "config": { "shots": 8 } })),
            version,
            expires_at_secs: expires,
        }
    }

    #[test]
    fn a_record_round_trips_through_an_item() {
        let original = record(7, Some(1_800_000_000));
        let item = to_item("owner-1", &original).expect("encodes");
        let back = from_item(&item).expect("decodes");
        assert_eq!(back.version, 7);
        assert_eq!(back.expires_at_secs, Some(1_800_000_000));
        assert_eq!(back.task.task_id, "t-1");
        assert_eq!(back.task.status, TaskStatus::Working);
        // A field the SDK added after this mapping was written must survive —
        // that is the whole reason the record is stored as serde JSON.
        assert_eq!(back.task.status_message.as_deref(), Some("running"));
        assert_eq!(back.envelope.expect("envelope")["config"]["shots"], 8);
    }

    #[test]
    fn the_key_attributes_are_the_tables_key() {
        let item = to_item("owner-1", &record(1, None)).expect("encodes");
        assert_eq!(item[ATTR_OWNER].as_s().expect("s"), "owner-1");
        assert_eq!(item[ATTR_TASK_ID].as_s().expect("s"), "t-1");
        assert!(
            !item.contains_key(ATTR_EXPIRES),
            "a task with no TTL must not get an expires_at, or DynamoDB TTL \
             would read a missing attribute as never-expiring by accident \
             rather than by design"
        );
    }

    #[test]
    fn an_oversized_record_is_refused_with_the_reason() {
        let mut oversized = record(1, None);
        oversized.envelope = Some(serde_json::json!({ "config": "x".repeat(MAX_ITEM_BYTES) }));
        let err = to_item("owner-1", &oversized).expect_err("must refuse");
        let message = err.to_string();
        assert!(message.contains("DynamoDB item"), "{message}");
        assert!(message.contains("config"), "{message}");
    }

    #[test]
    fn a_record_with_no_version_is_refused_rather_than_defaulted() {
        let mut item = to_item("owner-1", &record(3, None)).expect("encodes");
        item.remove(ATTR_VERSION);
        from_item(&item).expect_err("a versionless record cannot be safely CAS'd");
    }

    #[test]
    fn liveness_is_decided_against_the_expiry_second() {
        let expired = record(1, Some(100));
        assert!(is_live(&expired, 100), "the expiry second itself is live");
        assert!(!is_live(&expired, 101));
        assert!(is_live(&record(1, None), u64::MAX), "no TTL never expires");
    }

    #[test]
    fn every_expression_name_is_aliased() {
        let names = attr_names();
        // VERSION is a DynamoDB reserved word; an unaliased `version = :x`
        // condition fails at runtime with a parse error, not at compile time.
        assert_eq!(names["#ver"], ATTR_VERSION);
        assert_eq!(names["#owner"], ATTR_OWNER);
        assert_eq!(names["#tid"], ATTR_TASK_ID);
    }
}
