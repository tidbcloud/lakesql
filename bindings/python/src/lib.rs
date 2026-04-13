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

mod asyncio;
mod blocking;
mod exceptions;
mod types;
mod utils;

use pyo3::prelude::*;

use crate::asyncio::{AsyncLakeClient, AsyncLakeConnection};
use crate::blocking::{BlockingLakeClient, BlockingLakeConnection, BlockingLakeCursor};
use crate::types::{ConnectionInfo, Field, Row, RowIterator, Schema, ServerStats};

#[pymodule]
fn _lake_driver(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<AsyncLakeClient>()?;
    m.add_class::<AsyncLakeConnection>()?;
    m.add_class::<BlockingLakeClient>()?;
    m.add_class::<BlockingLakeConnection>()?;
    m.add_class::<BlockingLakeCursor>()?;
    m.add_class::<ConnectionInfo>()?;
    m.add_class::<Schema>()?;
    m.add_class::<Row>()?;
    m.add_class::<Field>()?;
    m.add_class::<RowIterator>()?;
    m.add_class::<ServerStats>()?;

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .target(env_logger::Target::Stdout)
        .init();

    // Register PEP-249 compliant exceptions
    exceptions::register_exceptions(py, m)?;

    Ok(())
}
