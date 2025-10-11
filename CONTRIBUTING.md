# Contributing to Shelldone

Welcome! Shelldone is a community-driven fork of WezTerm and we want
new contributors to feel productive immediately. This guide explains the
expectations, workflows, and tooling that keep the project fast and reliable.

## Code of Conduct

We follow the [Contributor Covenant](https://www.contributor-covenant.org/).
Be respectful, assume good intent, and report issues privately to
`team@shelldone.dev`.

## Quick Start Checklist

| Step | Command | Purpose |
| --- | --- | --- |
| 1 | `git clone git@github.com:imagray/Shelldone.git` | local working copy |
| 2 | `cd Shelldone && make dev` | compile + launch GUI for smoke testing |
| 3 | `make verify` | fmt + clippy + tests + nextest |
| 4 | `make servedocs` | preview docs changes |

Additional scripts live in `ci/` and `get-deps/` for packaging flows.

> Python tooling: используйте локальное окружение `./.venv` (или системный Python). Запуск проверок осуществляется через `make verify`/`scripts/verify.sh`.

## Workflow

1. **Issue first.** Start from an existing issue or create one with the proper
   template. Label it (`type/*`, `area/*`, `prio/*`).
2. **Branch naming.** Use `feature/<topic>`, `fix/<bug-id>`, or `docs/<section>`.
3. **Commits.** Keep commits scoped and message in imperative tone
   (`Add GPU telemetry collector`). Include issue ID when applicable.
4. **PR Checklist.**
   - `make verify` passes locally.
   - Docs updated when behavior changes.
   - Screenshots/recordings for UX changes.
   - Performance data when touching hot paths.
   - Fill out the PR template (created automatically).

We squash-merge by default; keep your branch linear (rebase vs merge).

## Code Style & Testing

- Rust edition 2021, `cargo fmt` (nightly) for formatting.
- `cargo clippy --workspace --all-targets -- -D warnings` no warnings allowed.
- `cargo nextest run` is our default test runner.
- Add tests for bug fixes or new features; describe edge cases in comments.
- Benchmarks belong under `tests/perf` and should guard against regressions.

## Documentation

Updating behavior? Update docs:
- User Guide (`docs/`)
- API docs (`docs/config/...`)
- Roadmap entries (`docs/ROADMAP/`)

The documentation site is powered by MkDocs: `make servedocs` reloads on save.

## Release & Distribution

`make ship` proxies to `make verify`. Release automation lives in
`ci/` and is run by maintainers. Contributors only need to ensure `make verify` passes
and changelog entries are accurate.

## Communication

- GitHub Discussions (`Ideas`, `Help`, `Show & Tell`).
- Matrix: `#shelldone:matrix.org` for live chat.
- `team@shelldone.dev` for security or sensitive topics.

We run regular roadmap syncs documented in `docs/ROADMAP`. Feel free to join! :)

Happy hacking!
