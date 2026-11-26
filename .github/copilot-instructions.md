# daps – Copilot Instructions

## Build, check, lint

```bash
cargo build --release          # release build (matches CI)
cargo check                    # fast type/syntax check
cargo clippy                   # lint (no custom config)
cargo fmt                      # format (no custom config)
cargo run -- --help            # show all flags/subcommands
cargo run -- --path /dev/ --verbose   # run with a specific SSM base path
```

No test suite exists yet. CI only runs `cargo build --verbose --release`.

## Architecture

`daps` has three distinct execution modes, all sharing `ParameterCompleter` as the central state:

| Mode | Entry point | When |
|------|-------------|------|
| **Interactive REPL** | `repl::run` | no subcommand given |
| **CLI subcommands** | `cli::run` | `get`, `gets`, `set`, `insert`, `search`, `reload`, `reload-paths`, `refresh`, `migrate`, `parse-db` |
| **MCP server** | `mcp::run` | `daps mcp` — JSON-RPC 2.0 over stdio |

### Data flow

1. `main.rs` parses args with `structopt`, routes to the right mode.
2. `ParameterCompleter` (`src/completer.rs`) owns all state:
   - `parameters: HashMap<String, Vec<String>>` — completion tree (parent path → child segments)
   - `values: HashMap<String, String>` — full path → decrypted value
   - `search_result: Vec<String>` — last fuzzy-search results (used by `sel <n>`)
3. `ParamStoreHelper` (`src/helper.rs`) wraps `ParameterCompleter` to implement rustyline's `Completer`, `Highlighter`, `Hinter`, and `Validator` traits.
4. `Command` enum (`src/command.rs`) is the single source of truth for REPL command parsing. Every new REPL command must be added here and to `keywords()`.

### Local cache

Parameters are persisted under `~/parameters/` (or `--store-dir`):
- `parameters_{sanitized_path}.txt` — one path per line (completion tree)
- `values_{sanitized_path}.txt` — `key=AES-GCM-base64-encrypted-value` per line

Sanitization: `/` → `_` (via `get_sanitized_base_path()`). On startup the completer merges **all** `parameters_*.txt` / `values_*.txt` files it finds in `store_dir` so completion is complete regardless of which base path was active when each cache was built.

### Encryption

All cached values are encrypted with AES-256-GCM. Key is derived via SHA-256 from `DAPS_ENCRYPTION_KEY` env var (defaults to `"default_key"`). The `migration` command re-encrypts an existing cache with the current key.

## Key conventions

- **No clipboard integration.** The `cpboard` module was removed; do not add clipboard deps back.
- **Color is TTY-gated.** `cli::run` sets `colored::control::set_override(false)` when stdout is not a TTY. Piped output must be plain text.
- **`parse-db` output:** colored in TTY, `key=value` lines when piped.
- **`insert` format:** `<path>:<value>:<type>` — colon-delimited positional string, parsed in `commands/insert.rs`.
- **Error propagation:** commands return `Result<String, Box<dyn std::error::Error>>` and are dispatched through `handle_command_result` which prints with `colored`.
- **Windows compat:** file paths use `\\` on Windows (checked with `cfg!(target_os = "windows")`). Home dir is `APPDATA` on Windows, `HOME` elsewhere.
- **Rusoto v0.47** is the AWS SDK — not aws-sdk-rust. All SSM calls use `SsmClient` from `rusoto_ssm`.

## Environment variables

| Variable | Purpose |
|----------|---------|
| `DAPS_ENCRYPTION_KEY` | AES-256-GCM cache encryption key |
| `AWS_PROFILE` | AWS profile (standard SDK behavior) |
| `AWS_REGION` | Override region (also settable via `--region`) |
