# LakeSQL

TiDB Cloud Lake Native Command Line Tool

[![crates.io](https://img.shields.io/crates/v/lakesql.svg)](https://crates.io/crates/lakesql)
![License](https://img.shields.io/crates/l/lakesql.svg)

## Install

```sh
cargo install lakesql
```

## Usage

```
❯ lakesql --help
TiDB Cloud Lake Native Command Line Tool

Usage: lakesql [OPTIONS]

Options:
      --help                       Print help information
      --flight                     Using flight sql protocol, ignored when --dsn is set
      --tls <TLS>                  Enable TLS, ignored when --dsn is set [possible values: true, false]
  -h, --host <HOST>                TiDB Cloud Lake Server host, Default: 127.0.0.1, ignored when --dsn is set
  -P, --port <PORT>                TiDB Cloud Lake Server port, Default: 8000, ignored when --dsn is set
  -u, --user <USER>                Default: root, overrides username in DSN
  -p, --password <PASSWORD>        Password, overrides password in DSN [env: LAKESQL_PASSWORD]
  -r, --role <ROLE>                Downgrade role name, overrides role in DSN
  -D, --database <DATABASE>        Database name, overrides database in DSN
      --set <SET>                  Settings, overrides settings in DSN
      --dsn <DSN>                  Data source name [env: LAKESQL_DSN]
  -n, --non-interactive            Force non-interactive mode
  -A, --no-auto-complete           Disable loading tables and fields for auto-completion, which offers a quicker start
  --check                      Check for server status and exit
      --query=<QUERY>              Query to execute
  -d, --data <DATA>                Data to load, @file or @- for stdin
      --load-method <LOAD_METHOD>  method to load data to table [default: stage] [possible values: stage, streaming]
  -o, --output <OUTPUT>            Output format [possible values: table, csv, tsv, null]
      --quote-style <QUOTE_STYLE>  Output quote style, applies to `csv` and `tsv` output formats [possible values: always, necessary, non-numeric, never]
      --progress                   Show progress for query execution in stderr, only works with output format `table` and `null`.
      --stats                      Show stats after query execution in stderr, only works with non-interactive mode.
  -q, --query-id                   Show the last query ID after each statement.
      --time[=<TIME>]              Only show execution time without results, will implicitly set output format to `null`. [possible values: local, server]
  -l, --log-level <LOG_LEVEL>      [default: info]
  -V, --version                    Print version
```

### REPL

```sql
❯ lakesql
Welcome to LakeSQL.
Connecting to localhost:8000 as user root.
Connected to TiDB Cloud Lake Query

lakesql> select avg(number) from numbers(10);

select
  avg(number)
from
  numbers(10)

╭───────────────────────────────────────────────────────╮
│ sum(number) / if(count(number) = 0, 1, count(number)) │
│                   Nullable(Float64)                   │
├───────────────────────────────────────────────────────┤
│                                                   4.5 │
╰───────────────────────────────────────────────────────╯
1 row read in 0.032 sec. Processed 10 row, 80 B (312.5 rows/s, 2.44 KiB/s)

lakesql> show tables like 'd%';

show tables like 'd%'

┌───────────────────┐
│ tables_in_default │
│       String      │
├───────────────────┤
│ data              │
│ data2             │
│ data3             │
│ data4             │
└───────────────────┘

4 rows in 0.106 sec. Processed 0 rows, 0B (0 rows/s, 0B/s)

lakesql> exit
Bye~
```

### StdIn Pipe

```bash
❯ echo "select number from numbers(3)" | lakesql -h localhost --port 8900 --flight
0
1
2
```

### Show Query ID

Use `--query-id` (or `!set show_query_id true` in REPL) to print the server query ID after every statement, which matches the `Query ID:` that SnowSQL exposes. This is useful for tracking or killing runaway queries.

```bash
❯ lakesql --query-id --query "select 1"
Query ID: 01c0c277-0004-011f-0000-000a83ee9129
1
```

### Put local files into stage

```
create stage s_temp;
put fs:///tmp/a*.txt @s_temp/abc;
```

### Generate TPCH/TPCDS datasets

```sql
root@localhost:8000/test> gendata(tpch, sf = 0.01, override = 1);

┌─────────────────────────────┐
│   table  │ status │   size  │
│  String  │ String │  UInt64 │
├──────────┼────────┼─────────┤
│ customer │ OK     │  130089 │
│ lineitem │ OK     │ 2551994 │
│ nation   │ OK     │    2195 │
│ orders   │ OK     │  598190 │
│ part     │ OK     │   77400 │
│ partsupp │ OK     │  437252 │
│ region   │ OK     │    1018 │
│ supplier │ OK     │   10653 │
└─────────────────────────────┘
```

## Features

- basic keywords highlight
- basic auto-completion
- select query support
- TBD
