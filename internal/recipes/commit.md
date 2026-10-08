---
name: commit
description: Commit staged changes with a message derived from the staged diff
match: commit | commit my changes | commit the staged changes | save my changes
steps:
  - git_status: {"dir": "{{dir}}"}
  - git_commit: {"dir": "{{dir}}"}
---
Shows status, then commits what is staged with a message derived from the
real staged diff. Refuses when nothing is staged.
