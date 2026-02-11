# Agent Merge Playbook

## Commit Strategy

- Use small conventional commits with one concern per commit.
- Prefer file-level isolation for concurrent work.
- If a file is shared with another active agent, skip it for your commit.

## Conflict-Minimizing Tactics

- Add new files instead of editing shared files when possible.
- Put experiments/tests in dedicated `tests/` files rather than existing suites.
- Keep docs updates in new packet files during heavy development.

## Regression Coverage Expectations

For each behavior change:

1. Add at least one targeted regression test.
2. Keep fixtures deterministic and local.
3. Test negative/edge cases (invalid payloads, reconnect paths, empty fields).

## Review Checklist

- Does this commit touch only intended files?
- Does the commit message match the actual change?
- Are tests/docs updated in separate commits when practical?
- Is this safe to cherry-pick into integration branches?
