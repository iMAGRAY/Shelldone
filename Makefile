PKG_CONFIG_PATH ?= /usr/lib/x86_64-linux-gnu/pkgconfig
.PHONY: all fmt build check test docs servedocs dev verify verify-fast verify-prepush verify-full verify-ci roadmap status roadmap-status ship clippy lint

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

ship: verify

dev:
	cargo run --bin shelldone-gui

docs:
	ci/build-docs.sh

servedocs:
	ci/build-docs.sh serve
