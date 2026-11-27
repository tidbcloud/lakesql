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

use crate::error_code::ErrorCode;
use crate::session::SessionState;
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

#[derive(Deserialize, Debug)]
pub struct LoginResponse {
    pub version: String,
}
#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum LoginResponseResult {
    Ok(LoginResponse),
    Err { error: ErrorCode },
}
