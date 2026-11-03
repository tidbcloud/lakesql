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

#[path = "../src/args.rs"]
pub mod args;
#[path = "../src/history.rs"]
mod history;
#[path = "../src/trace.rs"]
mod trace;

#[test]
fn tls_defaults_and_explicit_overrides() {
    let mut args = args::ConnectionArgs::default();
    args.configure_tls(None);
    assert!(!args.args.contains_key("sslmode"));
    args.configure_tls(Some(false));
    assert_eq!(args.args["sslmode"], "disable");
    args.configure_tls(Some(true));
    assert_eq!(args.args["sslmode"], "enable");
}

#[test]
fn credential_statements_are_not_retained_in_history() {
    for sql in [
        "ALTER USER u IDENTIFIED BY 'canary'",
        "create connection c storage_type='s3' secret_access_key='canary'",
        "SELECT 1; -- TOKEN=canary",
    ] {
        assert!(!history::should_record_history(sql));
    }
    assert!(history::should_record_history(
        "SELECT count(*) FROM events"
    ));
}

#[tokio::test]
async fn log_directory_is_private() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let guards = trace::init_logging(dir.path().to_str().unwrap(), "info")
        .await
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        stream.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    });
    let client = lake_client::APIClient::new(
        &format!("lake://127.0.0.1:{port}/?sslmode=disable&login=disable&retry_count=1"),
        None,
    )
    .await
    .unwrap();
    let _ = client
        .start_query("ALTER USER u IDENTIFIED BY 'SQL_LOG_CANARY'", false, None)
        .await;
    server.await.unwrap();
    log::logger().flush();
    drop(guards);
    let logs = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|p| std::fs::read_to_string(p.unwrap().path()).unwrap())
        .collect::<String>();
    assert!(logs.contains("start query"));
    assert!(!logs.contains("SQL_LOG_CANARY"));
}
