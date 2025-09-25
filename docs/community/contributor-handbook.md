# Shelldone Contributor Handbook

This handbook complements [`CONTRIBUTING.md`](../../CONTRIBUTING.md) with deeper
context on tooling, conventions, and quality expectations.

## Environment Setup

```bash
git clone git@github.com:imagray/Shelldone.git
cd Shelldone
./get-deps         # install native dependencies (optional but recommended)
make verify        # ensure tooling works locally
```

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
1. Create / pick up an issue and assign yourself.
2. Branch from `main` using `feature/<topic>` etc.
3. Develop with `make dev` + `RUST_LOG=debug` as needed.
4. Keep commits focused; rebase before opening PR.
5. Run `make verify` before every push.

## Testing Matrix
| Area | Command | Notes |
| --- | --- | --- |
| Type-check | `cargo check` | fast sanity |
| Unit & integration | `make verify` (includes `cargo test` + `nextest`) | required |
| Lua plugins | `cargo test -p luahelper` | when touching plugins |
| Performance | `cargo bench` or scripts in `tests/perf` | attach results |
| Docs | `make servedocs` | screenshot diff for UX updates |

## Coding Standards
- Prefer small, composable modules. Avoid panics in library code.
- Log with `tracing` (structured fields) at appropriate level.
- Validate external inputs; no unsafe path handling.
- For async code, favor `smol` primitives used in repo.
- Document public functions (rustdoc) and complex code paths with comments.

## Reviews & Merging
- Every PR requires review from at least one maintainer (CODEOWNERS auto-request).
- CI must be green; reviewers may request extra profiling or tests.
- We use squash merge; include final changelog entry if user-facing.

## Communication
- Daily chat: Matrix `#shelldone:matrix.org`
- Weekly sync agenda: `docs/ROADMAP/meetings.md` (TBD)
- Security: [team@shelldone.dev](mailto:team@shelldone.dev)

## Useful Commands
```bash
make dev                # start GUI build quickly
RUST_LOG=debug make dev # verbose logging
make verify             # fmt + clippy + tests + nextest
cargo clippy --fix      # apply lint suggestions (review diff!)
```

> Have ideas to improve the handbook? Open a docs issue or PR!
