# Nikki Product Acceptance and Reliability Suite

This suite verifies Nikki as a complete product from the user's perspective, running through realistic end-to-end workflows, terminal interaction, agent execution, failures, recovery, persistence, performance, and final repository state.

## Architecture

The product verification layer orchestrates existing and new tests to answer one question: "Does Nikki work for a real developer?"

- **Layer A**: Static checks (formatting, linting)
- **Layer B**: Unit tests
- **Layer C**: Integration tests
- **CLI Smoke**: Basic binary execution and argument validation
- **TUI/PTY Tests**: Real terminal interactions via `headless_tui.py`
- **Visual Regression**: VHS-based layout and rendering checks
- **E2E Agent Workflows**: Deterministic multi-agent runs using a mock LLM server

## Running the Suite

Use the orchestration script:

```bash
./scripts/product-verify.sh
```

## Scenarios & Determinism

We rely on `tests/integration/mock_llm.py` to simulate provider endpoints deterministically.
Tests are located in `tests/product/runners/run_scenarios.sh`.
This allows testing the core application boundary without depending on external network state or nondeterministic LLM outputs.

## Adding a Scenario

Add new golden scenarios to `evals/product/dataset.toml` and implement the corresponding bash runner inside `tests/product/runners/run_scenarios.sh`. Ensure you verify the final git and filesystem state.
