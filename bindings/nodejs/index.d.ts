/*
 * Copyright 2025 TiDB Cloud
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

import type { Readable } from "node:stream";

// native.d.ts is generated; keep the public export list explicit.
// Re-export new public native symbols here, using `export type` unless index.js also exports the value.
export { Client } from "./native";
export type {
  Connection,
  ConnectionInfo,
  Field,
  Row,
  RowIterator,
  RowIteratorExt,
  RowOrStats,
  Schema,
  ServerStats,
  ValueOptions,
} from "./native";

// Augment the native return type of queryIter() with the methods installed by index.js.
declare module "./native" {
  interface RowIterator {
    [Symbol.asyncIterator](): AsyncGenerator<Exclude<Awaited<ReturnType<RowIterator["next"]>>, null>, void, unknown>;
    /** Return an object-mode readable stream for the query result. */
    stream(): Readable;
  }
}
