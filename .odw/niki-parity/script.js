export const meta = {
  name: 'niki-parity',
  description:
    'Three-goal readiness audit for NIKI: (1) Codex parity, (2) test-suite wall-clock, ' +
    '(3) TUI correctness + a self-serve demo kit. Every parity and TUI claim is adversarially ' +
    'verified before it survives; speed strategies are judged, not refuted. Ends in one ' +
    'ordered, gated work plan.',
  phases: [
    { title: 'Parity sweep', detail: 'six subsystem agents, NIKI source vs openai/codex' },
    { title: 'Speed sweep', detail: 'four investigators over the real timing baseline' },
    { title: 'TUI sweep', detail: 'four agents over rendering, input, CLI, demo' },
    { title: 'Verify', detail: 'perspective-diverse skeptics refute or confirm each claim' },
    { title: 'Judge', detail: 'panel scores the speed strategies on evidence' },
    { title: 'Critique', detail: 'completeness critic names what every sweep missed' },
    { title: 'Synthesise', detail: 'one ordered plan with per-item acceptance criteria' },
  ],
}

// ── shared contracts ────────────────────────────────────────────────────────

const FINDING = {
  type: 'object',
  properties: {
    title: { type: 'string' },
    area: { type: 'string' },
    severity: { type: 'string', enum: ['critical', 'major', 'minor', 'polish'] },
    codexBehaviour: { type: 'string' },
    nikiBehaviour: { type: 'string' },
    evidence: { type: 'string' },
    codexEvidence: { type: 'string' },
    fix: { type: 'string' },
    effort: { type: 'string', enum: ['S', 'M', 'L'] },
  },
  required: ['title', 'area', 'severity', 'codexBehaviour', 'nikiBehaviour', 'evidence', 'codexEvidence', 'fix', 'effort'],
  additionalProperties: false,
}

const FINDINGS = {
  type: 'object',
  properties: { findings: { type: 'array', items: FINDING } },
  required: ['findings'],
  additionalProperties: false,
}

const VERDICT = {
  type: 'object',
  properties: {
    real: { type: 'boolean' },
    confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
    reason: { type: 'string' },
    fileLine: { type: 'string' },
  },
  required: ['real', 'confidence', 'reason', 'fileLine'],
  additionalProperties: false,
}

const STRATEGY = {
  type: 'object',
  properties: {
    name: { type: 'string' },
    area: { type: 'string' },
    minutesSaved: { type: 'string' },
    evidence: { type: 'string' },
    ramCost: { type: 'string' },
    risk: { type: 'string' },
    effort: { type: 'string', enum: ['S', 'M', 'L'] },
    change: { type: 'string' },
  },
  required: ['name', 'area', 'minutesSaved', 'evidence', 'ramCost', 'risk', 'effort', 'change'],
  additionalProperties: false,
}

const STRATEGIES = {
  type: 'object',
  properties: { strategies: { type: 'array', items: STRATEGY } },
  required: ['strategies'],
  additionalProperties: false,
}

const SCORES = {
  type: 'object',
  properties: {
    ranked: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          name: { type: 'string' },
          evidenceScore: { type: 'number' },
          impactScore: { type: 'number' },
          riskScore: { type: 'number' },
          total: { type: 'number' },
          verdict: { type: 'string' },
        },
        required: ['name', 'evidenceScore', 'impactScore', 'riskScore', 'total', 'verdict'],
        additionalProperties: false,
      },
    },
    rejected: { type: 'array', items: { type: 'string' } },
  },
  required: ['ranked', 'rejected'],
  additionalProperties: false,
}

// ── the facts the agents are not allowed to guess at ────────────────────────

const FACTS = `
# NIKI — verified ground truth (do NOT re-derive these; they were measured)

Repo: /home/shiva/projects/niki, branch \`rebuild/w-a-engine\`, PR #36 open, CI green.
Rust 2024, MSRV 1.88. ~817 lib unit tests + 42 integration test binaries.

## HARD RESOURCE RULE — this box has 7.5 GiB RAM and a prior full-suite run OOM-killed it
- NEVER run \`cargo build\`, \`cargo test\`, \`cargo clippy\`, or \`cargo nextest\`.
- NEVER run the full test suite in any form. Other processes share this machine.
- If a claim requires running something, say so explicitly in the finding as UNVERIFIED-BY-EXECUTION
  and reason from the source instead. Reading code is always safe; building is not.
- \`kilo\` and other agent processes are running concurrently — keep your own footprint tiny.

## Where things live
- src/ — 35 modules. Entry src/main.rs -> src/cli/.
- Tests: tests/*.rs (42 integration binaries), tests/common/, tests/integration/,
  tests/llm/, tests/visual/ (python, pytest_headless.ini), tests/tui_smoke/ (bash).
- CI: .github/workflows/{ci,codeql,deploy-docs,integration,nightly-eval,niki-review,tui-smoke,v-release}.yml
- scripts/test-layer.sh — local serial test runner. scripts/canary-gate.sh — mutation gate.
- .config/nextest.toml, mutants/canaries.toml, docs/claims-audit.md

## ALREADY CONFIRMED BY THE MAIN AGENT — do not re-report, build on it
- \`.config/nextest.toml\` declares \`[test-groups.heavy]\` and \`[test-groups.heap]\` with
  \`max-threads = 1\`, but NO test anywhere carries a \`#[test-group]\` attribute
  (\`grep -rn "test-group" --include=*.rs src/ tests/\` returns nothing). Both groups are
  EMPTY, so \`cargo nextest run --profile default\` runs the heavy binaries fully parallel —
  the exact opposite of what the file's own comment claims. \`scripts/test-layer.sh\` also
  says the nextest groups "mirror the categories below"; they do not.
- CI job durations, run 36388150898 (wall clock ~16 min):
  Product Acceptance Suite 14m, Visual regression 8m, Tests 7m, Windows 5m, Headless PTY
  (colour) 5m, Consumer journeys 5m, both macOS builds 5m, Real PTY TUI 4m, Headless PTY
  (NO_COLOR) 4m, E2E mock 4m, Linux build 3m, rest <2m.
  The \`Tests\` job runs \`cargo nextest run\` AND \`cargo build --release\`, and then
  \`Build x86_64-unknown-linux-gnu\` builds the same release binary again from scratch.
  \`Tests\` gates 6 downstream jobs, so it is a hard serialisation point.
- The previous wave of work left two known open items: (a) \`run_tui\` and \`run_chat\` are
  still two separate functions even though every seam is now shared; (b) the marketing
  screenshots in assets/screenshots/ show the first-run modal, not a diff.
`

const RULES = `
# How to work
- You are a READ-ONLY auditor. Read code, fetch sources, reason. Never modify a file.
- Prefer \`grep\`/\`read\` over guessing. Every claim must name a real file and, where you can
  manage it, a line number. A claim you cannot locate in the source is not a claim.
- You may fetch the real Codex source over the network, e.g.
  https://raw.githubusercontent.com/openai/codex/main/<path> and
  https://api.github.com/repos/openai/codex/contents/<dir>. Use it. If a fetch fails, set
  codexEvidence to "UNVERIFIED — no network" and keep going from knowledge.
- Rank by user-visible impact, not by how interesting the code is. A stranger running
  \`niki run "add a health endpoint"\` does not care about an internal trait. They care that
  it does not hang, does not lose their work, and tells them what happened.
- Return at most 5 findings. Five that are real beat twenty that are not. An empty array is
  a valid and respectable answer if the subsystem genuinely is at parity.
- Severity: critical = a user can lose work or get a silently wrong result; major = the
  product is visibly worse than Codex for a common path; minor = a rough edge; polish = taste.
`

// ── GOAL 1 — Codex parity, one agent per subsystem ──────────────────────────

const PARITY = [
  {
    key: 'engine',
    lens: 'Core engine and pipeline orchestration.',
    where: 'src/orchestrator/pipeline.rs, src/orchestrator/*.rs, src/agents/, src/risk/',
    ask:
      'Compare the revision loop, stage resolution, risk gating, the Critic placement, and ' +
      'RunOutcome handling against Codex. Specifically: is a single-source-of-truth run outcome ' +
      'actually derived in one place, and can any path still produce a verdict that disagrees ' +
      'with it? Does a hung or degraded provider fail the run or silently continue?',
  },
  {
    key: 'session',
    lens: 'Session lifecycle: start, resume, cancel, goals, persistence.',
    where: 'src/session/, src/goal/, src/persistence/, src/permissions/, src/cli/run.rs, tests/resume_cli.rs, tests/cancellation.rs',
    ask:
      'Compare session start/resume/cancel against Codex. Can a user Ctrl-C a run and get their ' +
      'artifacts and a truthful status? Can a run be resumed after a crash? Is cancel ' +
      'best-effort or fail-closed, and does a partial artefact ever masquerade as a complete one?',
  },
  {
    key: 'tools',
    lens: 'Tool runtime and sandbox isolation.',
    where: 'src/runtime/, src/sandbox/, src/tools/, src/safety/, src/mcp/',
    ask:
      'Compare the tool-call protocol, apply-patch, permission gating, and sandbox backends ' +
      'against Codex. Can a tool escape its sandbox, write outside its worktree, or reach the ' +
      'network when it should not? Is every destructive operation gated? What happens on a ' +
      'malformed tool call from the model?',
  },
  {
    key: 'llm',
    lens: 'Provider layer and the model-facing loop.',
    where: 'src/llm/, src/orchestrator/reflect.rs, src/orchestrator/provenance.rs, tests/llm_tool_calls.rs, tests/multi_provider.rs, tests/failover_chain.rs, tests/repair_retry.rs',
    ask:
      'Compare streaming, tool-call parsing, retry, failover, and context handling against Codex. ' +
      'What happens on a truncated tool call, a provider that returns malformed JSON, a rate ' +
      'limit, or a context overflow? Is there compaction, and does it lose the task spec?',
  },
  {
    key: 'cli',
    lens: 'The command-line surface a stranger actually types.',
    where: 'src/cli/, src/commands/, src/main.rs, README.md',
    ask:
      'Compare the one-line command surface against Codex: niki run, subcommand discovery, ' +
      'help text, --version, exit codes, --json output, and what happens on a bad flag or a ' +
      'missing API key. Would a first-time user get a useful error, or a panic or a silent no-op?',
  },
  {
    key: 'trust',
    lens: 'Config, secrets, audit trail, and supply chain.',
    where: 'src/config/, src/audit/, src/memory/, src/knowledge/, deny.toml, .github/workflows/codeql.yml, tests/secret_redaction.rs, tests/security_exec.rs',
    ask:
      'Compare against Codex: does a key ever reach a log, an artefact, or an LLM prompt? Is ' +
      'there an audit trail a user can read after a run? Does niki.toml precedence match the ' +
      'documentation exactly? Is anything in the trust story documented but not implemented?',
  },
]

// ── GOAL 2 — test wall-clock: investigators, then a judged panel ───────────
// Deliberately NOT refuted: a speed strategy is a bet, not a factual claim. Refuting it
// would kill exactly the unproven-but-real wins (sharding, caching) that matter most.

const SPEED = [
  {
    key: 'ci-graph',
    ask:
      'Take the CI job graph in .github/workflows/ci.yml and the measured job durations in ' +
      'FACTS. Find the critical path. Which jobs gate the most downstream work, which duplicate ' +
      'work another job already did, and which serialise only because of an over-broad `needs:`? ' +
      'Propose concrete graph edits with the wall-clock you expect to save on the measured numbers.',
  },
  {
    key: 'nextest',
    ask:
      'Audit the test-execution layer itself: .config/nextest.toml, scripts/test-layer.sh, and ' +
      'how heavy binaries are actually identified. Given that the declared nextest test-groups ' +
      'are EMPTY (see FACTS), work out what the correct grouping is, how to populate it without ' +
      'annotating hundreds of tests, and how to shard the suite across a CI matrix while keeping ' +
      'peak RAM bounded. Also check whether any test is slow for a fixable reason (a sleep, a ' +
      'fixed timeout, a fixture rebuilt per test) rather than for real work.',
  },
  {
    key: 'local-loop',
    ask:
      'The developer complaint is LOCAL iteration time — hours per loop. Audit what a developer ' +
      'actually has to run: scripts/test-layer.sh, the AGENTS.md rules, the build cost of a ' +
      'full clippy+test cycle, and whether a fast inner loop exists at all. Look for build-cache ' +
      'opportunities (sccache, a warm target dir, splitting debug from release), a genuinely ' +
      'useful watch mode, and a way to run only what changed. Propose the fastest possible ' +
      'under-one-minute inner loop, then the full gate.',
  },
  {
    key: 'sandboxed',
    ask:
      'Propose ways to take the expensive verification OFF this 7.5 GiB box entirely: GitHub ' +
      'Actions as a remote test runner, sharded matrices, running the heavy binaries on a ' +
      'self-hosted or larger runner, and a local "push and let CI prove it" path. For each, say ' +
      'what it costs, what it makes impossible, and what a developer loses by not having it locally. ' +
      'Check the free-tier/GitHub-hosted-minute reality rather than assuming.',
  },
]

// ── GOAL 3 — TUI correctness and a self-serve demo ─────────────────────────

const TUI = [
  {
    key: 'render',
    ask:
      'Audit RENDERING edge cases in src/display/: 40-column and 200-column terminals, a ' +
      'terminal resized mid-render, CJK and emoji and combining characters (display width vs ' +
      'byte length vs char count), a tool call that emits 5000 lines, a very long single line ' +
      'with no spaces, an empty transcript, and NO_COLOR vs forced colour. Use the width helpers ' +
      'the code actually has. Name the exact input that breaks the layout and the line where it breaks.',
  },
  {
    key: 'input',
    ask:
      'Audit INPUT and interaction in src/display/ and src/tui: keybindings, route_overlay_key, ' +
      'route_mouse, modal focus and escape hatches, bracketed paste, Ctrl-C in a modal, mouse ' +
      'capture with no mouse support, and what happens when a key is pressed mid-animation. ' +
      'Find inputs where the user presses something and NOTHING visibly happens. Those are the ' +
      'worst defects in a TUI. Name the exact key sequence.',
  },
  {
    key: 'onboard',
    ask:
      'Audit the FIRST-RUN path end to end, as a stranger: no config, no API key, no git repo, ' +
      'wrong API key, wrong model name, a git repo with uncommitted changes, a dirty target dir. ' +
      'Read the actual code paths. For each, state what the user sees and whether it is ' +
      'recoverable. Then judge the CLI one-liners (--version, --help, subcommand list) on clarity.',
  },
  {
    key: 'demo',
    ask:
      'Design the DEMO KIT. A stranger must be able to go from nothing to seeing NIKI actually ' +
      'work in under five minutes, with no API key and no container runtime. Inventory what ' +
      'already exists for this (tests/integration/mock_llm.py, scripts/, README, demo.tape, ' +
      'the worktree backend) and find the gap. Produce the concrete kit — the commands, the ' +
      'script, the prerequisites — and be honest about what still needs a real LLM key.',
  },
]

// ═══ script ════════════════════════════════════════════════════════════════

const CODE = `args && args.codexRef ? args.codexRef : 'https://github.com/openai/codex'`

// ── GOAL 1 ──────────────────────────────────────────────────────────────────
phase('Parity sweep')
log('six parity agents: NIKI source vs the real openai/codex tree')
const parityRaw = await parallel(
  PARITY.map((p) => () =>
    agent(
      `${FACTS}\n${RULES}\n\n# Your lens: ${p.lens}\n# Primary source: ${p.where}\n\n` +
        `${p.ask}\n\nReference implementation to compare against: ${CODE} (openai/codex, ` +
        `Rust). Fetch the real files where you can — e.g. ` +
        `https://raw.githubusercontent.com/openai/codex/main/codex-rs/<crate>/src/<file>.rs, ` +
        `and the directory listing at https://api.github.com/repos/openai/codex/contents/codex-rs ` +
        `to discover the crates.\n\nSet \`area\` to "${p.key}". Return at most 5 findings, most severe first.`,
      { executor: 'kilo', label: `parity:${p.key}`, phase: 'Parity sweep', schema: FINDINGS },
    ).then((r) => (r ? { lens: p.key, findings: r.findings || [] } : null)),
  ),
)
const parityFindings = parityRaw.filter(Boolean).flatMap((g) => g.findings)
log(`parity sweep: ${parityFindings.length} raw claims from ${parityRaw.filter(Boolean).length}/6 lenses`)

// ── GOAL 2 ──────────────────────────────────────────────────────────────────
phase('Speed sweep')
const speedRaw = await parallel(
  SPEED.map((s) => () =>
    agent(
      `${FACTS}\n${RULES}\n\n# Your assignment: cut test wall-clock.\n\n${s.ask}\n\n` +
        `Be concrete and quantitative: name files, name the exact edit, and state the minutes ` +
        `saved against the measured baseline in FACTS. If you propose changing CI, quote the YAML ` +
        `you would write. Do not propose anything that raises peak RAM on a 7.5 GiB runner.\n\n` +
        `Set \`area\` to "${s.key}". Return at most 5 strategies, highest impact first.`,
      { executor: 'kilo', label: `speed:${s.key}`, phase: 'Speed sweep', schema: STRATEGIES },
    ).then((r) => (r ? { lens: s.key, strategies: r.strategies || [] } : null)),
  ),
)
const speedStrategies = speedRaw.filter(Boolean).flatMap((g) => g.strategies)
log(`speed sweep: ${speedStrategies.length} strategies from ${speedRaw.filter(Boolean).length}/4 investigators`)

// ── GOAL 3 ──────────────────────────────────────────────────────────────────
phase('TUI sweep')
const tuiRaw = await parallel(
  TUI.map((t) => () =>
    agent(
      `${FACTS}\n${RULES}\n\n# Your assignment: the terminal UI.\n\n${t.ask}\n\n` +
        `If you are asked to design something rather than find a defect (the demo kit), set ` +
        `codexBehaviour to what Codex offers that NIKI does not, and put your actual deliverable ` +
        `in \`fix\`. For defects, put the exact failing input in \`nikiBehaviour\` and the line ` +
        `that breaks in \`evidence\`.\n\nSet \`area\` to "tui-${t.key}". Return at most 5 findings, most severe first.`,
      { executor: 'kilo', label: `tui:${t.key}`, phase: 'TUI sweep', schema: FINDINGS },
    ).then((r) => (r ? { lens: t.key, findings: r.findings || [] } : null)),
  ),
)
const tuiFindings = tuiRaw.filter(Boolean).flatMap((g) => g.findings)
log(`tui sweep: ${tuiFindings.length} raw claims from ${tuiRaw.filter(Boolean).length}/4 lenses`)

// ── GOAL 3 deliverable (not a refutable claim — kept whole) ─────────────────
const demoKit = await agent(
  `${FACTS}\n${RULES}\n\n# Deliverable: the self-serve demo kit.\n\n` +
    `A stranger with no API key and no container runtime must reach "I watched the agent work" in ` +
    `under five minutes. Write the actual kit as a concrete, runnable spec: the exact commands, the ` +
    `exact files to create, the prerequisites, and the honest limits (what still needs a real key). ` +
    `Check tests/integration/mock_llm.py, scripts/, README.md, demo.tape, and the \`--backend ` +
    `worktree\` path. Be specific enough that an engineer could implement it without asking you a ` +
    `question. Markdown, no JSON.`,
  { executor: 'kilo', label: 'demo-kit', phase: 'TUI sweep' },
)

// ── VERIFY: perspective-diverse skeptics ───────────────────────────────────
// Two skeptics per claim, deliberately holding different cards:
//   LOCATOR  — must find the defect in NIKI's own source or prove it absent.
//   PARITY   — must establish that Codex really behaves as claimed, or that the
//              gap really is user-visible. Catches "NIKI is missing a feature
//              Codex has" claims built on a hallucinated Codex behaviour.

const allClaims = [
  ...parityFindings.map((f) => ({ ...f, goal: 'parity' })),
  ...tuiFindings.map((f) => ({ ...f, goal: 'tui' })),
]
  // Deduplicate on area+title so two lenses reporting the same thing cost one verification.
const seen = {}
const claims = allClaims.filter((c) => {
  const k = `${c.goal}:${c.area}:${String(c.title).toLowerCase().slice(0, 60)}`
  if (seen[k]) return false
  seen[k] = true
  return true
})
log(`${allClaims.length} claims -> ${claims.length} after dedup; verifying each with 2 skeptics`)

const LOCATOR = `You are a SKEPTIC whose job is to REFUTE. A claim has been made about this repo. Your default answer is "not real". Open the cited files yourself and check.

CLAIM TITLE: {T}
AREA: {A}
WHAT CODEX DOES: {C}
WHAT NIKI DOES: {N}
CLAIMED EVIDENCE: {E}
CLAIMED FIX: {F}

Your lens: DOES IT EXIST IN NIKI'S SOURCE? Locate the exact code. If the claim describes missing behaviour, prove the absence by showing what IS there instead. If the claimed evidence names a file, read it and check it says what the claim says.

Set real=false if: the code contradicts it, the file does not exist, the behaviour is actually already handled elsewhere, the claim misreads the code, or the "fix" would break something you can see. Set real=true ONLY if you personally read the code and it confirms the claim.
Set fileLine to "path:line" you actually verified. Confidence must be honest: "low" if you could not find the file, "high" only if you read the decisive lines.`

const PARITYIST = `You are a SKEPTIC whose job is to REFUTE a PARITY claim — i.e. a claim that NIKI is worse than OpenAI's Codex. Your default answer is "not a real gap".

CLAIM TITLE: {T}
AREA: {A}
WHAT CODEX IS SAID TO DO: {C}
WHAT NIKI IS SAID TO DO: {N}
CODEX EVIDENCE CLAIMED: {CE}
NIKI EVIDENCE CLAIMED: {E}
CLAIMED FIX: {F}

Your lens: IS THE CODEX SIDE TRUE, AND DOES THE GAP MATTER? Codex is at {REF} — fetch the real files (https://raw.githubusercontent.com/openai/codex/main/... and the directory listing at https://api.github.com/repos/openai/codex/contents/codex-rs). Verify Codex actually does this. Then ask: would a real user hit this difference on a normal task, or is this an internal difference nobody can observe?

Set real=false if: Codex does not actually do this; the difference is cosmetic or internal-only; the user-visible impact is negligible; or NIKI deliberately made a defensible different choice and the claim mislabels it as a defect. Set real=true only if Codex demonstrably does it AND a user would feel it.
Set fileLine to the Codex path you verified, or "CODEX-UNVERIFIED" if you could not fetch.`

const verified = await pipeline(
  claims,
  (c) =>
    parallel([
      () =>
        agent(
          LOCATOR.replace('{T}', c.title).replace('{A}', c.area).replace('{C}', c.codexBehaviour)
            .replace('{N}', c.nikiBehaviour).replace('{E}', c.evidence).replace('{F}', c.fix) +
            `\n\n${FACTS}`,
          { executor: 'kilo', label: `locate:${c.area}`, phase: 'Verify', schema: VERDICT },
        ),
      () =>
        agent(
          PARITYIST.replace('{T}', c.title).replace('{A}', c.area).replace('{C}', c.codexBehaviour)
            .replace('{N}', c.nikiBehaviour).replace('{CE}', c.codexEvidence).replace('{E}', c.evidence)
            .replace('{F}', c.fix).replace('{REF}', CODE) +
            `\n\n${RULES}\n\nBe strict: most parity claims you will see are false. Codex is a large ` +
            `team's product; assume nothing about it that you have not read.`,
          { executor: 'kilo', label: `parity?:${c.area}`, phase: 'Verify', schema: VERDICT },
        ),
    ]).then((vs) => {
      const v = vs.filter(Boolean)
      if (v.length < 2) return null
      const realCount = v.filter((x) => x.real).length
      return { ...c, verdicts: v, survive: realCount === 2, realCount }
    }),
)
const confirmed = verified.filter(Boolean).filter((c) => c.survive)
const refuted = verified.filter(Boolean).filter((c) => !c.survive)
log(`verify: ${confirmed.length} confirmed, ${refuted.length} refuted of ${claims.length}`)

// ── JUDGE: score the speed strategies ──────────────────────────────────────
phase('Judge')
const ranked = speedStrategies.length
  ? await agent(
      `${FACTS}\n\n# Judge these test-speed strategies.\n\n${JSON.stringify(speedStrategies, null, 2)}\n\n` +
        `Score each 0-10 on: evidenceScore (is it grounded in this repo's actual files and the ` +
        `measured baseline, or is it a generic best practice?), impactScore (minutes actually ` +
        `saved), riskScore (how likely is it to introduce flakiness, RAM blowup, or a CI bill). ` +
        `total = evidenceScore*3 + impactScore*3 + riskScore*2. Put anything with evidenceScore <= 2 ` +
        `in \`rejected\` with a one-line reason — a plausible idea with no evidence behind it is the ` +
        `main failure mode here.`,
      { executor: 'kilo', label: 'judge:speed', phase: 'Judge', schema: SCORES },
    )
  : null

// ── CRITIQUE: what did every lens miss? ─────────────────────────────────────
phase('Critique')
const critique = await agent(
  `${FACTS}\n\nYou are the completeness critic. Three sweeps just ran over this repo. Their raw output:\n\n` +
    `## PARITY (${parityRaw.filter(Boolean).length}/6 lenses reported)\n${JSON.stringify(parityRaw.filter(Boolean), null, 2).slice(0, 20000)}\n\n` +
    `## SPEED (${speedRaw.filter(Boolean).length}/4 investigators reported)\n${JSON.stringify(speedRaw.filter(Boolean), null, 2).slice(0, 20000)}\n\n` +
    `## TUI (${tuiRaw.filter(Boolean).length}/4 lenses reported)\n${JSON.stringify(tuiRaw.filter(Boolean), null, 2).slice(0, 20000)}\n\n` +
    `# Your job\n1. Which lens returned an empty or suspiciously thin result? Name it — that is a ` +
    `coverage hole, not a clean bill of health.\n2. What whole area did ALL THREE sweeps skip? ` +
    `Think: install/dist packaging, the docs the user reads first, error messages, the Windows ` +
    `path, performance under load, accessibility of the TUI to a screen reader, i18n, what happens ` +
    `when the network dies mid-run.\n3. What is the single most load-bearing thing still missing ` +
    `for a stranger to install this and trust it?\nMarkdown, no JSON. Be blunt.`,
  { executor: 'kilo', label: 'completeness-critic', phase: 'Critique' },
)

// ── SYNTHESISE ──────────────────────────────────────────────────────────────
phase('Synthesise')
const plan = await agent(
  `${FACTS}\n\n# Build the execution plan.\n\n` +
    `## CONFIRMED findings — both skeptics agreed these are real (${confirmed.length})\n` +
    `${JSON.stringify(confirmed, null, 2).slice(0, 40000)}\n\n` +
    `## REFUTED — do NOT put these in the plan; they are here so you do not re-derive them\n` +
    `${JSON.stringify(refuted.map((r) => ({ title: r.title, area: r.area, why: r.verdicts.map((v) => v.reason) })), null, 2).slice(0, 12000)}\n\n` +
    `## SCORED speed strategies\n${JSON.stringify(ranked, null, 2).slice(0, 12000)}\n\n` +
    `## COMPLETENESS CRITIC\n${critique}\n\n` +
    `## PROPOSED DEMO KIT\n${demoKit}\n\n` +
    `# Output\n` +
    `One ordered plan in Markdown. For each item: what to change, which files, why it is on the ` +
    `list, how big it is (S/M/L), and an ACCEPTANCE CRITERION that a machine could check. Order by ` +
    `what unblocks the most else. Group under three headings matching the three goals.\n\n` +
    `Rules for the ordering: the fastest feedback loop first (a developer who cannot iterate cannot ` +
    `verify anything else), then the things that make a stranger's first five minutes work, then ` +
    `everything else. Call out explicitly if a goal is already in better shape than the evidence ` +
    `suggests — do not manufacture work. End with the three things you would do first if you had ` +
    `one day.`,
  { executor: 'kilo', label: 'synthesis', phase: 'Synthesise' },
)

return {
  counts: {
    parityLenses: parityRaw.filter(Boolean).length,
    parityRaw: parityFindings.length,
    tuiLenses: tuiRaw.filter(Boolean).length,
    tuiRaw: tuiFindings.length,
    speedInvestigators: speedRaw.filter(Boolean).length,
    speedStrategies: speedStrategies.length,
    claimsDeduped: claims.length,
    confirmed: confirmed.length,
    refuted: refuted.length,
  },
  confirmed,
  refuted: refuted.map((r) => ({ title: r.title, area: r.area, goal: r.goal })),
  speed: ranked,
  critique,
  demoKit,
  plan,
}
