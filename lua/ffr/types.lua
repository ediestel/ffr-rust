local M = {}

-- File classification ----------------------------------------------------

---@alias FFRKind
---| '"text"'
---| '"binary"'
---| '"image"'
---| '"pdf"'
---| '"archive"'
---| '"json"'
---| '"minified"'
---| '"unknown"'

---@alias FFRInitialMode
---| '"auto"'
---| '"full"'
---| '"chunked"'
---| '"preview"'

---@alias FFRLogLevel
---| '"trace"'
---| '"debug"'
---| '"info"'
---| '"warn"'
---| '"error"'

---@class FFRStatResult
---@field exists boolean
---@field is_file boolean
---@field size integer
---@field mtime integer
---@field readonly boolean

---@class FFRClassifyResult
---@field kind FFRKind
---@field binary boolean
---@field encoding string|nil
---@field line_ending '"lf"'|'"crlf"'|'"cr"'|nil
---@field estimated_lines integer|nil
---@field too_large_for_full_open boolean
---@field preview_allowed boolean
---@field reason string|nil
---@field likely_filetype string|nil
---@field minified boolean|nil

---@class FFRReadChunkResult
---@field chunk_id integer
---@field byte_start integer
---@field byte_end integer
---@field start_line integer
---@field end_line integer
---@field eof boolean
---@field text string

---@class FFRReadLinesResult
---@field start_line integer
---@field end_line integer
---@field actual_end_line integer
---@field eof boolean
---@field lines string[]

---@class FFROpenOpts
---@field source string|nil
---@field preview boolean|nil
---@field initial_mode FFRInitialMode|nil

---@class FFRResponse
---@field id integer
---@field ok boolean
---@field protocol_version string
---@field result table|nil
---@field error table|nil

-- Semantic chunking ------------------------------------------------------

---@class FFRSemanticChunk
---@field start_line integer
---@field end_line integer
---@field kind string       tree-sitter node type ("function_item", …)
---@field name string|nil   Extracted identifier, when available

---@class FFRSemanticRecord
---@field revision string   "{size}:{mtime}" when persisted
---@field chunks FFRSemanticChunk[]

-- Specialized handlers ---------------------------------------------------

---@class FFRSpecializedEntry
---@field name string
---@field size integer|nil
---@field is_dir boolean

---@class FFRSpecializedContent
---@field kind '"pdf"'|'"image"'|'"archive"'
---@field summary string
---@field text string|nil
---@field entries FFRSpecializedEntry[]
---@field metadata table[]       List of [key, value] pairs

-- Config schema ----------------------------------------------------------

---@class FFRLoggingConfig
---@field enabled boolean
---@field level FFRLogLevel
---@field file string|nil
---@field max_files integer

---@class FFRWatcherConfig
---@field enabled boolean
---@field debounce_ms integer

---@class FFREncodingsConfig
---@field fallback_order string[]

---@class FFRPreviewConfig
---@field syntax_highlight boolean
---@field line_numbers boolean
---@field max_preview_chunks integer

---@class FFRChunkKeymapsConfig
---@field enabled boolean
---@field next string
---@field prev string

---@class FFRChunkConfig
---@field prefetch_count integer
---@field keymaps FFRChunkKeymapsConfig

---@class FFRSpecializedPdfConfig
---@field enabled boolean
---@field max_pages integer

---@class FFRSpecializedImageConfig
---@field enabled boolean
---@field show_metadata boolean

---@class FFRSpecializedArchiveConfig
---@field enabled boolean
---@field max_entries integer

---@class FFRSpecializedConfig
---@field pdf FFRSpecializedPdfConfig
---@field image FFRSpecializedImageConfig
---@field archive FFRSpecializedArchiveConfig

---@class FFRAccessibilityConfig
---@field enabled boolean
---@field announce_chunks boolean

---@class FFRHighlightsConfig
---@field chunk_boundary string|nil
---@field preview_header string|nil
---@field rejected_banner string|nil
---@field binary_hex string|nil

---@class FFRHooks
---@field on_classify fun(path: string, result: FFRClassifyResult)|nil
---@field on_open fun(path: string, session: table)|nil
---@field on_chunk_load fun(path: string, chunk_id: integer)|nil

---@class FFRConfig
---@field backend_cmd string[]
---@field request_timeout_ms integer
---@field preview_max_bytes integer
---@field full_open_max_bytes integer
---@field chunk_bytes integer
---@field max_line_window integer
---@field binary_sniff_bytes integer
---@field minified_line_length_threshold integer
---@field enable_persistent_metadata_cache boolean
---@field metadata_cache_path string|nil
---@field logging FFRLoggingConfig
---@field watcher FFRWatcherConfig
---@field encodings FFREncodingsConfig
---@field preview FFRPreviewConfig
---@field chunk FFRChunkConfig
---@field specialized FFRSpecializedConfig
---@field accessibility FFRAccessibilityConfig
---@field highlights FFRHighlightsConfig
---@field hooks FFRHooks

-- Health -----------------------------------------------------------------

---@class FFRHealthBackend
---@field available boolean
---@field error string|nil

---@class FFRHealthMetadata
---@field path string|nil
---@field count integer
---@field disk_size integer
---@field error string|nil

---@class FFRHealthWatcher
---@field running boolean
---@field watched_count integer
---@field error string|nil

---@class FFRHealthClassifyRoundtrip
---@field ok boolean
---@field path string|nil
---@field error string|nil

---@class FFRHealthMcpBinary
---@field available boolean
---@field path string|nil
---@field error string|nil

---@class FFRHealthReport
---@field ok boolean
---@field backend FFRHealthBackend
---@field metadata FFRHealthMetadata
---@field watcher FFRHealthWatcher
---@field classify_roundtrip FFRHealthClassifyRoundtrip
---@field mcp_binary FFRHealthMcpBinary

return M
