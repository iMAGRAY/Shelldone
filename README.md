# Shelldone Terminal

<img height="128" alt="Shelldone Icon" src="https://raw.githubusercontent.com/shelldone/shelldone/main/assets/icon/shelldone-icon.svg" align="left"> *A GPU-accelerated cross-platform terminal emulator and multiplexer maintained by <a href="https://github.com/shelldone">@shelldone</a> (forked from <a href="https://github.com/wez">@wez</a>) and implemented in <a href="https://www.rust-lang.org/">Rust</a>*

User facing docs and guide at: https://shelldone.org/

![Screenshot](docs/screenshots/two.png)

*Screenshot of shelldone on macOS, running vim*

## Installation

https://shelldone.org/installation

## Getting Help

* Bug or feature? Open an issue: https://github.com/imagray/Shelldone/issues
* Questions, show & tell, or proposals: https://github.com/imagray/Shelldone/discussions
* Real-time chat: Matrix room [#shelldone:matrix.org](https://matrix.to/#/#shelldone:matrix.org)
* Security: [team@shelldone.dev](mailto:team@shelldone.dev)

## Supporting the Project

If you use and like Shelldone, please consider sponsoring it: your support helps
to cover the fees required to maintain the project and to validate the time
spent working on it!

[Read more about sponsoring](https://shelldone.org/sponsor.html).

* [![Sponsor Shelldone](https://img.shields.io/github/sponsors/shelldone?label=Sponsor%20Shelldone&logo=github&style=for-the-badge)](https://github.com/sponsors/shelldone)
* [Patreon](https://patreon.com/shelldone)
* [Ko-Fi](https://ko-fi.com/shelldone)
* [Liberapay](https://liberapay.com/shelldone)

## Community & Support

- [GitHub Discussions](https://github.com/imagray/Shelldone/discussions) — questions, show & tell, proposals.
- Matrix: [#shelldone:matrix.org](https://matrix.to/#/#shelldone:matrix.org) — low-latency chat.
- Security incidents: [team@shelldone.dev](mailto:team@shelldone.dev).
- Roadmap snapshots live in [`docs/ROADMAP`](docs/ROADMAP).

## Contributing

Want to help? Start with [`CONTRIBUTING.md`](CONTRIBUTING.md) for tooling,
workflow and review expectations. TL;DR:

```bash
make dev      # launch a dev shell
make verify   # orchestrated QA (fmt + clippy baseline + tests + nextest)
```

- Contributor resources: [`docs/community/contributor-handbook.md`](docs/community/contributor-handbook.md)
Bug reports and feature ideas belong in [GitHub Issues](https://github.com/imagray/Shelldone/issues).

### Quality pipeline

`make verify` is an orchestrated front-end for `scripts/verify.sh` and supports
four modes via the `VERIFY_MODE=fast|prepush|full|ci` environment variable.
Each run renders a summary table and (with `JSON=1`) a machine-readable report
under `artifacts/verify/summary.json`.

The QA pipeline enforces two baselines stored in `qa/baselines/`:

- `banned_markers.json` — snapshot of allowed `TODO|FIXME|XXX|???` usages.
  Update it with `python3 scripts/verify.py --update-marker-baseline` after
  intentionally removing or reorganising legacy markers.
- `clippy.json` — snapshot of current Rust lint warnings captured in
  `--message-format=json`. Update it with
  `python3 scripts/verify.py --update-clippy-baseline` only after eliminating
  warnings or when a refactor naturally reorders spans.

Both baselines are diffed against the live code; new warnings fail the run,
while resolved warnings ask you to refresh the baseline. See
[`docs/community/contributor-handbook.md`](docs/community/contributor-handbook.md)
for day-to-day workflows and remediation tips.

The project treats Rust warnings as hard failures. When iterating locally run
`cargo clippy --workspace --all-targets -- -D warnings` before pushing large
changes to ensure the baseline remains empty.

To assess roadmap progress run `make roadmap status` — the command reads
`todo.machine.md`, prints the true completion percentage, and exits with an
error if declared and computed values diverge. Use `STRICT=0` for a non-fatal
mode and `JSON=1` for automation.

### Architecture & Operations
- State and backups: `docs/architecture/state-and-storage.md`
- Security and secrets: `docs/architecture/security-and-secrets.md`
- Observability: `docs/architecture/observability.md`
- Releases and compatibility: `docs/architecture/release-and-compatibility.md`
