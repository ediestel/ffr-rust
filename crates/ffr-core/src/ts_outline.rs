//! Tree-sitter source outline.
//!
//! Precise alternative to the regex-based outline in `ffr-mcp/src/outline.rs`.
//! Feature-gated behind `tree-sitter` so that callers who do not need TS can
//! keep the default build small.
//!
//! The language → node-type table mirrors `lua/ffr/semantic.lua` so the MCP
//! and Neovim producers emit compatible `SemanticChunk` records.

use std::fs;
use std::path::Path;

use crate::db::SemanticChunk;
use crate::errors::FFRError;

/// Top-level node types to treat as outline entries. Kept in sync with
/// `lua/ffr/semantic.lua::TOP_LEVEL_NODES`.
fn top_level_node_types(lang: &str) -> Option<&'static [&'static str]> {
    match lang {
        "rust" => Some(&[
            "function_item",
            "impl_item",
            "trait_item",
            "struct_item",
            "enum_item",
            "mod_item",
        ]),
        "python" => Some(&["function_definition", "class_definition"]),
        "javascript" => Some(&[
            "function_declaration",
            "class_declaration",
            "method_definition",
            "arrow_function",
        ]),
        "typescript" => Some(&[
            "function_declaration",
            "class_declaration",
            "method_definition",
            "interface_declaration",
        ]),
        "tsx" => Some(&[
            "function_declaration",
            "class_declaration",
            "method_definition",
        ]),
        "c" => Some(&["function_definition", "declaration"]),
        "cpp" => Some(&["function_definition", "class_specifier", "struct_specifier"]),
        "go" => Some(&[
            "function_declaration",
            "method_declaration",
            "type_declaration",
        ]),
        "java" => Some(&[
            "method_declaration",
            "class_declaration",
            "interface_declaration",
        ]),
        _ => None,
    }
}

fn language_for(lang: &str) -> Option<tree_sitter::Language> {
    match lang {
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "c" => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        _ => None,
    }
}

/// Map a filename/extension to the tree-sitter language key. Returns `None`
/// for unsupported file types so callers can fall back to a regex outline.
pub fn lang_for_path(path: &str) -> Option<&'static str> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())?;
    match ext.as_str() {
        "rs" => Some("rust"),
        "py" | "pyi" => Some("python"),
        "js" | "mjs" | "cjs" | "jsx" => Some("javascript"),
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "c" | "h" => Some("c"),
        "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => Some("cpp"),
        "go" => Some("go"),
        "java" => Some("java"),
        _ => None,
    }
}

/// Compute a tree-sitter outline for `path`.
///
/// Returns `Ok(Some(chunks))` when the file was parsed successfully.
/// Returns `Ok(None)` when the language is unsupported (caller should fall
/// back to the regex outline). Returns `Err` only on I/O or setup errors.
pub fn outline_path(path: &str) -> Result<Option<Vec<SemanticChunk>>, FFRError> {
    let Some(lang) = lang_for_path(path) else {
        return Ok(None);
    };

    let Some(language) = language_for(lang) else {
        return Ok(None);
    };

    let wanted: &[&str] = top_level_node_types(lang).unwrap_or(&[]);
    if wanted.is_empty() {
        return Ok(None);
    }

    let source = fs::read(path)?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .map_err(|e| FFRError::Internal(format!("tree-sitter set_language({lang}): {e}")))?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| FFRError::Internal(format!("tree-sitter parse failed for {path}")))?;

    let root = tree.root_node();
    let mut out = Vec::new();
    collect(root, wanted, &source, &mut out);
    Ok(Some(out))
}

/// Depth-first walk that stops descending into matched top-level nodes so
/// nested function/class definitions don't double up.
fn collect(node: tree_sitter::Node, wanted: &[&str], src: &[u8], out: &mut Vec<SemanticChunk>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let ty = child.kind();
        if wanted.iter().any(|w| *w == ty) {
            let start_line = child.start_position().row as u64 + 1;
            let end_line = child.end_position().row as u64 + 1;
            let name = extract_name(child, src);
            out.push(SemanticChunk {
                start_line,
                end_line,
                kind: ty.to_string(),
                name,
            });
        } else {
            collect(child, wanted, src, out);
        }
    }
}

fn extract_name(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let name_node = node.child_by_field_name("name")?;
    let start = name_node.start_byte();
    let end = name_node.end_byte();
    if end <= src.len() {
        std::str::from_utf8(&src[start..end]).ok().map(String::from)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(name: &str, ext: &str, body: &str) -> String {
        let path = format!("/tmp/ffr_ts_outline_{name}.{ext}");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        path
    }

    #[test]
    fn rust_top_level() {
        let path = write_tmp(
            "rust_top_level",
            "rs",
            r#"
pub fn foo() {}

pub struct Bar {
    x: u32,
}

impl Bar {
    pub fn method(&self) {
        fn nested() {}
    }
}

pub trait Qux {}

pub enum Kind { A, B }

pub mod inner {}
"#,
        );

        let chunks = outline_path(&path).unwrap().unwrap();
        let _ = std::fs::remove_file(&path);

        let names: Vec<_> = chunks
            .iter()
            .map(|c| (c.kind.clone(), c.name.clone()))
            .collect();
        assert!(names.contains(&("function_item".into(), Some("foo".into()))));
        assert!(names.contains(&("struct_item".into(), Some("Bar".into()))));
        assert!(names.contains(&("enum_item".into(), Some("Kind".into()))));
        assert!(names.contains(&("trait_item".into(), Some("Qux".into()))));
        assert!(names.contains(&("mod_item".into(), Some("inner".into()))));

        // Nested `fn nested()` must NOT appear — walker stops at matched nodes.
        assert!(!names.iter().any(|(_, n)| n.as_deref() == Some("nested")));
    }

    #[test]
    fn python_defs() {
        let path = write_tmp(
            "python_defs",
            "py",
            r#"
def foo():
    pass

class Bar:
    def method(self):
        pass
"#,
        );

        let chunks = outline_path(&path).unwrap().unwrap();
        let _ = std::fs::remove_file(&path);
        let names: Vec<_> = chunks
            .iter()
            .map(|c| (c.kind.clone(), c.name.clone()))
            .collect();
        assert!(names.contains(&("function_definition".into(), Some("foo".into()))));
        assert!(names.contains(&("class_definition".into(), Some("Bar".into()))));
        // Method is inside the class body — we stop at class_definition so
        // `method` does not show up at the top level. That matches Neovim.
        assert!(!names.iter().any(|(_, n)| n.as_deref() == Some("method")));
    }

    #[test]
    fn go_funcs_and_types() {
        let path = write_tmp(
            "go_funcs",
            "go",
            r#"
package main

type T struct { X int }

func foo() {}

func (t *T) Bar() {}
"#,
        );
        let chunks = outline_path(&path).unwrap().unwrap();
        let _ = std::fs::remove_file(&path);
        let kinds: Vec<&str> = chunks.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"function_declaration"));
        assert!(kinds.contains(&"method_declaration"));
        assert!(kinds.contains(&"type_declaration"));
    }

    #[test]
    fn unsupported_extension_returns_none() {
        let path = write_tmp("unknown_ext", "xyzzy", "some text");
        let result = outline_path(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn typescript_interface() {
        let path = write_tmp(
            "ts_iface",
            "ts",
            r#"
export interface Foo { x: number; }
export class Bar { m() {} }
export function baz() {}
"#,
        );
        let chunks = outline_path(&path).unwrap().unwrap();
        let _ = std::fs::remove_file(&path);
        let kinds: Vec<&str> = chunks.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"interface_declaration"));
        assert!(kinds.contains(&"class_declaration"));
        assert!(kinds.contains(&"function_declaration"));
    }
}
