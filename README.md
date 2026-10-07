# Niki

Fast, terminal-native AI coding agent written in Go.

## Build & Run

```bash
make build      # builds bin/niki
./bin/niki      # interactive TUI
./bin/niki exec "summarize this repo"
./bin/niki doctor
```

## Development

```bash
go vet ./...
go test ./...
go build -o bin/niki ./cmd/niki
```

## Configuration

Layered TOML config (defaults → `~/.config/niki/niki.toml` → `./niki.toml` → `--config`):

```toml
[model]
name = "gpt-4o-mini"

[provider]
name = "openai"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"

[permissions]
mode = "workspace_write"
```

See `docs/architecture/references.md` for provenance.
