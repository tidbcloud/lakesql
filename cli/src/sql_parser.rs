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

use databend_common_ast::parser::token::{TokenKind, Tokenizer};

/// SQL parser utility for splitting SQL text into individual statements
pub struct SqlParser {
    delimiter: char,
    multi_line: bool,
    is_repl: bool,
}

impl SqlParser {
    pub fn new(delimiter: char, multi_line: bool, is_repl: bool) -> Self {
        Self {
            delimiter,
            multi_line,
            is_repl,
        }
    }

    /// Parse a single line incrementally, maintaining state
    /// Returns complete statements and updates the provided buffer
    pub fn parse_line(
        &self,
        line: &str,
        query_buffer: &mut String,
        err: &mut String,
    ) -> Vec<String> {
        if line.is_empty() {
            return vec![];
        }

        // Handle special commands for REPL mode
        if query_buffer.is_empty()
            && (line.starts_with('!')
                || line == "exit"
                || line == "quit"
                || line.to_uppercase().starts_with("PUT"))
        {
            return vec![line.to_owned()];
        }

        // Handle single line mode
        if !self.multi_line {
            if line.starts_with("--") {
                return vec![];
            } else {
                return vec![line.to_owned()];
            }
        }

        // Append line to query buffer
        if !query_buffer.is_empty() {
            query_buffer.push('\n');
        }
        query_buffer.push_str(line);

        // Parse the accumulated query to find statement boundaries
        let parsed = self.parse_statements(query_buffer);

        *err = parsed.err;
        // Update the buffer with remaining text
        *query_buffer = parsed.remaining;

        // Return complete statements
        parsed.statements
    }

    /// Find the byte offset where an unclosed block comment (`/*`) begins.
    /// Returns `None` if all block comments are properly closed.
    /// Skips `--` line comments, `$$` dollar-quoted strings, and respects
    /// single/double-quoted string literals.
    /// Block comments are non-nested (matches tokenizer behaviour): the
    /// first `*/` always closes the comment.
    fn unclosed_block_comment_start(s: &str) -> Option<usize> {
        let mut in_block_comment = false;
        let mut open_pos = None;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut in_dollar_quote = false;
        let mut in_line_comment = false;
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];

            // Newline resets line-comment state.
            if c == b'\n' {
                in_line_comment = false;
                i += 1;
                continue;
            }
            if in_line_comment {
                i += 1;
                continue;
            }

            // Inside a block comment, only look for `*/`.
            if in_block_comment {
                if c == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    in_block_comment = false;
                    open_pos = None;
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }

            // Inside a dollar-quoted string, only look for `$$`.
            if in_dollar_quote {
                if c == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'$' {
                    in_dollar_quote = false;
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }

            match c {
                b'\'' if !in_double_quote => in_single_quote = !in_single_quote,
                b'"' if !in_single_quote => in_double_quote = !in_double_quote,
                b'$' if !in_single_quote
                    && !in_double_quote
                    && i + 1 < bytes.len()
                    && bytes[i + 1] == b'$' =>
                {
                    in_dollar_quote = true;
                    i += 2;
                    continue;
                }
                b'-' if !in_single_quote
                    && !in_double_quote
                    && i + 1 < bytes.len()
                    && bytes[i + 1] == b'-' =>
                {
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                b'/' if !in_single_quote
                    && !in_double_quote
                    && i + 1 < bytes.len()
                    && bytes[i + 1] == b'*' =>
                {
                    in_block_comment = true;
                    open_pos = Some(i);
                    i += 2;
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
        if in_block_comment {
            open_pos
        } else {
            None
        }
    }

    /// Parse accumulated query text to extract complete statements
    fn parse_statements(&self, query: &str) -> ParseResult {
        // Split off the unclosed block-comment tail so the tokenizer only
        // sees text it can handle.  Statements before the `/*` are still
        // extracted normally; the comment portion stays in `remaining`.
        let (to_parse, comment_tail) = match Self::unclosed_block_comment_start(query) {
            Some(pos) => (&query[..pos], &query[pos..]),
            None => (query, ""),
        };

        let mut statements = Vec::new();
        let mut remaining_query = to_parse.to_string();
        let mut err = String::new();

        'Parser: loop {
            let mut is_valid = true;
            let tokenizer = Tokenizer::new(&remaining_query);
            let mut previous_token_backslash = false;

            for token in tokenizer {
                match token {
                    Ok(token) => {
                        // SQL end with `;` or `\G` in repl
                        let is_end_query = token.text() == self.delimiter.to_string();
                        let is_slash_g = self.is_repl
                            && (previous_token_backslash
                                && token.kind == TokenKind::Ident
                                && token.text() == "G")
                            || (token.text().ends_with("\\G"));

                        if is_end_query || is_slash_g {
                            // Extract the statement and continue with remaining text
                            let (sql, remain) = remaining_query.split_at(token.span.end as usize);
                            if is_valid
                                && !sql.is_empty()
                                && sql.trim() != self.delimiter.to_string()
                            {
                                let sql = sql.trim_end_matches(self.delimiter);
                                statements.push(sql.trim().to_string());
                            }
                            remaining_query = remain.to_string();
                            continue 'Parser;
                        }
                        previous_token_backslash = matches!(token.kind, TokenKind::Backslash);
                    }
                    Err(e) => {
                        // ignore current query if have invalid token.
                        is_valid = false;
                        err = e.to_string();
                        continue;
                    }
                }
            }
            break;
        }

        // Re-attach the unclosed comment tail so it keeps accumulating.
        if !comment_tail.is_empty() {
            remaining_query.push_str(comment_tail);
        }

        ParseResult {
            statements,
            remaining: remaining_query,
            err,
        }
    }
}

struct ParseResult {
    statements: Vec<String>,
    remaining: String,
    err: String,
}
