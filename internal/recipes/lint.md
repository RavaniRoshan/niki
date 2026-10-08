---
name: lint
description: Check formatting and vet the project
match: lint | check formatting | run vet | format check | gofmt check
steps:
  - shell: {"command": "gofmt -l .", "dir": "{{dir}}"}
  - shell: {"command": "go vet ./... 2>&1 | tail -20", "dir": "{{dir}}"}
---
Lists unformatted files (`gofmt -l`, empty means clean) then runs `go vet`.
