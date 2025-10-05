PKG_CONFIG_PATH ?= /usr/lib/x86_64-linux-gnu/pkgconfig
.PHONY: all fmt build check test docs servedocs dev shelldone verify verify-fast verify-prepush verify-full verify-ci agents-smoke run-agentd perf-utif roadmap status roadmap-status ship clippy lint test-e2e test-e2e-verbose perf-policy perf-baseline perf-ci ci

all: build

test:
	cargo nextest run
	cargo nextest run -p shelldone-escape-parser # no_std by default

check:
	cargo check
	cargo check -p shelldone-escape-parser
	cargo check -p shelldone-cell
	cargo check -p shelldone-surface
	cargo check -p shelldone-ssh

build:
	cargo build $(BUILD_OPTS) -p shelldone
	cargo build $(BUILD_OPTS) -p shelldone-gui
	cargo build $(BUILD_OPTS) -p shelldone-mux-server
	cargo build $(BUILD_OPTS) -p strip-ansi-escapes

fmt:
	cargo +nightly fmt

clippy lint:
	cargo clippy --workspace --all-targets

verify:
	VERIFY_MODE=$(VERIFY_MODE) JSON=$(JSON) CHANGED_ONLY=$(CHANGED_ONLY) TIMEOUT_MIN=$(TIMEOUT_MIN) NET=$(NET) scripts/verify.sh

verify-fast:
	VERIFY_MODE=fast scripts/verify.sh

verify-prepush:
	VERIFY_MODE=prepush scripts/verify.sh

verify-full:
	VERIFY_MODE=full scripts/verify.sh

verify-ci:
	VERIFY_MODE=ci JSON=1 scripts/verify.sh

roadmap-status:
	scripts/roadmap_status.py $(if $(filter 1,$(JSON)),--json,)$(if $(filter 0,$(STRICT)),, --strict)

roadmap: roadmap-status

status: roadmap
	python3 scripts/status.py

ship: verify

dev:
	cargo run --bin shelldone-gui

shelldone:
	cargo run --release --bin shelldone-gui

docs:
	ci/build-docs.sh

servedocs:
	ci/build-docs.sh serve

agents-smoke:
	python3 scripts/agentd.py smoke

run-agentd:
	cargo run -p shelldone-agentd -- --state-dir state

perf-utif:
	python3 -m perf_runner run --probe utif_exec

# E2E and performance testing targets
test-e2e:
	cargo test -p shelldone-agentd --test e2e_ack

test-e2e-verbose:
	cargo test -p shelldone-agentd --test e2e_ack -- --nocapture

perf-policy:
	python3 -m perf_runner run --probe policy_perf

perf-baseline:
	python3 -m perf_runner run

perf-ci:
	SHELLDONE_PERF_PROFILE=ci python3 -m perf_runner run


ci: verify-ci test-e2e perf-ci
	@echo "Full CI pipeline complete"
