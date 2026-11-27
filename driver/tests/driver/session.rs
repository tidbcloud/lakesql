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
async fn change_password_and_reconnect() {
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

    let mut user_dsn = url::Url::parse(dsn).unwrap();
    user_dsn.set_username("u1").unwrap();
    user_dsn.set_password(Some("p1")).unwrap();
    let user_conn = Client::new(user_dsn.to_string()).get_conn().await.unwrap();
    assert!(user_conn.query_row("select 1").await.unwrap().is_some());
    user_conn.close().await.unwrap();

    assert_eq!(
        conn.exec("alter user u1 identified by 'p2'").await.unwrap(),
        0
    );

    user_dsn.set_password(Some("p2")).unwrap();
    let user_conn = Client::new(user_dsn.to_string()).get_conn().await.unwrap();
    let row = user_conn.query_row("select 1").await.unwrap().unwrap();
    let (val,): (i64,) = row.try_into().unwrap();
    assert_eq!(val, 1);
    user_conn.close().await.unwrap();
    conn.exec("drop user u1").await.unwrap();
}
