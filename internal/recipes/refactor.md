---
name: refactor
description: Rename a symbol across Go files, then format and verify the build
match: rename | refactor | rename the symbol | replace across files
steps:
  - shell: {"command": "grep -rl '{{old}}' --include='*.go' . | head -20", "dir": "{{dir}}"}
  - shell: {"command": "grep -rl '{{old}}' --include='*.go' . | xargs sed -i 's/{{old}}/{{new}}/g'", "dir": "{{dir}}"}
  - shell: {"command": "gofmt -w . && go build ./... && echo REFACTOR-OK", "dir": "{{dir}}"}
---
Lists affected files first, renames the symbol with sed, formats, and
rebuilds. Review the change with git diff afterwards.
