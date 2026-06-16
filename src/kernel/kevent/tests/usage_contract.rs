use buckyos_api::{match_event_patterns, validate_eventid, validate_pattern, KEventClient};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
struct TruthRecord {
    id: String,
    state: &'static str,
}

#[derive(Default)]
struct TruthSource {
    records: BTreeMap<String, TruthRecord>,
    reads: usize,
}

impl TruthSource {
    fn insert(&mut self, id: &str, state: &'static str) {
        self.records.insert(
            id.to_string(),
            TruthRecord {
                id: id.to_string(),
                state,
            },
        );
    }

    fn read(&mut self, id: &str) -> Option<TruthRecord> {
        self.reads += 1;
        self.records.get(id).cloned()
    }
}

#[derive(Debug, Eq, PartialEq)]
enum TruthLocator {
    Task(String),
    MsgRecord(String),
    MsgBox { owner: String, box_name: String },
    Kmsg { queue_urn: String, index: u64 },
}

fn locator_from_payload(payload: &Value) -> Option<TruthLocator> {
    if let Some(task_id) = payload.get("task_id").and_then(Value::as_str) {
        return Some(TruthLocator::Task(task_id.to_string()));
    }

    if let Some(record_id) = payload.get("record_id").and_then(Value::as_str) {
        return Some(TruthLocator::MsgRecord(record_id.to_string()));
    }

    if let (Some(owner), Some(box_name)) = (
        payload.get("owner").and_then(Value::as_str),
        payload.get("box").and_then(Value::as_str),
    ) {
        return Some(TruthLocator::MsgBox {
            owner: owner.to_string(),
            box_name: box_name.to_string(),
        });
    }

    if let (Some(queue_urn), Some(index)) = (
        payload.get("queue_urn").and_then(Value::as_str),
        payload.get("index").and_then(Value::as_u64),
    ) {
        return Some(TruthLocator::Kmsg {
            queue_urn: queue_urn.to_string(),
            index,
        });
    }

    None
}

struct TruthDrivenConsumer {
    pending_ids: BTreeSet<String>,
    processed_ids: BTreeSet<String>,
}

impl TruthDrivenConsumer {
    fn new(pending_ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            pending_ids: pending_ids.into_iter().map(Into::into).collect(),
            processed_ids: BTreeSet::new(),
        }
    }

    fn tick(&mut self, event_payload: Option<&Value>, truth: &mut TruthSource) {
        if let Some(TruthLocator::Task(task_id)) = event_payload.and_then(locator_from_payload) {
            self.pending_ids.insert(task_id);
        }

        let pending_ids = self.pending_ids.iter().cloned().collect::<Vec<_>>();
        for id in pending_ids {
            if let Some(record) = truth.read(&id) {
                if record.state == "Completed" {
                    self.processed_ids.insert(record.id.clone());
                    self.pending_ids.remove(&record.id);
                }
            }
        }
    }
}

fn assert_patterns_match(patterns: &[String], eventid: &str) {
    validate_eventid(eventid).unwrap();
    for pattern in patterns {
        validate_pattern(pattern).unwrap();
    }
    assert!(
        match_event_patterns(patterns, eventid),
        "patterns {:?} should match eventid {}",
        patterns,
        eventid
    );
}

fn assert_patterns_do_not_match(patterns: &[String], eventid: &str) {
    validate_eventid(eventid).unwrap();
    for pattern in patterns {
        validate_pattern(pattern).unwrap();
    }
    assert!(
        !match_event_patterns(patterns, eventid),
        "patterns {:?} should not match eventid {}",
        patterns,
        eventid
    );
}

#[test]
fn msg_center_box_events_match_current_consumers() {
    let owner = "alice";
    let control_panel_patterns = vec![
        format!("/msg_center/{owner}/box/in/**"),
        format!("/msg_center/{owner}/box/out/**"),
    ];
    let opendan_patterns = [
        "in",
        "inbox",
        "group_in",
        "group_inbox",
        "request",
        "request_box",
    ]
    .into_iter()
    .flat_map(|box_name| {
        [
            format!("/msg_center/{owner}/box/{box_name}/**"),
            format!("/msg_center/{owner}/{box_name}/**"),
        ]
    })
    .collect::<Vec<_>>();

    assert_patterns_match(&control_panel_patterns, "/msg_center/alice/box/in/changed");
    assert_patterns_match(&control_panel_patterns, "/msg_center/alice/box/out/changed");
    assert_patterns_do_not_match(
        &control_panel_patterns,
        "/msg_center/alice/box/group_in/changed",
    );

    for eventid in [
        "/msg_center/alice/box/in/changed",
        "/msg_center/alice/box/inbox/changed",
        "/msg_center/alice/box/group_in/changed",
        "/msg_center/alice/box/group_inbox/changed",
        "/msg_center/alice/box/request/changed",
        "/msg_center/alice/box/request_box/changed",
        "/msg_center/alice/in/changed",
        "/msg_center/alice/group_in/changed",
        "/msg_center/alice/request/changed",
    ] {
        assert_patterns_match(&opendan_patterns, eventid);
    }
}

#[test]
fn task_manager_wait_pattern_matches_task_events_only() {
    let task_id = 42;
    let wait_patterns = vec![format!("/task_mgr/{task_id}")];

    assert_patterns_match(&wait_patterns, "/task_mgr/42");
    assert_patterns_do_not_match(&wait_patterns, "/task_mgr/43");
    assert_patterns_do_not_match(&wait_patterns, "/task_mgr/42/done");
}

#[test]
fn event_payload_carries_locator_not_truth() {
    assert_eq!(
        locator_from_payload(&json!({
            "task_id": "task-42",
            "state": "Failed"
        })),
        Some(TruthLocator::Task("task-42".to_string()))
    );
    assert_eq!(
        locator_from_payload(&json!({
            "record_id": "msg-001",
            "state": "Deleted"
        })),
        Some(TruthLocator::MsgRecord("msg-001".to_string()))
    );
    assert_eq!(
        locator_from_payload(&json!({
            "owner": "alice",
            "box": "in",
            "changed": true
        })),
        Some(TruthLocator::MsgBox {
            owner: "alice".to_string(),
            box_name: "in".to_string()
        })
    );
    assert_eq!(
        locator_from_payload(&json!({
            "queue_urn": "buckycli::devtest::queue",
            "index": 7,
            "payload_preview": "not authoritative"
        })),
        Some(TruthLocator::Kmsg {
            queue_urn: "buckycli::devtest::queue".to_string(),
            index: 7
        })
    );
    assert!(locator_from_payload(&json!({ "state": "Completed" })).is_none());
}

#[test]
fn event_wakeup_reloads_truth_source_instead_of_trusting_payload() {
    let mut truth = TruthSource::default();
    truth.insert("task-42", "Completed");
    let mut consumer = TruthDrivenConsumer::new(Vec::<String>::new());

    consumer.tick(
        Some(&json!({
            "task_id": "task-42",
            "state": "Running",
            "result": "stale event payload"
        })),
        &mut truth,
    );

    assert_eq!(truth.reads, 1);
    assert!(consumer.processed_ids.contains("task-42"));
    assert!(consumer.pending_ids.is_empty());
}

#[test]
fn timeout_duplicate_and_bad_events_still_converge_by_truth_source() {
    let mut truth = TruthSource::default();
    truth.insert("task-timeout", "Completed");
    truth.insert("task-duplicate", "Completed");
    truth.insert("task-bad-payload", "Completed");
    let mut consumer =
        TruthDrivenConsumer::new(["task-timeout", "task-duplicate", "task-bad-payload"]);

    consumer.tick(None, &mut truth);
    assert!(consumer.processed_ids.contains("task-timeout"));

    consumer.tick(
        Some(&json!({ "task_id": "task-duplicate", "state": "Running" })),
        &mut truth,
    );
    consumer.tick(
        Some(&json!({ "task_id": "task-duplicate", "state": "Running" })),
        &mut truth,
    );
    assert!(consumer.processed_ids.contains("task-duplicate"));

    consumer.tick(Some(&json!({ "state": "Completed" })), &mut truth);
    assert!(consumer.processed_ids.contains("task-bad-payload"));
    assert!(consumer.pending_ids.is_empty());
}

#[tokio::test]
async fn dynamic_readers_fanout_unsubscribe_and_rebuild() {
    let client = KEventClient::new_local("usage_contract");
    let reader_a = client
        .create_event_reader(vec![
            "session_event".to_string(),
            "session_event".to_string(),
        ])
        .await
        .unwrap();
    let reader_b = client
        .create_event_reader(vec!["other_event".to_string()])
        .await
        .unwrap();
    reader_b
        .add_patterns(vec!["session_event".to_string()])
        .await
        .unwrap();

    client
        .pub_event("session_event", json!({ "seq": 1 }))
        .await
        .unwrap();
    assert_eq!(
        reader_a.pull_event(Some(0)).await.unwrap().unwrap().data["seq"],
        json!(1)
    );
    assert_eq!(
        reader_b.pull_event(Some(0)).await.unwrap().unwrap().data["seq"],
        json!(1)
    );
    assert!(reader_a.pull_event(Some(0)).await.unwrap().is_none());
    assert!(reader_b.pull_event(Some(0)).await.unwrap().is_none());

    reader_b
        .remove_patterns(vec!["session_event".to_string()])
        .await
        .unwrap();
    client
        .pub_event("session_event", json!({ "seq": 2 }))
        .await
        .unwrap();
    assert_eq!(
        reader_a.pull_event(Some(0)).await.unwrap().unwrap().data["seq"],
        json!(2)
    );
    assert!(reader_b.pull_event(Some(0)).await.unwrap().is_none());

    reader_a.close().await.unwrap();
    client
        .pub_event("session_event", json!({ "seq": 3 }))
        .await
        .unwrap();
    let reader_c = client
        .create_event_reader(vec!["session_event".to_string()])
        .await
        .unwrap();
    assert!(reader_c.pull_event(Some(0)).await.unwrap().is_none());

    client
        .pub_event("session_event", json!({ "seq": 4 }))
        .await
        .unwrap();
    assert_eq!(
        reader_c.pull_event(Some(0)).await.unwrap().unwrap().data["seq"],
        json!(4)
    );
}
