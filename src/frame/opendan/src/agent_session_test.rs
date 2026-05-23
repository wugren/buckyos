use super::*;

fn todo_json(
    todo_id: &str,
    status: &str,
    title: &str,
    content: &str,
    skills: Vec<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "todo_id": todo_id,
        "session_id": "s-1",
        "order_index": 0,
        "status": status,
        "title": title,
        "content": content,
        "skills": skills,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
    })
}

#[test]
fn compose_human_text_skips_empties() {
    let v = vec!["  ".to_string(), "hello".to_string(), "".to_string()];
    assert_eq!(compose_human_text(&v).as_deref(), Some("hello"));
}

#[test]
fn compose_human_text_joins() {
    let v = vec!["a".to_string(), "b".to_string()];
    assert_eq!(compose_human_text(&v).as_deref(), Some("a\n\nb"));
}

#[test]
fn compose_turn_message_preserves_structured_blocks() {
    let msg = AiMessage::new(
        AiRole::User,
        vec![
            AiContent::text("see this"),
            AiContent::Image {
                source: buckyos_api::ResourceRef::url(
                    "https://example.test/a.png".to_string(),
                    Some("image/png".to_string()),
                ),
            },
        ],
    );
    let out = compose_turn_message(&[msg], Some("environment".to_string())).unwrap();
    assert_eq!(out.role, AiRole::User);
    assert_eq!(out.content.len(), 2);
    assert_eq!(
        out.text_content(),
        "<background_environment>\nenvironment\n</background_environment>\n\nsee this"
    );
    assert!(matches!(out.content[1], AiContent::Image { .. }));
}

#[test]
fn append_turn_message_preserves_behavior_step_records() {
    let request = LLMContextRequest {
        owner: ContextOwnerRef::Agent {
            session_id: "s-1".to_string(),
        },
        trace: Some("old-trace".to_string()),
        objective: "objective".to_string(),
        behavior_name: "do".to_string(),
        input: vec![
            AiMessage::text(AiRole::System, "system"),
            AiMessage::text(AiRole::User, "initial"),
        ],
        model_policy: Default::default(),
        tool_policy: Default::default(),
        output: Default::default(),
        budget: Default::default(),
        human_policy: Default::default(),
        error_policy: Default::default(),
        forbid_next_behavior: false,
    };
    let mut state = LLMContextState::from_request(&request, 1);
    state.rounds_left = 0;
    state.steps.push(llm_context::behavior_loop::StepRecord {
        meta: llm_context::behavior_loop::StepMeta {
            behavior_name: "plan".to_string(),
            step_index: 0,
            started_at_ms: 10,
            ended_at_ms: Some(20),
            compression_level: Default::default(),
        },
        assistant_text: "<response><actions><exec_bash>todo add \"first\"</exec_bash></actions><next_behavior>DO</next_behavior></response>".to_string(),
        thought: Some("planned todos".to_string()),
        actions: vec![buckyos_api::AiToolCall {
            name: "exec_bash".to_string(),
            args: std::collections::HashMap::new(),
            call_id: "call-1".to_string(),
        }],
        action_results: vec![Observation::Success {
            call_id: "call-1".to_string(),
            content: serde_json::json!({"ok": true}),
            bytes: 12,
            truncated: false,
            tool_result: None,
        }],
        ..Default::default()
    });
    state.next_step_index = 1;

    let snapshot = LLMContextSnapshot { request, state };
    let out = append_turn_message_to_snapshot(
        snapshot,
        Some(AiMessage::text(AiRole::User, "continue task")),
        Vec::new(),
        "new-trace",
        true,
    );

    assert_eq!(out.request.trace.as_deref(), Some("new-trace"));
    assert_eq!(out.request.input.len(), 2);
    assert_eq!(out.state.accumulated.len(), 3);
    assert_eq!(out.state.accumulated[2].text_content(), "continue task");
    assert_eq!(out.state.steps.len(), 1);
    assert!(out.state.steps[0].assistant_text.contains("todo add"));
    assert_eq!(out.state.steps[0].meta.behavior_name, "plan");
    assert!(
        out.state.last_step.is_none(),
        "cross-behavior inherited steps must not become the new behavior hot tail"
    );
    assert_eq!(out.state.next_step_index, 1);
    assert_eq!(out.state.rounds_left, out.request.tool_policy.max_rounds);
}

#[test]
fn append_turn_message_promotes_current_behavior_step_as_hot_tail() {
    let request = LLMContextRequest {
        owner: ContextOwnerRef::Agent {
            session_id: "s-1".to_string(),
        },
        trace: Some("old-trace".to_string()),
        objective: "objective".to_string(),
        behavior_name: "do".to_string(),
        input: vec![
            AiMessage::text(AiRole::System, "system"),
            AiMessage::text(AiRole::User, "initial"),
        ],
        model_policy: Default::default(),
        tool_policy: Default::default(),
        output: Default::default(),
        budget: Default::default(),
        human_policy: Default::default(),
        error_policy: Default::default(),
        forbid_next_behavior: false,
    };
    let mut state = LLMContextState::from_request(&request, 1);
    state.steps.push(llm_context::behavior_loop::StepRecord {
        meta: llm_context::behavior_loop::StepMeta {
            behavior_name: "do".to_string(),
            step_index: 0,
            started_at_ms: 10,
            ended_at_ms: Some(20),
            compression_level: Default::default(),
        },
        assistant_text: "<response><thinking>do work</thinking></response>".to_string(),
        thought: Some("do work".to_string()),
        ..Default::default()
    });
    state.next_step_index = 1;

    let snapshot = LLMContextSnapshot { request, state };
    let out = append_turn_message_to_snapshot(
        snapshot,
        Some(AiMessage::text(AiRole::User, "continue same behavior")),
        Vec::new(),
        "new-trace",
        true,
    );

    assert!(out.state.steps.is_empty());
    assert_eq!(
        out.state
            .last_step
            .as_ref()
            .map(|step| step.meta.behavior_name.as_str()),
        Some("do")
    );
    assert_eq!(out.state.next_step_index, 1);
}

#[test]
fn append_process_end_history_input_preserves_steps_without_user_tail() {
    let request = LLMContextRequest {
        owner: ContextOwnerRef::Agent {
            session_id: "s-1".to_string(),
        },
        trace: Some("old-trace".to_string()),
        objective: "objective".to_string(),
        behavior_name: "do".to_string(),
        input: vec![
            AiMessage::text(AiRole::System, "system"),
            AiMessage::text(AiRole::User, "initial"),
        ],
        model_policy: Default::default(),
        tool_policy: Default::default(),
        output: Default::default(),
        budget: Default::default(),
        human_policy: Default::default(),
        error_policy: Default::default(),
        forbid_next_behavior: false,
    };
    let mut state = LLMContextState::from_request(&request, 1);
    state.steps.push(llm_context::behavior_loop::StepRecord {
        meta: llm_context::behavior_loop::StepMeta {
            behavior_name: "plan".to_string(),
            step_index: 0,
            started_at_ms: 10,
            ended_at_ms: Some(20),
            compression_level: Default::default(),
        },
        thought: Some("planned todos".to_string()),
        ..Default::default()
    });
    state.next_step_index = 1;

    let snapshot = LLMContextSnapshot { request, state };
    let out = append_turn_message_to_snapshot(
        snapshot,
        None,
        vec![HistoryInputRecord {
            source: "system".to_string(),
            text: "Continue TASK_ANCHOR.".to_string(),
            at_ms: 42,
        }],
        "new-trace",
        true,
    );

    assert_eq!(out.state.accumulated.len(), out.request.input.len());
    assert_eq!(out.state.steps.len(), 1);
    assert_eq!(out.state.history_inputs.len(), 1);
    assert_eq!(out.state.history_inputs[0].text, "Continue TASK_ANCHOR.");
    assert!(out.state.last_step.is_none());
}

#[test]
fn append_on_switch_message_after_step_history_as_user_tail() {
    let request = LLMContextRequest {
        owner: ContextOwnerRef::Agent {
            session_id: "s-1".to_string(),
        },
        trace: Some("old-trace".to_string()),
        objective: "objective".to_string(),
        behavior_name: "do".to_string(),
        input: vec![
            AiMessage::text(AiRole::System, "system"),
            AiMessage::text(AiRole::User, "initial"),
        ],
        model_policy: Default::default(),
        tool_policy: Default::default(),
        output: Default::default(),
        budget: Default::default(),
        human_policy: Default::default(),
        error_policy: Default::default(),
        forbid_next_behavior: false,
    };
    let mut state = LLMContextState::from_request(&request, 1);
    state.steps.push(llm_context::behavior_loop::StepRecord {
        meta: llm_context::behavior_loop::StepMeta {
            behavior_name: "plan".to_string(),
            step_index: 0,
            started_at_ms: 10,
            ended_at_ms: Some(20),
            compression_level: Default::default(),
        },
        thought: Some("planned todos".to_string()),
        ..Default::default()
    });
    state.next_step_index = 1;

    let snapshot = LLMContextSnapshot { request, state };
    let out = append_turn_message_to_snapshot(
        snapshot,
        Some(AiMessage::text(AiRole::User, "Continue TASK_ANCHOR.")),
        Vec::new(),
        "new-trace",
        true,
    );

    assert_eq!(out.state.history_inputs.len(), 0);
    assert_eq!(out.state.steps.len(), 1);
    assert_eq!(out.state.accumulated.len(), out.request.input.len() + 1);
    assert_eq!(
        out.state.accumulated.last().map(|msg| msg.text_content()),
        Some("Continue TASK_ANCHOR.".to_string())
    );
    assert!(out.state.last_step.is_none());
    assert!(is_runtime_auto_user_pending("opendan:on_switch"));
    assert!(!is_history_input_pending("on-switch-s-1-do-0"));
}

#[test]
fn output_text_extraction() {
    let out = ContextOutput::Text {
        content: "hi".to_string(),
    };
    assert_eq!(output_to_text(&out).as_deref(), Some("hi"));
    let out = ContextOutput::Text {
        content: String::new(),
    };
    assert!(output_to_text(&out).is_none());
}

#[test]
fn pending_input_dedup_key_distinguishes_variants() {
    let msg = PendingInput::Msg {
        record_id: "abc".to_string(),
        from: "alice".to_string(),
        from_did: None,
        from_name: None,
        tunnel_did: None,
        text: "hi".to_string(),
        ai_message: AiMessage::text(AiRole::User, "hi"),
    };
    let event = PendingInput::Event {
        event_id: "abc".to_string(),
        data: serde_json::Value::Null,
    };
    assert_eq!(msg.dedup_key(), "msg:abc");
    assert_eq!(event.dedup_key(), "event:abc");
    assert_ne!(msg.dedup_key(), event.dedup_key());
}

#[test]
fn format_event_for_turn_includes_id_and_data() {
    let s = format_event_for_turn("/timer/wake", &serde_json::json!({"tick": 1}));
    assert!(s.contains("/timer/wake"));
    assert!(s.contains("tick"));
}

#[test]
fn format_event_for_turn_handles_null_payload() {
    let s = format_event_for_turn("/timer/wake", &serde_json::Value::Null);
    assert!(s.contains("/timer/wake"));
    assert!(!s.contains("null"));
}

#[test]
fn format_event_for_turn_uses_subscription_template() {
    let subscriptions = vec![EventSubscription {
        pattern: "/approval/**".to_string(),
        subscribed_at_ms: 0,
        message_template: Some("Approval changed to {status}: {message}".to_string()),
    }];
    let s = format_event_for_turn_with_subscriptions(
        "/approval/doc-1",
        &serde_json::json!({"status": "approved", "message": "ready"}),
        &subscriptions,
        None,
    );
    assert_eq!(s, "Approval changed to approved: ready");
}

#[test]
fn render_current_todo_list_marks_first_open_todo() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("todos.json"),
        serde_json::json!([
            todo_json("T01", "completed", "done task", "done task details", vec![]),
            todo_json("T02", "pending", "next task", "next task details", vec![])
        ])
        .to_string(),
    )
    .unwrap();

    let rendered = render_current_todo_list(dir.path());
    assert!(rendered.contains("- T01 [completed] done task"));
    assert!(rendered.contains("- T02 [pending current] next task"));
}

#[test]
fn load_current_todo_returns_first_open_todo() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("todos.json"),
        serde_json::json!([
            todo_json("T01", "completed", "done task", "done task details", vec![]),
            todo_json(
                "T02",
                "running",
                "next task",
                "next task details",
                vec!["docs"]
            )
        ])
        .to_string(),
    )
    .unwrap();

    let todo = load_current_todo(dir.path());
    assert_eq!(todo["todo_id"], "T02");
    assert_eq!(todo["title"], "next task");
    assert_eq!(todo["content"], "next task details");
    assert_eq!(todo["skills"][0], "docs");
}

#[test]
fn load_current_todo_returns_null_when_all_terminal() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("todos.json"),
        serde_json::json!([todo_json(
            "T01",
            "completed",
            "done task",
            "done task details",
            vec![]
        )])
        .to_string(),
    )
    .unwrap();

    assert!(load_current_todo(dir.path()).is_null());
}

#[test]
fn template_failure_detail_points_to_null_todo_access() {
    let dir = tempfile::tempdir().unwrap();
    let behavior_path = dir.path().join("behaviors/do.toml");
    std::fs::create_dir_all(behavior_path.parent().unwrap()).unwrap();
    std::fs::write(
        &behavior_path,
        r#"[meta]
name = "do"
objective = "execute"

[prompt]
parser = "xml"
on_init = """
__INCLUDE(/role.md)__

<<TASK_ANCHOR>>
{{ session.current_todo.todo_id }}: {{session.current_todo.title}}

{{ session.current_todo.content }}
<</TASK_ANCHOR>>
"""
"#,
    )
    .unwrap();

    let mut behavior = BehaviorCfg::from_toml_str(
        r#"
        [meta]
        name = "do"

        [prompt]
        on_init = """
__INCLUDE(/role.md)__

<<TASK_ANCHOR>>
{{ session.current_todo.todo_id }}: {{session.current_todo.title}}

{{ session.current_todo.content }}
<</TASK_ANCHOR>>
"""
    "#,
    )
    .unwrap();
    behavior.source_path = Some(behavior_path.clone());

    let env = AgentSessionEnv {
        session_id: "s-1".into(),
        session_kind: "work",
        session_title: "title".into(),
        session_objective: "objective".into(),
        session_owner: "owner".into(),
        session_current_todo: serde_json::Value::Null,
        session_current_todo_list: "(empty)".into(),
        behavior_name: "do".into(),
        behavior_objective: "execute".into(),
        behavior_mode: "behavior",
        behavior_template_dir: behavior_path.parent().map(|path| path.to_path_buf()),
        workspace_id: Some("ws1".into()),
        workspace_root: Some(dir.path().join("workspace/ws1")),
        agent_root: dir.path().to_path_buf(),
        session_root: dir.path().join("sessions/s-1"),
        input_text: String::new(),
        input_has_user_text: false,
        input_has_events: false,
        recent_activity: String::new(),
        clock_unix_ms: 1,
    };
    let detail = render_template_failure_detail(
        &behavior,
        "prompt.on_init",
        behavior.prompt.on_init.trim(),
        &env,
        &"none does not support key-based access",
    );

    assert!(detail.contains("behavior=`do`"));
    assert!(detail.contains("field=`prompt.on_init`"));
    assert!(detail.contains("do.toml:8"));
    assert!(detail.contains("do.toml:11"));
    assert!(detail.contains("session.current_todo` is null"));
    assert!(detail.contains("{{ session.current_todo.todo_id }}"));
}

#[test]
fn event_batch_formats_single_user_wakeup() {
    let batch = format_event_batch_for_turn(&[
        EventForTurn {
            event_id: "/approval/doc-1".to_string(),
            data: serde_json::json!({"status": "approved"}),
            message: "Approval changed to approved".to_string(),
        },
        EventForTurn {
            event_id: "/task/7".to_string(),
            data: serde_json::Value::Null,
            message: "Task 7 completed".to_string(),
        },
    ])
    .expect("batch");
    assert!(batch.starts_with("[event batch]"));
    assert!(batch.contains("handled together as one wakeup"));
    assert!(batch.contains("Approval changed"));
    assert!(batch.contains("Task 7 completed"));
}

#[test]
fn pending_event_replacement_keeps_terminal_over_progress() {
    let existing = PendingInput::Event {
        event_id: "/task/7".to_string(),
        data: serde_json::json!({"to_status": "Completed"}),
    };
    let incoming = PendingInput::Event {
        event_id: "/task/7".to_string(),
        data: serde_json::json!({"to_status": "Running"}),
    };
    assert!(!should_replace_pending_event(&existing, &incoming));
    assert!(should_replace_pending_event(&incoming, &existing));
}

#[test]
fn worksession_report_delivery_modes_match_context_depth() {
    assert!(worksession_report_delivery_allows(
        ReportDeliveryMode::FinalOnly,
        WorksessionReportPhase::Final,
        0
    ));
    assert!(!worksession_report_delivery_allows(
        ReportDeliveryMode::FinalOnly,
        WorksessionReportPhase::Checkpoint,
        0
    ));
    assert!(!worksession_report_delivery_allows(
        ReportDeliveryMode::FinalOnly,
        WorksessionReportPhase::Final,
        1
    ));
    assert!(worksession_report_delivery_allows(
        ReportDeliveryMode::TopLevel,
        WorksessionReportPhase::Checkpoint,
        0
    ));
    assert!(!worksession_report_delivery_allows(
        ReportDeliveryMode::TopLevel,
        WorksessionReportPhase::Checkpoint,
        1
    ));
    assert!(worksession_report_delivery_allows(
        ReportDeliveryMode::All,
        WorksessionReportPhase::Checkpoint,
        3
    ));
}

#[test]
fn worksession_report_msg_carries_source_metadata() {
    let agent = name_lib::DID::from_str("did:dev:agent").unwrap();
    let peer = name_lib::DID::from_str("did:dev:alice").unwrap();
    let data = serde_json::json!({
        "type": "worksession_report",
        "report_id": "report:work-1:final:abc",
        "source_session_id": "work-1",
        "target_session_id": "ui-1",
        "title": "build demo",
        "objective": "ship the demo",
        "workspace_id": "ws-1",
        "behavior": "plan",
        "context_depth": 0,
        "phase": "final",
        "report": "done",
        "is_final": true,
        "trace_id": "trace-1",
        "created_at_ms": 42u64,
    });

    let msg = build_worksession_report_msg(&agent, &peer, "ui-1", &data);

    assert_eq!(msg.content.content, "done");
    assert_eq!(
        msg.content.title.as_deref(),
        Some("WorkSession report: build demo")
    );
    assert_eq!(msg.thread.topic.as_deref(), Some("ui-1"));
    assert_eq!(msg.thread.correlation_id.as_deref(), Some("work-1"));
    assert_eq!(
        msg.meta.get("message_type").and_then(|v| v.as_str()),
        Some("worksession_report")
    );
    assert_eq!(
        msg.meta
            .get("source")
            .and_then(|v| v.pointer("/kind"))
            .and_then(|v| v.as_str()),
        Some("worksession")
    );
    assert_eq!(
        msg.meta
            .get("source")
            .and_then(|v| v.pointer("/session_id"))
            .and_then(|v| v.as_str()),
        Some("work-1")
    );
}

#[test]
fn pending_queue_limit_drops_events_then_non_mentions() {
    let mut pending = vec![
        PendingInput::Msg {
            record_id: "m1".to_string(),
            from: "alice".to_string(),
            from_did: None,
            from_name: None,
            tunnel_did: None,
            text: "hello".to_string(),
            ai_message: AiMessage::text(AiRole::User, "hello"),
        },
        PendingInput::Event {
            event_id: "e1".to_string(),
            data: serde_json::Value::Null,
        },
        PendingInput::Msg {
            record_id: "m2".to_string(),
            from: "bob".to_string(),
            from_did: None,
            from_name: None,
            tunnel_did: None,
            text: "@jarvis please check".to_string(),
            ai_message: AiMessage::text(AiRole::User, "@jarvis please check"),
        },
    ];

    assert_eq!(enforce_pending_queue_limit(&mut pending, 1, "jarvis"), 2);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].dedup_key(), "msg:m2");
}

#[test]
fn session_meta_round_trips_pending_inputs() {
    // SessionMeta + PendingInput must round-trip through JSON so
    // `.meta/session.json` correctly preserves unconsumed inputs across
    // process restarts. If this breaks, persisted pendings are lost.
    let meta = SessionMeta {
        session_id: "s1".to_string(),
        kind: SessionKind::Ui,
        current_behavior: "ui_default".to_string(),
        status: SessionStatus::WaitingInput,
        owner: "alice".to_string(),
        one_line_status: String::new(),
        pending_inputs: vec![
            PendingInput::Msg {
                record_id: "rec-1".to_string(),
                from: "alice".to_string(),
                from_did: Some("did:dev:alice".to_string()),
                from_name: Some("Alice".to_string()),
                tunnel_did: Some("did:dev:tunnel".to_string()),
                text: "hi".to_string(),
                ai_message: AiMessage::text(AiRole::User, "hi"),
            },
            PendingInput::Event {
                event_id: "/timer/wake".to_string(),
                data: serde_json::json!({"tick": 7}),
            },
        ],
        peer_did: Some("did:dev:alice".to_string()),
        peer_tunnel_did: Some("did:dev:tunnel".to_string()),
        event_subscriptions: vec![EventSubscription {
            pattern: "/timer/**".to_string(),
            subscribed_at_ms: 0,
            message_template: None,
        }],
        workspace_id: Some("ws-1".to_string()),
        pending_task_calls: vec![PendingTaskCall {
            call_id: "call-1".to_string(),
            tool_name: "download".to_string(),
            task_id: 42,
            event_pattern: "/task_mgr/42".to_string(),
        }],
        title: "design review".to_string(),
        objective: "draft the rollout plan".to_string(),
        bootstrap_done: true,
        process_entry: "planner".to_string(),
        process_stack: vec![ProcessFrame {
            entry: "ui_default".to_string(),
            current: "ui_default".to_string(),
            fork: false,
        }],
        last_report_delivery: None,
    };
    let json = serde_json::to_string(&meta).unwrap();
    let restored: SessionMeta = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.pending_inputs.len(), 2);
    match &restored.pending_inputs[0] {
        PendingInput::Msg {
            record_id,
            text,
            from_did,
            from_name,
            tunnel_did,
            ..
        } => {
            assert_eq!(record_id, "rec-1");
            assert_eq!(text, "hi");
            assert_eq!(from_did.as_deref(), Some("did:dev:alice"));
            assert_eq!(from_name.as_deref(), Some("Alice"));
            assert_eq!(tunnel_did.as_deref(), Some("did:dev:tunnel"));
        }
        _ => panic!("expected Msg variant first"),
    }
    match &restored.pending_inputs[1] {
        PendingInput::Event { event_id, data } => {
            assert_eq!(event_id, "/timer/wake");
            assert_eq!(data.get("tick").and_then(|v| v.as_i64()), Some(7));
        }
        _ => panic!("expected Event variant second"),
    }
    assert_eq!(restored.peer_did.as_deref(), Some("did:dev:alice"));
    assert_eq!(restored.event_subscriptions.len(), 1);
    assert_eq!(restored.event_subscriptions[0].pattern, "/timer/**");
    assert_eq!(restored.workspace_id.as_deref(), Some("ws-1"));
    assert_eq!(restored.pending_task_calls.len(), 1);
    assert_eq!(restored.pending_task_calls[0].task_id, 42);
    assert_eq!(restored.pending_task_calls[0].call_id, "call-1");
    assert_eq!(restored.title, "design review");
    assert_eq!(restored.objective, "draft the rollout plan");
    assert!(restored.bootstrap_done);
    assert_eq!(restored.process_entry, "planner");
    assert_eq!(restored.process_stack.len(), 1);
    assert_eq!(restored.process_stack[0].entry, "ui_default");
    assert_eq!(restored.process_stack[0].current, "ui_default");
    assert!(!restored.process_stack[0].fork);
}

#[test]
fn session_meta_backfills_process_entry_for_legacy_json() {
    // Older `.meta/session.json` files predate the
    // `process_entry` / `process_stack` fields. They must still
    // deserialize (serde defaults) and `AgentSession::new`'s restore
    // path backfills `process_entry` from `current_behavior` so the
    // independent-mode snapshot path is well-formed.
    let legacy = serde_json::json!({
        "session_id": "s2",
        "kind": "ui",
        "current_behavior": "ui_default",
        "status": "idle",
    });
    let restored: SessionMeta = serde_json::from_value(legacy).unwrap();
    assert_eq!(restored.process_entry, "");
    assert!(restored.process_stack.is_empty());
    // (The backfill itself lives in AgentSession::new and is exercised
    // by the restore-path integration tests; here we only assert that
    // the legacy JSON does NOT fail to deserialize.)
}

#[test]
fn observation_from_task_event_translates_completed() {
    let payload = serde_json::json!({
        "to_status": "Completed",
        "data": {"result": "ok"},
    });
    let obs = observation_from_task_event("call-9", &payload).expect("terminal observation");
    match obs {
        Observation::Success {
            call_id, content, ..
        } => {
            assert_eq!(call_id, "call-9");
            assert_eq!(content.get("result").and_then(|v| v.as_str()), Some("ok"));
        }
        _ => panic!("expected Success"),
    }
}

#[test]
fn observation_from_task_event_translates_failed() {
    let payload = serde_json::json!({
        "to_status": "Failed",
        "message": "network unreachable",
    });
    let obs = observation_from_task_event("call-9", &payload).expect("terminal observation");
    match obs {
        Observation::Error {
            call_id, message, ..
        } => {
            assert_eq!(call_id, "call-9");
            assert!(message.contains("network"));
        }
        _ => panic!("expected Error"),
    }
}

#[test]
fn observation_from_task_event_ignores_non_terminal_status() {
    // Running / Progress events shouldn't move the session — they emit
    // frequently and the session must wait for the terminal one.
    let payload = serde_json::json!({"to_status": "Running"});
    assert!(observation_from_task_event("c", &payload).is_none());
}

#[test]
fn compress_messages_preserves_short_history_verbatim() {
    // Under the keep-tail threshold ⇒ no compression, output == input.
    let msgs = vec![
        AiMessage::text(AiRole::System, "sys"),
        AiMessage::text(AiRole::User, "u1"),
        AiMessage::text(AiRole::Assistant, "a1"),
    ];
    let out = compress_messages_for_context_limit(msgs.clone());
    assert_eq!(out.len(), msgs.len());
    assert_eq!(out[0].role, AiRole::System);
}

#[test]
fn compress_messages_drops_middle_and_keeps_tail() {
    let mut msgs = vec![AiMessage::text(AiRole::System, "sys")];
    // Generate alternating user/assistant pairs well beyond the tail cap.
    for i in 0..(COMPRESS_KEEP_TAIL + 20) {
        let role = if i % 2 == 0 {
            AiRole::User
        } else {
            AiRole::Assistant
        };
        msgs.push(AiMessage::text(role, format!("m-{i}")));
    }
    let out = compress_messages_for_context_limit(msgs);
    assert_eq!(out[0].role, AiRole::System);
    // Second message is the synthetic compression note.
    assert_eq!(out[1].role, AiRole::User);
    let note = out[1]
        .content
        .iter()
        .find_map(|b| match b {
            AiContent::Text { text } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(note.contains("context compressed"));
    assert!(note.contains("earlier"));
    // Tail length is at most the keep cap (may be one less when we
    // realign past a leading Assistant).
    let tail_len = out.len() - 2;
    assert!(tail_len <= COMPRESS_KEEP_TAIL);
    assert!(tail_len >= COMPRESS_KEEP_TAIL - 1);
    // No two assistant messages in a row (our realignment guarantee).
    for w in out.windows(2) {
        assert!(
            !(w[0].role == AiRole::Assistant && w[1].role == AiRole::Assistant),
            "compress must not produce back-to-back assistant messages"
        );
    }
}

#[test]
fn model_directory_context_window_resolves_logical_mount() {
    let directory = serde_json::json!({
        "providers": [{
            "models": [{
                "exact_model": "gpt-5@openai",
                "provider_model_id": "gpt-5",
                "logical_mounts": ["llm.chat"],
                "capabilities": {"max_context_tokens": 128000}
            }, {
                "exact_model": "tiny@local",
                "provider_model_id": "tiny",
                "logical_mounts": ["llm.chat"],
                "capabilities": {"max_context_tokens": 32000}
            }]
        }]
    });
    assert_eq!(
        context_window_tokens_from_model_directory(&directory, "llm.chat"),
        Some(32000)
    );
}

#[test]
fn model_directory_context_window_follows_directory_targets() {
    let directory = serde_json::json!({
        "directory": {
            "llm.plan": {
                "opus": {"target": "llm.opus", "weight": 1.0}
            }
        },
        "providers": [{
            "models": [{
                "exact_model": "claude-opus@anthropic",
                "provider_model_id": "claude-opus",
                "logical_mounts": ["llm.opus"],
                "capabilities": {"max_context_tokens": 200000}
            }]
        }]
    });
    assert_eq!(
        context_window_tokens_from_model_directory(&directory, "llm.plan"),
        Some(200000)
    );
}

#[test]
fn turns_since_last_compress_counts_user_turns_after_marker() {
    let msgs = vec![
        AiMessage::text(AiRole::System, "sys"),
        AiMessage::text(AiRole::Assistant, "[LLM_MESSAGE_COMPRESS_META_V1]"),
        AiMessage::text(AiRole::User, "[LLM_MESSAGE_COMPRESS_SUMMARY_V1] summary"),
        AiMessage::text(AiRole::User, "u1"),
        AiMessage::text(AiRole::Assistant, "a1"),
        AiMessage::text(AiRole::User, "u2"),
    ];
    assert_eq!(turns_since_last_llm_message_compress(&msgs), 2);
}

#[test]
fn merge_env_and_human_combines_both_with_env_first() {
    let m = merge_env_and_human(Some("E".into()), Some("H".into()));
    assert_eq!(m.as_deref(), Some("E\n\nH"));
}

#[test]
fn merge_env_and_human_handles_missing_pieces() {
    assert_eq!(
        merge_env_and_human(None, Some("h".into())).as_deref(),
        Some("h")
    );
    assert_eq!(
        merge_env_and_human(Some("e".into()), None).as_deref(),
        Some("e")
    );
    assert!(merge_env_and_human(None, None).is_none());
}

#[test]
fn session_meta_tolerates_missing_pending_inputs_field() {
    // Older session.json files were written before pending_inputs
    // existed; restoring them must default the field to an empty
    // vec rather than erroring out.
    let legacy = r#"{
        "session_id": "old",
        "kind": "ui",
        "current_behavior": "ui_default",
        "status": "idle",
        "owner": "alice"
    }"#;
    let meta: SessionMeta = serde_json::from_str(legacy).unwrap();
    assert!(meta.pending_inputs.is_empty());
    assert_eq!(meta.owner, "alice");
}
