local M = {}

local config = require("ffr.config")
local client = require("ffr.client")
local policy = require("ffr.policy")
local buffer = require("ffr.buffer")
local session = require("ffr.session")
local render = require("ffr.render")
local cache = require("ffr.cache")

M.debug_state = { enabled = false }

local function maybe_prefetch_ahead(path, chunk_id)
  local cfg = config.get()
  local count = cfg.chunk and cfg.chunk.prefetch_count or 0
  if not count or count <= 0 then return end
  local ok_rust, rust = pcall(require, 'ffr.rust')
  if not ok_rust then return end
  local mod, _ = rust.get()
  if not mod or not mod.prefetch_hint then return end
  pcall(mod.prefetch_hint, {
    path = path,
    chunk_id = chunk_id + 1,
    chunk_bytes = cfg.chunk_bytes,
    count = count,
  })
end

---@param opts? table
function M.setup(opts)
  local cfg = config.setup(opts)

  -- Register highlight groups from (possibly user-overridden) defaults.
  require('ffr.highlights').setup(cfg.highlights or {})

  -- Lifecycle: ensures graceful VimLeavePre shutdown (watcher + LMDB).
  require('ffr.lifecycle').init()

  cache.load_persistent_metadata()

  -- Spawn watcher lazily — client.configure() is still the path that calls
  -- into the Rust configure() to init LMDB, so the watcher must be spawned
  -- after that. We hook via on_init so it runs once client is ready.
  if cfg.watcher and cfg.watcher.enabled then
    vim.defer_fn(function()
      local ok_client, client_mod = pcall(require, 'ffr.client')
      if not ok_client then return end
      local started, _ = client_mod.start()
      if not started then return end
      local ok_rust, rust = pcall(require, 'ffr.rust')
      if not ok_rust then return end
      local mod, _ = rust.get()
      if not mod or not mod.watcher_spawn then return end
      pcall(mod.watcher_spawn, { debounce_ms = cfg.watcher.debounce_ms })
    end, 0)
  end

  if cfg.chunk and (cfg.chunk.prefetch_count or 0) > 0 then
    vim.defer_fn(function()
      local ok_rust, rust = pcall(require, 'ffr.rust')
      if not ok_rust then return end
      local mod, _ = rust.get()
      if not mod or not mod.prefetch_spawn then return end
      pcall(mod.prefetch_spawn)
    end, 0)
  end
end

---@param path string
---@param opts? FFROpenOpts
---@return integer? bufnr, string? err
function M.open(path, opts)
  opts = opts or {}

  local cfg = config.get()

  local ok, start_err = client.start()
  if not ok then
    return nil, start_err
  end

  -- Stat: always fresh
  local stat_result, stat_err = client.stat_path(path)
  if stat_err then
    return nil, stat_err
  end

  -- Classify: use metadata cache if valid, else ask backend
  local classify_result, classify_err
  if cache.is_metadata_valid(path, stat_result) then
    local cached = cache.get_metadata(path)
    classify_result = cached.classification
  end

  if not classify_result then
    classify_result, classify_err = client.classify_path(path)
    if classify_err then
      return nil, classify_err
    end

    -- Populate metadata cache
    cache.set_metadata(path, {
      path = path,
      size = stat_result.size,
      mtime = stat_result.mtime,
      revision = ("%s:%s"):format(stat_result.size or 0, stat_result.mtime or 0),
      classification = classify_result,
      line_index_ready = false,
      line_count = classify_result.estimated_lines,
    })
  end

  local decision, reason = policy.decide(stat_result, classify_result, opts)

  if decision == "reject_open" then
    local bufnr = buffer.create_buffer(true, true)
    buffer.set_ffr_vars(bufnr, path, "preview", nil)
    local temp_session = {
      id = "reject:" .. tostring(bufnr),
      path = path,
      mode = "preview",
      bufnr = bufnr,
      winid = nil,
      classification = classify_result,
      current_chunk = nil,
      loaded_chunks = {},
      line_index_ready = false,
      eof = true,
      revision = "",
      last_rendered_range = nil,
      source = opts.source,
    }
    render.render_rejection(temp_session, reason or "file rejected")
    buffer.ensure_visible(bufnr)
    return bufnr, nil
  end

  if decision == "specialized_handler" then
    return policy.route_specialized(path, classify_result)
  end

  if decision == "full_text_open" then
    local max_lines = cfg.max_line_window
    local read_result, read_err = client.read_lines(path, 1, max_lines)
    if read_err then
      return nil, read_err
    end

    local bufnr = buffer.create_buffer(true, true)
    local sess = session.create({
      id = tostring(bufnr),
      path = path,
      mode = "full",
      bufnr = bufnr,
      winid = nil,
      classification = classify_result,
      current_chunk = nil,
      loaded_chunks = {},
      line_index_ready = false,
      eof = read_result.eof == true,
      revision = ("%s:%s"):format(stat_result.size or 0, stat_result.mtime or 0),
      last_rendered_range = {
        start_line = read_result.start_line,
        end_line = read_result.actual_end_line,
      },
      source = opts.source,
    })

    buffer.set_ffr_vars(bufnr, path, "full", sess.id)
    local render_ok, render_err = render.render_full(sess, read_result)
    if not render_ok then
      return nil, render_err
    end

    buffer.ensure_visible(bufnr)
    return bufnr, nil
  end

  if decision == "chunked_text_open" then
    local index_result, index_err = client.build_line_index(path)
    if index_err then
      return nil, index_err
    end

    -- Update metadata cache with line index info
    local cached_meta = cache.get_metadata(path)
    if cached_meta then
      cached_meta.line_index_ready = true
      cached_meta.line_count = index_result.line_count
      cache.set_metadata(path, cached_meta)
    end

    local chunk_result, chunk_err = client.read_chunk(path, 0)
    if chunk_err then
      return nil, chunk_err
    end

    -- Cache the chunk content
    cache.set_content_chunk(path, chunk_result.chunk_id, chunk_result)

    local bufnr = buffer.create_buffer(true, true)
    local sess = session.create({
      id = tostring(bufnr),
      path = path,
      mode = "chunked",
      bufnr = bufnr,
      winid = nil,
      classification = classify_result,
      current_chunk = chunk_result.chunk_id,
      loaded_chunks = { [chunk_result.chunk_id] = true },
      line_index_ready = index_result.indexed == true,
      eof = chunk_result.eof == true,
      revision = ("%s:%s"):format(stat_result.size or 0, stat_result.mtime or 0),
      last_rendered_range = {
        start_line = chunk_result.start_line,
        end_line = chunk_result.end_line,
      },
      source = opts.source,
    })

    buffer.set_ffr_vars(bufnr, path, "chunked", sess.id)
    local render_ok, render_err = render.render_chunk(sess, chunk_result)
    if not render_ok then
      return nil, render_err
    end

    maybe_prefetch_ahead(path, chunk_result.chunk_id)
    require("ffr.keymaps").bind_chunked(bufnr)
    buffer.ensure_visible(bufnr)
    return bufnr, nil
  end

  if decision == "preview_only" then
    local preview_result, preview_err = client.read_chunk(path, 0)
    if preview_err then
      return nil, preview_err
    end

    cache.set_content_chunk(path, preview_result.chunk_id, preview_result)

    local bufnr = buffer.create_buffer(true, true)
    local sess = session.create({
      id = tostring(bufnr),
      path = path,
      mode = "preview",
      bufnr = bufnr,
      winid = nil,
      classification = classify_result,
      current_chunk = preview_result.chunk_id,
      loaded_chunks = { [preview_result.chunk_id] = true },
      line_index_ready = false,
      eof = preview_result.eof == true,
      revision = ("%s:%s"):format(stat_result.size or 0, stat_result.mtime or 0),
      last_rendered_range = {
        start_line = preview_result.start_line,
        end_line = preview_result.end_line,
      },
      source = opts.source,
    })

    buffer.set_ffr_vars(bufnr, path, "preview", sess.id)
    local render_ok, render_err = render.render_chunk(sess, preview_result)
    if not render_ok then
      return nil, render_err
    end

    require("ffr.keymaps").bind_chunked(bufnr)
    buffer.ensure_visible(bufnr)
    return bufnr, nil
  end

  return nil, ("unhandled policy decision: %s"):format(tostring(decision))
end

---@param bufnr? integer
---@return string
function M.statusline(bufnr)
  return require("ffr.statusline").get(bufnr)
end

---@param path string
---@param opts? FFROpenOpts
---@return integer? bufnr, string? err
function M.preview(path, opts)
  opts = opts or {}
  opts.preview = true
  return M.open(path, opts)
end

---@return boolean ok, string? err
function M.next_chunk()
  local sess = session.current()
  if not sess then
    return false, "no active ffr session"
  end

  if sess.mode ~= "chunked" and sess.mode ~= "preview" then
    return false, "current buffer is not chunked"
  end

  if sess.eof then
    return false, "already at eof"
  end

  local next_chunk_id = (sess.current_chunk or 0) + 1

  -- Check content cache first
  local chunk_result = cache.get_content_chunk(sess.path, next_chunk_id)
  local chunk_err
  if not chunk_result then
    chunk_result, chunk_err = client.read_chunk(sess.path, next_chunk_id)
    if chunk_err then
      return false, chunk_err
    end
    cache.set_content_chunk(sess.path, next_chunk_id, chunk_result)
  end

  local updated = session.update(sess.id, {
    current_chunk = chunk_result.chunk_id,
    eof = chunk_result.eof == true,
    last_rendered_range = {
      start_line = chunk_result.start_line,
      end_line = chunk_result.end_line,
    },
    loaded_chunks = vim.tbl_extend("force", sess.loaded_chunks, {
      [chunk_result.chunk_id] = true,
    }),
  })

  if not updated then
    return false, "failed to update session"
  end

  maybe_prefetch_ahead(sess.path, chunk_result.chunk_id)
  return render.render_chunk(updated, chunk_result)
end

---@return boolean ok, string? err
function M.prev_chunk()
  local sess = session.current()
  if not sess then
    return false, "no active ffr session"
  end

  if sess.mode ~= "chunked" and sess.mode ~= "preview" then
    return false, "current buffer is not chunked"
  end

  local current = sess.current_chunk or 0
  if current == 0 then
    return false, "already at first chunk"
  end

  local prev_chunk_id = current - 1

  -- Check content cache first
  local chunk_result = cache.get_content_chunk(sess.path, prev_chunk_id)
  local chunk_err
  if not chunk_result then
    chunk_result, chunk_err = client.read_chunk(sess.path, prev_chunk_id)
    if chunk_err then
      return false, chunk_err
    end
    cache.set_content_chunk(sess.path, prev_chunk_id, chunk_result)
  end

  local updated = session.update(sess.id, {
    current_chunk = chunk_result.chunk_id,
    eof = false,
    last_rendered_range = {
      start_line = chunk_result.start_line,
      end_line = chunk_result.end_line,
    },
    loaded_chunks = vim.tbl_extend("force", sess.loaded_chunks, {
      [chunk_result.chunk_id] = true,
    }),
  })

  if not updated then
    return false, "failed to update session"
  end

  return render.render_chunk(updated, chunk_result)
end

---@return boolean ok, string? err
function M.reload()
  local sess = session.current()
  if not sess then
    return false, "no active ffr session"
  end
  -- Invalidate caches for this path on reload
  cache.invalidate_metadata(sess.path)
  cache.clear_content(sess.path)
  local _, err = M.open(sess.path, { preview = sess.mode == "preview", source = sess.source })
  if err then
    return false, err
  end
  return true, nil
end

---@return table|nil info, string? err
function M.info()
  local sess = session.current()
  if not sess then
    return nil, "no active ffr session"
  end
  return sess, nil
end

return M
