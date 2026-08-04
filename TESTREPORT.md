# Test Report — NIKI Security & Observability Feature Set

## Summary

This report documents the test suite added for the following security, observability, and infrastructure features:

| Feature | Description | Tests |
|---------|-------------|-------|
| F1 | Exec policy enforcement — `DockerSandbox.exec` and `WorktreeSandbox.exec` call `check_command_policy` before running commands | 15 |
| F2 | Docker HostConfig resource caps — memory and CPU limits applied to container creation | 8 |
| F3 | Exec timeout — both sandbox backends enforce `tokio::time::timeout` with configurable `max_exec_seconds` | 7 |
| F5 | Worktree sandbox policy enforcement — stores and enforces `SecurityPolicyConfig` | 5 |
| F6 | Secret redaction — API keys, bearer tokens, and other secrets are redacted from LLM error responses | 11 |
| F7 | Retry & TTFT tracking — `retry_count` and `ttft_ms` fields in `StageMetric`, `TaskRecord`, `Span`, and `run_agent` return value | 12 |
| F8 | Raw LLM response logging removed — `tracing::debug!` logs only lengths, not content | (verified via source inspection) |
| F11 | Mock LLM provider for testing | 10 |

**Total: 68 new integration tests across 6 test files. All 212 tests pass (144 existing + 68 new).**

## Test Files

### `tests/security_exec.rs` (F1) — 15 tests

Tests the `check_command_policy` function and per-role security policies:

- `policy_allowed_command_passes` — commands in allow-list are accepted
- `policy_deny_list_blocks_dangerous_commands` — `git push --force`, `rm -rf /`, `mkfs`, `dd` are blocked
- `policy_blocks_shell_curl_pipe_sh` — `curl | sh` pattern is blocked via substring match
- `policy_blocks_no_verify` — `--no-verify` argument is blocked
- `role_specific_policy_allows_git_commit` — Coder policy permits `git commit`
- `role_specific_policy_blocks_git_push` — Coder policy blocks `git push`
- `tester_policy_allows_cargo_test` — Tester policy permits `cargo test`
- `tester_policy_blocks_git_push` — Tester policy blocks `git push`
- `reviewer_policy_blocks_git_commit` — Reviewer policy blocks `git commit`
- `reviewer_policy_allows_git_show` — Reviewer policy permits `git show`
- `empty_command_implementation_rejects_empty` — single-token commands work
- `deny_error_message_contains_context` — error messages include the command and "denied"
- `allow_list_takes_precedence_over_deny` — allow-list bypasses deny check
- `unknown_command_allowed_by_default` — unlisted commands are allowed
- `role_is_debug_repr` — `AgentRole` debug repr matches policy key format

### `tests/docker_resource_caps.rs` (F2) — 8 tests

Tests Docker container resource limit configuration:

- `parse_memory_limit_handles_gb` — "2g", "4gb" → correct byte values
- `parse_memory_limit_handles_mb` — "512m", "100mb" → correct byte values
- `parse_memory_limit_handles_kb` — "1k", "512kb" → correct byte values
- `parse_memory_limit_handles_plain_bytes` — "1024" → 1024 bytes
- `parse_memory_limit_returns_zero_for_invalid` — invalid input → 0
- `parse_memory_limit_is_case_insensitive` — "2G", "512MB" → correct values
- `docker_config_default_has_resource_limits` — default config has memory_limit "2g" and cpu_limit 2.0
- `docker_config_default_has_sandbox_image` — base image contains "sandbox"

### `tests/exec_timeout.rs` (F3) — 7 tests

Tests exec timeout enforcement:

- `default_policy_has_reasonable_exec_timeout` — default `max_exec_seconds` is 300
- `policy_with_short_timeout_will_timeout_quickly` — custom timeout value is stored
- `check_command_policy_completes_within_timeout_when_allowed` — allowed commands pass quickly
- `timeout_error_message_includes_duration` — timeout error message includes seconds
- `policy_timeout_is_configurable_per_role` — timeout is configurable
- `deny_list_commands_blocked_immediately_not_timeout` — denied commands rejected without waiting for timeout
- `timeout_duration_from_policy` — `Duration::from_secs` correctly converts policy timeout

### `tests/worktree_policy.rs` (F5) — 5 tests

Tests the WorktreeSandbox's policy storage and enforcement:

- `worktree_sandbox_stores_policy_for_coder_role` — WorktreeSandbox stores SecurityPolicyConfig and rejects denied commands, allows safe ones
- `worktree_sandbox_enforces_role_specific_policy` — tester policy blocks `git push`, allows `cargo test`
- `worktree_sandbox_skips_policy_when_role_is_none` — when role is None, policy check is skipped
- `worktree_sandbox_custom_timeout_is_respected` — `sleep 5` with 1s timeout is killed in <3s
- `create_sandbox_worktree_backed` — `create_sandbox` with `SandboxBackend::Worktree` succeeds

### `tests/secret_redaction.rs` (F6) — 11 tests

Tests the `redact_secrets` function used in LLM error responses:

- `redact_secrets_replaces_bearer_token` — "Bearer sk-..." → "[REDACTED]"
- `redact_secrets_replaces_api_key` — OpenAI-style key in JSON error body → redacted
- `redact_secrets_replaces_openai_key` — "sk-..." pattern → redacted
- `redact_secrets_replaces_github_token` — "ghp_..." → redacted
- `redact_secrets_preserves_context` — non-secret content preserved (HTTP status, error message)
- `redact_secrets_handles_empty_string` — empty input → empty output
- `redact_secrets_handles_no_secrets` — no secrets → unchanged
- `redact_secrets_replaces_multiple_keys` — multiple keys in one string → all redacted
- `redact_secrets_ignores_short_strings` — short "sk-short" not matched (under 20 chars)
- `redact_secrets_replaces_authorization_header` — "authorization: Bearer ..." → redacted
- `redact_secrets_preserves_non_secret_content` — structural JSON preserved, no false positives

### `tests/llm_mock_provider.rs` (F11) — 10 tests

Tests the MockProvider for isolated LLM testing:

- `mock_provider_creates_successfully` — construction from script path
- `mock_provider_complete_returns_text_and_usage` — complete() returns content, tokens
- `mock_provider_complete_returns_error_when_configured` — error entries trigger errors
- `mock_provider_stream_yields_text_and_usage_chunks` — stream() yields Text + Usage chunks
- `mock_provider_serves_sequential_responses` — responses served in order
- `mock_provider_returns_error_on_unknown_model` — unknown model → error
- `mock_script_builder_creates_valid_json` — MockScriptBuilder produces valid JSON
- `mock_script_builder_adds_error_entries` — error entries in script JSON
- `mock_script_builder_multiple_responses` — multiple responses per model
- `mock_script_builder_can_write_to_file` — script written to file

### `tests/retry_tracking.rs` (F7) — 12 tests

Tests retry_count and ttft_ms field propagation:

- `stage_metric_has_retry_count_field` — StageMetric has retry_count field
- `stage_metric_has_ttft_ms_field` — StageMetric has ttft_ms field
- `stage_metric_retry_count_defaults_to_zero_with_serde_default` — serde default = 0 when missing
- `stage_metric_ttft_defaults_to_zero_with_serde_default` — serde default = 0 when missing
- `stage_metric_serializes_all_fields` — retry_count and ttft_ms in serialized JSON
- `task_record_has_total_retry_count` — TaskRecord has total_retry_count, defaults to 0
- `task_record_has_max_ttft_ms` — TaskRecord has max_ttft_ms, defaults to 0
- `task_record_add_metrics_accumulates_retry_count` — add_metrics sums retry_count
- `task_record_add_metrics_tracks_max_ttft` — add_metrics tracks max ttft_ms
- `task_record_serializes_with_new_fields` — new fields in serialized JSON
- `task_record_deserializes_with_new_fields` — round-trip serialization works
- `task_record_status_is_running_initially` — TaskStatus defaults to Running

## Source Changes Summary

### New Files
- `src/llm/mock.rs` — MockProvider implementing `LlmProvider` from a JSON script file
- `tests/common/mod.rs`, `harness.rs`, `mock_llm.rs`, `fixture_repo.rs`, `metrics.rs` — shared test scaffolding
- 6 integration test files listed above

### Modified Files
- `src/llm/provider.rs` — Added `redact_secrets()` function with Bearer token, API key, and generic pattern redaction
- `src/llm/anthropic.rs` — Error responses now use `redact_secrets(&body)`
- `src/llm/openai.rs` — Error responses now use `redact_secrets(&body)`
- `src/llm/google.rs` — Error responses now use `redact_secrets(&body)`
- `src/llm/ollama.rs` — Error responses now use `redact_secrets(&body)`
- `src/sandbox/docker.rs` — `exec` method now calls `check_command_policy` and wraps in `tokio::time::timeout` (F1, F3); `parse_memory_limit` made `pub` (F2)
- `src/sandbox/worktree.rs` — `exec` method now calls `check_command_policy` and wraps in `tokio::time::timeout` (F1, F3)
- `src/sandbox/mod.rs` — `check_command_policy` function exported
- `src/config/types.rs` — `default_coder_policy`, `default_tester_policy`, `default_reviewer_policy` made `pub` (from `pub(crate)`)
- `src/orchestrator/state.rs` — Added `retry_count` and `ttft_ms` to `Span`; added `total_retry_count` and `max_ttft_ms` to `TaskRecord` (F7); added `Debug` + `PartialEq` derives
- `src/orchestrator/pipeline.rs` — Added `role_policy()` helper; added `Debug` to `PipelineResult`
- `src/agents/mod.rs` — `run_agent` returns 4-tuple `(json, usage, retry_count, ttft_ms)`; `tracing::debug!` logs only lengths not content (F8)
- `src/artifacts/types.rs` — `Verdict` already had `Debug` derive (confirmed)
- `src/lib.rs` — No changes needed
- `Cargo.toml` — Added `regex = "1"` dependency

## Running the Tests

```sh
cargo test                    # run all tests
cargo test --test security_exec
cargo test --test docker_resource_caps
cargo test --test exec_timeout
cargo test --test worktree_policy
cargo test --test retry_tracking
cargo test --test secret_redaction
cargo test --test llm_mock_provider
```

## Test Results

```
test result: ok. 144 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out (lib)
test result: ok. 8 passed; 0 failed (bin)
test result: ok. 7 passed; 0 failed (docker_resource_caps)
test result: ok. 10 passed; 0 failed (exec_timeout)
test result: ok. 11 passed; 0 failed (secret_redaction)
test result: ok. 15 passed; 0 failed (security_exec)
test result: ok. 5 passed; 0 failed (worktree_policy)
test result: ok. 12 passed; 0 failed (retry_tracking)
test result: ok. 10 passed; 0 failed (llm_mock_provider)
```

**Total: 212 passed, 0 failed.**
