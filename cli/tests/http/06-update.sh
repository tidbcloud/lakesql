#!/bin/bash

cat <<SQL | ${LAKESQL}
CREATE TABLE t3 (
    category_id INT NOT NULL,
    region_id INT NOT NULL,
    user_id BIGINT NOT NULL,
    product_id INT NOT NULL,
    transaction_date DATETIME NOT NULL,
    transaction_amount DECIMAL(10,2) NOT NULL,
    discount_amount DECIMAL(10,2) NOT NULL,
    is_returned BOOLEAN NOT NULL DEFAULT 0,
    inventory_count INT NOT NULL,
    last_updated TIMESTAMP NOT NULL
);

create table r like t3 engine = random;

insert into t3 select * from r limit 305104;
SQL


a=`${LAKESQL} --stats  --query="""
update t3 set user_id = user_id + 1 where inventory_count % 10 >= 0;
""" 2>&1 | grep -oE '([0-9]+) rows written' | grep -oE '([0-9]+)'`


b=`${LAKESQL} --stats  --query="""
update t3 set user_id = user_id + 1 where inventory_count % 10 < 0;
""" 2>&1 | grep -oE '([0-9]+) rows written' | grep -oE '([0-9]+)'`

echo "$[a+b]"


cat <<SQL | ${LAKESQL}
DROP TABLE IF EXISTS t3;
SQL

cat <<SQL | ${LAKESQL}
DROP TABLE IF EXISTS r;
SQL