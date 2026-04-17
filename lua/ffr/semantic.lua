--- Tree-sitter semantic chunking with LMDB-persisted cache.
---
--- Chunks are computed via `vim.treesitter.get_parser` (reusing the user's
--- installed parsers — no Rust TS dep), then persisted in a second LMDB
--- named database keyed by absolute path. The stored record carries a
--- `revision` ("{size}:{mtime}") so reopening a modified file recomputes.

local M = {}

local TOP_LEVEL_NODES = {
  rust = { 'function_item', 'impl_item', 'trait_item', 'struct_item', 'enum_item', 'mod_item' },
  python = { 'function_definition', 'class_definition' },
  javascript = { 'function_declaration', 'class_declaration', 'method_definition', 'arrow_function' },
  typescript = { 'function_declaration', 'class_declaration', 'method_definition', 'interface_declaration' },
  tsx = { 'function_declaration', 'class_declaration', 'method_definition' },
  lua = { 'function_declaration', 'function_definition', 'method_function' },
  c = { 'function_definition', 'declaration' },
  cpp = { 'function_definition', 'class_specifier', 'struct_specifier' },
  go = { 'function_declaration', 'method_declaration', 'type_declaration' },
  java = { 'method_declaration', 'class_declaration', 'interface_declaration' },
}

---@param node TSNode
---@param bufnr integer
---@return string|nil
local function extract_name(node, bufnr)
  local name_node = node:field('name')[1]
  if not name_node then return nil end
  local ok, text = pcall(vim.treesitter.get_node_text, name_node, bufnr)
  if ok then return text end
  return nil
end

---@param bufnr integer
---@param lang? string
---@return table[]
local function compute_chunks(bufnr, lang)
  if not vim.api.nvim_buf_is_valid(bufnr) then return {} end
  lang = lang or vim.bo[bufnr].filetype
  if not lang or lang == '' then return {} end

  local ok_parser, parser = pcall(vim.treesitter.get_parser, bufnr, lang)
  if not ok_parser or not parser then return {} end
  local tree = parser:parse()[1]
  if not tree then return {} end
  local root = tree:root()

  local wanted = TOP_LEVEL_NODES[lang]
  if not wanted then return {} end
  local wanted_set = {}
  for _, t in ipairs(wanted) do wanted_set[t] = true end

  local results = {}
  local function walk(node)
    for child in node:iter_children() do
      local t = child:type()
      if wanted_set[t] then
        local s, _, e, _ = child:range()
        table.insert(results, {
          start_line = s + 1,
          end_line = e + 1,
          kind = t,
          name = extract_name(child, bufnr),
        })
      else
        walk(child)
      end
    end
  end
  walk(root)
  table.sort(results, function(a, b) return a.start_line < b.start_line end)
  return results
end

---@param path string absolute file path (bufname)
---@return string|nil revision  "{size}:{mtime}", nil if stat fails
local function current_revision(path)
  local stat = vim.uv.fs_stat(path)
  if not stat then return nil end
  return string.format('%d:%d', stat.size, stat.mtime and stat.mtime.sec or 0)
end

---@param bufnr integer
---@param lang? string
---@return table[] list of { start_line, end_line, kind, name }
function M.chunks_for_buffer(bufnr, lang)
  if not vim.api.nvim_buf_is_valid(bufnr) then return {} end
  local path = vim.api.nvim_buf_get_name(bufnr)
  if not path or path == '' then
    return compute_chunks(bufnr, lang)
  end

  local revision = current_revision(path)

  -- LMDB lookup first.
  local ok_rust, rust = pcall(require, 'ffr.rust')
  if ok_rust then
    local mod = rust.get()
    if mod and mod.semantic_get then
      local ok, record = pcall(mod.semantic_get, path)
      if ok and type(record) == 'table' and record.revision == revision then
        return record.chunks or {}
      end
    end
  end

  -- Compute fresh.
  local chunks = compute_chunks(bufnr, lang)

  -- Persist if we have a valid revision.
  if ok_rust and revision then
    local mod = rust.get()
    if mod and mod.semantic_upsert then
      pcall(mod.semantic_upsert, {
        path = path,
        revision = revision,
        chunks = chunks,
      })
    end
  end

  return chunks
end

---@param bufnr integer
---@param cursor_line integer
---@param direction "next"|"prev"
---@return table|nil chunk
function M.find_neighbor(bufnr, cursor_line, direction)
  local chunks = M.chunks_for_buffer(bufnr)
  if #chunks == 0 then return nil end
  if direction == 'next' then
    for _, c in ipairs(chunks) do
      if c.start_line > cursor_line then return c end
    end
    return nil
  else
    local best = nil
    for _, c in ipairs(chunks) do
      if c.start_line < cursor_line then
        best = c
      else
        break
      end
    end
    return best
  end
end

--- Manually evict the cached chunk list for a path.
---@param path string
function M.invalidate(path)
  local ok, rust = pcall(require, 'ffr.rust')
  if not ok then return end
  local mod = rust.get()
  if mod and mod.semantic_remove then
    pcall(mod.semantic_remove, path)
  end
end

return M
