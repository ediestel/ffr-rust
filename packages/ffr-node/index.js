// Node FFI bindings for the ffr-c shared library via koffi.

const koffi = require('koffi');
const { existsSync } = require('node:fs');
const path = require('node:path');

const EXT = { darwin: 'dylib', win32: 'dll' }[process.platform] || 'so';

function resolveLibPath() {
  if (process.env.FFR_LIB_PATH) return process.env.FFR_LIB_PATH;

  const localDev = path.resolve(__dirname, '..', '..', 'target', 'release', `libffr_c.${EXT}`);
  if (existsSync(localDev)) return localDev;

  const platformKey = `${process.platform}-${process.arch}`;
  const pkgMap = {
    'darwin-arm64': '@ffr/bin-darwin-arm64',
    'darwin-x64': '@ffr/bin-darwin-x64',
    'linux-x64': '@ffr/bin-linux-x64-gnu',
    'linux-arm64': '@ffr/bin-linux-arm64-gnu',
    'win32-x64': '@ffr/bin-win32-x64',
    'win32-arm64': '@ffr/bin-win32-arm64',
  };
  const pkg = pkgMap[platformKey];
  if (pkg) {
    try {
      return require.resolve(`${pkg}/libffr_c.${EXT}`);
    } catch {
      /* fall through */
    }
  }
  throw new Error(
    `@ffr/node: no prebuilt library for ${platformKey}. Build locally with 'cargo build --release -p ffr-c', then set FFR_LIB_PATH.`
  );
}

const lib = koffi.load(resolveLibPath());

const ffr_free_string = lib.func('void ffr_free_string(char*)');
const ffr_c_stat = lib.func('char* ffr_c_stat(const char*)');
const ffr_c_classify = lib.func(
  'char* ffr_c_classify(const char*, size_t, uint64_t, size_t)'
);
const ffr_c_read_chunk = lib.func('char* ffr_c_read_chunk(const char*, uint64_t, size_t)');
const ffr_c_read_lines = lib.func('char* ffr_c_read_lines(const char*, size_t, size_t)');
const ffr_c_version = lib.func('char* ffr_c_version()');

function parse(cstrPtr) {
  const text = koffi.decode(cstrPtr, 'char', -1);
  ffr_free_string(cstrPtr);
  if (!text) throw new Error('ffr: empty response');
  try {
    const parsed = JSON.parse(text);
    if (parsed && parsed.error) {
      const err = new Error(parsed.error.message || 'ffr error');
      err.code = parsed.error.code;
      throw err;
    }
    return parsed;
  } catch (e) {
    if (e && e.code) throw e;
    const err = new Error('ffr: invalid JSON response');
    err.raw = text;
    throw err;
  }
}

module.exports = {
  stat(p) {
    return parse(ffr_c_stat(p));
  },
  classify(p, opts = {}) {
    const {
      sniff_bytes = 4096,
      full_open_max_bytes = 2 * 1024 * 1024,
      minified_threshold = 1000,
    } = opts;
    return parse(ffr_c_classify(p, sniff_bytes, BigInt(full_open_max_bytes), minified_threshold));
  },
  readChunk(p, chunkId, chunkBytes = 64 * 1024) {
    return parse(ffr_c_read_chunk(p, BigInt(chunkId), chunkBytes));
  },
  readLines(p, startLine, endLine) {
    return parse(ffr_c_read_lines(p, startLine, endLine));
  },
  version() {
    const ptr = ffr_c_version();
    const text = koffi.decode(ptr, 'char', -1);
    ffr_free_string(ptr);
    return text;
  },
};
