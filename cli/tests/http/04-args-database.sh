#!/bin/bash

cat <<SQL | ${LAKESQL}
DROP DATABASE IF EXISTS books_04;
CREATE DATABASE IF NOT EXISTS books_04;
SQL

cat <<SQL | ${LAKESQL}
DROP TABLE IF EXISTS books_04_d;
CREATE TABLE books_04_d
(
    title VARCHAR,
    author VARCHAR,
    date VARCHAR
);
SQL

cat <<SQL | ${LAKESQL} -D books_04
DROP TABLE IF EXISTS books_04_t;
CREATE TABLE books_04_t
(
    title VARCHAR,
    author VARCHAR,
    date VARCHAR
);
SQL

echo "---- tables ----"
cat <<SQL | ${LAKESQL}
SHOW TABLES;
SQL

echo "---- databases ----"
cat <<SQL | ${LAKESQL}
SHOW DATABASES;
SQL

echo "---- tables in books_04 ----"
cat <<SQL | ${LAKESQL} -D books_04
SHOW TABLES;
SQL

cat <<SQL | ${LAKESQL}
DROP TABLE IF EXISTS books_04_d;
DROP DATABASE IF EXISTS books_04;
SQL
