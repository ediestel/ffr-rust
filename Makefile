.PHONY: build build-debug install test test-rust test-lua bench profile format lint check clean

BIN_DIR ?= $(HOME)/.local/bin

build:
	cargo build --release -p ffr-nvim -p ffr-mcp

install: build
	mkdir -p $(BIN_DIR)
	ln -sf $(CURDIR)/target/release/ffr-mcp $(BIN_DIR)/ffr-mcp
	@echo "installed: $(BIN_DIR)/ffr-mcp"
	@command -v ff-doctor >/dev/null 2>&1 && ff-doctor --quiet || true

build-debug:
	cargo build -p ffr-nvim -p ffr-mcp

test: test-rust test-lua

test-rust:
	cargo test --workspace

test-lua:
	nvim --headless -u tests/lua/minimal_init.lua -c "luafile tests/lua/ffr_spec.lua" -c "qa!"

bench:
	cargo bench -p ffr-core

profile:
	@echo "usage: cargo run --release -p ffr-nvim --bin read_profiler -- <path>"
	@echo "       cargo run --release -p ffr-nvim --bin chunk_profiler -- <path>"

format:
	cargo fmt --all

lint:
	cargo clippy --workspace -- -W warnings

check: format lint

clean:
	cargo clean
