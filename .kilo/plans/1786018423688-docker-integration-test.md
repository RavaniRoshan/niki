# Docker Integration Test Plan for NIKI

## Goal
Run a real end-to-end integration test of NIKI inside a Docker Ubuntu container, using the worktree backend (no nested container runtime), with a real API key to verify the full Planner → Coder → Tester → Reviewer pipeline works on the `goal/niki-10x-features-v2` branch.

## Prerequisites (on host)
- Docker or Podman running on the host
- The `goal/niki-10x-features-v2` branch must be committed and pushed to GitHub first
- An LLM API key (ANTHROPIC_API_KEY or OPENAI_API_KEY) — passed via Docker env, never baked into images

## Steps

### Step 1: Commit and push the current branch
- Stage all new and modified files on `goal/niki-10x-features-v2`
- Commit with a descriptive message
- Push to GitHub (`git push -u origin goal/niki-10x-features-v2`)

### Step 2: Create a Dockerfile for the integration test
Create `tests/integration/Dockerfile`:

```dockerfile
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    git curl build-essential pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Clone NIKI from GitHub
RUN git clone https://github.com/RavaniRoshan/niki.git /niki

WORKDIR /niki

# Build release binary
RUN cargo build --release

# Create a sample test project inside the container
RUN mkdir -p /test-project && cd /test-project && \
    git init && \
    echo '# Test App\n\nconsole.log("hello");' > index.js && \
    echo 'node_modules/\n.niki/' > .gitignore && \
    git add . && git commit -m "initial"

WORKDIR /test-project
CMD ["/bin/bash"]
```

### Step 3: Build and run the Docker container
```bash
docker build -t niki-integration-test -f tests/integration/Dockerfile .

docker run --rm -it \
  -e ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY" \
  niki-integration-test \
  /bin/bash -c "/niki/target/release/niki run 'Add a health check endpoint that returns {status: ok}' --project /test-project --backend worktree --verbose"
```

### Step 4: Verify artifacts inside the container
After the pipeline completes, exec into the container (or chain commands):
- Check `git -C /test-project branch | grep niki/` — a niki branch exists
- Check `/test-project/.niki/` directory has `report.md`, `changes.patch`, `artifacts/*.json`
- Check exit code is 0

### Step 5: Inspect the branch
- `git -C /test-project log niki/* --oneline` — shows the commit NIKI made
- `git -C /test-project diff main..niki/*` — shows the actual code changes
- Read `report.md` — shows reviewer verdict, scores, revision count

### Step 6: Clean up
- Docker container is `--rm`, so it auto-removes
- Remove the test image: `docker rmi niki-integration-test`

## What This Validates
1. ✅ `cargo build --release` succeeds on a clean Ubuntu 24.04
2. ✅ Binary runs and all CLI subcommands work
3. ✅ Full Planner → Coder → Tester → Reviewer pipeline completes
4. ✅ Worktree backend creates isolated git worktrees correctly
5. ✅ Git branch creation and commit work
6. ✅ Artifacts (report, patch, JSON) are generated
7. ✅ No panics or runtime errors in production-like environment

## Files to Create
- `tests/integration/Dockerfile` — the integration test container definition

## Risk Mitigation
- API key is passed via `-e` env var, never stored in image layers
- `--rm` flag ensures container cleanup
- Worktree backend avoids nested container runtime complexity
- Container is ephemeral — no host state modified

## Verification Checklist
After running:
- [ ] Docker container builds without errors
- [ ] `cargo build --release` compiles in container
- [ ] `niki run` completes with exit code 0
- [ ] `niki/<id>` branch exists in test project
- [ ] `.niki/report.md` contains reviewer verdict
- [ ] `.niki/changes.patch` contains a diff
- [ ] `.niki/artifacts/` contains JSON files for each agent
