#!/bin/bash

cat <<SQL | ${LAKESQL}
CREATE DATABASE IF NOT EXISTS stream_test;

CREATE OR REPLACE TABLE stream_test.abc
(
    title VARCHAR,
    author VARCHAR,
    date VARCHAR
);

CREATE OR REPLACE STREAM stream_test.s on table stream_test.abc;
SQL

cat <<SQL | ${LAKESQL} -D stream_test
DROP TABLE abc
SQL

cat <<SQL | ${LAKESQL} -D stream_test
select 1;
SQL

cat <<SQL | ${LAKESQL}
DROP DATABASE IF EXISTS stream_test;
SQL
