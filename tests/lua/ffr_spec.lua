--- ffr.nvim test suite
--- Run with: nvim --headless -u tests/lua/minimal_init.lua -c "lua require('plenary.test_harness').test_directory('tests/lua', {minimal_init='tests/lua/minimal_init.lua'})"
--- Or without plenary: nvim --headless -u tests/lua/minimal_init.lua -c "luafile tests/lua/ffr_spec.lua" -c "qa!"

local ok_plenary, _ = pcall(require, "plenary")
local describe, it, assert_
if ok_plenary then
  describe = require("plenary.busted").describe
  it = require("plenary.busted").it
  assert_ = require("luassert")
else
  -- Minimal fallback for running without plenary
  local pass_count = 0
  local fail_count = 0

  describe = function(name, fn)
    print("--- " .. name .. " ---")
    fn()
  end

  it = function(name, fn)
    local ok, err = pcall(fn)
    if ok then
      pass_count = pass_count + 1
      print("  PASS: " .. name)
    else
      fail_count = fail_count + 1
      print("  FAIL: " .. name .. " => " .. tostring(err))
    end
  end

  assert_ = setmetatable({}, {
    __index = function(_, key)
      if key == "are" then
        return setmetatable({}, {
          __index = function(_, subkey)
            if subkey == "same" then
              return function(a, b)
                assert(a == b, string.format("expected %s == %s", tostring(a), tostring(b)))
              end
            elseif subkey == "truthy" then
              return function(a) assert(a, "expected truthy") end
            elseif subkey == "falsy" then
              return function(a) assert(not a, "expected falsy") end
            end
          end,
        })
      elseif key == "is_not" then
        return setmetatable({}, {
          __index = function(_, subkey)
            if subkey == "Nil" then
              return function(a) assert(a ~= nil, "expected not nil") end
            end
          end,
        })
      elseif key == "is_nil" then
        return function(a) assert(a == nil, "expected nil, got " .. tostring(a)) end
      elseif key == "is_true" then
        return function(a) assert(a == true, "expected true") end
      end
    end,
  })

  -- Print summary at end
  vim.schedule(function()
    print(string.format("\n%d passed, %d failed", pass_count, fail_count))
    if fail_count > 0 then
      vim.cmd("cquit 1")
    end
  end)
end

-- =====================
-- Unit tests (no backend needed)
-- =====================

describe("ffr.config", function()
  it("returns defaults when no setup called", function()
    package.loaded["ffr.config"] = nil
    local config = require("ffr.config")
    config.values = nil
    local cfg = config.get()
    assert_.are.same(cfg.chunk_bytes, 64 * 1024)
    assert_.are.same(cfg.full_open_max_bytes, 2 * 1024 * 1024)
    assert_.are.same(cfg.binary_sniff_bytes, 4096)
    assert_.are.same(cfg.max_line_window, 2000)
    assert_.are.same(cfg.minified_line_length_threshold, 1000)
    assert_.are.same(cfg.enable_persistent_metadata_cache, true)
  end)

  it("merges user opts over defaults", function()
    package.loaded["ffr.config"] = nil
    local config = require("ffr.config")
    local cfg = config.setup({ chunk_bytes = 128 * 1024, max_line_window = 500 })
    assert_.are.same(cfg.chunk_bytes, 128 * 1024)
    assert_.are.same(cfg.max_line_window, 500)
    assert_.are.same(cfg.binary_sniff_bytes, 4096) -- unchanged default
  end)

  it("migrates deprecated flat keys to nested schema and warns once", function()
    package.loaded["ffr.config"] = nil
    package.loaded["ffr.config_validation"] = nil
    local validation = require("ffr.config_validation")
    validation._reset_warnings()

    local warnings = {}
    local orig_notify = vim.notify
    vim.notify = function(msg, _lvl, _opts) table.insert(warnings, msg) end

    local config = require("ffr.config")
    local cfg = config.setup({
      watcher_enabled = false,
      chunk_keymaps_enabled = false,
      log_level = "debug",
    })

    vim.notify = orig_notify

    assert_.are.same(cfg.watcher.enabled, false)
    assert_.are.same(cfg.chunk.keymaps.enabled, false)
    assert_.are.same(cfg.logging.level, "debug")
    -- Deprecated keys must have been stripped before merging:
    assert_.is.Nil(cfg.watcher_enabled)
    assert_.is.Nil(cfg.chunk_keymaps_enabled)
    assert_.is.Nil(cfg.log_level)
    -- Three distinct deprecated keys -> three warnings.
    assert_.are.same(#warnings, 3)
  end)

  it("does not re-warn for the same deprecated key in one session", function()
    package.loaded["ffr.config"] = nil
    package.loaded["ffr.config_validation"] = nil
    local validation = require("ffr.config_validation")
    validation._reset_warnings()

    local warnings = {}
    local orig_notify = vim.notify
    vim.notify = function(msg, _lvl, _opts) table.insert(warnings, msg) end

    local config = require("ffr.config")
    config.setup({ watcher_enabled = false })
    config.setup({ watcher_enabled = true })

    vim.notify = orig_notify
    assert_.are.same(#warnings, 1)
  end)

  it("auto-generates metadata_cache_path when nil (LMDB dir or legacy JSON)", function()
    package.loaded["ffr.config"] = nil
    local config = require("ffr.config")
    local cfg = config.setup({})
    assert_.is_not.Nil(cfg.metadata_cache_path)
    -- Default is the LMDB dir unless a legacy JSON file already exists.
    local matches_dir = cfg.metadata_cache_path:match("ffr/metadata%-db$") ~= nil
    local matches_json = cfg.metadata_cache_path:match("ffr/metadata_cache%.json$") ~= nil
    assert(matches_dir or matches_json, "path should end with ffr/metadata-db or legacy ffr/metadata_cache.json")
  end)
end)

describe("ffr.policy", function()
  local policy

  local function reload()
    package.loaded["ffr.config"] = nil
    package.loaded["ffr.policy"] = nil
    local config = require("ffr.config")
    config.setup({})
    policy = require("ffr.policy")
  end

  it("rejects non-file paths", function()
    reload()
    local decision, reason = policy.decide(
      { exists = false, is_file = false, size = 0, mtime = 0 },
      { kind = "text", binary = false },
      {}
    )
    assert_.are.same(decision, "reject_open")
  end)

  it("rejects binary files", function()
    reload()
    local decision, _ = policy.decide(
      { exists = true, is_file = true, size = 100, mtime = 1 },
      { kind = "binary", binary = true, reason = "binary file" },
      {}
    )
    assert_.are.same(decision, "reject_open")
  end)

  it("routes pdf to specialized_handler", function()
    reload()
    local decision, reason = policy.decide(
      { exists = true, is_file = true, size = 100, mtime = 1 },
      { kind = "pdf", binary = true },
      {}
    )
    assert_.are.same(decision, "specialized_handler")
    assert_.are.same(reason, "pdf")
  end)

  it("allows full_text_open for small text", function()
    reload()
    local decision, _ = policy.decide(
      { exists = true, is_file = true, size = 1024, mtime = 1 },
      { kind = "text", binary = false, too_large_for_full_open = false, preview_allowed = true },
      {}
    )
    assert_.are.same(decision, "full_text_open")
  end)

  it("routes large text to chunked_text_open", function()
    reload()
    local decision, _ = policy.decide(
      { exists = true, is_file = true, size = 10 * 1024 * 1024, mtime = 1 },
      { kind = "text", binary = false, too_large_for_full_open = true, preview_allowed = true },
      {}
    )
    assert_.are.same(decision, "chunked_text_open")
  end)

  it("routes minified files to preview_only", function()
    reload()
    local decision, _ = policy.decide(
      { exists = true, is_file = true, size = 1024, mtime = 1 },
      { kind = "minified", binary = false, minified = true, too_large_for_full_open = false, preview_allowed = true },
      {}
    )
    assert_.are.same(decision, "preview_only")
  end)

  it("honors preview=true opt", function()
    reload()
    local decision, _ = policy.decide(
      { exists = true, is_file = true, size = 100, mtime = 1 },
      { kind = "text", binary = false, too_large_for_full_open = false, preview_allowed = true },
      { preview = true }
    )
    assert_.are.same(decision, "preview_only")
  end)

  it("honors initial_mode=chunked from fff", function()
    reload()
    local decision, reason = policy.decide(
      { exists = true, is_file = true, size = 100, mtime = 1 },
      { kind = "text", binary = false, too_large_for_full_open = false, preview_allowed = true },
      { initial_mode = "chunked", source = "fff" }
    )
    assert_.are.same(decision, "chunked_text_open")
    assert(reason:match("initial_mode"), "reason should mention initial_mode")
  end)

  it("honors initial_mode=preview from fff", function()
    reload()
    local decision, _ = policy.decide(
      { exists = true, is_file = true, size = 100, mtime = 1 },
      { kind = "text", binary = false },
      { initial_mode = "preview", source = "fff" }
    )
    assert_.are.same(decision, "preview_only")
  end)

  it("auto initial_mode falls through to standard logic", function()
    reload()
    local decision, _ = policy.decide(
      { exists = true, is_file = true, size = 100, mtime = 1 },
      { kind = "text", binary = false, too_large_for_full_open = false, preview_allowed = true },
      { initial_mode = "auto", source = "fff" }
    )
    assert_.are.same(decision, "full_text_open")
  end)
end)

describe("ffr.cache", function()
  it("stores and retrieves metadata", function()
    package.loaded["ffr.cache"] = nil
    package.loaded["ffr.config"] = nil
    require("ffr.config").setup({})
    local c = require("ffr.cache")

    assert_.is_nil(c.get_metadata("/test/path"))

    c.set_metadata("/test/path", {
      path = "/test/path",
      size = 1024,
      mtime = 1700000000,
      revision = "1024:1700000000",
      classification = { kind = "text", binary = false },
      line_index_ready = false,
      line_count = 50,
    })

    local entry = c.get_metadata("/test/path")
    assert_.is_not.Nil(entry)
    assert_.are.same(entry.size, 1024)
  end)

  it("validates metadata against stat", function()
    package.loaded["ffr.cache"] = nil
    package.loaded["ffr.config"] = nil
    require("ffr.config").setup({})
    local c = require("ffr.cache")

    c.set_metadata("/test/path", {
      path = "/test/path",
      size = 1024,
      mtime = 1700000000,
    })

    assert_.is_true(c.is_metadata_valid("/test/path", { size = 1024, mtime = 1700000000 }))
    assert_.are.falsy(c.is_metadata_valid("/test/path", { size = 2048, mtime = 1700000000 }))
    assert_.are.falsy(c.is_metadata_valid("/test/path", { size = 1024, mtime = 1700000001 }))
  end)

  it("stores and retrieves content chunks", function()
    package.loaded["ffr.cache"] = nil
    package.loaded["ffr.config"] = nil
    require("ffr.config").setup({})
    local c = require("ffr.cache")

    assert_.is_nil(c.get_content_chunk("/test/path", 0))

    c.set_content_chunk("/test/path", 0, { text = "hello", chunk_id = 0 })
    local chunk = c.get_content_chunk("/test/path", 0)
    assert_.is_not.Nil(chunk)
    assert_.are.same(chunk.text, "hello")
  end)

  it("clears content for a path", function()
    package.loaded["ffr.cache"] = nil
    package.loaded["ffr.config"] = nil
    require("ffr.config").setup({})
    local c = require("ffr.cache")

    c.set_content_chunk("/test/path", 0, { text = "hello" })
    c.clear_content("/test/path")
    assert_.is_nil(c.get_content_chunk("/test/path", 0))
  end)

  it("invalidates metadata", function()
    package.loaded["ffr.cache"] = nil
    package.loaded["ffr.config"] = nil
    require("ffr.config").setup({})
    local c = require("ffr.cache")

    c.set_metadata("/x", { size = 1 })
    c.invalidate_metadata("/x")
    assert_.is_nil(c.get_metadata("/x"))
  end)
end)

describe("ffr.session", function()
  it("creates and retrieves sessions", function()
    package.loaded["ffr.session"] = nil
    local s = require("ffr.session")

    local sess = s.create({
      id = "test-1",
      path = "/test",
      mode = "full",
      bufnr = 999,
      classification = {},
      current_chunk = nil,
      loaded_chunks = {},
      line_index_ready = false,
      eof = false,
      revision = "100:1",
    })

    assert_.are.same(s.get("test-1").path, "/test")
    assert_.are.same(s.get_by_bufnr(999).id, "test-1")
    assert_.is_true(s.has("test-1"))
  end)

  it("updates sessions with patch", function()
    package.loaded["ffr.session"] = nil
    local s = require("ffr.session")

    s.create({
      id = "test-2",
      path = "/test",
      mode = "chunked",
      bufnr = 998,
      current_chunk = 0,
      loaded_chunks = { [0] = true },
      eof = false,
    })

    local updated = s.update("test-2", {
      current_chunk = 1,
      eof = true,
    })

    assert_.are.same(updated.current_chunk, 1)
    assert_.is_true(updated.eof)
    assert_.are.same(updated.path, "/test") -- unchanged
  end)

  it("destroys sessions", function()
    package.loaded["ffr.session"] = nil
    local s = require("ffr.session")

    s.create({ id = "test-3", path = "/test", bufnr = 997 })
    assert_.is_true(s.destroy("test-3"))
    assert_.is_nil(s.get("test-3"))
    assert_.is_nil(s.get_by_bufnr(997))
  end)
end)

describe("ffr.types", function()
  it("loads without error", function()
    package.loaded["ffr.types"] = nil
    local types = require("ffr.types")
    assert_.is_not.Nil(types)
  end)
end)

-- =====================
-- Neovim integration tests (buffer/render, no backend needed)
-- =====================

describe("ffr.buffer", function()
  it("creates a scratch buffer with ffr vars", function()
    package.loaded["ffr.buffer"] = nil
    local buf = require("ffr.buffer")

    local bufnr = buf.create_buffer(true, true)
    assert_.is_not.Nil(bufnr)
    assert_.is_true(buf.is_valid(bufnr))

    buf.set_ffr_vars(bufnr, "/test/file.lua", "full", "sess-1")
    assert_.are.same(vim.b[bufnr].ffr_path, "/test/file.lua")
    assert_.are.same(vim.b[bufnr].ffr_mode, "full")
    assert_.are.same(vim.b[bufnr].ffr_session_id, "sess-1")

    vim.api.nvim_buf_delete(bufnr, { force = true })
  end)

  it("sets and retrieves buffer lines", function()
    package.loaded["ffr.buffer"] = nil
    local buf = require("ffr.buffer")

    local bufnr = buf.create_buffer(true, true)
    buf.set_lines(bufnr, { "line1", "line2", "line3" })

    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
    assert_.are.same(#lines, 3)
    assert_.are.same(lines[1], "line1")
    assert_.are.same(lines[3], "line3")

    vim.api.nvim_buf_delete(bufnr, { force = true })
  end)
end)

describe("ffr.render", function()
  it("renders full content into a buffer", function()
    package.loaded["ffr.buffer"] = nil
    package.loaded["ffr.render"] = nil
    local buf = require("ffr.buffer")
    local render = require("ffr.render")

    local bufnr = buf.create_buffer(true, true)
    local session = {
      bufnr = bufnr,
      path = "/test/file.lua",
      mode = "full",
      classification = { likely_filetype = "lua" },
    }
    local result = {
      lines = { "local x = 1", "return x" },
    }

    local ok, err = render.render_full(session, result)
    assert_.is_true(ok)

    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
    assert_.are.same(lines[1], "local x = 1")
    assert_.are.same(lines[2], "return x")

    vim.api.nvim_buf_delete(bufnr, { force = true })
  end)

  it("renders chunk content into a buffer", function()
    package.loaded["ffr.buffer"] = nil
    package.loaded["ffr.render"] = nil
    local buf = require("ffr.buffer")
    local render = require("ffr.render")

    local bufnr = buf.create_buffer(true, true)
    local session = {
      bufnr = bufnr,
      path = "/test/big.log",
      mode = "chunked",
      classification = {},
    }
    local chunk = { text = "chunk line 1\nchunk line 2" }

    local ok, err = render.render_chunk(session, chunk)
    assert_.is_true(ok)

    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
    assert_.are.same(lines[1], "chunk line 1")
    assert_.are.same(lines[2], "chunk line 2")

    vim.api.nvim_buf_delete(bufnr, { force = true })
  end)

  it("renders rejection message", function()
    package.loaded["ffr.buffer"] = nil
    package.loaded["ffr.render"] = nil
    local buf = require("ffr.buffer")
    local render = require("ffr.render")

    local bufnr = buf.create_buffer(true, true)
    local session = { bufnr = bufnr, path = "/test/binary.exe" }

    local ok = render.render_rejection(session, "binary file")
    assert_.is_true(ok)

    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
    assert_.are.same(lines[1], "ffr: open rejected")
    assert_.are.same(lines[3], "binary file")

    vim.api.nvim_buf_delete(bufnr, { force = true })
  end)
end)

describe("ffr.commands", function()
  it("registers all user commands", function()
    package.loaded["ffr.commands"] = nil
    local commands = require("ffr.commands")
    commands.create_user_commands()

    local cmds = vim.api.nvim_get_commands({})
    assert_.is_not.Nil(cmds["FFR"])
    assert_.is_not.Nil(cmds["FFRPreview"])
    assert_.is_not.Nil(cmds["FFRChunkNext"])
    assert_.is_not.Nil(cmds["FFRChunkPrev"])
    assert_.is_not.Nil(cmds["FFRReload"])
    assert_.is_not.Nil(cmds["FFRInfo"])
  end)
end)
