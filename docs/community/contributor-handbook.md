# Shelldone Contributor Handbook

This handbook complements [`CONTRIBUTING.md`](../../CONTRIBUTING.md) with deeper
context on tooling, conventions, and quality expectations.

## Environment Setup

```bash
git clone git@github.com:imagray/Shelldone.git
cd Shelldone
./get-deps         # install native dependencies (optional but recommended)
make verify        # run full orchestrated QA pipeline
```

Additional architecture details live in `docs/architecture/` — prioritise
`state-and-storage.md`, `security-and-secrets.md`, `observability.md`, and
`release-and-compatibility.md`.

### Toolchain Versions
- Rust stable (updated monthly) + nightly for `cargo fmt`
- `cargo nextest` for test execution
- `clang`/`lld` recommended on macOS for faster builds
- Optional: `just`, `direnv`, `nix` (flakes coming soon)

## Repository Layout
- `shelldone/` — CLI + multiplexer entrypoint
- `shelldone-gui/` — windowing front-end
- `term/` — core terminal model
- `mux/` — multiplexer backend
- `docs/` — user & contributor docs (MkDocs)
- `ci/` — pipelines, packaging, release tooling

## Development Workflow
1. Pick or create an issue and assign yourself.
2. Branch from `main` (e.g. `feature/<topic>`).
3. Develop with `make dev` and `RUST_LOG=debug` when needed.
4. Keep commits focused; rebase before opening a PR.
5. Run `make verify` (or `make verify-prepush`) before every push.

### Verification Modes & Baselines

`make verify` proxies `scripts/verify.sh` and supports
`VERIFY_MODE=fast|prepush|full|ci` (default `prepush`). Each run:

- Validates docs (links, `todo.machine.md`, roadmap tables).
- Executes the Rust pipeline (`fmt`, `clippy`, `cargo test`, `cargo nextest`, `cargo doc` in full/ci).
- Scans for forbidden markers via `qa/baselines/banned_markers.json`.
- Writes a summary to `artifacts/verify/summary.json` (add `JSON=1` for machine output).

Technical debt is captured in the baselines:

| File | Purpose | Update command |
| --- | --- | --- |
| `qa/baselines/banned_markers.json` | Remaining `TODO|FIXME|XXX|???` occurrences | `python3 scripts/verify.py --update-marker-baseline` |
| `qa/baselines/clippy.json` | Current Rust/Clippy warnings | `python3 scripts/verify.py --update-clippy-baseline` |

Update a baseline **only** after fixing warnings or deliberately accepting them.
Any new warning in the pipeline results in an immediate failure with explicit
diffs.

Use `make roadmap status` to compute program/epic progress from
`todo.machine.md`; the command fails if declared progress differs by more than
0.5 percentage points (`JSON=1` and `STRICT=0` are available).

## Testing Matrix
| Area | Command | Notes |
| --- | --- | --- |
| Type-check | `cargo check` | Fast sanity check |
| Unit & integration | `make verify` (runs `cargo test` + `nextest`) | Required |
| Lua plugins | `cargo test -p luahelper` | When touching plugins |
| Performance | `cargo bench` or scripts in `tests/perf` | Attach results |
| Docs | `make servedocs` | Include screenshots for UX updates |

## Coding Standards
- Prefer small, composable modules; avoid panics in library code.
- Use `tracing` with structured fields at appropriate levels.
- Validate external inputs; never rely on unsafe path handling.
- For async work, favour the `smol` primitives already used in the repo.
- Document public functions (rustdoc) and non-trivial code paths.

## Reviews & Merging
- Every PR requires at least one maintainer review (managed via CODEOWNERS).
- CI must be green; reviewers may request additional profiling or tests.
- We use squash merge; add a changelog entry for user-facing changes.

## Communication
- Daily chat: Matrix `#shelldone:matrix.org`
- Weekly sync agenda: `docs/ROADMAP/meetings.md` (TBD)
- Security contact: [team@shelldone.dev](mailto:team@shelldone.dev)

## Useful Commands
```bash
make dev                # start the GUI quickly
RUST_LOG=debug make dev # verbose logging
make verify             # fmt + clippy + tests + nextest
cargo clippy --fix      # apply lint suggestions (review the diff!)
```

> Have ideas to improve the handbook? Open a docs issue or PR!
