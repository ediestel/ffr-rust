# FF Reader

**Classified, bounded, chunk-capable file reading for Neovim and AI agents.**

FF Reader is the reading counterpart to [FF Framework](https://github.com/eckhartd/fff.nvim):
FF finds paths, FF Reader reads them. Where FF focuses on fuzzy file *search*, FF Reader focuses on
*safely* loading files — classifying them first, refusing binaries, chunking large files,
and exposing a consistent surface to Neovim (via in-process FFI), AI agents (via MCP),
and any other language (via a C FFI + Bun/Node bindings).

> The file reading engine is original work. The search layer it pairs with is built on [fff.nvim](https://github.com/dmtrKovalenko/fff.nvim) by [Dmitriy Kovalenko](https://github.com/dmtrKovalenko).

---

## Why ffr

Loading an arbitrary file into a buffer (or into an LLM context window) is unsafe by
default. Binaries, minified bundles, and multi-gigabyte logs can freeze an editor or
blow through a token budget before anyone notices.

ffr fixes that with a deliberate pipeline:

```
path → stat → classify → policy → read → decode → render
```

Every read is bounded. Every file is inspected before being touched. The output is
either an editor buffer (with treesitter highlights, a chunk-nav keymap, and a
preview header) or a structured record for AI agents.

---

## Feature matrix

| Capability | Status | Notes |
|---|---|---|
| Classify (kind, encoding, binary, line endings, minification) | ✓ | `ffr_core::classify::classify_path` |
| Multi-encoding decode (UTF-8, UTF-16 LE/BE, windows-1252, latin1, …) | ✓ | Via `encoding_rs`; BOM sniff + configurable fallback order |
| Deterministic byte-aligned chunks | ✓ | `chunk_id` stable per `(size, mtime)` |
| Random-access line reads | ✓ | O(log n) after one-pass line index |
| LMDB persistent metadata cache | ✓ | `stdpath('data')/ffr/metadata-db/`; auto-migrates legacy JSON |
| LMDB persistent semantic chunk cache | ✓ | Second named DB keyed by path + revision |
| File watcher (notify-debouncer-full) | ✓ | Auto-invalidates line-index + metadata + semantic caches |
| Async chunk prefetch | ✓ | `std::thread` + `crossbeam-channel`; warm the OS page cache |
| Tree-sitter semantic chunking | ✓ | Lua-side via `vim.treesitter`; reuses installed parsers |
| Syntax-highlighted preview | ✓ | `vim.treesitter.start` on preview buffers |
| Preview header with kind / encoding / chunk pos | ✓ | Rendered as a virtual line (source text untouched) |
| Chunk-navigation keymaps (`]c`/`[c`) | ✓ | Buffer-local; configurable |
| Semantic chunk keymaps (`]f`/`[f`) | ✓ | Jump fn/class boundaries |
| Statusline helper | ✓ | `require('ffr').statusline()` |
| Specialized handlers | ✓ | PDF text · image metadata + EXIF · zip/tar/tar.gz listings |
| MCP server (rmcp, stdio) | ✓ | 11 tools (see below) |
| `ffr-mcp --healthcheck` | ✓ | Exits 0/1 with structured diagnostics |
| MCP update check (opt-in) | ✓ | `--no-update-check` to disable |
| C FFI (cdylib + staticlib) | ✓ | `crates/ffr-c` + `cbindgen.toml` |
| Bun / Node bindings | ✓ | `@ffr/bun`, `@ffr/node`; 8 prebuilt-binary packages |
| Criterion benches | ✓ | `make bench` |
| Profiler binaries | ✓ | `read_profiler`, `chunk_profiler` |
| Tracing to rotating log file | ✓ | Configurable level, default `stdpath('data')/ffr/ffr.log` |
| Health check (`:checkhealth ffr`) | ✓ | Backend, LMDB, watcher, classify roundtrip, MCP binary |
| mimalloc global allocator | ✓ | Feature-gated, default on |
| CI: rust / lua / release / panvimdoc / stylua | ✓ | `.github/workflows/` |
| Nix flake | ✓ | `nix build`, `nix develop` |

---

## Install

### Neovim (lazy.nvim)

```lua
{
  'eckhartd/ffr.nvim',
  build = 'make build',
  opts = {},                   -- defaults below
}
```

### Neovim (packer.nvim)

```lua
use {
  'eckhartd/ffr.nvim',
  run = 'make build',
  config = function() require('ffr').setup({}) end,
}
```

### Neovim (rocks.nvim / vim.pack)

```lua
vim.pack.add({
  { src = 'https://github.com/eckhartd/ffr.nvim' },
})
require('ffr').setup({})
```

### MCP server (Claude Desktop / agent clients)

```json
{
  "mcpServers": {
    "ffr": {
      "command": "ffr-mcp",
      "args": ["--log-level", "info"]
    }
  }
}
```

Verify with:

```sh
ffr-mcp --healthcheck
```

### Bun / Node

```sh
bun add @ffr/bun          # or: npm i @ffr/node
```

```js
import ffr from '@ffr/bun';

const info = ffr.classify('./large.log');
if (!info.binary && !info.too_large_for_full_open) {
  const { lines } = ffr.readLines('./large.log', 1, 500);
  console.log(lines.join('\n'));
}
```

### Nix

```sh
nix develop             # cargo + rust-toolchain + clang
nix build .#ffr-nvim    # build the Neovim plugin
```

---

## Commands

| Command                       | Purpose                                                                                              |
|-------------------------------|------------------------------------------------------------------------------------------------------|
| `:FFR {path}`                 | Open a file through the full pipeline. Chooses `full` / `chunked` / `preview` / `specialized` / `reject`. |
| `:FFRPreview {path}`          | Force preview mode (first chunk only).                                                               |
| `:FFRChunkNext`               | Next chunk in a chunked session.                                                                     |
| `:FFRChunkPrev`               | Previous chunk.                                                                                      |
| `:FFRChunkNextSemantic`       | Jump to the next fn/class boundary via tree-sitter.                                                  |
| `:FFRChunkPrevSemantic`       | Jump to the previous boundary.                                                                       |
| `:FFRReload`                  | Invalidate caches and re-open the current file.                                                      |
| `:FFRInfo`                    | Session info for the current buffer.                                                                 |
| `:FFRHealth`                  | Structured health report.                                                                            |
| `:FFRClearCache [all\|metadata\|content]` | Clear caches. Default `all`.                                                               |
| `:FFROpenLog`                 | Open the tracing log file.                                                                           |
| `:FFRWatchStatus`             | List watched paths + watcher state.                                                                  |
| `:FFRInvalidate {path}`       | Drop metadata/line/semantic entries for one path.                                                    |
| `:FFRDebug [on\|off\|toggle]` | Toggle in-buffer debug overlay.                                                                      |
| `:checkhealth ffr`            | Native Neovim health check.                                                                          |

Default keymaps inside chunked/preview buffers: `]c`/`[c` (byte chunks), `]f`/`[f` (semantic).

---

## Configuration reference

All fields are optional. Merged over defaults by `setup()`.

| Key | Default | Purpose |
|---|---|---|
| `chunk_bytes` | `65536` | Size of one byte chunk. |
| `full_open_max_bytes` | `2097152` | Max size for `full` mode. |
| `max_line_window` | `2000` | Max lines returned in one `read_lines` call. |
| `binary_sniff_bytes` | `4096` | Prefix scanned for binary detection. |
| `minified_line_length_threshold` | `1000` | Above this line length → minified. |
| `enable_persistent_metadata_cache` | `true` | Enable the LMDB cache. |
| `metadata_cache_path` | `stdpath('data')/ffr/metadata-db` | LMDB dir (or legacy `.json`, auto-migrated). |
| `logging.enabled` | `true` | Write tracing spans to a file. |
| `logging.level` | `"info"` | One of `trace` / `debug` / `info` / `warn` / `error`. |
| `logging.file` | auto | Defaults to `stdpath('data')/ffr/ffr.log`. |
| `logging.max_files` | `3` | Soft hint; current implementation truncates on open. |
| `watcher.enabled` | `true` | Spawn the per-file fs watcher. |
| `watcher.debounce_ms` | `250` | `notify-debouncer-full` debounce. |
| `encodings.fallback_order` | `utf-8, utf-16le, utf-16be, windows-1252, latin1` | Tried after BOM/heuristic fails. |
| `preview.syntax_highlight` | `true` | `vim.treesitter.start` in preview buffers. |
| `preview.line_numbers` | `true` | Enable `number` in preview windows. |
| `preview.max_preview_chunks` | `4` | Ceiling for preview-mode chunk loading. |
| `chunk.prefetch_count` | `1` | Chunks prefetched ahead of the active one. |
| `chunk.keymaps.enabled` | `true` | Auto-bind `]c`/`[c` in chunked buffers. |
| `chunk.keymaps.next` / `.prev` | `"]c"` / `"[c"` | Keymap bindings. |
| `specialized.pdf.enabled` | `true` | PDF text extraction. |
| `specialized.pdf.max_pages` | `50` | Cap (advisory). |
| `specialized.image.enabled` | `true` | Image metadata + EXIF. |
| `specialized.image.show_metadata` | `true` | Include EXIF in output. |
| `specialized.archive.enabled` | `true` | Zip/tar entry listing. |
| `specialized.archive.max_entries` | `500` | Cap on entries returned. |
| `accessibility.enabled` | `false` | Enable a11y announcements. |
| `accessibility.announce_chunks` | `false` | `vim.notify` on chunk change. |
| `highlights.chunk_boundary` / `preview_header` / `rejected_banner` / `binary_hex` | `nil` | Override default highlight links. |
| `hooks.on_classify` / `on_open` / `on_chunk_load` | `nil` | User callbacks. |

### Deprecated flat keys

These are still accepted but emit a one-time `vim.notify` warning and are migrated
to the new schema:

- `watcher_enabled` → `watcher.enabled`
- `chunk_keymaps_enabled` → `chunk.keymaps.enabled`
- `log_level` → `logging.level`

---

## MCP tools

11 tools exposed over stdio. All tools return text; structured data is JSON-embedded.

| Tool | Inputs | Output |
|---|---|---|
| `stat_file` | `path` | JSON: `exists`, `is_file`, `size`, `mtime`, `readonly` |
| `classify_file` | `path` | JSON: `kind`, `encoding`, `binary`, `too_large_for_full_open`, `likely_filetype`, … |
| `read_file` | `path`, `startLine?`, `endLine?` | Lines `[start, end]` with line numbers; default `[1, 2000]` |
| `read_chunk` | `path`, `chunkId` | One byte-aligned chunk; header shows byte + line range; deterministic per file revision |
| `read_range_around_line` | `path`, `line`, `radius?` | Lines `[line-radius, line+radius]` with cursor marker; radius default `30` |
| `search_in_file` | `path`, `pattern`, `maxResults?`, `cursor?` | Streaming literal or regex search via `grep-searcher`; paginated |
| `outline` | `path`, `maxEntries?` | Heuristic top-level decl list (fn/struct/class/trait/type) per extension |
| `list_archive` | `path`, `maxEntries?` | Zip/tar entry listing |
| `extract_pdf_text` | `path` | PDF plain text via `pdf-extract` |
| `diff_files` | `left`, `right` | Line-level diff |
| (implicit) | — | Update notice injected into server instructions (opt-out) |

`ffr-mcp --healthcheck` runs LMDB open + classify-self + tool-count checks and exits.

### Recommended workflow (from `MCP_INSTRUCTIONS`)

1. Always `classify_file` before reading an unknown path.
2. Small text → `read_file`. Large text → `read_chunk` with `chunkId=0` and increment.
3. PDF → `extract_pdf_text`. Archive → `list_archive`. Binary → stop.
4. For "show me the function at line N" → `read_range_around_line` or `outline` followed by `read_range_around_line`.
5. For literal / regex search in one file → `search_in_file` (streaming; paginate via cursor).

---

## Architecture

Four Rust crates + one Lua layer + two JS packages:

```
                ┌───────────────────── Neovim Plugin Layer (lua/ffr/) ─────────────────────┐
                │  init · client · config · config_validation · policy · cache · buffer   │
                │  session · render · commands · health · highlights · a11y · keymaps     │
                │  statusline · semantic · download · lifecycle · handlers/{pdf,image,archive}│
                └──────────┬──────────────────────────────────┬────────────────────────────┘
                           │                                  │
                      ┌────▼─────────┐                ┌───────▼─────────┐      ┌───────────────────┐
                      │ ffr-nvim     │                │ ffr-mcp (stdio) │      │ ffr-c (cdylib)    │
                      │ CDylib/mlua  │                │ rmcp server     │      │ + @ffr/bun        │
                      │              │                │ + 11 tools      │      │ + @ffr/node       │
                      └────┬─────────┘                └───────┬─────────┘      │ + 8 ffr-bin-*     │
                           │                                  │                └────────┬──────────┘
                           │                                  │                         │
                      ┌────▼──────────────────────────────────▼─────────────────────────▼──────┐
                      │                           ffr-core (shared lib)                        │
                      │   stat · classify · decode (encoding_rs) · read · lines · cache        │
                      │   shared · db (LMDB: metadata + semantic) · log (tracing) · watcher    │
                      │   prefetch (crossbeam) · specialized/{pdf,image,archive} · types       │
                      └─────────────────────────────────────────────────────────────────────────┘
```

### Buffer modes

| Mode | When | Behavior |
|---|---|---|
| `full` | small text (`size ≤ full_open_max_bytes`) | Entire file loaded. |
| `chunked` | large text | One chunk visible; `]c`/`[c` paginate. |
| `preview` | `preview=true`, minified, or `initial_mode=preview` | First chunk, navigation limited. |
| `specialized:pdf` | `.pdf` | Extracted text + metadata header. |
| `specialized:image` | image extension | Dimensions + format + EXIF. |
| `specialized:archive` | `.zip` / `.tar` / `.tar.gz` / `.tgz` | Entry listing. |
| `reject` | binary / unsafe / too-large-no-preview | Rejection banner with reason. |

---

## Benchmarks

```sh
make bench                              # criterion: classify, read, line_index
cargo run --release -p ffr-nvim --bin read_profiler -- /path/to/file
cargo run --release -p ffr-nvim --bin chunk_profiler -- /path/to/file
```

HTML reports land in `target/criterion/`.

---

## Build

```sh
make build                  # release: ffr-nvim + ffr-mcp
make build-debug            # development build
make test                   # cargo test --workspace + Lua spec
make bench                  # criterion
make check                  # fmt + clippy
```

---

## Release

```sh
./scripts/release.sh 0.2.0
```

`scripts/release.sh` bumps `Cargo.toml`, tags `v0.2.0`, and pushes. CI
(`.github/workflows/release.yml`) builds the 8-platform binary matrix, uploads
release artifacts, and fires off the panvimdoc job to regenerate `doc/ffr.txt`
from the README.

---

## Troubleshooting

- **`ffr-nvim` not found**: run `make build` in the plugin repo, or set `FFR_LIB_PATH`.
- **`:FFRHealth` shows "metadata store not initialized"**: setup hasn't called `client.start()` yet. Open a file with `:FFR` to trigger it, or set `enable_persistent_metadata_cache = false`.
- **Watcher not catching changes**: some filesystems (network mounts, containers with inotify limits) drop `notify` events. Run `:FFRInvalidate` manually on affected paths, or raise the debounce.
- **PDF extraction returns empty**: the PDF is image-only or encrypted. Nothing ffr can do — this is a limitation of `pdf-extract`.
- **`bun install @ffr/bun` fails at runtime**: the prebuilt binary package isn't published yet. Use `FFR_LIB_PATH=$(pwd)/target/release/libffr_c.dylib` to point at a local build.

---

## Contributing

See `doc/ffr.txt` for the full Lua API. For Rust work:

```sh
cargo fmt --all
cargo clippy --workspace
cargo test --workspace --lib
```

For Lua:

```sh
stylua lua/ tests/
luacheck lua/
```

Run `make check` before pushing. CI enforces fmt + clippy + spec.

---

## License

MIT.
