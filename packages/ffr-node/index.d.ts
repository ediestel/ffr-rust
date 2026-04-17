export interface StatResult {
  exists: boolean;
  is_file: boolean;
  size: number;
  mtime: number;
  readonly: boolean;
}

export interface ClassifyResult {
  kind: string;
  binary: boolean;
  encoding: string | null;
  line_ending: 'crlf' | 'lf' | 'cr' | null;
  estimated_lines: number | null;
  too_large_for_full_open: boolean;
  preview_allowed: boolean;
  reason: string | null;
  likely_filetype: string | null;
  minified: boolean | null;
}

export interface ClassifyOptions {
  sniff_bytes?: number;
  full_open_max_bytes?: number;
  minified_threshold?: number;
}

export interface ReadChunkResult {
  chunk_id: number;
  byte_start: number;
  byte_end: number;
  start_line: number;
  end_line: number;
  eof: boolean;
  text: string;
}

export interface ReadLinesResult {
  start_line: number;
  end_line: number;
  actual_end_line: number;
  eof: boolean;
  lines: string[];
}

export function stat(path: string): StatResult;
export function classify(path: string, opts?: ClassifyOptions): ClassifyResult;
export function readChunk(path: string, chunkId: number, chunkBytes?: number): ReadChunkResult;
export function readLines(path: string, startLine: number, endLine: number): ReadLinesResult;
export function version(): string;
