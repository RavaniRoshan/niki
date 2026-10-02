# BLOCKERS

Things this programme cannot do for itself.

## B6 · OpenRouter key expired; NVIDIA key works (2026-10-02)

The OpenRouter key supplied for live testing is **expired**:

```
$ curl -s https://openrouter.ai/api/v1/key -H "Authorization: Bearer sk-or-v1-…"
{"error":{"message":"API key expired.","code":401,…"API key expired"}}
```

The **NVIDIA key works**, and NIKI already ships an `nvidia` provider pointed
at `https://integrate.api.nvidia.com/v1`, so this is no longer a blocker for
real-model work:

```
$ NVIDIA_API_KEY=… niki providers models --provider nvidia --plain
nvidia	deepseek-ai/deepseek-v4.1-flash
…
```

Measured on that key, with `max_tokens: 512`:

| model | first token | notes |
| --- | --- | --- |
| `openai/gpt-oss-20b` | 676 ms | reasoning model; needs token headroom |
| `nvidia/nemotron-3.5-lightning-30b-a3b` | 920 ms | |
| `nvidia/nemotron-3-super-120b-a12b` | 2.4 s | **the one used for the runs in `EVIDENCE.md`** |
| `z-ai/glm-5.3-flash` | 60 s | too slow to iterate on |

Three real pipeline runs were executed against
`nvidia/nemotron-3-super-120b-a12b` — Planner → Coder → Tester → Reviewer, all
four stages, correct fixes, green suites, branches verified independently of
NIKI's own reporting. See `EVIDENCE.md`.

**A note for any future live test:** `max_tokens` must leave room for
reasoning. At `max_tokens: 16` both `gpt-oss-20b` and `nemotron-3-super`
returned `content: null` with every token spent in `reasoning_content` and
`finish_reason: "length"` — which reads as a broken provider rather than a
truncated budget.

`reasoning_effort` is confirmed working end to end against a real provider.
NVIDIA validates it and its accepted set is exactly:

```
$ curl … -d '{"…","reasoning_effort":"banana"}'
unknown variant `banana`, expected one of
  `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`
```

NIKI sends the field **only when configured**, never inferred from a model
name — which is what keeps a provider that does not know the key from
rejecting the whole request. Proven on the wire by
`tests/agent_tool_loop.rs::reasoning_effort_reaches_the_wire_only_when_set`,
and a live run with `reasoning_effort = "high"` completed all four stages.

**Image input is not supported.** There is no image/multimodal path in the
provider layer — `CompletionRequest` carries text only. `space-bunny-alpha`
advertises `['text','image','video'] -> ['text']` and 1M context, but NIKI
cannot use the first without a new content type through the whole request
path. Logged in `ROADMAP.md` rather than treated as a defect.

## B7 · The classifier's model — answered from prior art (2026-10-02)

Researched rather than asked. Sources were fetched, not recalled; anything that
could not be sourced is marked as such below.

### What Claude Code actually does (all `[source]`)

Anthropic published a two-layer design and it is **already-built spec**, not a
guess. From `anthropic.com/engineering/claude-code-auto-mode` and
`code.claude.com/docs/en/permission-modes`:

- **Two layers, and they are the two NIKI already has.** *"At the input
  layer, a server-side prompt-injection probe scans tool outputs … before they
  enter the agent's context."* Then *"at the output layer, the transcript
  classifier … evaluates each action."*
- **Two-stage classifier.** *"A fast single-token filter … followed by
  chain-of-thought reasoning only if the first filter flags the transcript."*
  Stage 1 is *"instructed to err on the side of blocking."*
- **The reasoning-blind claim in the brief is correct.** *"The classifier sees
  only user messages and the agent's tool calls; we strip out Claude's own
  messages and tool outputs, making it reasoning-blind by design."*
- **The denial-as-tool-result claim is correct too.** A denial *"comes back as
  a tool result along with an instruction to treat the boundary in good faith:
  find a safer path, don't try to route around the block."*
- **Fails closed on every uncertainty.** *"The server gives no verdict for an
  action: Claude Code denies the action rather than run it unreviewed."*
- **Escalation limits are exactly NIKI's.** *"3 times in a row or 20 times
  total,"* and they are **not configurable**.
- **The model is not yours to pick.** *"The classifier runs on Claude Sonnet 5
  by default rather than on your `/model` selection."* That is the one part
  NIKI cannot copy: Claude Code controls the model, NIKI is BYOK.
- **Honest numbers, published.** On real traffic (n=10,000): 0.4% FPR, **17%
  FNR**. Anthropic calls 17% *"the honest number"* and concedes it is *"a
  regression"* versus careful human review, while being a large improvement on
  no gate.

### Codex (the closer prior art — it is Rust)

- Sandbox: `read-only | workspace-write | danger-full-access`.
  Approval: `on-request | never | granular`.
- Per-command rules are **Starlark** files, not TOML: `prefix_rule(pattern,
  decision = "allow"|"prompt"|"forbidden")`, and *"the most restrictive decision
  wins."* Test them with `codex execpolicy check`.
- **It has a classifier**: `approvals_reviewer = "auto_review"` (default
  `user`). The reviewer *"is itself a Codex agent"*, and is *"a reviewer swap,
  not a permission grant."*
- **Fails closed**: *"Prompt-build, review-session, and parse failures fail
  closed. Timeouts are surfaced separately, but the action still does not run."*
- Escalation differs from Claude Code's: *"3 consecutive denials or 10 denials
  within a rolling window of the last 50 reviews in the same turn."*
- **A contradiction worth knowing.** Codex's reviewer sees **tool outputs** —
  *"user messages, surfaced assistant updates, relevant tool calls and tool
  outputs … Hidden assistant reasoning is not included."* So it is
  reasoning-blind but **not** result-blind. Claude Code strips results; Codex
  does not.

### Decisions this settles, without needing the owner

1. **Fail-closed stays the default.** Both vendors chose it, and both treat a
   *timeout* as a denial, not as permission. NIKI's existing gate already
   does this. No longer an open question.
2. **The config key gets no default.** Claude Code can name a model; NIKI
   cannot, so the key is read from config with **no built-in value**. A default
   would silently pick a vendor and spend the user's credits.
3. **Off by default until it is implemented and measured.** Claude Code can ship
   a classifier that is the product's default because it is *their* model. A
   BYOK harness that defaults to spending an extra call per unlisted tool, on
   an unknown provider, with unknown latency, is a worse default than asking.
4. **Keep the thresholds at Claude Code's numbers** (3 consecutive / 20 total)
   so behaviour is comparable, and add Codex's rolling window as a second
   breaker for long sessions, since 20-total is a session-lifetime counter.
5. **Ship the honest framing.** If NIKI's gate is enabled, its docs must state
   that it is a large improvement on no gate and a regression against careful
   human review — with the measured numbers, once they exist. 17% FNR is what
   the vendor publishes about its own.
6. **Prioritise tiering over classifier quality.** Both vendors let
   built-in-safe tools and in-project writes through statically. A per-call
   round trip on every read would cost more than the gate saves.

### Not found

- No published latency figure for either classifier.
- No `openai/codex` issues about `auto_review`; the link to an "Alignment
  Research post" with eval numbers was not fetched, so NIKI has no Codex
  FPR/FNR to compare against.

### Still blocked on the owner, and honestly so

Only one thing: **which provider and model id answers the safety question for
you specifically.** The design decisions above no longer need you — they follow
from what both vendors shipped. But a BYOK harness cannot default this, and
picking one for you would spend your credits on your safety decisions.

`nvidia/llama-3.1-nemotron-safety-guard-8b-v3` and
`nvidia/nemotron-3.5-content-safety` are both published on the working NVIDIA
key and are purpose-built for this. They are a plausible default *for you*, but
they are not a default *for NIKI*.

## B8 · A live NVIDIA API key is committed, and G5 could not see it (2026-10-02)

**This is the most urgent item in this file. Rotate the key first.**

`src/cli/doctor.rs` has held a **real, working** NVIDIA API key since commit
`1182110` (2026-09-30, *"fix(security): the doctor's redaction Pass is now a
real check"*). It is on `github.com/RavaniRoshan/niki`, which is public.

It was committed as two adjacent string literals:

```rust
splice(&[
    "nvapi-2XcDwy…"   # first half,
    "dU8Up1X9…"   # second half,
]),
```

which concatenates at runtime to a byte-identical copy of the live key. Every
other row of that corpus is visibly fabricated — `sk-proj-AAAA…`,
`AKIA…" (a fake AWS example key, AWS' documented sample)`, `ghp_0123456789…`. This one row was real, almost
certainly copied from a working session while the corpus was being written, and
split across two literals so a line-based scanner would not match it.

**The G5 gate reported `PASS no credentials in the tree or in history`
throughout.** Its pattern was one `grep -E`:

```
(sk-ant-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9]{32,}|ghp_[A-Za-z0-9]{30,}|AKIA[0-9A-Z]{16})
```

Two independent holes, and the key went through both:

1. `nvapi` was not in the list at all — a whole provider this project
   explicitly supports had zero coverage.
2. Split literals are invisible to any regex over raw bytes.

### What has already been done

- `src/cli/doctor.rs` now holds a fabricated value of the same shape. The
  corpus still tests the same rule — `redact_secrets` catches it through the
  generic mixed-case/digit catch-all, exactly as it caught the real one — and
  `tests/secret_redaction.rs` still passes.
- `scripts/scan-secrets.py` replaces the grep. It scans every tracked file
  twice: raw, and with every string literal's contents concatenated. That
  second pass is what sees a split secret. Vendor prefixes added: `nvapi`,
  `sk-or-v1-`, `sk-proj-`, `hf_`, `sk_live_`, `xox[baprs]-`.
- `tests/secret_scan_can_fail.rs` feeds the scanner the exact shape that got
  through and asserts it is found — so the gate is known to be able to fail,
  which the previous one was not.
- `verify.sh` G5 now runs both the tree scan and a path-aware history scan.

```
$ python3 scripts/scan-secrets.py ; echo $?
0                                    # the tree is clean

$ git log --all -p --format="" | python3 scripts/scan-secrets.py --diff
EVIDENCE.md: openai (sk-): sk-canary012…
src/cli/doctor.rs: nvidia (nvapi-): nvapi-2XcDwy…
                                    # history is NOT clean, correctly
```

### What needs the owner, and why it is not a slice

1. **Rotate the key.** This is the only step that actually contains the
   exposure. Every copy of a public repository, every fork, every clone and
   every GitHub cache has it. Rewriting history does not unpublish anything;
   only rotation invalidates it. The key is also in this project's chat
   transcript, so treat it as compromised regardless of what is done to git.

2. **Rewrite history** to purge it (`git filter-repo`, then a force-push).
   Force-pushing a rewritten `master` is on the stop-and-ask list and I am not
   doing it unilaterally. It also does not rescue the key — see (1).

3. **`EVIDENCE.md` history finding.** My own doing: a fixture canary
   (`sk-canary…" (a fixture canary)`) is quoted in the command output of
   the CodeQL slice. It is not a credential and the current tree masks it, but
   it is in an earlier commit, so the history scan reports it. It disappears
   in the same rewrite as (2). If the rewrite is declined, the right answer is
   to leave the history scan red and say so — a red gate that is telling the
   truth beats a green one that is not.

### If history is never rewritten

Then this belongs in the release report as an accepted, disclosed finding:
*the repository's history contains one third-party API key, rotated on
2026-10-02, and the working tree does not.* That is a materially different
statement from silence, and it is the only honest one available.
