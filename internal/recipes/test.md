---
name: test
description: Run the Go test suite for the project
match: run the tests | run tests | test the code | make sure tests pass | run the test suite
steps:
  - shell: {"command": "go test ./... 2>&1 | tail -20", "dir": "{{dir}}"}
---
Runs `go test ./...` in the project root and shows the tail of the output.
Refuses (with the tool error) when the suite fails.
