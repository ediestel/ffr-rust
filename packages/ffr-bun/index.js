// Bun FFI bindings for the ffr-c shared library.
//
// Resolution order for the native library:
//   1. $FFR_LIB_PATH env override (useful for local dev from ../target/release)
//   2. Platform-specific optional dep @ffr/bin-<platform>
//   3. Fallback: throw with a helpful build hint.

import { dlopen, FFIType, suffix, ptr, CString } from 'bun:ffi';
import { existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const PLATFORM = `${process.platform}-${process.arch}`;

function resolveLibPath() {
  if (process.env.FFR_LIB_PATH) return process.env.FFR_LIB_PATH;

  const localDev = resolve(__dirname, '..', '..', 'target', 'release', `libffr_c.${suffix}`);
  if (existsSync(localDev)) return localDev;

  const pkgMap = {
    'darwin-arm64': '@ffr/bin-darwin-arm64',
    'darwin-x64': '@ffr/bin-darwin-x64',
    'linux-x64': '@ffr/bin-linux-x64-gnu',
    'linux-arm64': '@ffr/bin-linux-arm64-gnu',
    'win32-x64': '@ffr/bin-win32-x64',
    'win32-arm64': '@ffr/bin-win32-arm64',
  };
  const pkg = pkgMap[PLATFORM];
  if (pkg) {
    try {
      return require.resolve(`${pkg}/libffr_c.${suffix}`);
    } catch {
      /* fall through */
    }
  }
  throw new Error(
    `@ffr/bun: no prebuilt library for ${PLATFORM}. Build locally with 'cargo build --release -p ffr-c' from the repo root, then set FFR_LIB_PATH.`
  );
}

const LIB_PATH = resolveLibPath();

const { symbols } = dlopen(LIB_PATH, {
  ffr_free_string: { args: [FFIType.ptr], returns: FFIType.void },
  ffr_c_stat: { args: [FFIType.cstring], returns: FFIType.cstring },
  ffr_c_classify: {
    args: [FFIType.cstring, FFIType.u64, FFIType.u64, FFIType.u64],
    returns: FFIType.cstring,
  },
  ffr_c_read_chunk: {
    args: [FFIType.cstring, FFIType.u64, FFIType.u64],
    returns: FFIType.cstring,
  },
  ffr_c_read_lines: {
    args: [FFIType.cstring, FFIType.u64, FFIType.u64],
    returns: FFIType.cstring,
  },
  ffr_c_version: { args: [], returns: FFIType.cstring },
});

function parseJsonResult(cstring) {
  const text = cstring.toString();
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

export function stat(path) {
  const buf = Buffer.from(path + '\0');
  const result = symbols.ffr_c_stat(buf);
  return parseJsonResult(result);
}

export function classify(path, opts = {}) {
  const {
    sniff_bytes = 4096,
    full_open_max_bytes = 2 * 1024 * 1024,
    minified_threshold = 1000,
  } = opts;
  const buf = Buffer.from(path + '\0');
  const result = symbols.ffr_c_classify(
    buf,
    BigInt(sniff_bytes),
    BigInt(full_open_max_bytes),
    BigInt(minified_threshold)
  );
  return parseJsonResult(result);
}

export function readChunk(path, chunkId, chunkBytes = 64 * 1024) {
  const buf = Buffer.from(path + '\0');
  const result = symbols.ffr_c_read_chunk(buf, BigInt(chunkId), BigInt(chunkBytes));
  return parseJsonResult(result);
}

export function readLines(path, startLine, endLine) {
  const buf = Buffer.from(path + '\0');
  const result = symbols.ffr_c_read_lines(buf, BigInt(startLine), BigInt(endLine));
  return parseJsonResult(result);
}

export function version() {
  return symbols.ffr_c_version().toString();
}

export default { stat, classify, readChunk, readLines, version };
