---
name: Build Problem
about: Failing to build Shelldone from source.
title: ""
labels: [build, needs:triage]
assignees: ""
---

## Environment
- OS / distribution (`lsb_release -a`, `uname -a`)
- Compiler / toolchain (clang/gcc/MSVC + versions)
- `rustup show`
- How Shelldone was cloned (git commit/branch, shallow/full)

## Steps
Explain exactly what command you ran (e.g. `make verify`, `cargo build --release`).

## Output
Paste the full build log (or attach as a file) with `RUST_BACKTRACE=1` enabled if available.

## Dependency Check
- Did you run `./get-deps`? Outcome?
- Did you run `git submodule update --init --recursive`?
- Have you tried cleaning with `cargo clean` or removing `target/`?

## Additional context
Anything else that might help (custom env vars, container, CI provider).
