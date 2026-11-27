// Copyright 2025 TiDB Cloud
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use std::sync::atomic::AtomicUsize;

fn not_found(body: impl Into<String>) -> MockReply {
    MockReply {
        status: 404,
        body: body.into(),
        ..MockReply::json(Value::Null)
    }
}

async fn unshared_client(server: &MockServer) -> APIClient {
    Arc::try_unwrap(client(server).await).ok().unwrap()
}

#[tokio::test]
async fn detached_kill_polls_status_without_inheriting_the_transaction() {
    for code in [1053, 1063] {
        let server = MockServer::new(move |request| match request.path.as_str() {
            "/v1/query" => {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(body["sql"], "KILL QUERY 'a\\\\b''c'");
                assert_eq!(
                    body["session"],
                    json!({"role":"r1", "secondary_roles":["r2"]})
                );
                assert!(!request.headers.contains_key("x-databend-sticky-node"));
                let mut body = query_reply(Some("/v1/query/q1"), json!([]));
                body["stats_uri"] = body["next_uri"].clone();
                body["session"] = json!({"txn_state":"AutoCommit"});
                let mut reply = MockReply::json(body);
                reply.headers.push((HEADER_ROUTE_HINT, "ignored-route"));
                reply
            }
            "/v1/query/q1" => {
                assert_eq!(request.method, "GET");
                assert_eq!(request.headers["x-databend-sticky-node"], "node-1");
                MockReply::json(
                    json!({"state":"Failed", "error":{"code":code,"message":"kill failed"}}),
                )
            }
            _ => panic!("unexpected request"),
        })
        .await;
        let client = client(&server).await;
        client.route_hint.set("original-route");
        client.set_last_node_id("original-node".into());
        client.set_last_query_id(Some("original".into()));
        {
            let mut state = client.session_state.lock();
            state.txn_state = Some("Active".into());
            state.need_sticky = Some(true);
            state.role = Some("r1".into());
            state.secondary_roles = Some(vec!["r2".into()]);
        }
        let error = client.kill_query("a\\b'c").await.unwrap_err();
        assert!(
            matches!(error, Error::WithContext { inner, .. } if matches!(inner.as_ref(), Error::QueryFailed(e) if e.code == code))
        );
        assert_eq!(client.last_query_id().as_deref(), Some("original"));
        assert_eq!(client.last_node_id().as_deref(), Some("original-node"));
        assert_eq!(client.route_hint.current(), "original-route");
        assert_eq!(client.session_state().txn_state.as_deref(), Some("Active"));
        server.assert_native_requests(2);
    }
}

#[tokio::test]
async fn detached_kill_does_not_replay_after_an_ambiguous_failure() {
    for failure in ["timeout", "truncated", "unavailable"] {
        let server = MockServer::new(move |request| {
            assert_eq!(request.path, "/v1/query");
            let mut reply = MockReply::json(Value::Null);
            match failure {
                "timeout" => reply.gate = Some(Arc::new(Notify::new())),
                "truncated" => reply.truncated = true,
                "unavailable" => reply.status = 503,
                _ => unreachable!(),
            }
            reply
        })
        .await;
        let mut client = unshared_client(&server).await;
        client.retry_count = 3;
        client.retry_delay_secs = 0;
        client.page_request_timeout = Duration::from_millis(250);
        assert!(client.kill_query("other").await.is_err());
        server.assert_native_requests(1);
    }
}

#[tokio::test]
async fn early_kill_retries_with_the_original_id_and_picks_up_the_node() {
    let start_seen = Arc::new(Notify::new());
    let start_gate = Arc::new(Notify::new());
    let first_kill_seen = Arc::new(Notify::new());
    let first_kill_gate = Arc::new(Notify::new());
    let server = MockServer::new({
        let start_seen = start_seen.clone();
        let start_gate = start_gate.clone();
        let first_kill_seen = first_kill_seen.clone();
        let first_kill_gate = first_kill_gate.clone();
        move |request| {
            let id = &request.headers["x-databend-query-id"];
            if request.path == "/v1/query" {
                let mut body = query_reply(Some("/v1/query/q1/page/0"), json!([]));
                body["id"] = json!(id);
                start_seen.notify_one();
                return MockReply {
                    gate: Some(start_gate.clone()),
                    ..MockReply::json(body)
                };
            }
            assert_eq!(request.path, format!("/v1/query/{id}/kill"));
            if let Some(node) = request.headers.get("x-databend-sticky-node") {
                assert_eq!(node, "node-1");
                MockReply::json(Value::Null)
            } else {
                first_kill_seen.notify_one();
                MockReply {
                    gate: Some(first_kill_gate.clone()),
                    ..not_found(format!("Query ID {id} not found on node other"))
                }
            }
        }
    })
    .await;
    let mut client = unshared_client(&server).await;
    client.server_version = Some(Version::new(1, 2, 960));
    assert!(matches!(
        client.kill_query("unknown").await,
        Err(Error::BadArgument(_))
    ));
    let client = Arc::new(client);
    let conn = client.clone();
    let start = tokio::spawn(async move { conn.start_query("SELECT 1", false, None).await });
    server.wait_for(start_seen.notified()).await;
    let id = client.last_query_id().unwrap();
    let conn = client.clone();
    let kill = tokio::spawn(async move { conn.kill_query(&id).await });
    server.wait_for(first_kill_seen.notified()).await;
    start_gate.notify_one();
    let _pages = server.wait_for(start).await.unwrap().unwrap();
    first_kill_gate.notify_one();
    server.wait_for(kill).await.unwrap().unwrap();
    server.assert_native_requests(3);
    let requests = server.requests.lock();
    assert_eq!(requests[1].path, requests[2].path);
}

#[tokio::test]
async fn early_kill_after_dropped_submission_only_retries_query_not_found() {
    for missing in [false, true] {
        let attempts = AtomicUsize::new(0);
        let server = MockServer::new(move |request| {
            assert_eq!(request.path, "/v1/query/pending/kill");
            match attempts.fetch_add(1, Ordering::Relaxed) {
                0 if missing => MockReply {
                    status: 404,
                    ..MockReply::json(json!({"error":{"code":404,"message":"Query ID pending not found on node other"}}))
                },
                _ => not_found("Unrelated 404"),
            }
        }).await;
        let client = client(&server).await;
        let mut submission = QuerySubmission::new(&client, "pending".into());
        submission.begin_attempt();
        drop(submission);
        let error = server
            .wait_for(client.kill_query("pending"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("Unrelated 404"));
        server.assert_native_requests(if missing { 2 } else { 1 });
    }
}

#[tokio::test]
async fn early_kill_returns_persistent_404_at_the_registration_deadline() {
    let server = MockServer::new(|request| {
        assert_eq!(request.path, "/v1/query/pending/kill");
        not_found("Query ID pending not found on node other")
    })
    .await;
    let mut client = unshared_client(&server).await;
    client.wait_time_secs = Some(0);
    let _submission = QuerySubmission::new(&client, "pending".into());
    let started = tokio::time::Instant::now();
    let error = server
        .wait_for(client.kill_query("pending"))
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), Some(StatusCode::NOT_FOUND));
    assert!(started.elapsed() >= Duration::from_secs(2));
    assert!(server.requests.lock().len() > 1);
}

#[tokio::test]
async fn corrupt_arrow_batch_keeps_the_server_id_owned() {
    use arrow_array::Int64Array;
    use arrow_ipc::writer::StreamWriter;
    use arrow_schema::{DataType, Field, Schema};

    let header = query_reply(Some("/v1/query/q1/page/0"), json!([]));
    let schema = Arc::new(Schema::new_with_metadata(
        vec![Field::new("n", DataType::Int64, false)],
        HashMap::from([("response_header".into(), header.to_string())]),
    ));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(Int64Array::from_iter_values(0..1000))],
    )
    .unwrap();
    let mut bytes = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).unwrap();
        writer.write(&batch).unwrap();
        writer.finish().unwrap();
    }
    bytes.truncate(bytes.len() - 100);
    let response = HttpResponseData {
        status: StatusCode::OK,
        headers: HeaderMap::from_iter([(
            CONTENT_TYPE,
            HeaderValue::from_static(CONTENT_TYPE_ARROW),
        )]),
        body: bytes.into(),
    };
    let client = APIClient::default();
    let mut submission = QuerySubmission::new(&client, "client-id".into());
    submission.begin_attempt();
    let result = client.handle_page(response, Some("client-id")).await;
    drop(submission);
    assert!(matches!(result, Err(Error::Decode(e)) if e.contains("arrow batch")));
    assert_eq!(client.last_query_id().as_deref(), Some("q1"));
    let owned = client.owned_queries.lock();
    assert_eq!(owned.is_reading("q1"), Some(false));
    assert_eq!(
        owned.kill_target("q1").unwrap().node_id.as_deref(),
        Some("node-1")
    );
    assert!(owned.kill_target("client-id").is_none());
}
