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

use std::collections::BTreeMap;

use crate::error::{Error, RequestKind, Result};
use crate::error_code::{ErrorCode, ResponseWithErrorCode};
use crate::session::SessionState;
use reqwest::StatusCode;
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct LoginRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<BTreeMap<String, String>>,
}

impl From<&SessionState> for LoginRequest {
    fn from(value: &SessionState) -> Self {
        // Login validates databases in the default catalog. For other catalogs,
        // defer applying the database until /v1/query, which receives both fields.
        let database = match value.catalog.as_deref() {
            Some(catalog) if !catalog.eq_ignore_ascii_case("default") => None,
            _ => value.database.clone(),
        };
        Self {
            role: value.role.clone(),
            settings: value.settings.clone(),
            database,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LoginResponse {
    Ok { version: String },
    Err { error: ErrorCode },
}

pub(crate) fn decode_login_version(status: StatusCode, body: &[u8]) -> Result<Version> {
    // A server may still send credentials or echo sensitive values in an error.
    // Keep the HTTP status, server error code, and decode location, not the text.
    if status != StatusCode::OK {
        let error = match serde_json::from_slice::<ResponseWithErrorCode>(body) {
            Ok(resp) => {
                Error::from_status_error_code(status, sanitized_auth_error(resp.error.code))
            }
            Err(_) => Error::Response {
                status,
                msg: " authentication response body omitted".to_string(),
            },
        };
        return Err(error.with_context(RequestKind::Login));
    }
    let response = serde_json::from_slice(body).map_err(|e| {
        Error::Decode(format!(
            "invalid authentication JSON response ({:?}, line {}, column {})",
            e.classify(),
            e.line(),
            e.column()
        ))
        .with_context(RequestKind::Login)
    })?;
    match response {
        LoginResponse::Ok { version } => version.parse().map_err(|_| {
            Error::Decode("invalid server version in login response".into())
                .with_context(RequestKind::Login)
        }),
        LoginResponse::Err { error } => Err(Error::AuthFailure(sanitized_auth_error(error.code))),
    }
}

fn sanitized_auth_error(code: u16) -> ErrorCode {
    ErrorCode {
        code,
        message: "authentication request failed (server message omitted)".to_string(),
        detail: None,
    }
}
