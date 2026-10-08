# NikiCode
- Read docs/PACK.md and docs/CAPABILITY_PACK.md completely at the start of every session and after any /compact.
- Memory: docs/PROGRESS.md, DECISIONS.md, CHECKLIST.md, PERF.md, PARITY.md. Re-read before each phase.
- Never read leaked or extracted proprietary source. Never publish or push.
- Build hygiene: loop on `go build ./...` and `go test ./internal/<pkg>/...`; run golangci-lint, `go vet`, and the whole test suite at the end of a slice.
