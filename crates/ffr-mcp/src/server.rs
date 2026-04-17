use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};

use ffr_core::{classify, lines, read, specialized, stat};

use crate::cursor::Cursor;
use crate::log::log_call;
use crate::update_check::get_update_notice;

pub const MCP_INSTRUCTIONS: &str = r#"ffr is a classified, bounded, chunk-capable file reading engine.
Use these tools when you need to *read* a file (vs. searching for one — see fff).

# Workflow

1. ALWAYS call `classify_file` before reading an unknown file.
   It tells you whether the file is text/binary/pdf/image/archive, the encoding,
   an estimated line count, and whether it's too large for a single open.

2. Based on the classification, pick the right tool:

   * text, small (`too_large_for_full_open = false`): use `read_file` (returns lines 1-2000).
   * text, large: use `read_chunk` with `chunkId=0`, then increment `chunkId` to paginate.
   * pdf: use `extract_pdf_text` (returns extracted plain text) or `classify_file` for a summary.
   * image: use `classify_file` — there is no image pixel content reading tool.
   * archive (zip/tar/tar.gz): use `list_archive` to enumerate entries.
   * unknown / binary: DO NOT read. classify_file already told you it's unsafe.

3. For targeted lookups inside a single file, prefer `search_in_file` (pattern match,
   returns line numbers + snippets) or `read_range_around_line` (returns N lines
   centered on a target line, good for "show me the function at line 420" queries).

4. For side-by-side comparison, use `diff_files`.

# Tool reference

- stat_file(path)                            → exists, size, mtime, readonly
- classify_file(path)                        → kind, encoding, binary, estimated_lines, too_large_for_full_open, likely_filetype
- read_file(path, startLine?, endLine?)      → lines within [start, end]; default [1, 2000]
- read_chunk(path, chunkId)                  → deterministic byte-aligned chunk; stable per size+mtime
- read_range_around_line(path, line, radius?) → lines [line-radius, line+radius]; radius defaults to 30
- search_in_file(path, pattern, maxResults?) → line_number + snippet per match
- outline(path, maxEntries?)                 → top-level fn/class/struct/trait/type list (tree-sitter with regex fallback; LMDB-cached by revision)
- list_archive(path, maxEntries?)            → names+sizes for zip/tar/tar.gz entries
- extract_pdf_text(path)                     → plain-text extraction from .pdf files
- diff_files(left, right)                    → unified diff (line-level)

# Best practices

* Classify before reading. Binary detection is not guessable from extension alone.
* For iterative exploration of a large file, use `read_chunk` with stable pagination
  rather than growing `read_file` ranges. The chunk ID space is stable for the file
  revision so you can cache results client-side.
* `search_in_file` auto-detects regex vs literal: if the pattern contains regex
  metacharacters (e.g. `.` `*` `+` `?` `(` `[` `\`) it is compiled as a regex;
  otherwise the pattern is escaped and matched literally. Case-smart matching
  is always on. Invalid regex patterns return an error rather than silently
  falling back to literal.
"#;

/// Number of MCP tools exposed — used by the `--healthcheck` diagnostic.
/// Keep in sync with the `#[tool(...)]` decorations below.
pub fn tool_catalog_size() -> usize {
    11
}

// ---------------------------------------------------------------------------
// Parameter structs
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct StatFileParams {
    pub path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ClassifyFileParams {
    pub path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadFileParams {
    pub path: String,
    #[serde(rename = "startLine")]
    pub start_line: Option<usize>,
    #[serde(rename = "endLine")]
    pub end_line: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadChunkParams {
    pub path: String,
    #[serde(rename = "chunkId")]
    pub chunk_id: u64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadRangeAroundLineParams {
    pub path: String,
    pub line: usize,
    pub radius: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchInFileParams {
    pub path: String,
    pub pattern: String,
    #[serde(rename = "maxResults")]
    pub max_results: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListArchiveParams {
    pub path: String,
    #[serde(rename = "maxEntries")]
    pub max_entries: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExtractPdfTextParams {
    pub path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DiffFilesParams {
    pub left: String,
    pub right: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OutlineParams {
    pub path: String,
    #[serde(rename = "maxEntries")]
    pub max_entries: Option<usize>,
}

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

pub struct FfrServer {
    tool_router: ToolRouter<Self>,
    pub chunk_bytes: usize,
    pub sniff_bytes: usize,
    pub full_open_max_bytes: u64,
    pub minified_line_length_threshold: usize,
}

impl FfrServer {
    pub fn new(
        chunk_bytes: usize,
        sniff_bytes: usize,
        full_open_max_bytes: u64,
        minified_line_length_threshold: usize,
    ) -> Self {
        let mut server = Self {
            tool_router: ToolRouter::new(),
            chunk_bytes,
            sniff_bytes,
            full_open_max_bytes,
            minified_line_length_threshold,
        };
        server.tool_router = Self::tool_router();
        server
    }
}

// ---------------------------------------------------------------------------
// MCP tool implementations
// ---------------------------------------------------------------------------

#[tool_router]
impl FfrServer {
    #[tool(
        name = "stat_file",
        description = "Get file metadata: existence, size, mtime, readonly status."
    )]
    fn stat_file(
        &self,
        Parameters(params): Parameters<StatFileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = stat::stat_path(&params.path).map_err(|e| {
            log_call("stat_file", &params.path, 0, &format!("error:{}", e.code()));
            ErrorData::internal_error(e.to_string(), None)
        })?;
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        log_call("stat_file", &params.path, text.len(), "ok");
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "classify_file",
        description = "Classify a file: kind (text/binary/image/pdf/archive/json/minified), encoding, line endings, and whether it's safe for full open. ALWAYS call this before reading an unknown file."
    )]
    fn classify_file(
        &self,
        Parameters(params): Parameters<ClassifyFileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = classify::classify_path(
            &params.path,
            self.sniff_bytes,
            self.full_open_max_bytes,
            self.minified_line_length_threshold,
        )
        .map_err(|e| {
            log_call("classify_file", &params.path, 0, &format!("error:{}", e.code()));
            ErrorData::internal_error(e.to_string(), None)
        })?;

        let outcome = if result.binary {
            "rejected:binary"
        } else if result.kind == "minified" {
            "rejected:minified"
        } else if result.too_large_for_full_open {
            "large"
        } else {
            "ok"
        };

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        log_call("classify_file", &params.path, text.len(), outcome);
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "read_file",
        description = "Read lines from a text file. Builds a line index for fast random access on large files. Default range: 1-2000."
    )]
    fn read_file(
        &self,
        Parameters(params): Parameters<ReadFileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let start = params.start_line.unwrap_or(1);
        let end = params.end_line.unwrap_or(2000);
        let result = lines::read_lines(&params.path, start, end).map_err(|e| {
            log_call("read_file", &params.path, 0, &format!("error:{}", e.code()));
            ErrorData::internal_error(e.to_string(), None)
        })?;

        let mut output = String::new();
        for (i, line) in result.lines.iter().enumerate() {
            let line_num = result.start_line + i;
            output.push_str(&format!("{line_num:>6}\t{line}\n"));
        }
        if result.eof {
            output.push_str("[EOF]\n");
        } else {
            output.push_str(&format!(
                "[...truncated at line {}. Use startLine/endLine to read more.]\n",
                result.actual_end_line
            ));
        }

        log_call(
            "read_file",
            &params.path,
            output.len(),
            &format!("ok:lines={}", result.lines.len()),
        );
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        name = "read_chunk",
        description = "Read a deterministic byte-aligned chunk. Stable per file revision (size+mtime). chunkId=0 for the first chunk, increment to paginate."
    )]
    fn read_chunk(
        &self,
        Parameters(params): Parameters<ReadChunkParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = read::read_chunk(&params.path, params.chunk_id, self.chunk_bytes).map_err(
            |e| {
                log_call(
                    "read_chunk",
                    &params.path,
                    0,
                    &format!("error:{}", e.code()),
                );
                ErrorData::internal_error(e.to_string(), None)
            },
        )?;

        let mut output = format!(
            "chunk {}: bytes {}..{}, lines {}..{}\n",
            result.chunk_id, result.byte_start, result.byte_end, result.start_line,
            result.end_line,
        );
        if result.eof {
            output.push_str("[last chunk]\n");
        }
        output.push_str("---\n");
        output.push_str(&result.text);

        log_call(
            "read_chunk",
            &params.path,
            output.len(),
            &format!("ok:chunk={},eof={}", result.chunk_id, result.eof),
        );
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        name = "read_range_around_line",
        description = "Read lines centered on a target line. radius defaults to 30. Useful for 'show me the function at line N' queries."
    )]
    fn read_range_around_line(
        &self,
        Parameters(params): Parameters<ReadRangeAroundLineParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let radius = params.radius.unwrap_or(30);
        let start = params.line.saturating_sub(radius).max(1);
        let end = params.line.saturating_add(radius);

        let result = lines::read_lines(&params.path, start, end).map_err(|e| {
            log_call(
                "read_range_around_line",
                &params.path,
                0,
                &format!("error:{}", e.code()),
            );
            ErrorData::internal_error(e.to_string(), None)
        })?;

        let mut output = String::new();
        for (i, line) in result.lines.iter().enumerate() {
            let line_num = result.start_line + i;
            let marker = if line_num == params.line { "→" } else { " " };
            output.push_str(&format!("{marker} {line_num:>6}\t{line}\n"));
        }

        log_call(
            "read_range_around_line",
            &params.path,
            output.len(),
            &format!("ok:center={},radius={}", params.line, radius),
        );
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        name = "search_in_file",
        description = "Streaming pattern search within a single file via ripgrep. Auto-detects mode: patterns containing regex metacharacters (. * + ? ( [ \\ …) run as regex; plain patterns are matched literally. Case-smart. Returns line numbers + snippets with pagination via cursor. Output header shows the interpreted mode."
    )]
    fn search_in_file(
        &self,
        Parameters(params): Parameters<SearchInFileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        use grep_regex::RegexMatcherBuilder;
        use grep_searcher::{Searcher, Sink, SinkMatch};

        let max = params.max_results.unwrap_or(50).min(500);
        let offset = params
            .cursor
            .as_deref()
            .and_then(Cursor::decode)
            .map(|c| c.offset)
            .unwrap_or(0);

        let (compiled_pattern, is_regex) = resolve_search_pattern(&params.pattern);
        let matcher = RegexMatcherBuilder::new()
            .case_smart(true)
            .build(&compiled_pattern)
            .map_err(|e| {
                log_call(
                    "search_in_file",
                    &params.path,
                    0,
                    &format!("error:regex:{e}"),
                );
                let mode = if is_regex { "regex" } else { "literal" };
                ErrorData::internal_error(
                    format!("invalid {mode} pattern {:?}: {e}", params.pattern),
                    None,
                )
            })?;

        struct Collector {
            matches: Vec<(u64, String)>,
            count: usize,
            offset: usize,
            limit: usize,
        }

        impl Sink for Collector {
            type Error = std::io::Error;
            fn matched(
                &mut self,
                _searcher: &Searcher,
                mat: &SinkMatch<'_>,
            ) -> Result<bool, Self::Error> {
                self.count += 1;
                if self.count > self.offset && self.matches.len() < self.limit {
                    let lineno = mat.line_number().unwrap_or(0);
                    let line = std::str::from_utf8(mat.bytes())
                        .unwrap_or("<non-utf8 line>")
                        .trim_end_matches('\n')
                        .to_string();
                    self.matches.push((lineno, line));
                }
                // Keep scanning past the limit so we can report the true total
                // up to a generous cap (no "offset+limit only" lie).
                Ok(self.count < self.offset + self.limit + 10_000)
            }
        }

        let mut collector = Collector {
            matches: Vec::with_capacity(max),
            count: 0,
            offset,
            limit: max,
        };

        let file = std::fs::File::open(&params.path).map_err(|e| {
            log_call("search_in_file", &params.path, 0, &format!("error:{e}"));
            ErrorData::internal_error(e.to_string(), None)
        })?;

        let mut searcher = Searcher::new();
        if let Err(e) = searcher.search_file(&matcher, &file, &mut collector) {
            log_call("search_in_file", &params.path, 0, &format!("error:{e}"));
            return Err(ErrorData::internal_error(e.to_string(), None));
        }

        let total = collector.count;
        let returned = collector.matches.len();
        let end = offset + returned;

        let mode = if is_regex { "regex" } else { "literal" };
        let mut output = format!(
            "# search_in_file ({mode}) — {} match(es) scanned; showing {}..{}\n",
            total,
            offset + 1,
            end
        );
        for (lineno, line) in &collector.matches {
            output.push_str(&format!("{lineno:>6}\t{line}\n"));
        }
        if end < total {
            let next_cursor = Cursor {
                offset: end,
                limit: max,
            }
            .encode();
            output.push_str(&format!("\n[next cursor: {next_cursor}]\n"));
        }

        log_call(
            "search_in_file",
            &params.path,
            output.len(),
            &format!("ok:mode={mode},total={total}"),
        );
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        name = "list_archive",
        description = "List entries in a zip/tar/tar.gz archive. Returns names, sizes, and directory flags."
    )]
    fn list_archive(
        &self,
        Parameters(params): Parameters<ListArchiveParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let max = params.max_entries.unwrap_or(500);
        let content = specialized::extract_specialized(&params.path).map_err(|e| {
            log_call("list_archive", &params.path, 0, &format!("error:{}", e.code()));
            ErrorData::internal_error(e.to_string(), None)
        })?;

        let mut output = format!("{}\n", content.summary);
        for (i, entry) in content.entries.iter().enumerate() {
            if i >= max {
                output.push_str(&format!("[truncated at {max} entries]\n"));
                break;
            }
            let marker = if entry.is_dir { "d" } else { "-" };
            let size = entry.size.unwrap_or(0);
            output.push_str(&format!("{marker}  {size:>10}  {}\n", entry.name));
        }

        log_call("list_archive", &params.path, output.len(), "ok");
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        name = "extract_pdf_text",
        description = "Extract plain text from a PDF file. Returns the raw text content; image-only or encrypted PDFs return empty."
    )]
    fn extract_pdf_text(
        &self,
        Parameters(params): Parameters<ExtractPdfTextParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let content = specialized::extract_specialized(&params.path).map_err(|e| {
            log_call(
                "extract_pdf_text",
                &params.path,
                0,
                &format!("error:{}", e.code()),
            );
            ErrorData::internal_error(e.to_string(), None)
        })?;

        let mut output = format!("{}\n---\n", content.summary);
        output.push_str(content.text.as_deref().unwrap_or("(no text)"));

        log_call("extract_pdf_text", &params.path, output.len(), "ok");
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        name = "outline",
        description = "Source-code outline: top-level fn/struct/class/trait/impl/type per file. Tries tree-sitter first (precise, cached in LMDB keyed by size+mtime); falls back to a regex heuristic for languages without a bundled grammar. Output header shows which source was used (cache, tree-sitter, or regex)."
    )]
    fn outline(
        &self,
        Parameters(params): Parameters<OutlineParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let max = params.max_entries.unwrap_or(200).min(2000);
        let (chunks, source) = resolve_outline(&params.path, max).map_err(|e| {
            log_call("outline", &params.path, 0, &format!("error:{e}"));
            ErrorData::internal_error(e.to_string(), None)
        })?;

        let mut output = format!(
            "# outline ({}): {} entries (max {})\n",
            source.as_str(),
            chunks.len(),
            max
        );
        for c in &chunks {
            let lines = if c.start_line == c.end_line {
                format!("{}", c.start_line)
            } else {
                format!("{}..{}", c.start_line, c.end_line)
            };
            let name = c.name.as_deref().unwrap_or("");
            output.push_str(&format!("{:>10}\t{:20}\t{}\n", lines, c.kind, name));
        }
        if chunks.is_empty() {
            output.push_str(
                "(no top-level defs found — file may be empty, unsupported, or have no matched nodes)\n",
            );
        }

        log_call(
            "outline",
            &params.path,
            output.len(),
            &format!("ok:source={},entries={}", source.as_str(), chunks.len()),
        );
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        name = "diff_files",
        description = "Compute a line-level unified diff between two text files. Useful for comparing revisions or related files."
    )]
    fn diff_files(
        &self,
        Parameters(params): Parameters<DiffFilesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let left = std::fs::read_to_string(&params.left)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let right = std::fs::read_to_string(&params.right)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let diff = line_diff(&left, &right, &params.left, &params.right);
        log_call("diff_files", &params.left, diff.len(), "ok");
        Ok(CallToolResult::success(vec![Content::text(diff)]))
    }
}

// Naive LCS-free line diff: marks inserted/removed lines with +/- by walking
// both files in parallel. Good enough for quick "what's different" inspection
// without pulling in a diff crate.
fn line_diff(left: &str, right: &str, left_name: &str, right_name: &str) -> String {
    let ll: Vec<&str> = left.lines().collect();
    let rl: Vec<&str> = right.lines().collect();
    let max = ll.len().max(rl.len());

    let mut out = format!("--- {left_name}\n+++ {right_name}\n");
    for i in 0..max {
        match (ll.get(i), rl.get(i)) {
            (Some(a), Some(b)) if a == b => {
                out.push_str(&format!("  {i:>5} {a}\n"));
            }
            (Some(a), Some(b)) => {
                out.push_str(&format!("- {i:>5} {a}\n"));
                out.push_str(&format!("+ {i:>5} {b}\n"));
            }
            (Some(a), None) => {
                out.push_str(&format!("- {i:>5} {a}\n"));
            }
            (None, Some(b)) => {
                out.push_str(&format!("+ {i:>5} {b}\n"));
            }
            (None, None) => break,
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Outline resolution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutlineSource {
    Cache,
    TreeSitter,
    Regex,
}

impl OutlineSource {
    fn as_str(self) -> &'static str {
        match self {
            OutlineSource::Cache => "cache",
            OutlineSource::TreeSitter => "tree-sitter",
            OutlineSource::Regex => "regex",
        }
    }
}

fn compute_revision_for_path(path: &str) -> Option<String> {
    let md = std::fs::metadata(path).ok()?;
    let size = md.len();
    let mtime = md
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(ffr_core::index::compute_revision(size, mtime))
}

// Multi-stage outline resolver: cache → tree-sitter → regex. Revision-tagged
// cache entries survive unchanged files across server restarts. Tree-sitter is
// precise but only covers the grammars bundled by the `tree-sitter` feature;
// everything else falls through to the regex outline.
fn resolve_outline(
    path: &str,
    max: usize,
) -> Result<(Vec<ffr_core::db::SemanticChunk>, OutlineSource), ffr_core::errors::FFRError> {
    let revision = compute_revision_for_path(path);

    if let Some(rev) = revision.as_deref() {
        if let Ok(Some(rec)) = ffr_core::cache::get_semantic(path) {
            if rec.revision == rev {
                let mut chunks = rec.chunks;
                chunks.truncate(max);
                return Ok((chunks, OutlineSource::Cache));
            }
        }
    }

    match ffr_core::ts_outline::outline_path(path) {
        Ok(Some(chunks)) => {
            if let Some(rev) = revision {
                let rec = ffr_core::db::SemanticRecord {
                    revision: rev,
                    chunks: chunks.clone(),
                };
                if let Err(e) = ffr_core::cache::upsert_semantic(path, &rec) {
                    tracing::debug!(error = %e, path = %path, "upsert_semantic failed");
                }
            }
            let mut truncated = chunks;
            truncated.truncate(max);
            return Ok((truncated, OutlineSource::TreeSitter));
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(error = %e, path = %path, "tree-sitter outline failed; falling back to regex");
        }
    }

    let entries = crate::outline::outline_file(path, max)
        .map_err(|e| ffr_core::errors::FFRError::IOError(e.to_string()))?;
    let chunks: Vec<_> = entries
        .into_iter()
        .map(|e| ffr_core::db::SemanticChunk {
            start_line: e.line as u64,
            end_line: e.line as u64,
            kind: e.kind,
            name: Some(e.name),
        })
        .collect();
    Ok((chunks, OutlineSource::Regex))
}

// Decide how `search_in_file` compiles a user-supplied pattern. If the input
// contains any regex metacharacter (detected via `regex::escape` changing the
// string), it is passed through as a regex; otherwise it is escaped and
// matched literally. Returned tuple is `(pattern_to_compile, is_regex)`.
fn resolve_search_pattern(input: &str) -> (String, bool) {
    let escaped = regex::escape(input);
    if escaped == input {
        (escaped, false)
    } else {
        (input.to_string(), true)
    }
}

// ---------------------------------------------------------------------------
// MCP server handler
// ---------------------------------------------------------------------------

#[tool_handler]
impl ServerHandler for FfrServer {
    fn get_info(&self) -> ServerInfo {
        let notice = get_update_notice();
        let instructions = if notice.is_empty() {
            MCP_INSTRUCTIONS.to_string()
        } else {
            format!("{MCP_INSTRUCTIONS}\n{notice}")
        };

        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("ffr", env!("CARGO_PKG_VERSION")))
            .with_instructions(instructions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_plain_identifier_is_literal() {
        let (compiled, is_regex) = resolve_search_pattern("InProgressQuote");
        assert!(!is_regex);
        assert_eq!(compiled, "InProgressQuote");
    }

    #[test]
    fn resolve_identifier_with_underscore_is_literal() {
        let (_, is_regex) = resolve_search_pattern("actor_auth_123");
        assert!(!is_regex);
    }

    #[test]
    fn resolve_spaces_are_literal() {
        let (compiled, is_regex) = resolve_search_pattern("hello world");
        assert!(!is_regex);
        assert_eq!(compiled, "hello world");
    }

    #[test]
    fn resolve_dot_triggers_regex() {
        let (compiled, is_regex) = resolve_search_pattern("foo.bar");
        assert!(is_regex);
        assert_eq!(compiled, "foo.bar");
    }

    #[test]
    fn resolve_alternation_triggers_regex() {
        let (_, is_regex) = resolve_search_pattern("a(b|c)");
        assert!(is_regex);
    }

    #[test]
    fn resolve_escape_sequence_triggers_regex() {
        let (compiled, is_regex) = resolve_search_pattern(r"\d+");
        assert!(is_regex);
        assert_eq!(compiled, r"\d+");
    }

    #[test]
    fn resolve_plus_triggers_regex_and_preserves_input() {
        let (compiled, is_regex) = resolve_search_pattern("a+b");
        assert!(is_regex);
        assert_eq!(compiled, "a+b");
    }

    #[test]
    fn resolve_compiled_pattern_actually_matches_literal_when_expected() {
        use grep_regex::RegexMatcherBuilder;
        use grep_matcher::Matcher;

        let (compiled, is_regex) = resolve_search_pattern("foo.bar");
        assert!(is_regex);
        let m = RegexMatcherBuilder::new().build(&compiled).unwrap();
        assert!(m.is_match(b"foo.bar").unwrap());
        assert!(m.is_match(b"fooXbar").unwrap(), "regex mode: dot matches any char");

        // `a+b` is a regex meaning "one or more a followed by b" — documents
        // that a pattern with metachars is NOT auto-escaped into literal mode.
        // Users who want to match the literal string "a+b" must avoid passing
        // `+`, or accept regex semantics.
        let (compiled, is_regex) = resolve_search_pattern("a+b");
        assert!(is_regex);
        let m = RegexMatcherBuilder::new().build(&compiled).unwrap();
        assert!(m.is_match(b"aaab").unwrap());
        assert!(m.is_match(b"ab").unwrap());
        assert!(!m.is_match(b"a+b").unwrap());

        let (compiled, is_regex) = resolve_search_pattern("plain text");
        assert!(!is_regex);
        let m = RegexMatcherBuilder::new().build(&compiled).unwrap();
        assert!(m.is_match(b"plain text").unwrap());
        assert!(!m.is_match(b"plainXtext").unwrap());
    }

    // ------------------------------------------------------------------
    // resolve_outline
    // ------------------------------------------------------------------

    fn write_tmp(name: &str, ext: &str, body: &str) -> String {
        use std::io::Write;
        let path = format!("/tmp/ffr_resolve_outline_{name}.{ext}");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        path
    }

    #[test]
    fn outline_uses_tree_sitter_for_rust() {
        let path = write_tmp(
            "ts_rust",
            "rs",
            "pub fn foo() {}\npub struct Bar;\n",
        );
        let (chunks, source) = resolve_outline(&path, 100).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(source, OutlineSource::TreeSitter);
        let kinds: Vec<&str> = chunks.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"function_item"));
        assert!(kinds.contains(&"struct_item"));
    }

    #[test]
    fn outline_falls_back_to_regex_for_lua() {
        // Lua has a regex outline in outline.rs but no bundled TS grammar in
        // the current ts_outline module, so resolution should end at regex.
        let path = write_tmp(
            "regex_lua",
            "lua",
            "local function foo()\nend\n\nfunction Mod.bar()\nend\n",
        );
        let (chunks, source) = resolve_outline(&path, 100).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(source, OutlineSource::Regex);
        assert!(chunks.iter().any(|c| c.name.as_deref() == Some("foo")));
    }

    #[test]
    fn outline_empty_for_unsupported_extension() {
        let path = write_tmp("unknown", "xyzzy", "plain text\nno code here\n");
        let (chunks, source) = resolve_outline(&path, 100).unwrap();
        let _ = std::fs::remove_file(&path);
        // Regex path returns empty (unsupported ext), caller sees
        // OutlineSource::Regex with 0 entries.
        assert_eq!(source, OutlineSource::Regex);
        assert!(chunks.is_empty());
    }

    #[test]
    fn outline_respects_max_limit() {
        let mut body = String::new();
        for i in 0..20 {
            body.push_str(&format!("pub fn f{i}() {{}}\n"));
        }
        let path = write_tmp("max_limit", "rs", &body);
        let (chunks, _source) = resolve_outline(&path, 5).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(chunks.len(), 5);
    }
}
