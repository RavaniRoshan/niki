# niki-starter

A small project with a real, failing test — and one command to hand it to NIKI.

The task: `src/server.js` does not implement `GET /health`, and
`test/server.test.js` fails because of it. That is the whole exercise. NIKI
plans it, writes the code, runs the tests, and reviews its own work, and you
end up holding a branch rather than an answer.

```bash
./run.sh
```

That is the intended first command. It checks your setup, tells you if
something is missing, and then runs the pipeline.

---

## What you need

| | |
|---|---|
| `git`, `node`, `npm`, `python3`, `curl` | on your `PATH` |
| [Ollama](https://ollama.com), running | `ollama serve` |
| One coding model pulled | `ollama pull qwen2.5-coder:7b` |
| NIKI | `brew install niki`, or the [install script](https://github.com/RavaniRoshan/niki) |

**No API key. No account. No container runtime. No cost.** The configuration in
`niki.toml` uses the git-worktree backend and a local model.

If you have a cloud key instead, change the four `[agents.*]` sections in
`niki.toml` to your provider and model, and run `niki init --interactive` if you
would rather be asked than edit. Both paths are supported; this one is just
cheaper and does not need a network.

---

## Before you start: watch it fail

```bash
node --test test/
```

Four of the five tests fail, all of them on `/health`. If they pass, something
is wrong with your copy — see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

Knowing the starting state matters more than it sounds. When NIKI is done you
will run this exact command again, and the only interesting thing about the
result is that it changed.

---

## Running it

```bash
./run.sh
```

or, if you would rather see each step:

```bash
niki doctor
niki run "Implement GET /health so it returns 200 with a JSON body of exactly { status: 'ok' }, and add tests for it"
niki report
```

You should see four stages run in order — **Planner → Coder → Tester →
Reviewer** — and then a line naming a branch like `niki/4f2a91c8`.

---

## What you end up with

```text
niki/<id>          a branch with the change, committed
changes.patch      the same diff, as text
report.md          what every agent decided, and why
artifacts/*.json   the raw structured output of each stage
```

Start with `report.md` — [REPORT-GUIDE.md](REPORT-GUIDE.md) explains each
section. It is the most interesting artifact NIKI produces, and nobody has ever
written the guide that makes it obvious, which is a large part of why this
starter exists.

To look at the change:

```bash
git switch niki/<id>          # the id is printed at the end of the run
git diff main..HEAD           # or whatever your starting branch is called
```

Your working branch is not modified. NIKI commits to a new branch and leaves
your tree alone, so `git checkout .` is always available if you want to try
something else.

---

## If it stops partway

That is normal and it is not a failure of the product. Each stage has to emit a
JSON artifact matching a schema, and a small local model often cannot. When a
stage stops, the run says which one and why, and `niki report` shows the same
detail.

[HONESTY.md](HONESTY.md) says plainly what does not work yet, and
[TROUBLESHOOTING.md](TROUBLESHOOTING.md) has the three failures you are most
likely to hit and the exact command that fixes each.

---

## What to look at afterwards

1. `report.md` — read the Coder's notes and the Reviewer's verdict. The Reviewer
   runs as a *separate* agent that never saw the Coder's reasoning; it only got
   the diff and the test results. That is the design, and it is why the review
   is worth reading.
2. `artifacts/coder.json` and `artifacts/reviewer.json` — the raw structured
   output. If the run stopped, this is where the model's actual words are.
3. `changes.patch` — the diff on its own, to paste anywhere.

---

## Licence

Apache-2.0, same as NIKI. Do what you like with it.
