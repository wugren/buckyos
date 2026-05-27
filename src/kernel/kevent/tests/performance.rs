use buckyos_api::KEventClient;
use kevent::KEventService;
use serde_json::json;
use std::time::Instant;

#[tokio::test]
#[ignore]
async fn local_pub_sub_baseline() {
    let client = KEventClient::new_local("node_a");
    let reader = client
        .create_event_reader(vec!["perf_tick".to_string()])
        .await
        .unwrap();

    let start = Instant::now();
    for seq in 0..10_000 {
        client
            .pub_event("perf_tick", json!({ "seq": seq }))
            .await
            .unwrap();
    }
    let publish_elapsed = start.elapsed();

    let start = Instant::now();
    let mut consumed = 0usize;
    let mut last_seq = None;
    while let Some(event) = reader.pull_event(Some(0)).await.unwrap() {
        consumed += 1;
        last_seq = event.data["seq"].as_u64();
    }
    let consume_elapsed = start.elapsed();
    assert!(consumed > 0);
    assert_eq!(last_seq, Some(9_999));

    eprintln!(
        "{{\"kevent_local_publish_10k_ms\":{},\"kevent_local_consume_retained_ms\":{},\"kevent_local_retained\":{}}}",
        publish_elapsed.as_millis(),
        consume_elapsed.as_millis(),
        consumed
    );
}

#[tokio::test]
#[ignore]
async fn service_publish_pull_baseline() {
    let service = KEventService::new_with_capacity("node_a", 10_000);
    service
        .register_reader("perf", vec!["/perf/**".to_string()])
        .await
        .unwrap();

    let start = Instant::now();
    for seq in 0..10_000 {
        service
            .publish_local_global("/perf/event", json!({ "seq": seq }))
            .await
            .unwrap();
    }
    let publish_elapsed = start.elapsed();

    let start = Instant::now();
    for seq in 0..10_000 {
        let event = service.pull_event("perf", Some(1000)).await.unwrap().unwrap();
        assert_eq!(event.data["seq"], json!(seq));
    }
    let consume_elapsed = start.elapsed();

    eprintln!(
        "{{\"kevent_service_publish_10k_ms\":{},\"kevent_service_consume_10k_ms\":{}}}",
        publish_elapsed.as_millis(),
        consume_elapsed.as_millis()
    );
}
