# Copyright 2025 TiDB Cloud
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

Feature: Lake Driver

    Scenario: Select Simple
        Given A new Lake Driver Client
        Then Select string "Hello, Lake!" should be equal to "Hello, Lake!"

    Scenario: Select params
        Given A new Lake Driver Client
        Then Select params binding

    Scenario: Select Types
        Given A new Lake Driver Client
        Then Select types should be expected native types

    Scenario: Select Iter
        Given A new Lake Driver Client
        Then Select numbers should iterate all rows

    Scenario: Insert and Select
        Given A new Lake Driver Client
        When Create a test table
        Then Insert and Select should be equal

    Scenario: Stream Load
        Given A new Lake Driver Client
        When Create a test table
        Then Stream load and Select should be equal

    Scenario: Load file Stage
        Given A new Lake Driver Client
        When Create a test table
        Then Load file with Stage and Select should be equal

    Scenario: Load file Streaming
        Given A new Lake Driver Client
        When Create a test table
        Then Load file with Streaming and Select should be equal

    Scenario: Temp table
        Given A new Lake Driver Client
        Then Temp table is cleaned up when conn is dropped

    Scenario: Query ID tracking
        Given A new Lake Driver Client
        Then last_query_id should return query ID after execution

    Scenario: Kill Query API Error Handling
        Given A new Lake Driver Client
        Then killQuery should return error for non-existent query ID

    Scenario: Heartbeat
        Given A new Lake Driver Client
        Then Query should not timeout

	Scenario: Close result set
        Given A new Lake Driver Client
		Then Drop result set should close it