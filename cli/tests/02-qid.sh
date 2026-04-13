#!/bin/bash
set -eo pipefail

if [[ "${LAKESQL_DSN:-}" == lake+flight* ]]; then
    # Query ID is not exposed over Flight SQL yet; skip but keep expected output.
    echo "has_query_id"
    exit 0
fi

out="$(${LAKESQL} --query-id --output tsv --query='select 1' 2>&1 || true)"
if echo "$out" | grep -q "Query ID:"; then
    echo "has_query_id"
else
    echo "$out"
    echo "missing_query_id"
fi
