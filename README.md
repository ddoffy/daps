# daps — AWS Parameter Store CLI

Fast, interactive CLI for AWS SSM Parameter Store (and Secrets Manager).  
Two modes: **interactive REPL** (default) and **non-interactive CLI subcommands** — no agent protocol, no MCP, just your terminal.

---

## Installation

**Build from source (recommended):**
```sh
cargo build --release
# binary at ./target/release/daps
ln -sf "$(pwd)/target/release/daps" ~/.local/bin/daps
```

**Or install directly:**
```sh
cargo install --path .
```

**Pre-built binaries:** [GitHub Releases](https://github.com/ddoffy/daps/releases) — Linux x86_64, Windows x86_64.

---

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `DAPS_ENCRYPTION_KEY` | Yes | AES-256 key for encrypting the local cache |
| `AWS_REGION` / `--region` | No | Defaults to `us-east-1` |
| `AWS_PROFILE` | No | Standard AWS profile selection |

---

## Global flags

```
daps [FLAGS] [SUBCOMMAND]

FLAGS:
  --region <region>       AWS region           [default: us-east-1]
  -p, --path <path>       SSM base path prefix [default: /]
  -r, --refresh           Force cache refresh on startup
  --store-dir <dir>       Local cache directory [default: ~/parameters]
  --verbose               Verbose output
```

---

## Mode 1 — Interactive REPL

```sh
daps                          # start REPL at /
daps -p /myapp/prod/          # start REPL scoped to a prefix
daps -p /myapp/prod/ -r       # start and force-refresh cache
```

**Tab completion** navigates the full SSM path hierarchy.  
Type a partial path and press `Tab` to expand.

### REPL commands

| Command | Description |
|---|---|
| `<path>` | Navigate to path — prints its decrypted value |
| `reload` | Re-fetch the currently selected parameter from AWS |
| `refresh` | Re-fetch all parameters under the base path |
| `set <value>` | Update the currently selected parameter's value |
| `insert <path>:<value>:<type>` | Create a new parameter (`type`: `String`, `SecureString`, `StringList`) |
| `search <term>` | Fuzzy-search parameter names |
| `sel <n>` | Select result `n` from the last search |
| `parse-db` | Parse the selected parameter as a Postgres connection string |
| `secret-get <name>` | Fetch a Secrets Manager secret value |
| `secret-list [filter]` | List Secrets Manager secrets (optional name filter) |
| `secret-set <value>` | Update the currently selected secret's value |
| `secret-create <name>:<value>[:<description>]` | Create a new Secrets Manager secret |
| `exit` / `Ctrl-D` / `Ctrl-C` | Quit |

---

## Mode 2 — CLI subcommands (non-interactive / scriptable)

All subcommands exit after execution — suitable for scripts, CI, shell aliases.

### SSM Parameters

```sh
# Get a single parameter value (decrypted)
daps get /myapp/prod/db_password

# Get all parameters under a prefix (key=value, one per line)
daps gets /myapp/prod/

# Set a parameter value
daps set /myapp/prod/db_password "newpassword"

# Insert a new parameter
daps insert /myapp/prod/db_password:mysecret:SecureString

# Search parameter names
daps search db_pass

# Reload one parameter into cache
daps reload /myapp/prod/db_password

# Reload multiple prefixes into cache
daps reload-paths /myapp/prod/ /myapp/staging/

# Re-fetch entire tree under base path
daps refresh

# Re-encrypt local cache with a new DAPS_ENCRYPTION_KEY
daps migrate

# Parse a Postgres connection string stored as a parameter
daps parse-db /myapp/prod/database_url
# Output (piped): KEY=value lines (plain text, no color)
# Output (TTY):   formatted table
```

### Secrets Manager

```sh
# Fetch a secret value
daps secret-get /myapp/prod/api_key

# List secrets (optional filter)
daps secret-list
daps secret-list myapp

# Update an existing secret's value
daps secret-set /myapp/prod/api_key "newvalue"

# Create a new secret
daps secret-create /myapp/prod/api_key:mysecretvalue
daps secret-create /myapp/prod/api_key:mysecretvalue:"Production API key"
```

### Shell completions

Generate a completion script for `bash`, `zsh`, or `fish` and write it where your
shell looks for completions:

```sh
# bash
daps completion bash > /etc/bash_completion.d/daps
#   or, per-user:  daps completion bash > ~/.local/share/bash-completion/completions/daps

# zsh — put it on your $fpath, then run compinit
daps completion zsh > ~/.zfunc/_daps

# fish
daps completion fish > ~/.config/fish/completions/daps.fish
```

---

## Scripting patterns

```sh
# Export all params under a prefix as env vars
eval "$(daps gets /myapp/prod/ | sed 's|.*/||; s/=/ =/; s/^/export /')"

# Inline DB connection
export DATABASE_URL="$(daps get /myapp/prod/database_url)"

# Parse DB connection string into shell vars
eval "$(daps parse-db /myapp/prod/database_url)"

# Use in a pipeline
daps gets /myapp/ | grep API_KEY
```

---

## Local cache

Parameters and secrets are cached in `~/parameters/` (override with `--store-dir`):

```
~/parameters/
  parameters_{sanitized_path}.txt   # completion tree
  values_{sanitized_path}.txt       # encrypted key:value pairs
  secrets_{sanitized_path}.txt      # encrypted secret key:value pairs
```

Encryption: AES-256-GCM, key derived from `DAPS_ENCRYPTION_KEY` via SHA-256.  
Run `daps migrate` after rotating the key.

---

## Contributing

PRs welcome. Please see [CONTRIBUTING.md](CONTRIBUTING.md) if it exists, otherwise open an issue.

## License

MIT — see [LICENSE](LICENSE).
