use buckyos_api::{Event, SharedKEventRingBuffer, DEFAULT_RINGBUFFER_PATH_ENV};
use serde_json::json;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static RING_ENV_LOCK: Mutex<()> = Mutex::new(());

fn set_unique_ring_path(test_name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "buckyos_{}_{}_{}.shm",
        test_name,
        std::process::id(),
        nanos
    ));
    let _ = std::fs::remove_file(&path);
    std::env::set_var(DEFAULT_RINGBUFFER_PATH_ENV, &path);
    path
}

fn event(seq: u64) -> Event {
    Event {
        eventid: format!("/shared/ring/{}", seq),
        source_node: "node_a".to_string(),
        source_pid: std::process::id(),
        ingress_node: Some("node_a".to_string()),
        timestamp: seq,
        data: json!({ "seq": seq }),
    }
}

#[test]
fn shared_ring_delivers_first_event_from_late_producer() {
    let _guard = RING_ENV_LOCK.lock().unwrap();
    let path = set_unique_ring_path("late_producer");

    let consumer = SharedKEventRingBuffer::open().unwrap();
    consumer.prime_cursors();
    let producer = SharedKEventRingBuffer::open().unwrap();
    producer.publish_event(&event(1)).unwrap();

    let events = consumer.drain_events::<Event>(8);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].eventid, "/shared/ring/1");
    assert_eq!(events[0].data["seq"], json!(1));

    drop(producer);
    drop(consumer);
    let _ = std::fs::remove_file(path);
}

#[test]
fn shared_ring_overrun_drops_oldest_without_corrupting_events() {
    let _guard = RING_ENV_LOCK.lock().unwrap();
    let path = set_unique_ring_path("overrun");

    let producer = SharedKEventRingBuffer::open().unwrap();
    let consumer = SharedKEventRingBuffer::open().unwrap();
    consumer.prime_cursors();

    for seq in 0..700 {
        producer.publish_event(&event(seq)).unwrap();
    }

    let events = consumer.drain_events::<Event>(800);
    assert!(!events.is_empty());
    assert!(events.len() <= 512);
    let mut previous = None;
    for event in &events {
        let seq = event.data["seq"].as_u64().unwrap();
        if let Some(previous) = previous {
            assert!(seq > previous);
        }
        previous = Some(seq);
        assert!(event.eventid.starts_with("/shared/ring/"));
    }
    assert!(previous.unwrap() >= 699);

    drop(producer);
    drop(consumer);
    let _ = std::fs::remove_file(path);
}
