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

fn reply(id: &str, next_uri: Option<&str>) -> Value {
    let mut body = query_reply(next_uri, json!([]));
    body["id"] = json!(id);
    body
}

fn query_not_found(request: &MockRequest, structured: bool) -> MockReply {
    let id = &request.headers["x-databend-query-id"];
    let message = format!("Query ID {id} not found on node node-0");
    let mut reply = MockReply::json(json!({"error": {"code": 404, "message": message}}));
    if !structured {
        reply.body = message.into_bytes();
        reply.content_type = "text/plain";
    }
    reply.status = 404;
    reply
}

async fn unshared_client(server: &MockServer) -> APIClient {
    Arc::try_unwrap(client(server).await).ok().unwrap()
}

#[tokio::test]
async fn owned_query_uses_its_server_id_and_node_even_on_old_servers() {
    let server = MockServer::new(|request| match request.path.as_str() {
        "/v1/query" => MockReply::json(reply("owned", Some("/v1/query/owned/page/1"))),
        "/v1/query/owned/kill" => {
            assert_eq!(request.method, "POST");
            assert_eq!(request.headers["x-databend-query-id"], "owned");
            assert_eq!(request.headers["x-databend-sticky-node"], "node-1");
            MockReply::json(Value::Null)
        }
        _ => panic!("unexpected request"),
    })
    .await;
    let mut client = unshared_client(&server).await;
    client.server_version = Some(Version::new(1, 2, 960));
    let client = Arc::new(client);
    let pages = client.start_query("SELECT 1", false, None).await.unwrap();
    // Do not use the connection's most recent node instead of the query's node.
    client.set_last_node_id("other-node".into());
    assert_eq!(client.last_query_id().as_deref(), Some("owned"));
    client.kill_query("owned").await.unwrap();
    drop(pages);
    assert_eq!(client.owned_queries.lock().is_reading("owned"), Some(false));
    client.kill_query("owned").await.unwrap();
    assert!(matches!(
        client.kill_query("unknown").await,
        Err(Error::BadArgument(_))
    ));
    server.assert_native_requests(3);
}

#[tokio::test]
async fn detached_kill_drains_without_touching_connection_state() {
    let server = MockServer::new(|request| match request.path.as_str() {
        "/v1/query" => {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["sql"], "KILL QUERY 'a\\\\b''c'");
            let session = body["session"].as_object().unwrap();
            assert_eq!(session.len(), 2);
            assert_eq!(session["role"], "kill_role");
            assert_eq!(session["secondary_roles"], json!(["r2"]));
            assert_eq!(request.headers["x-databend-route-hint"], "original-route");
            assert!(!request.headers.contains_key("x-databend-sticky-node"));
            assert!(
                !request.headers.contains_key("accept")
                    || request.headers["accept"] != CONTENT_TYPE_ARROW
            );
            assert_ne!(request.headers["x-databend-query-id"], "original");
            let mut body = reply("detached", Some("/v1/query/detached/page/1"));
            body["session"] = json!({"txn_state":"AutoCommit","settings":{"warehouse":"other"}});
            let mut response = MockReply::json(body);
            response.headers.push((HEADER_ROUTE_HINT, "ignore-route"));
            response
        }
        "/v1/query/detached/page/1" => {
            assert_eq!(request.method, "GET");
            assert_eq!(request.headers["x-databend-sticky-node"], "node-1");
            let mut body = reply("detached", Some("/v1/query/detached/final"));
            body["state"] = json!("Succeeded");
            body["final_uri"] = body["next_uri"].clone();
            MockReply::json(body)
        }
        "/v1/query/detached/final" => {
            assert_eq!(request.method, "POST");
            assert_eq!(request.headers["x-databend-sticky-node"], "node-1");
            // Cleanup errors should not obscure the outcome of the KILL.
            query_not_found(request, true)
        }
        _ => panic!("unexpected request"),
    })
    .await;
    let mut client = unshared_client(&server).await;
    client.query_result_format = "arrow".into();
    let version = Version::parse("1.2.966-nightly").unwrap();
    client.capability = Capability::from_server_version(&version);
    client.server_version = Some(version);
    client.set_last_query_id(Some("original".into()));
    client.set_last_node_id("original-node".into());
    client.set_warehouse("original-warehouse");
    client.route_hint.set("original-route");
    {
        let mut state = client.session_state.lock();
        state.txn_state = Some("Active".into());
        state.need_sticky = Some(true);
        state.role = Some("kill_role".into());
        state.secondary_roles = Some(vec!["r2".into()]);
    }
    client.kill_query("a\\b'c").await.unwrap();
    assert_eq!(client.last_query_id().as_deref(), Some("original"));
    assert_eq!(client.last_node_id().as_deref(), Some("original-node"));
    assert_eq!(client.route_hint.current(), "original-route");
    assert_eq!(
        client.current_warehouse().as_deref(),
        Some("original-warehouse")
    );
    assert_eq!(
        client.session_state.lock().txn_state.as_deref(),
        Some("Active")
    );
    assert_eq!(client.owned_queries.lock().len(), 0);
    assert!(client.queries_need_heartbeat.lock().is_empty());
    server.assert_native_requests(3);
}

#[tokio::test]
async fn detached_kill_polls_state_uri_and_preserves_sql_errors() {
    for code in [1053, 1063] {
        let polls = Arc::new(AtomicUsize::new(0));
        let counter = polls.clone();
        let server = MockServer::new(move |request| match request.path.as_str() {
            "/v1/query" => {
                let mut body = reply("kill", Some("/v1/query/kill"));
                body["stats_uri"] = body["next_uri"].clone();
                MockReply::json(body)
            }
            "/v1/query/kill" => {
                assert_eq!(request.method, "GET");
                assert_eq!(request.headers["x-databend-sticky-node"], "node-1");
                if counter.fetch_add(1, Ordering::Relaxed) == 0 {
                    MockReply::json(json!({"state":"Running"}))
                } else {
                    MockReply::json(
                        json!({"state":"Failed", "error":{"code":code,"message":"kill failed"}}),
                    )
                }
            }
            _ => panic!("unexpected request"),
        })
        .await;
        let client = client(&server).await;
        let err = client.kill_query("other-query").await.unwrap_err();
        assert!(
            matches!(err, Error::WithContext { request_kind: Some(RequestKind::QueryKill), query_id: Some(id), inner, .. } if id == "other-query" && matches!(inner.as_ref(), Error::QueryFailed(e) if e.code == code))
        );
        for invalid in ["", "x\ny", "\u{0}"] {
            assert!(matches!(
                client.kill_query(invalid).await,
                Err(Error::BadArgument(_))
            ));
        }
        assert!(client.last_query_id().is_none());
        assert_eq!(polls.load(Ordering::Relaxed), 2);
        server.assert_native_requests(3);
    }
}

#[tokio::test]
async fn detached_kill_never_replays_a_possibly_executed_statement() {
    for failure in ["timeout", "truncated", "unavailable"] {
        let server = MockServer::new(move |request| {
            assert_eq!(request.path, "/v1/query");
            let mut reply = MockReply::json(reply("kill", None));
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
async fn early_kill_retries_after_a_possibly_sent_submission_is_dropped() {
    let sent = Arc::new(Notify::new());
    let notification = sent.clone();
    let kills = Arc::new(AtomicUsize::new(0));
    let counter = kills.clone();
    let server = MockServer::new(move |request| {
        if request.path == "/v1/query" {
            let mut reply = MockReply::json(Value::Null);
            reply.gate = Some(Arc::new(Notify::new()));
            notification.notify_one();
            return reply;
        }
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.path,
            format!("/v1/query/{}/kill", request.headers["x-databend-query-id"])
        );
        assert!(!request.headers.contains_key("x-databend-sticky-node"));
        match counter.fetch_add(1, Ordering::Relaxed) {
            0 => query_not_found(request, false),
            1 => query_not_found(request, true),
            _ => MockReply::json(Value::Null),
        }
    })
    .await;
    let client = client(&server).await;
    server.wait_for(async {
        tokio::select! {
            _ = sent.notified() => {},
            _ = client.start_query("SELECT sleep(30)", false, None) => panic!("submission completed before the gate"),
        }
    }).await;
    let id = client.last_query_id().unwrap();
    server.wait_for(client.kill_query(&id)).await.unwrap();
    assert_eq!(kills.load(Ordering::Relaxed), 3);
    server.assert_native_requests(4);
}

#[tokio::test]
async fn early_kill_returns_persistent_404_at_the_registration_deadline() {
    let server = MockServer::new(|request| {
        assert_eq!(request.path, "/v1/query/pending/kill");
        query_not_found(request, false)
    })
    .await;
    let mut client = unshared_client(&server).await;
    client.wait_time_secs = Some(0);
    // The POST may have reached the server, but no response ever arrived.
    let mut submission = QuerySubmission::new(&client, "pending".into());
    submission.begin_attempt();
    drop(submission);

    let started = tokio::time::Instant::now();
    let error = server
        .wait_for(client.kill_query("pending"))
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), Some(StatusCode::NOT_FOUND));
    assert!(error.to_string().contains("Query ID pending not found"));
    assert!(started.elapsed() >= Duration::from_secs(2));
    assert!(server.requests.lock().len() > 1);
}

#[tokio::test]
async fn early_kill_switches_to_sticky_node_but_does_not_retry_other_404s() {
    for is_missing_query in [true, false] {
        let start_gate = Arc::new(Notify::new());
        let gate = start_gate.clone();
        let start_arrived = Arc::new(Notify::new());
        let start_seen = start_arrived.clone();
        let kill_arrived = Arc::new(Notify::new());
        let first_kill = kill_arrived.clone();
        let kill_gate = Arc::new(Notify::new());
        let first_kill_response = kill_gate.clone();
        let kills = Arc::new(AtomicUsize::new(0));
        let counter = kills.clone();
        let server = MockServer::new(move |request| {
            if request.path == "/v1/query" {
                let id = &request.headers["x-databend-query-id"];
                let uri = format!("/v1/query/{id}/page/1");
                let mut response = MockReply::json(reply(id, Some(&uri)));
                response.gate = Some(gate.clone());
                start_seen.notify_one();
                return response;
            }
            assert!(request.path.ends_with("/kill"));
            if counter.fetch_add(1, Ordering::Relaxed) == 0 {
                assert!(!request.headers.contains_key("x-databend-sticky-node"));
                first_kill.notify_one();
                let mut response = query_not_found(request, false);
                if !is_missing_query {
                    response.body = br#"{"error":"OrgNotFound"}"#.to_vec();
                }
                response.gate = Some(first_kill_response.clone());
                response
            } else {
                assert!(is_missing_query);
                assert_eq!(request.headers["x-databend-sticky-node"], "node-1");
                MockReply::json(Value::Null)
            }
        })
        .await;
        let client = client(&server).await;
        let cloned = client.clone();
        let submission =
            tokio::spawn(async move { cloned.start_query("SELECT 1", false, None).await });
        server.wait_for(start_arrived.notified()).await;
        let id = client.last_query_id().unwrap();
        let cloned = client.clone();
        let kill = tokio::spawn(async move { cloned.kill_query(&id).await });
        server.wait_for(kill_arrived.notified()).await;
        start_gate.notify_one();
        let pages = server.wait_for(submission).await.unwrap().unwrap();
        kill_gate.notify_one();
        let result = server.wait_for(kill).await.unwrap();
        if is_missing_query {
            result.unwrap();
            server.assert_native_requests(3);
        } else {
            assert_eq!(
                result.unwrap_err().status_code(),
                Some(StatusCode::NOT_FOUND)
            );
            server.assert_native_requests(2);
        }
        drop(pages);
    }
}

#[tokio::test]
async fn completed_or_unusable_responses_settle_ownership() {
    let server = MockServer::new(|request| match request.path.as_str() {
        "/v1/query" => {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            match body["sql"].as_str().unwrap() {
                "SELECT broken" => MockReply::json(json!({"unexpected":"body"})),
                "SELECT 1" => MockReply::json(reply("done", Some("/v1/query/done/page/1"))),
                "KILL QUERY 'done'" => MockReply::json(reply("kill", None)),
                _ => panic!("unexpected SQL"),
            }
        }
        "/v1/query/done/page/1" => MockReply::json(reply("done", None)),
        _ => {
            let id = &request.headers["x-databend-query-id"];
            assert_eq!(request.path, format!("/v1/query/{id}/kill"));
            MockReply::json(Value::Null)
        }
    })
    .await;
    let client = client(&server).await;
    assert!(matches!(
        client.start_query("SELECT broken", false, None).await,
        Err(Error::Decode(_))
    ));
    let id = client.last_query_id().unwrap();
    assert_eq!(client.owned_queries.lock().is_reading(&id), Some(false));
    client.kill_query(&id).await.unwrap();
    client.query_all("SELECT 1", None).await.unwrap();
    client.kill_query("done").await.unwrap();
    server.assert_native_requests(5);
}

#[tokio::test]
async fn corrupt_arrow_batch_keeps_the_server_id_owned() {
    use arrow_array::Int64Array;
    use arrow_ipc::writer::StreamWriter;
    use arrow_schema::{DataType, Field, Schema};

    let header = reply("server-id", Some("/v1/query/server-id/page/1"));
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
    let server = MockServer::new(move |request| match request.path.as_str() {
        "/v1/query" => MockReply {
            content_type: CONTENT_TYPE_ARROW,
            body: bytes.clone(),
            ..MockReply::json(Value::Null)
        },
        "/v1/query/server-id/kill" => {
            assert_eq!(request.headers["x-databend-sticky-node"], "node-1");
            MockReply::json(Value::Null)
        }
        _ => panic!("unexpected path"),
    })
    .await;
    let client = client(&server).await;
    let result = client.start_query("SELECT 1", false, None).await;
    assert!(matches!(result, Err(Error::Decode(e)) if e.contains("arrow batch")));
    assert_eq!(client.last_query_id().as_deref(), Some("server-id"));
    assert_eq!(
        client.owned_queries.lock().is_reading("server-id"),
        Some(false)
    );
    assert_eq!(client.owned_queries.lock().len(), 1);
    client.kill_query("server-id").await.unwrap();
    server.assert_native_requests(2);
}
