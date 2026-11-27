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
use crate::error_code::ErrorCode;

/// Registration can lag behind publication of the client-generated query ID.
const KILL_REGISTRATION_SLACK: Duration = Duration::from_secs(2);

#[derive(Deserialize)]
struct DetachedQueryState {
    state: String,
    error: Option<ErrorCode>,
}

impl APIClient {
    /// Cancel a running query by ID. Tracked queries use their own HTTP query
    /// session and node. Before the first response, a query-not-found 404 is
    /// retried with backoff for up to `wait_time_secs` plus two seconds.
    ///
    /// Other IDs use SQL `KILL QUERY` without joining the caller's transaction
    /// or changing its query ID or routing state. The server checks privileges
    /// (owners can cancel their own queries; global `SUPER` can cancel others).
    /// SQL cancellation requires server support (v1.2.966-nightly or later).
    pub async fn kill_query(&self, query_id: &str) -> Result<()> {
        if query_id.is_empty() || query_id.chars().any(char::is_control) {
            return Err(Error::BadArgument(
                "query ID must be nonempty and contain no control characters".into(),
            ));
        }
        let target = self.owned_queries.lock().kill_target(query_id);
        let Some(mut target) = target else {
            return self.kill_query_by_sql(query_id).await;
        };
        let deadline = tokio::time::Instant::now() + self.kill_registration_wait();
        let mut delay = Duration::from_millis(50);
        loop {
            let result = self
                .end_query(query_id, true, target.node_id.as_deref())
                .await;
            let error = match result {
                Err(e) if target.retry_not_found && is_query_not_found(&e, query_id) => e,
                result => return result,
            };
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(error);
            }
            sleep(delay.min(remaining)).await;
            delay = (delay * 2).min(Duration::from_secs(1));
            // Pick up the first response's node, but never switch IDs or replay
            // via SQL if the original ID was replaced or is no longer tracked.
            match self.owned_queries.lock().kill_target(query_id) {
                Some(next) => target = next,
                None => return Err(error),
            }
        }
    }

    fn kill_registration_wait(&self) -> Duration {
        let wait = self
            .wait_time_secs
            .unwrap_or(10)
            .clamp(0, i64::from(u32::MAX));
        Duration::from_secs(wait as u64) + KILL_REGISTRATION_SLACK
    }

    async fn kill_query_by_sql(&self, query_id: &str) -> Result<()> {
        if let Some(version) = &self.server_version {
            if !self.capability.kill_query_by_id {
                return Err(Error::BadArgument(format!(
                    "cross-connection cancellation needs a server newer than v1.2.965 (server is v{version})"
                )));
            }
        }
        // Backslash is an escape character in SQL string literals.
        let sql = format!(
            "KILL QUERY '{}'",
            query_id.replace('\\', "\\\\").replace('\'', "''")
        );
        self.exec_detached_kill(&sql).await.map_err(|e| match e {
            Error::WithContext {
                inner, retry_times, ..
            } => Error::WithContext {
                inner,
                request_kind: Some(RequestKind::QueryKill),
                query_id: Some(query_id.into()),
                retry_times,
            },
            other => other
                .with_context(RequestKind::QueryKill)
                .with_query_id(query_id),
        })
    }

    /// Use the regular HTTP session cookie and configured credentials. Only the
    /// roles are copied into the SQL session, so no transaction, sticky node,
    /// or caller session settings are passed to this statement. Never apply its
    /// response to the caller's session.
    async fn exec_detached_kill(&self, sql: &str) -> Result<()> {
        let session = {
            let state = self.session_state.lock();
            let mut session = SessionState::default();
            session.role.clone_from(&state.role);
            session.secondary_roles.clone_from(&state.secondary_roles);
            session
        };
        let req = QueryRequest::new(sql).with_session(Some(session));
        let query_id = self.gen_query_id();
        let headers = self.make_headers(Some(&query_id))?;
        let builder = self.cli.post(self.endpoint.join("v1/query")?).json(&req);
        let request = self
            .auth
            .wrap(builder)?
            .headers(headers)
            .timeout(self.page_request_timeout)
            .build()?;
        let response = self
            .query_request_helper(request, false, RequestKind::QueryKill, None)
            .await?;
        let mut resp: QueryResponse = Self::decode_detached_page(response)?;
        loop {
            if let Some(error) = resp.error {
                return Err(Error::QueryFailed(error));
            }
            let Some(next_uri) = resp.next_uri else {
                return detached_outcome(&resp.state);
            };
            if resp.final_uri.as_deref() == Some(&next_uri) {
                detached_outcome(&resp.state)?;
            } else if resp.stats_uri.as_deref() == Some(&next_uri) {
                // The state-only URI has no result page envelope.
                tokio::time::timeout(
                    self.page_request_timeout,
                    self.wait_for_detached_query(&resp.id, &next_uri, &resp.node_id),
                )
                .await
                .map_err(|_| Error::Request("timed out waiting for KILL QUERY status".into()))??;
            } else {
                let response = self
                    .send_page_request(&resp.id, &next_uri, &resp.node_id, false)
                    .await?;
                resp = Self::decode_detached_page(response)?;
                continue;
            }
            // The final response is cleanup only; the execution outcome above
            // is authoritative. Use the configured endpoint, not a supplied URI.
            if let Err(e) = self.final_query(&resp.id, resp.node_id.as_deref()).await {
                debug!("failed to finalize detached query {}: {e}", resp.id);
            }
            return Ok(());
        }
    }

    async fn wait_for_detached_query(
        &self,
        query_id: &str,
        state_uri: &str,
        node_id: &Option<String>,
    ) -> Result<()> {
        let mut delay = Duration::from_millis(50);
        loop {
            let response = self
                .send_page_request(query_id, state_uri, node_id, false)
                .await?;
            let status: DetachedQueryState = Self::decode_detached_page(response)?;
            if let Some(error) = status.error {
                return Err(Error::QueryFailed(error));
            }
            match status.state.as_str() {
                "Running" | "Starting" => sleep(delay).await,
                _ => return detached_outcome(&status.state),
            }
            delay = (delay * 2).min(Duration::from_secs(1));
        }
    }

    fn decode_detached_page<T: for<'a> Deserialize<'a>>(response: HttpResponseData) -> Result<T> {
        if response.status != StatusCode::OK {
            return Err(Self::status_body_to_error(response.status, &response.body));
        }
        json_from_slice(&response.body)
    }
}

fn detached_outcome(state: &str) -> Result<()> {
    if state == "Succeeded" {
        Ok(())
    } else {
        Err(Error::Request(format!(
            "KILL QUERY did not complete successfully (state: {state})"
        )))
    }
}

fn is_query_not_found(error: &Error, query_id: &str) -> bool {
    let body = match error {
        Error::WithContext { inner, .. } => return is_query_not_found(inner, query_id),
        Error::Response {
            status: StatusCode::NOT_FOUND,
            msg,
        } => msg,
        _ => return false,
    };
    let prefix = format!("Query ID {query_id} not found on node ");
    if body.starts_with(&prefix) {
        return true;
    }
    serde_json::from_str::<ResponseWithErrorCode>(body)
        .is_ok_and(|response| response.error.message.starts_with(&prefix))
}
