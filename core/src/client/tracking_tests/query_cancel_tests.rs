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
async fn detached_kill_does_not_replay_after_an_ambiguous_failure() {
    for failure in ["timeout", "truncated", "unavailable"] {
        let server = MockServer::new(move |request| {
            assert_eq!(request.path, "/v1/query");
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["sql"], "KILL QUERY 'a\\\\b''c'");
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
        assert!(client.kill_query("a\\b'c").await.is_err());
        server.assert_native_requests(1);
    }
}

#[tokio::test]
async fn early_kill_retries_only_while_registration_is_pending() {
    const MISSING: &str = "Query ID pending not found on node other";
    for case in ["registered", "unrelated 404", "deadline"] {
        let seen = Arc::new(Notify::new());
        let gate = Arc::new(Notify::new());
        let server = MockServer::new({
            let seen = seen.clone();
            let gate = gate.clone();
            move |request| {
                assert_eq!(request.method, "POST");
                assert_eq!(request.path, "/v1/query/pending/kill");
                assert_eq!(request.headers["x-databend-query-id"], "pending");
                let node = request.headers.get("x-databend-sticky-node");
                match (case, node.map(String::as_str)) {
                    ("registered", Some("node-1")) => MockReply::json(Value::Null),
                    ("registered", None) => {
                        seen.notify_one();
                        MockReply {
                            gate: Some(gate.clone()),
                            status: 404,
                            ..MockReply::json(json!({"error":{"code":404,"message":MISSING}}))
                        }
                    }
                    ("unrelated 404", None) => not_found("Unrelated 404"),
                    ("deadline", None) => not_found(MISSING),
                    _ => panic!("unexpected sticky node: {node:?}"),
                }
            }
        })
        .await;
        let mut client = unshared_client(&server).await;
        client.wait_time_secs = Some(0);
        // An owned ID must still use /kill on servers without SQL cancellation.
        client.server_version = Some(Version::new(1, 2, 960));
        assert!(matches!(
            client.kill_query("unknown").await,
            Err(Error::BadArgument(_))
        ));
        let mut submission = QuerySubmission::new(&client, "pending".into());
        submission.begin_attempt();

        if case == "registered" {
            let kill = client.kill_query("pending");
            tokio::pin!(kill);
            server.wait_for(async {
                tokio::select! {
                    _ = seen.notified() => {},
                    result = &mut kill => panic!("KILL returned before the first response: {result:?}"),
                }
            }).await;
            // The query's first response arrives while the initial /kill is in flight.
            let mut body = query_reply(Some("/v1/query/pending/page/0"), json!([]));
            body["id"] = json!("pending");
            let response = HttpResponseData {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body: Bytes::from(body.to_string()),
            };
            client.handle_page(response, Some("pending")).await.unwrap();
            drop(submission);
            gate.notify_one();
            server.wait_for(kill).await.unwrap();
            server.assert_native_requests(2);
        } else {
            // Dropping a possibly sent submission must not stop the retries.
            drop(submission);
            let started = tokio::time::Instant::now();
            let error = server
                .wait_for(client.kill_query("pending"))
                .await
                .unwrap_err();
            assert_eq!(error.status_code(), Some(StatusCode::NOT_FOUND));
            if case == "deadline" {
                assert!(error.to_string().contains(MISSING));
                assert!(started.elapsed() >= Duration::from_secs(2));
                assert!(server.requests.lock().len() > 1);
            } else {
                assert!(error.to_string().contains("Unrelated 404"));
                server.assert_native_requests(1);
            }
        }
    }
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
