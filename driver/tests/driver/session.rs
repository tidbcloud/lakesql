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

use lake_driver::Client;

use crate::common::DEFAULT_DSN;

#[tokio::test]
async fn set_timezone() {
    let dsn = option_env!("TEST_LAKE_DSN").unwrap_or(DEFAULT_DSN);
    let client = Client::new(dsn.to_string());
    let conn = client.get_conn().await.unwrap();

    let row = conn.query_row("select timezone()").await.unwrap();
    assert!(row.is_some());
    let row = row.unwrap();
    let (val,): (String,) = row.try_into().unwrap();
    assert_eq!(val, "UTC");

    conn.exec("set timezone='Europe/London'").await.unwrap();
    let row = conn.query_row("select timezone()").await.unwrap();
    assert!(row.is_some());
    let row = row.unwrap();
    let (val,): (String,) = row.try_into().unwrap();
    assert_eq!(val, "Europe/London");
}

#[tokio::test]
async fn set_timezone_with_dsn() {
    let dsn = option_env!("TEST_LAKE_DSN").unwrap_or(DEFAULT_DSN);
    if dsn.starts_with("lake+flight://") {
        // skip dsn variable test for flight
        return;
    }
    let client = Client::new(format!("{dsn}&timezone=Europe/London"));
    let conn = client.get_conn().await.unwrap();

    let row = conn.query_row("select timezone()").await.unwrap();
    assert!(row.is_some());
    let row = row.unwrap();
    let (val,): (String,) = row.try_into().unwrap();
    assert_eq!(val, "Europe/London");
}

#[tokio::test]
async fn change_password() {
    let dsn = option_env!("TEST_LAKE_DSN").unwrap_or(DEFAULT_DSN);
    if dsn.starts_with("lake+flight://") {
        return;
    }
    let client = Client::new(dsn.to_string());
    let conn = client.get_conn().await.unwrap();
    let n = conn.exec("drop user if exists u1 ").await.unwrap();
    assert_eq!(n, 0);
    let n = conn
        .exec("create user u1 identified by 'p1' ")
        .await
        .unwrap();
    assert_eq!(n, 0);

    // Extract host from the original DSN to avoid hardcoding localhost:8000
    let parsed = url::Url::parse(dsn).unwrap();
    let host = parsed.host_str().unwrap();
    let port = parsed.port().unwrap_or(443);
    let db = parsed.path().trim_start_matches('/');
    let db = if db.is_empty() { "default" } else { db };
    // Preserve extra query params (warehouse, sslmode, etc.) from original DSN
    let mut extra_params = String::new();
    for (k, v) in parsed.query_pairs() {
        if k != "session_token" {
            extra_params.push_str(&format!("&{}={}", k, v));
        }
    }
    let user_dsn = format!(
        "lake://u1:p1@{}:{}/{}?session_token=enable{}",
        host, port, db, extra_params,
    );
    let client = Client::new(user_dsn);
    let conn = client.get_conn().await.unwrap();

    let n = conn
        .exec("alter user u1 identified by 'p2' ")
        .await
        .unwrap();
    assert_eq!(n, 0);

    let row = conn.query_row("select 1").await.unwrap();
    assert!(row.is_some());
    let row = row.unwrap();
    let (val,): (i64,) = row.try_into().unwrap();
    assert_eq!(val, 1);
}
