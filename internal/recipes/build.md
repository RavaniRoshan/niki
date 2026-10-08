---
name: build
description: Compile the whole project
match: build | compile | build the project | make sure it builds
steps:
  - shell: {"command": "go build ./... 2>&1 | tail -20", "dir": "{{dir}}"}
---
Runs `go build ./...`. Empty output means it compiled.
