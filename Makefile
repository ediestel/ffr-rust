.PHONY: build build-debug test test-rust test-lua bench profile format lint check clean

build:
	cargo build --release -p ffr-nvim -p ffr-mcp

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
