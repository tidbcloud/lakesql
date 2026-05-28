# LakeSQL

TiDB Cloud Lake Native Client in Rust 0

## Components

- [**core**](core): TiDB Cloud Lake RestAPI Rust Client

- [**driver**](driver): TiDB Cloud Lake SQL Client for both RestAPI and FlightSQL in Rust

- [**cli**](cli): TiDB Cloud Lake Native CLI

### Bindings

- [**python**](bindings/python): TiDB Cloud Lake Python Client

- [**nodejs**](bindings/nodejs): TiDB Cloud Lake Node.js Client

- [**java**](bindings/java): TiDB Cloud Lake Java Client (upcoming)

## Installation for LakeSQL

### Recommended: installation script

```bash
curl -fsSL https://lakesql-bin.tidbcloud.com/install/lakesql.sh | bash
```

or

```bash
curl -fsSL https://lakesql-bin.tidbcloud.com/install/lakesql.sh | bash -s -- -y --prefix /usr/local
```

### Alternative: install via cargo-binstall

If you already use Rust tooling and have `cargo-binstall` available, you can install the prebuilt `lakesql` binary from the published release artifacts:

```bash
cargo binstall lakesql
```

If `cargo-binstall` is not installed yet, install it first:

```bash
cargo install cargo-binstall
```

### Linux package repositories

Use the install script above if you want the least setup. Native package feeds are also published for `apt`, `dnf`/`yum`, and `apk`.

#### Debian / Ubuntu

```bash
curl -fsSL https://lakesql-bin.tidbcloud.com/keys/lakesql-archive-keyring.gpg \
  | sudo tee /usr/share/keyrings/lakesql-archive-keyring.gpg >/dev/null
echo "deb [signed-by=/usr/share/keyrings/lakesql-archive-keyring.gpg] https://lakesql-bin.tidbcloud.com/apt stable main" \
  | sudo tee /etc/apt/sources.list.d/lakesql.list >/dev/null
sudo apt-get update
sudo apt-get install -y lakesql
```

#### Fedora / RHEL

```bash
sudo tee /etc/yum.repos.d/lakesql.repo >/dev/null <<'EOF'
[lakesql]
name=LakeSQL
baseurl=https://lakesql-bin.tidbcloud.com/rpm/stable/$basearch
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=https://lakesql-bin.tidbcloud.com/keys/RPM-GPG-KEY-lakesql
EOF
sudo dnf install -y lakesql
```

#### Alpine

```bash
curl -fsSL https://lakesql-bin.tidbcloud.com/keys/lakesql-packages.rsa.pub \
  | sudo tee /etc/apk/keys/lakesql-packages.rsa.pub >/dev/null
echo "https://lakesql-bin.tidbcloud.com/apk/stable/$(apk --print-arch)" \
  | sudo tee -a /etc/apk/repositories >/dev/null
sudo apk update
sudo apk add lakesql
```

### Manual binary download

1. Resolve the latest version:

```bash
curl -fsSL https://lakesql-bin.tidbcloud.com/lakesql/latest.json
```

2. Download the archive that matches your platform:

```text
https://lakesql-bin.tidbcloud.com/lakesql/vX.Y.Z/lakesql-<target>.tar.gz
```

Supported targets in the binary release flow:

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`

### Other install options

- Rust CLI source install: `cargo install lakesql`
- Python bindings: `pip install tidbcloudlake-driver`
- Node.js bindings: `npm install tidbcloudlake-driver`

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
  -d, --data <DATA>                Data to load, @file or @- for stdin. The `--query` should use the syntax: `INSERT FROM <table> from @_tidbcloud_load file_format=(<file_format_options>)`
  -o, --output <OUTPUT>            Output format [possible values: table, csv, tsv, null]
      --quote-style <QUOTE_STYLE>  Output quote style, applies to `csv` and `tsv` output formats [possible values: always, necessary, non-numeric, never]
      --progress                   Show progress for query execution in stderr, only works with output format `table` and `null`.
      --stats                      Show stats after query execution in stderr, only works with non-interactive mode.
      --time[=<TIME>]              Only show execution time without results, will implicitly set output format to `null`. [possible values: local, server]
  -l, --log-level <LOG_LEVEL>      [default: info]
  -V, --version                    Print version
```

## Custom configuration

By default lakesql will read configuration from `~/.lakesql/config.toml` and `~/.config/lakesql/config.toml`
sequentially if exists.

- Example file

```
❯ cat ~/.lakesql/config.toml
[connection]
host = "127.0.0.1"
tls = false

[connection.args]
connect_timeout = "30"

[settings]
display_pretty_sql = true
progress_color = "green"
no_auto_complete = true
prompt = ":) "
```

- Connection section

| Parameter  | Description                 |
| ---------- | --------------------------- |
| `host`     | Server host to connect.     |
| `port`     | Server port to connect.     |
| `user`     | User name.                  |
| `database` | Which database to connect.  |
| `args`     | Additional connection args. |

- Settings section

| Parameter            | Description                                                                                                         |
| -------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `display_pretty_sql` | Whether to display SQL queries in a formatted way.                                                                  |
| `prompt`             | The prompt to display before asking for input.                                                                      |
| `progress_color`     | The color to use for the progress bar.                                                                              |
| `show_progress`      | Whether to show a progress bar when executing queries.                                                              |
| `show_stats`         | Whether to show statistics after executing queries.                                                                 |
| `no_auto_complete`   | Whether to disable loading tables and fields for auto-completion on startup.                                        |
| `max_display_rows`   | The maximum number of rows to display in table output format.                                                       |
| `max_width`          | Limit display render box max width, 0 means default to the size of the terminal. 65535 means no limit for max_width |
| `max_col_width`      | Limit display render each column max width, smaller than 3 means disable the limit.                                 |
| `output_format`      | The output format to use.                                                                                           |
| `expand`             | Expand table format display, default auto, could be on/off/auto.                                                    |
| `time`               | Whether to show the time elapsed when executing queries.                                                            |
| `multi_line`         | Whether to allow multi-line input.                                                                                  |
| `quote_string`       | Whether to quote string values in table output format, default false.                                               |
| `sql_delimiter`      | SQL delimiter, default `;`.                                                                                         |

## Commands in REPL

| Commands       | Description             |
| -------------- | ----------------------- |
| `!exit`        | Exit lakesql            |
| `!quit`        | Exit lakesql            |
| `!configs`     | Show current settings   |
| `!set`         | Set settings            |
| `!source file` | Source file and execute |

## Setting commands in REPL

We can use `!set CMD_NAME VAL` to update the `Settings` above in runtime, example:

```
❯ lakesql

:) !set display_pretty_sql false
:) !set max_display_rows 10
:) !set expand auto
```

## DSN

Format:

```
lake[+flight]://user:[password]@host[:port]/[database][?sslmode=disable][&arg1=value1]
```

Examples:

- `lake://root:@localhost:8000/?sslmode=disable&presign=detect`

- `lake://user1:password1@tnxxxx.gw.aws-us-east-2.default.tidbcloud.com:443/benchmark?warehouse=default&enable_dphyp=1`

- `lake+flight://root:@localhost:8900/database1?connect_timeout=10`

### Available Args

#### Common

| Arg               | Description                          |
| ----------------- | ------------------------------------ |
| `tenant`          | Tenant ID, TiDB Cloud only.      |
| `warehouse`       | Warehouse name, TiDB Cloud only. |
| `sslmode`         | Set to `disable` if not using tls.   |
| `tls_ca_file`     | Custom root CA certificate path.     |
| `connect_timeout` | Connect timeout in seconds           |

#### RestAPI Client

| Arg                         | Description                                                                                                                                                      | Default   |
|-----------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------|
| `query_result_format`       | (Since v0.33.1) Format to fetch result set, available arguments are `json`/`arrow`.                                                                              | `JSON`    |
| `sslmode`                   | SSL mode, available values are `enable`/`disable`.                                                                                                               | `disable` |
| `wait_time_secs`            | Request wait time for page.                                                                                                                                      | `1`       |
| `max_rows_per_page`         | Max result rows for a single page.                                                                                                                               | `10000`   |
| `page_request_timeout_secs` | Timeout for a single page request.                                                                                                                               | `30`      |
| `presign`                   | Whether to enable presign for data loading, available arguments are `auto`/`detect`/`on`/`off`. Default to `auto` which only enable presign for `TiDB Cloud` | `auto`    |

#### FlightSQL Client

| Arg                         | Description                                                               |
|-----------------------------| ------------------------------------------------------------------------- |
| `query_timeout`             | Query timeout seconds                                                     |
| `tcp_nodelay`               | Default to `true`                                                         |
| `tcp_keepalive`             | Tcp keepalive seconds, default to `3600`, set to `0` to disable keepalive |
| `http2_keep_alive_interval` | Keep alive interval in seconds, default to `300`                          |
| `keep_alive_timeout`        | Keep alive timeout in seconds, default to `20`                            |
| `keep_alive_while_idle`     | Default to `true`                                                         |

#### Query Settings

see: [TiDB Cloud Lake Settings](https://docs.tidbcloud.com/sql/sql-commands/administration-cmds/show-settings)

## Development

### Cargo fmt, clippy, deny

```bash
make check
```

### Development mode

- Run the CLI directly with `make run`
- Build release binaries with `make build`

### Unit tests

```bash
make test
```

### integration tests

_Note: Docker and Docker Compose needed_

```bash
make integration
```
