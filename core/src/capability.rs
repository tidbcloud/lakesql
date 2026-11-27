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

use semver::Version;

#[derive(Debug, Default)]
pub struct Capability {
    /// SQL `KILL QUERY '<query_id>'`, including cancellation by the owner.
    pub kill_query_by_id: bool,
}

impl Capability {
    pub fn from_server_version(version: &Version) -> Self {
        Self {
            kill_query_by_id: version > &Version::new(1, 2, 965),
        }
    }
}
