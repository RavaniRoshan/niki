# NIKI Site — Landing Page Update Prompt

> **Run this prompt in the `research/refs/niki-site` directory.**
> The niki-site repo is a Vite + React app deployed to Vercel.
> The main repo is at `/home/shiva/projects/niki` (reference for copy/assets).

---

## Context

NIKI v0.4.0 just completed an open-source launch prep (Phases 0–7). The landing page needs to be updated to match the new positioning and fix known issues.

**New positioning (from the main repo's `docs/positioning.md`):**
> Niki is the open-source multi-agent coding pipeline that plans, codes, tests, and reviews — then hands you a verified git branch.

**Short version (hero):**
> One sentence in, a verified pull request out.

**Technical:**
> Four independent LLM agents run in isolated sandboxes and produce a reviewable `niki/<id>` branch with full audit trail.

---

## Tasks

### 1. Hero Copy Update (`src/components/Hero.jsx`)

**Current hero:**
```
"Describe it. Niki ships a verified pull request."
```

**Update to:**
```
"One sentence in, a verified pull request out."
```

**Current lede (below hero):**
```
Niki is an AI software engineering assistant that turns a requested change into a
verified pull request branch. Four isolated agents — Planner, Coder, Tester, Reviewer —
```

**Update to:**
```
Four independent LLM agents — Planner, Coder, Tester, Reviewer — run in hermetic
sandboxes and hand you a reviewable niki/<id> branch with a full audit trail.
Your working tree is never touched.
```

**Why:** The old copy says "AI software engineering assistant" (too generic) and "isolated agents" (weaker than "independent"). The new copy uses the exact positioning statement and adds the concrete output format.

### 2. WhatIsNiki Section (`src/components/WhatIsNiki.jsx`)

Review and update the section title and body to match:
- Title: "What is Niki?"
- Body should include: "Niki is the open-source multi-agent coding pipeline that plans, codes, tests, and reviews — then hands you a verified git branch."
- Mention: BYOK (bring your own key), no telemetry, hermetic sandbox, 4 independent agents

### 3. Compare Table (`src/components/Compare.jsx`)

**Current comparison rows:**
```js
const ROWS = [
  ['Agent topology', 'One model, shared context', 'Four isolated roles · typed handoffs'],
  ['Filesystem safety', 'Edits live workspace', 'Podman / Docker / worktree sandbox on a copy'],
  ['Output shape', 'Opaque file churn', 'niki/<id> branch + patch + report'],
  ['Model policy', 'Vendor lock-in common', 'BYOK · mix providers per agent'],
  ['Audit trail', 'Chat scrollback', 'JSON artifacts + optional security pass'],
  ['Runtime', 'Proprietary cloud agents', 'Rust CLI · Podman / Docker / worktree / cloud'],
]
```

**Add two more rows:**
```js
['Revision loop', 'Manual steering', 'Reviewer bounces back to Coder until approved'],
['Security', 'Trust the model', 'CapDrop ALL · network off · secret redaction · spend cap'],
```

**Update heading:**
```
"Built for review, not auto-merge theater"
```
→
```
"Four agents, one verified branch"
```

### 4. Install Section (`src/components/Install.jsx`)

Verify the install commands match the main README:
```bash
# macOS
brew install niki

# Linux/macOS
curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.sh | bash
```

Make sure the config step is clear:
```bash
cp niki.example.toml niki.toml
export ANTHROPIC_API_KEY=sk-ant-...
```

### 5. SEO / Meta Tags (`src/seo.jsx` + `index.html`)

**Current title:**
```
Niki · AI coding agents that ship reviewable pull requests
```

**Update to:**
```
Niki · Multi-agent coding pipeline that ships verified pull requests
```

**Current description:**
```
Niki is a hermetic multi-agent AI coding system where Planner, Coder, Tester, and Reviewer agents collaborate in a sandbox and deliver a clean pull request branch.
```

**Update to:**
```
Four independent LLM agents plan, code, test, and review in hermetic sandboxes — then hand you a verified niki/<id> branch with a full audit trail. Open source, BYOK, no telemetry.
```

**Verify `index.html`:**
- `<meta property="og:title">` matches new title
- `<meta property="og:description">` matches new description
- `<meta name="description">` matches new description
- `<script type="application/ld+json">` has correct name and description
- `robots.txt` allows indexing
- `llms.txt` exists in public/ (for LLM crawlers)

### 6. Social Proof / Trust Signals

If the ProductHunt component is live, update the copy to match the new positioning. Remove any "not yet wired" or "beta" badges if v0.4.0 is now the stable launch version.

### 7. Dead Link Audit

Check every `<a href>` and `<Link to>` in the site components:
- `src/components/Hero.jsx` — GitHub link, CTA link
- `src/components/Install.jsx` — install commands, docs link
- `src/components/Footer.jsx` — social links, docs link
- `src/components/TopNav.jsx` — nav links

Fix any broken links. The docs site is at a different URL (not the landing page).

### 8. Visual QA

After making changes, verify:
- [ ] Dark mode renders correctly (theme toggle)
- [ ] Light mode renders correctly
- [ ] Mobile responsive (no horizontal scroll)
- [ ] Hero gradient displays properly
- [ ] Compare table is readable on mobile
- [ ] Install code blocks have proper syntax highlighting
- [ ] Footer links work
- [ ] No console errors

---

## Files to Modify

| File | Change |
|---|---|
| `src/components/Hero.jsx` | Hero title + lede |
| `src/components/WhatIsNiki.jsx` | Section copy |
| `src/components/Compare.jsx` | Table rows + heading |
| `src/components/Install.jsx` | Verify install commands |
| `src/seo.jsx` | Title + description |
| `index.html` | OG tags, meta, JSON-LD |
| `public/robots.txt` | Verify allows indexing |
| `public/llms.txt` | Verify exists and is current |

---

## Commit Message

```
feat: update landing page for v0.4.0 launch

- Hero: 'One sentence in, a verified pull request out.'
- Positioning: multi-agent coding pipeline
- Compare table: add revision loop + security rows
- SEO: updated title, description, OG tags
- Dead link audit
```

---

## Verification

```bash
cd research/refs/niki-site
npm run dev        # local dev server
# Open http://localhost:5173
# Check dark mode, light mode, mobile
# Verify all links work
# Check SEO with: curl -s http://localhost:5173 | grep -i "og:title"
```
