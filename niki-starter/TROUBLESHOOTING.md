# Troubleshooting

The three failures you are most likely to hit, and the exact command that fixes
each. If yours is not here, `niki doctor` is the first thing to run and
`report.md` has the detail.

---

## "Container runtime error: NIKI selected the container backend"

**Cause.** Your config asks for the container backend and you do not have one
running.

**Fix — for this project, you do not need a container at all:**

```bash
# niki.toml must contain:
[docker]
backend = "worktree"
```

`niki init --interactive` writes this automatically when it finds no container
runtime, so if you ran the wizard you should already have it. If you set
`backend = "docker"` by hand, that is the line to change back.

**If you would rather have container isolation**, install Podman and build the
sandbox image:

```bash
podman build -t niki-sandbox:24.04 -f docker/Dockerfile .
# then set backend = "docker" in niki.toml
```

---

## The run stops at the Coder stage, or no branch is created

**Cause.** Every stage must emit a JSON artifact matching a strict schema, and a
small local model frequently cannot produce the Coder's. This is the most common
outcome with a 3B model and it is not a bug in your setup.

**Check quickly, before retrying the whole thing:**

```bash
niki smoke
```

That runs a trivial task and tells you the stage that failed.

**Fix:**

```bash
ollama pull qwen2.5-coder:14b
```

then change all four `model =` lines in `niki.toml` to that model, and run
again. Or use a hosted model — change the `provider` and `model` lines in each
`[agents.*]` section and put your key in the environment.

**Where to look afterwards.** `.niki/tasks/<id>/artifacts/coder.json` holds what
the model actually returned. When a stage fails, that file is the interesting
one; it is where you can see whether the model produced prose where a diff was
required.

---

## "Failing test suite" — the branch was not created

**Cause.** The Tester ran the suite, something failed, and NIKI did not cut a
branch. By default this is the correct behaviour: you asked for a working change
and did not get one.

**If the tests pass and you are still stuck**, the run log has the real reason.

**If you want the branch anyway** — to read what was attempted, or because you
want to finish the job by hand:

```bash
niki run "<task>" --force
```

`--force` creates the branch despite the failure. It does not make the code
correct, and the report will still say the suite failed.

---

## `niki doctor` says something failed

**Look at the category.** The ones that matter on a fresh machine:

| Category | What it means |
|---|---|
| `container runtime` | A warning, not a failure, if you are on the worktree backend. If it is red, your config says `docker` — see the first section above. |
| `providers` | Usually Ollama not running. Start it: `ollama serve`. |
| `config` | A real problem with `niki.toml`. `niki config check` explains it in more detail. |
| `sandbox image` | Only relevant if you are on the container backend. |

**Nothing failed but you still cannot run?** The most common cause is a model
that cannot produce schema-valid output. `niki smoke` will show you.

---

## The run "degrades" instead of stopping

**Cause.** Some stages are best-effort by design — provenance and reflection
warn rather than fail, and the report says so. This is deliberate: a run should
not be lost because an advisory section could not be assembled.

**How to tell.** The run completes and `report.md` names what was skipped. The
run's exit status and the JSON envelope (`--output-format json`) carry the real
verdict, including whether anything independently reviewed the change.

---

## Nothing works and the messages make no sense

Get more detail:

```bash
niki run "<task>" --output-format json > run.json
```

That gives a machine-readable envelope with the stage, the reason, and the cost.

If you are filing a bug, include `niki --version`, your OS, the exact command,
the backend you used, and `report.md`. Never paste API keys — NIKI redacts them
from its own output, but a terminal screenshot does not.
