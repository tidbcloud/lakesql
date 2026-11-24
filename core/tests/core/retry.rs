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

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lake_client::{APIClient, Error};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    query_id: Option<String>,
    accept: Option<String>,
}

type ResponseHandler = dyn Fn(usize, &RecordedRequest) -> (u16, &'static str, String) + Send + Sync;

fn ok_query_response(value: &str) -> String {
    format!(
        r#"{{
  "id": "query-id",
  "node_id": null,
  "session_id": null,
  "session": null,
  "schema": [{{"name":"c1","type":"String"}}],
  "data": [["{value}"]],
  "state": "Succeeded",
  "settings": null,
  "error": null,
  "warnings": null,
  "stats": {{
    "scan_progress": {{"rows":0,"bytes":0}},
    "write_progress": {{"rows":0,"bytes":0}},
    "result_progress": {{"rows":0,"bytes":0}},
    "spill_progress": {{"file_nums":0,"bytes":0}},
    "running_time_ms": 0.0,
    "total_scan": null
  }},
  "result_timeout_secs": null,
  "stats_uri": null,
  "final_uri": null,
  "next_uri": null,
  "kill_uri": null
}}"#
    )
}

fn unauthorized_json_body() -> String {
    r#"{"error":{"code":3900,"message":"unauthorized","detail":null}}"#.to_string()
}

fn logic_error_json_body(code: u16, message: &str) -> String {
    format!(r#"{{"error":{{"code":{code},"message":"{message}","detail":null}}}}"#)
}

fn query_response_with_next_uri(value: &str, next_uri: &str) -> String {
    format!(
        r#"{{
  "id": "query-id",
  "node_id": null,
  "session_id": null,
  "session": null,
  "schema": [{{"name":"c1","type":"String"}}],
  "data": [["{value}"]],
  "state": "Running",
  "settings": null,
  "error": null,
  "warnings": null,
  "stats": {{
    "scan_progress": {{"rows":0,"bytes":0}},
    "write_progress": {{"rows":0,"bytes":0}},
    "result_progress": {{"rows":0,"bytes":0}},
    "spill_progress": {{"file_nums":0,"bytes":0}},
    "running_time_ms": 0.0,
    "total_scan": null
  }},
  "result_timeout_secs": null,
  "stats_uri": null,
  "final_uri": null,
  "next_uri": "{next_uri}",
  "kill_uri": null
}}"#
    )
}

fn build_dsn(port: u16, retry_count: u32, extra: &str) -> String {
    if extra.is_empty() {
        format!(
            "lake://127.0.0.1:{port}/default?sslmode=disable&login=disable&retry_delay_secs=0&retry_count={retry_count}"
        )
    } else {
        format!(
            "lake://127.0.0.1:{port}/default?sslmode=disable&login=disable&retry_delay_secs=0&retry_count={retry_count}&{extra}"
        )
    }
}

fn spawn_test_server(
    handler: Arc<ResponseHandler>,
) -> (u16, Arc<Mutex<Vec<RecordedRequest>>>, JoinHandle<()>) {
    spawn_test_server_with_first_body_delay(handler, Duration::ZERO)
}

fn spawn_test_server_with_first_body_delay(
    handler: Arc<ResponseHandler>,
    first_body_delay: Duration,
) -> (u16, Arc<Mutex<Vec<RecordedRequest>>>, JoinHandle<()>) {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std_listener.set_nonblocking(true).unwrap();
    let port = std_listener.local_addr().unwrap().port();
    let listener = TcpListener::from_std(std_listener).unwrap();

    let requests = Arc::new(Mutex::new(Vec::<RecordedRequest>::new()));
    let requests_for_task = Arc::clone(&requests);
    let handle = tokio::spawn(async move {
        let mut idx = 0usize;
        while let Ok((mut stream, _)) = listener.accept().await {
            let req = match read_request(&mut stream).await {
                Ok(req) => req,
                Err(_) => continue,
            };
            requests_for_task.lock().unwrap().push(req.clone());
            let (status, content_type, body) = handler(idx, &req);
            let delay = if idx == 0 {
                first_body_delay
            } else {
                Duration::ZERO
            };
            idx += 1;
            let _ = write_response(&mut stream, status, content_type, &body, delay).await;
        }
    });

    (port, requests, handle)
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn read_request(stream: &mut TcpStream) -> io::Result<RecordedRequest> {
    let mut buf = Vec::<u8>::new();
    let mut chunk = [0u8; 1024];

    let header_end = loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before request headers",
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request headers too large",
            ));
        }
    };

    let headers_end = header_end + 4;
    let headers_bytes = &buf[..header_end];
    let headers_str = std::str::from_utf8(headers_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid request headers"))?;

    let mut lines = headers_str.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?;
    let mut request_line_parts = request_line.split_whitespace();
    let method = request_line_parts.next().unwrap_or_default().to_string();
    let path = request_line_parts.next().unwrap_or_default().to_string();

    let mut content_length = 0usize;
    let mut authorization = None;
    let mut query_id = None;
    let mut accept = None;

    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let value = v.trim().to_string();
        match key.as_str() {
            "content-length" => content_length = value.parse::<usize>().unwrap_or(0),
            "authorization" => authorization = Some(value),
            "x-databend-query-id" => query_id = Some(value),
            "accept" => accept = Some(value),
            _ => {}
        }
    }

    let already_read_body = buf.len().saturating_sub(headers_end);
    if content_length > already_read_body {
        let mut remaining = vec![0u8; content_length - already_read_body];
        stream.read_exact(&mut remaining).await?;
    }

    Ok(RecordedRequest {
        method,
        path,
        authorization,
        query_id,
        accept,
    })
}

async fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
    delay: Duration,
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Unknown",
    };
    let body_bytes = body.as_bytes();
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    stream.write_all(head.as_bytes()).await?;
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    stream.write_all(body_bytes).await?;
    stream.shutdown().await
}

fn create_temp_token_file(initial_token: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "lake-client-retry-token-{}-{}.txt",
        std::process::id(),
        Uuid::new_v4()
    ));
    std::fs::write(&path, initial_token).unwrap();
    path
}

#[tokio::test]
async fn retry_503_then_success() {
    let handler: Arc<ResponseHandler> = Arc::new(|idx, req| {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/v1/query");
        if idx == 0 {
            (
                503,
                "text/plain",
                "server unavailable, retry later".to_string(),
            )
        } else {
            (200, "application/json", ok_query_response("42"))
        }
    });
    let (port, requests, server) = spawn_test_server(handler);

    let dsn = build_dsn(port, 2, "");
    let client = APIClient::new(&dsn, None).await.unwrap();
    let result = client.query_all("select 42", None).await.unwrap();

    assert_eq!(result.data, vec![vec![Some("42".to_string())]]);
    assert_eq!(requests.lock().unwrap().len(), 2);

    server.abort();
}

#[tokio::test]
async fn query_start_does_not_retry_a_body_timeout() {
    let handler: Arc<ResponseHandler> =
        Arc::new(|_, _| (200, "application/json", ok_query_response("42")));
    let (port, requests, server) =
        spawn_test_server_with_first_body_delay(handler, Duration::from_millis(1250));
    let dsn = build_dsn(port, 2, "presign=off&page_request_timeout_secs=1");
    let client = APIClient::new(&dsn, None).await.unwrap();

    assert!(client.query_all("select 42", None).await.is_err());
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/query");
    assert_eq!(client.last_query_id(), requests[0].query_id);
    server.abort();
}

#[tokio::test]
async fn query_id_is_visible_before_first_response() {
    for outcome in ["success", "sql-error", "invalid-response", "abort"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (received, request) = oneshot::channel();
        let (release, gate) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            received
                .send(read_request(&mut stream).await.unwrap())
                .unwrap();
            if gate.await.is_err() {
                return;
            }
            let mut body: serde_json::Value =
                serde_json::from_str(&ok_query_response("42")).unwrap();
            if outcome == "sql-error" {
                body["data"] = serde_json::json!([]);
                body["state"] = serde_json::json!("Failed");
                body["error"] = serde_json::json!({"code": 1005, "message": "invalid SQL"});
            }
            let body = if outcome == "invalid-response" {
                "not JSON".to_owned()
            } else {
                body.to_string()
            };
            write_response(&mut stream, 200, "application/json", &body, Duration::ZERO)
                .await
                .unwrap();
        });
        let client = APIClient::new(&build_dsn(port, 1, "presign=off"), None)
            .await
            .unwrap();
        assert_eq!(client.last_query_id(), None);
        let query_client = client.clone();
        let query = tokio::spawn(async move { query_client.query_all("select 42", None).await });
        let request = tokio::time::timeout(Duration::from_secs(5), request)
            .await
            .expect("query submission did not reach the mock server")
            .unwrap();
        assert_eq!(request.path, "/v1/query");
        let submitted_id = request.query_id.expect("missing submitted query ID");
        assert_eq!(
            client.last_query_id().as_deref(),
            Some(submitted_id.as_str())
        );

        if outcome == "abort" {
            query.abort();
            assert!(matches!(query.await, Err(err) if err.is_cancelled()));
            assert_eq!(
                client.last_query_id().as_deref(),
                Some(submitted_id.as_str())
            );
            drop(release);
        } else {
            release.send(()).unwrap();
            let result = query.await.unwrap();
            match outcome {
                "success" => assert!(result.is_ok()),
                "sql-error" => {
                    assert!(matches!(result, Err(Error::QueryFailed(code)) if code.code == 1005))
                }
                "invalid-response" => assert!(matches!(result, Err(Error::Decode(_)))),
                _ => unreachable!(),
            }
            let expected = if outcome == "invalid-response" {
                submitted_id.as_str()
            } else {
                "query-id"
            };
            assert_eq!(client.last_query_id().as_deref(), Some(expected));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn query_id_rolls_back_when_no_attempt_was_sent() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    // Try both an HTTP request construction error and a refused connection.
    for extra in ["presign=off&warehouse=bad%0Aname", "presign=off"] {
        let client = APIClient::new(&build_dsn(port, 2, extra), None)
            .await
            .unwrap();
        client.set_last_query_id(Some("previous-id".to_owned()));
        assert!(client.query_all("select 42", None).await.is_err());
        assert_eq!(client.last_query_id().as_deref(), Some("previous-id"));
    }
}

#[tokio::test]
async fn query_id_survives_connect_failure_after_a_503() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_request(&mut stream).await.unwrap();
        drop(listener); // The retry cannot connect after this response.
        write_response(
            &mut stream,
            503,
            "text/plain",
            "unavailable",
            Duration::ZERO,
        )
        .await
        .unwrap();
        request.query_id.unwrap()
    });
    let client = APIClient::new(&build_dsn(port, 2, "presign=off"), None)
        .await
        .unwrap();
    client.set_last_query_id(Some("previous-id".to_owned()));
    assert!(matches!(
        client.query_all("select 42", None).await,
        Err(Error::WithContext {
            retry_times: Some(1),
            ..
        })
    ));
    assert_eq!(client.last_query_id(), Some(server.await.unwrap()));
}

#[tokio::test]
async fn login_not_found_is_an_error() {
    let handler: Arc<ResponseHandler> = Arc::new(|_, _| (404, "text/plain", "not found".into()));
    let (port, requests, server) = spawn_test_server(handler);
    let dsn = format!("lake://127.0.0.1:{port}/default?sslmode=disable&presign=off");
    let err = APIClient::new(&dsn, None)
        .await
        .err()
        .expect("a missing login endpoint should fail the connection");
    assert_eq!(err.status_code().unwrap().as_u16(), 404);
    assert!(matches!(
        err,
        Error::WithContext {
            request_kind: Some(lake_client::RequestKind::Login),
            ..
        }
    ));
    assert_eq!(requests.lock().unwrap().len(), 1);
    server.abort();
}

#[tokio::test]
async fn arrow_is_available_with_login_disabled() {
    let handler: Arc<ResponseHandler> =
        Arc::new(|_, _| (200, "application/json", ok_query_response("42")));
    let (port, requests, server) = spawn_test_server(handler);
    let dsn = build_dsn(port, 1, "presign=off&query_result_format=arrow");
    let client = APIClient::new(&dsn, None).await.unwrap();
    assert!(client.capability().streaming_load);
    assert!(client.capability().server_side_params);
    client.query_all("select 42", None).await.unwrap();
    client
        .query_all_inner("select 42", true, None)
        .await
        .unwrap();

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|req| req.path == "/v1/query"));
    let arrow = Some("application/vnd.apache.arrow.stream");
    assert_eq!(requests[0].accept.as_deref(), arrow);
    assert_ne!(requests[1].accept.as_deref(), arrow);
    server.abort();
}

#[tokio::test]
async fn retry_401_reapplies_refreshed_session_token() {
    for wrapped in [false, true] {
        check_refreshed_session_token(wrapped).await;
    }
}

async fn check_refreshed_session_token(wrapped: bool) {
    let old = "old-test-token";
    let new = "new-test-token";
    let refresh = "test-refresh-token";
    let handler: Arc<ResponseHandler> = Arc::new(move |idx, req| {
        let body = match (idx, req.path.as_str(), req.authorization.as_deref()) {
            (0, "/v1/session/login", _) => serde_json::json!({
                "version": "1.2.925-nightly",
                "tokens": {"session_token": old, "refresh_token": refresh}
            })
            .to_string(),
            (1, "/v1/query", Some("Bearer old-test-token")) => {
                return (
                    401,
                    "application/json",
                    logic_error_json_body(5101, "session token expired"),
                );
            }
            (2, "/v1/session/refresh", Some("Bearer test-refresh-token")) => {
                let tokens = serde_json::json!({"session_token": new, "refresh_token": refresh});
                if wrapped {
                    serde_json::json!({"tokens": tokens}).to_string()
                } else {
                    tokens.to_string()
                }
            }
            (3, "/v1/query", Some("Bearer new-test-token")) => ok_query_response("42"),
            (4, "/v1/session/logout", Some("Bearer new-test-token")) => "{}".to_string(),
            _ => panic!("unexpected mock request: {req:?}"),
        };
        (200, "application/json", body)
    });
    let (port, requests, server) = spawn_test_server(handler);
    let dsn = format!("lake://127.0.0.1:{port}/default?sslmode=disable&presign=off&session_token=enable&retry_delay_secs=0&retry_count=1");
    let client = APIClient::new(&dsn, None).await.unwrap();
    assert!(client.capability().streaming_load);
    assert!(client.capability().arrow_data);
    assert!(client.capability().server_side_params);
    let response = client.query_all("select 42", None).await.unwrap();
    assert_eq!(response.data, vec![vec![Some("42".to_string())]]);
    client.close().await;
    assert_eq!(requests.lock().unwrap().len(), 5);
    server.abort();
}

#[tokio::test]
async fn retry_401_with_access_token_file_reload_then_success() {
    let token_file = create_temp_token_file("old-token");
    let token_file_for_server = token_file.clone();

    let handler: Arc<ResponseHandler> = Arc::new(move |idx, req| {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/v1/query");
        if idx == 0 {
            assert_eq!(
                req.authorization.as_deref(),
                Some("Bearer old-token"),
                "first request should use token before reload"
            );
            std::fs::write(&token_file_for_server, "new-token").unwrap();
            (401, "application/json", unauthorized_json_body())
        } else {
            assert_eq!(
                req.authorization.as_deref(),
                Some("Bearer new-token"),
                "second request should use reloaded token file value"
            );
            (200, "application/json", ok_query_response("reloaded"))
        }
    });
    let (port, requests, server) = spawn_test_server(handler);

    let dsn = build_dsn(
        port,
        3,
        &format!("access_token_file={}", token_file.to_string_lossy()),
    );
    let client = APIClient::new(&dsn, None).await.unwrap();
    let result = client.query_all("select 'reloaded'", None).await.unwrap();

    assert_eq!(result.data, vec![vec![Some("reloaded".to_string())]]);
    assert_eq!(requests.lock().unwrap().len(), 2);

    server.abort();
    let _ = std::fs::remove_file(&token_file);
}

#[tokio::test]
async fn retry_401_auth_reload_stops_at_max_retries() {
    let token_file = create_temp_token_file("still-invalid-token");
    let handler: Arc<ResponseHandler> = Arc::new(|_idx, req| {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/v1/query");
        (401, "application/json", unauthorized_json_body())
    });
    let (port, requests, server) = spawn_test_server(handler);

    let retry_count = 3u32;
    let dsn = build_dsn(
        port,
        retry_count,
        &format!("access_token_file={}", token_file.to_string_lossy()),
    );
    let client = APIClient::new(&dsn, None).await.unwrap();
    let err = match client.query_all("select 1", None).await {
        Ok(_) => panic!("expected unauthorized error"),
        Err(err) => err,
    };

    match err {
        Error::AuthFailure(ec) => assert_eq!(ec.code, 3900),
        other => panic!("expected AuthFailure, got {other}"),
    }
    assert_eq!(requests.lock().unwrap().len(), retry_count as usize);

    server.abort();
    let _ = std::fs::remove_file(&token_file);
}

#[tokio::test]
async fn start_query_404_keeps_logic_error() {
    let handler: Arc<ResponseHandler> = Arc::new(|idx, req| {
        assert_eq!(idx, 0);
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/v1/query");
        (
            404,
            "application/json",
            logic_error_json_body(2001, "route /v1/query not found"),
        )
    });
    let (port, _requests, server) = spawn_test_server(handler);

    let dsn = build_dsn(port, 2, "");
    let client = APIClient::new(&dsn, None).await.unwrap();
    let err = match client.query_all("select 1", None).await {
        Ok(_) => panic!("expected logic error"),
        Err(err) => err,
    };
    match err {
        Error::Logic(status, ec) => {
            assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
            assert_eq!(ec.code, 2001);
            assert_eq!(ec.message, "route /v1/query not found");
        }
        other => panic!("expected Logic(404), got {other}"),
    }

    server.abort();
}

#[tokio::test]
async fn query_page_404_maps_to_query_not_found() {
    let handler: Arc<ResponseHandler> = Arc::new(|idx, req| match idx {
        0 => {
            assert_eq!(req.method, "POST");
            assert_eq!(req.path, "/v1/query");
            (
                200,
                "application/json",
                query_response_with_next_uri("first-page", "/v1/query/page/1"),
            )
        }
        1 => {
            assert_eq!(req.method, "GET");
            assert_eq!(req.path, "/v1/query/page/1");
            (
                404,
                "application/json",
                logic_error_json_body(1003, "query no longer exists"),
            )
        }
        _ => panic!("unexpected request index: {idx}"),
    });
    let (port, _requests, server) = spawn_test_server(handler);

    let dsn = build_dsn(port, 2, "");
    let client = APIClient::new(&dsn, None).await.unwrap();
    let err = match client.query_all("select 1", None).await {
        Ok(_) => panic!("expected QueryNotFound"),
        Err(err) => err,
    };
    match err {
        Error::QueryNotFound(msg) => assert_eq!(msg, "query no longer exists"),
        other => panic!("expected QueryNotFound, got {other}"),
    }

    server.abort();
}

#[tokio::test]
async fn query_page_rejects_cross_origin_before_sending_credentials() {
    let handler: Arc<ResponseHandler> =
        Arc::new(|_, _| (200, "application/json", ok_query_response("ok")));
    let (port, requests, server) = spawn_test_server(handler);
    let client = APIClient::new(&build_dsn(port, 1, ""), None).await.unwrap();
    for uri in [
        "http://other.invalid/page".to_string(),
        format!(
            "http://127.0.0.1:{}/page",
            if port == 65535 { port - 1 } else { port + 1 }
        ),
        format!("https://127.0.0.1:{port}/page"),
    ] {
        let result = client.query_page("q", &uri, &None).await;
        assert!(matches!(result, Err(Error::BadArgument(_))));
    }
    assert!(requests.lock().unwrap().is_empty());
    client.query_page("q", "/page", &None).await.unwrap();
    assert_eq!(requests.lock().unwrap().len(), 1);
    server.abort();
}

const AUTH_RESPONSE_CANARY: &str = "AUTH_RESPONSE_CANARY";

fn rejected_auth_responses() -> [(u16, &'static str, String); 5] {
    let structured_error = serde_json::json!({
        "error": {"code": 3900, "message": AUTH_RESPONSE_CANARY, "detail": AUTH_RESPONSE_CANARY}
    })
    .to_string();
    [
        (
            200,
            "JSON response",
            format!("{{\"tokens\":\"{AUTH_RESPONSE_CANARY}\""),
        ),
        (
            200,
            "JSON response",
            serde_json::json!({"tokens": AUTH_RESPONSE_CANARY}).to_string(),
        ),
        (
            500,
            "500",
            format!("upstream error: {AUTH_RESPONSE_CANARY}"),
        ),
        (200, "3900", structured_error.clone()),
        (401, "3900", structured_error),
    ]
}

fn assert_auth_diagnostic(error: &Error, expected: &str) {
    let display = error.to_string();
    assert!(display.contains(expected), "missing diagnostic: {display}");
    assert!(!display.contains(AUTH_RESPONSE_CANARY));
    assert!(!format!("{error:?}").contains(AUTH_RESPONSE_CANARY));
}

#[tokio::test]
async fn login_does_not_expose_auth_response_text() {
    for (status, expected, body) in rejected_auth_responses() {
        let handler: Arc<ResponseHandler> = Arc::new(move |_, req| {
            assert_eq!(req.path, "/v1/session/login");
            (status, "application/json", body.clone())
        });
        let (port, requests, server) = spawn_test_server(handler);
        let dsn = format!("lake://127.0.0.1:{port}/default?sslmode=disable&presign=off&session_token=enable&retry_count=1");
        let error = APIClient::new(&dsn, None)
            .await
            .err()
            .expect("login must fail");
        assert_auth_diagnostic(&error, expected);
        assert_eq!(requests.lock().unwrap().len(), 1);
        server.abort();
    }
}

#[tokio::test]
async fn refresh_does_not_expose_auth_response_text() {
    for (status, expected, body) in rejected_auth_responses() {
        let handler: Arc<ResponseHandler> = Arc::new(move |_, req| match req.path.as_str() {
            "/v1/session/login" => (
                200,
                "application/json",
                serde_json::json!({
                    "version": "1.2.900",
                    "tokens": {"session_token": "mock-session", "refresh_token": "mock-refresh"}
                })
                .to_string(),
            ),
            "/v1/query" => (
                401,
                "application/json",
                logic_error_json_body(5101, "expired"),
            ),
            "/v1/session/refresh" => (status, "application/json", body.clone()),
            "/v1/session/logout" => (200, "application/json", "{}".to_string()),
            path => panic!("unexpected path: {path}"),
        });
        let (port, requests, server) = spawn_test_server(handler);
        let dsn = format!("lake://127.0.0.1:{port}/default?sslmode=disable&presign=off&session_token=enable&retry_count=1");
        let client = APIClient::new(&dsn, None).await.unwrap();
        let error = client
            .query_all("select 42", None)
            .await
            .err()
            .expect("refresh must fail");
        assert_auth_diagnostic(&error, expected);
        assert_eq!(requests.lock().unwrap().len(), 3);
        client.close().await;
        server.abort();
    }
}
