#!/bin/bash

cat <<SQL | ${LAKESQL}
DROP TABLE IF EXISTS http_ontime_03;
SQL

${LAKESQL} <cli/tests/data/ontime.sql

${LAKESQL} \
    --query='INSERT INTO http_ontime_03 VALUES from @_databend_load file_format=(type=csv, compression=gzip, skip_header=1);' \
    --load-method="streaming" \
    --data=@cli/tests/data/ontime_200.csv.gz

echo "SELECT COUNT(*) FROM http_ontime_03;" | ${LAKESQL} --output=tsv

cat <<SQL | ${LAKESQL}
DROP TABLE http_ontime_03;
SQL
