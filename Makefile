.PHONY: all fmt build check test docs servedocs

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

docs:
	ci/build-docs.sh

servedocs:
	ci/build-docs.sh serve
