#!/bin/bash

cat <<SQL | ${LAKESQL}
DROP TABLE IF EXISTS http_books_01;
SQL

cat <<SQL | ${LAKESQL}
CREATE TABLE http_books_01 (title VARCHAR NULL, author VARCHAR NULL, date VARCHAR NULL, publish_time TIMESTAMP NULL);
SQL

${LAKESQL} --query='INSERT INTO http_books_01 VALUES from @_databend_load file_format=(type=csv)' --data=@- <cli/tests/data/books.csv

${LAKESQL} --query='SELECT * FROM http_books_01 LIMIT 10;' --output=tsv

cat <<SQL | ${LAKESQL}
DROP TABLE http_books_01;
SQL
