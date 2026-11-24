// Copyright 2026 TiDB Cloud
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

#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio::time::timeout;

#[tokio::test]
async fn credentials_are_masked_in_the_running_cli() {
    let home = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let dsn_canary = "dsn-canary";
    let mut child = Command::new(env!("CARGO_BIN_EXE_lakesql"))
        .args([
            "-nApcli-canary",
            "--check",
            "--dsn",
            &format!("lake://root:{dsn_canary}@127.0.0.1:{port}/?sslmode=disable&access_token=token-canary&presign=off"),
            "--set=access_token=set-canary",
        ])
        .env("HOME", home.path())
        .env("NO_PROXY", "127.0.0.1")
        .env("no_proxy", "127.0.0.1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    // Wait until the child has parsed argv and sent the request, then keep it
    // alive by withholding the reply while examining the OS-visible command.
    let (socket, _) = timeout(Duration::from_secs(10), listener.accept())
        .await
        .expect("CLI did not connect")
        .unwrap();
    let mut lines = BufReader::new(socket).lines();
    let headers = timeout(Duration::from_secs(5), async {
        let mut headers = String::new();
        while let Some(line) = lines.next_line().await.unwrap() {
            if line.is_empty() {
                break;
            }
            headers.push_str(&line);
            headers.push('\n');
        }
        headers
    })
    .await
    .unwrap();
    assert!(headers.starts_with("POST /v1/session/login"));
    assert!(headers
        .to_ascii_lowercase()
        .contains("authorization: bearer token-canary\n"));

    let pid = child.id().unwrap();
    #[cfg(target_os = "linux")]
    let argv = std::fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap();
    #[cfg(target_os = "macos")]
    let argv = {
        let output = std::process::Command::new("/bin/ps")
            .args(["-ww", "-p", &pid.to_string(), "-o", "command="])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    for value in ["cli-canary", dsn_canary, "token-canary", "set-canary"] {
        assert!(!argv.contains(value), "sentinel still visible: {value}");
    }
    assert!(argv.contains("-nAp**********"));
    assert!(argv.contains("root:**********@127.0.0.1"));
    assert!(argv.contains("access_token=************&presign=off"));
    assert!(argv.contains("--set=access_token=**********"));
    child.kill().await.unwrap();
}
