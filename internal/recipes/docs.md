---
name: docs
description: Capture an exported-symbol outline into a GODOC.md summary file
match: document | generate docs | generate the docs | write docs | write documentation | update the docs | update docs
steps:
  - shell: {"command": "{ echo '# Package outline'; echo; echo '## Exported functions'; grep -h '^func [A-Z]' -- *.go || true; echo; echo '## Exported types'; grep -h '^type [A-Z]' -- *.go || true; } > GODOC.md 2>&1; wc -l GODOC.md", "dir": "{{dir}}"}
---
Writes a grep-based outline of exported functions and types to GODOC.md
and reports its size. It is an outline, not prose: review and expand it.
