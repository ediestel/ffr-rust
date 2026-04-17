//! Heuristic source-code outline (regex fallback).
//!
//! Regex-based detector for top-level definitions across common languages.
//! No tree-sitter dependency — intentionally approximate. The MCP `outline`
//! tool prefers `ffr_core::ts_outline` (tree-sitter) when a grammar is
//! bundled; this module is the fallback for extensions without a grammar.
//! Inside Neovim, `lua/ffr/semantic.lua` uses the user's installed TS parsers.

use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct OutlineEntry {
    pub line: usize,
    pub kind: String,
    pub name: String,
}

struct LangSpec {
    extensions: &'static [&'static str],
    patterns: &'static [&'static str],
}

static LANG_SPECS: &[LangSpec] = &[
    LangSpec {
        extensions: &["rs"],
        patterns: &[
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+(?P<sname>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+(?P<ename>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?trait\s+(?P<tname>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*impl(?:<[^>]*>)?\s+(?P<iname>[A-Za-z_][A-Za-z0-9_:<>]*)",
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?P<mname>[A-Za-z_][A-Za-z0-9_]*)",
        ],
    },
    LangSpec {
        extensions: &["py"],
        patterns: &[
            r"^\s*def\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*async\s+def\s+(?P<aname>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*class\s+(?P<cname>[A-Za-z_][A-Za-z0-9_]*)",
        ],
    },
    LangSpec {
        extensions: &["js", "mjs", "cjs", "jsx", "ts", "tsx"],
        patterns: &[
            r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)",
            r"^\s*(?:export\s+)?class\s+(?P<cname>[A-Za-z_$][A-Za-z0-9_$]*)",
            r"^\s*(?:export\s+)?interface\s+(?P<iname>[A-Za-z_$][A-Za-z0-9_$]*)",
            r"^\s*(?:export\s+)?type\s+(?P<tname>[A-Za-z_$][A-Za-z0-9_$]*)",
            r"^\s*(?:export\s+)?(?:const|let|var)\s+(?P<vname>[A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*(?:async\s+)?(?:\([^)]*\)|[A-Za-z_$][A-Za-z0-9_$]*)\s*=>",
        ],
    },
    LangSpec {
        extensions: &["lua"],
        patterns: &[
            r"^\s*(?:local\s+)?function\s+(?P<name>[A-Za-z_][A-Za-z0-9_.:]*)",
            r"^\s*(?:local\s+)?(?P<vname>[A-Za-z_][A-Za-z0-9_.]*)\s*=\s*function",
        ],
    },
    LangSpec {
        extensions: &["c", "h", "cpp", "hpp", "cc", "hh", "cxx"],
        patterns: &[
            // function: <return-type> <name>(...) {
            r"^\s*(?:static\s+|inline\s+|extern\s+)*[A-Za-z_][\w\s\*&<>,:]*\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*\([^)]*\)\s*\{",
            r"^\s*(?:class|struct|union)\s+(?P<cname>[A-Za-z_][A-Za-z0-9_]*)",
        ],
    },
    LangSpec {
        extensions: &["go"],
        patterns: &[
            r"^\s*func\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*func\s+\([^)]*\)\s+(?P<mname>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*type\s+(?P<tname>[A-Za-z_][A-Za-z0-9_]*)",
        ],
    },
    LangSpec {
        extensions: &["java", "kt", "kts"],
        patterns: &[
            r"^\s*(?:public|private|protected)?\s*(?:static\s+)?(?:final\s+)?class\s+(?P<cname>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*(?:public|private|protected)?\s*(?:static\s+)?(?:final\s+)?interface\s+(?P<iname>[A-Za-z_][A-Za-z0-9_]*)",
            r"^\s*(?:public|private|protected)?\s*(?:static\s+)?(?:final\s+)?[A-Za-z_][\w<>,\[\]\s]*\s+(?P<fname>[A-Za-z_][A-Za-z0-9_]*)\s*\(",
        ],
    },
];

fn compile_patterns(spec: &LangSpec) -> Vec<Regex> {
    spec.patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
}

fn compiled_for_ext(ext: &str) -> Option<&'static [Regex]> {
    static CACHE: OnceLock<std::collections::HashMap<&'static str, Vec<Regex>>> = OnceLock::new();
    let map = CACHE.get_or_init(|| {
        let mut m = std::collections::HashMap::new();
        for spec in LANG_SPECS {
            let compiled = compile_patterns(spec);
            for e in spec.extensions {
                m.insert(*e, compiled.clone());
            }
        }
        m
    });
    map.get(ext).map(|v| v.as_slice())
}

fn ext_of(path: &str) -> Option<&str> {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.rsplit('.').next().filter(|e| !e.contains('/'))
}

/// Compute an outline for `path` by streaming the file one line at a time.
/// Capped at `max_entries` to keep output bounded.
pub fn outline_file(path: &str, max_entries: usize) -> std::io::Result<Vec<OutlineEntry>> {
    use std::io::{BufRead, BufReader};

    let ext = ext_of(path).unwrap_or("");
    let patterns: &[Regex] = match compiled_for_ext(ext) {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };

    let f = std::fs::File::open(path)?;
    let reader = BufReader::new(f);
    let mut entries = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if entries.len() >= max_entries {
            break;
        }
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        for re in patterns {
            if let Some(caps) = re.captures(&line) {
                // first non-"0" named capture wins
                for (idx, n) in re.capture_names().enumerate() {
                    if idx == 0 {
                        continue;
                    }
                    if let Some(name) = n.and_then(|nm| caps.name(nm)) {
                        entries.push(OutlineEntry {
                            line: i + 1,
                            kind: name_kind(re.as_str()),
                            name: name.as_str().to_string(),
                        });
                        break;
                    }
                }
                break;
            }
        }
    }

    Ok(entries)
}

fn name_kind(pattern: &str) -> String {
    // Extract the first keyword that isn't a regex metachar/modifier. This is
    // a best-effort label; exact names are the source of truth.
    for tag in &[
        "fn", "struct", "enum", "trait", "impl", "mod", "class", "interface", "type", "def",
        "function", "async def", "func",
    ] {
        if pattern.contains(tag) {
            return (*tag).to_string();
        }
    }
    "decl".to_string()
}
