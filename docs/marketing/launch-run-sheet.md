# 5-Day Launch Run Sheet

## Pre-launch (Day -2 to Day 0)
- [ ] Final code freeze at end of Day 0
- [ ] Tag v0.4.0 on master
- [ ] Verify `cargo install niki` works against crates.io
- [ ] Verify `brew install niki` formula
- [ ] Verify `curl -L https://niki.ai/install | bash` (or https://github.com/RavaniRoshan/niki/releases)
- [ ] Generate demo.gif and demo.mp4 via VHS
- [ ] Create 3 PH gallery images (1270×760)
- [ ] Write Show HN post from established account
- [ ] Draft PH first comment
- [ ] Write social media posts (X thread, LinkedIn, dev.to, Reddit)
- [ ] Create landing page and deploy to GitHub Pages
- [ ] Verify README has new demo.gif, install matrix, badge set, "What it's NOT" section
- [ ] Run final `cargo test` + `cargo clippy` + `cargo audit` + `cargo deny check`
- [ ] Verify SECURITY.md and CONTRIBUTING.md are up to date

## Day 0 — Truth & Release Plumbing (Morning)
- [ ] Delete/silence skeleton config traps (DONE via goal/c4d8f1)
- [ ] Fix package manifests (DONE via goal/c4d8f1)
- [ ] Tag v0.4.0 (TODO)
- [ ] Enable github-attestations + SBOM in release workflow (TODO)
- [ ] Add integration tests to CI (DONE via goal/c4d8f1)
- [ ] Run integration tests, fix failures (TODO)
- [ ] Set up release: cargo-dist build matrix

## Day 1 — Authentication Flow (Morning)
- [ ] niki auth login (DONE via goal/c7e9d3)
- [ ] niki doctor (DONE via goal/c7e9d3)
- [ ] niki init --interactive wizard (DONE via goal/c7e9d3)
- [ ] JSON config schema export (DONE via goal/c7e9d3)
- [ ] Error UX pass: top 5 failure modes with fix commands (TODO)
- [ ] Secret redaction test (TODO)

## Day 1 — Auth Flow (Afternoon)
- [ ] Verify auth flow end-to-end
- [ ] Test login/logout/status cycle
- [ ] Test doctor on clean machine (CI cache wipe)
- [ ] Write docs for auth flow

## Day 2 — Marketing Kit (Morning)
- [ ] Generate VHS demo (script done, rendering needed)
- [ ] Finalize landing page content
- [ ] Create PH gallery images
- [ ] Write Show HN post
- [ ] Schedule PH launch (12:01 AM PST Day 4)

## Day 2 — Chat-UI Gate (Afternoon)
- [ ] Attempt to wire chat input path (bounded 1.5 days)
- [ ] If works: enable in TUI
- [ ] If fails: fall back to viewer-only TUI, mark chat as post-launch #1

## Day 3 — Security & Trust Pass (Morning)
- [ ] Verify trust-boundary docs (DONE via SECURITY.md update)
- [ ] Symlink handling audit
- [ ] No config auto-exec in sandbox
- [ ] Command deny-list regression test
- [ ] SSRF guard test
- [ ] Secret storage 0600 permissions
- [ ] LLM retry + timeout test

## Day 3 — Security & Trust Pass (Afternoon)
- [ ] Enable GitHub security: private advisories, secret scanning, Dependabot, CodeQL
- [ ] Branch protection: PR + 1 approval
- [ ] Add FUNDING.yml
- [ ] Add 2nd admin
- [ ] Spendsafety: per-run token cap
- [ ] Telemetry decision: opt-in (DONE — default off)

## Day 4 — Launch Day (AM)
- 00:01 PST: Product Hunt goes live + first comment posted
- 00:01 PST: Share HN thread link in PH comment
- 07:00-10:00 PST: Show HN post (if karma account ready)
- All day: Monitor both, reply to comments

## Day 4 — Launch Day (PM)
- 12:00 PST: Post LinkedIn update
- 13:00 PST: Post dev.to tutorial
- 14:00 PST: Post to relevant Reddit communities
- 15:00 PST: Start X thread (if not posted earlier)
- All day: Comment duty (shifts if possible)

## Day 5 — Post-launch
- 08:00 PST: PH badge install on landing page
- 09:00 PST: Waitlist nurture email (first batch)
- 10:00 PST: Retrospective — what broke, what worked
- 11:00 PST: Triage all feedback, file issues
- Evening: X/LinkedIn follow-ups

## Ongoing (post-launch day 5)
- Respond to all HN/PH comments for 48 hours
- Monitor GitHub issues for bugs
- Collect email list signups
- Plan Phase A: MCP, sessions, headless CI mode
