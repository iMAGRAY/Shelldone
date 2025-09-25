PKG_CONFIG_PATH ?= /usr/lib/x86_64-linux-gnu/pkgconfig
.PHONY: all fmt build check test docs servedocs dev verify ship clippy lint

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

verify: fmt
	PKG_CONFIG_PATH=$(PKG_CONFIG_PATH) cargo test --workspace
	PKG_CONFIG_PATH=$(PKG_CONFIG_PATH) cargo nextest run
	PKG_CONFIG_PATH=$(PKG_CONFIG_PATH) cargo nextest run -p shelldone-escape-parser

ship: verify

dev:
	cargo run --bin shelldone-gui

docs:
	ci/build-docs.sh

servedocs:
	ci/build-docs.sh serve
