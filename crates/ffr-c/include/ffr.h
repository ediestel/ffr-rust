/* ffr.h — C header for the ffr-c shared library.
 * Every function that returns char* allocates a fresh UTF-8 JSON string
 * owned by the caller. Free it with `ffr_free_string`.
 *
 * Errors are returned as JSON of the form:
 *   { "error": { "code": "...", "message": "..." } }
 */

#ifndef FFR_H
#define FFR_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

void ffr_free_string(char* ptr);

char* ffr_c_stat(const char* path);
char* ffr_c_classify(const char* path,
                     size_t sniff_bytes,
                     uint64_t full_open_max,
                     size_t minified_threshold);
char* ffr_c_read_chunk(const char* path, uint64_t chunk_id, size_t chunk_bytes);
char* ffr_c_read_lines(const char* path, size_t start_line, size_t end_line);
char* ffr_c_version(void);

#ifdef __cplusplus
}
#endif

#endif /* FFR_H */
