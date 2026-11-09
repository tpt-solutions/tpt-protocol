/*
 * TPT Protocol: decode one industrial frame to TUP JSON.
 *
 * Usage:
 *   char *json = NULL;
 *   int rc = tpt_decode_json("modbus", bytes, len, &json);
 *   if (rc == TPT_OK) { ... use json ... }   // a JSON array of data points
 *   else              { ... json holds the message ... }
 *   tpt_free_string(json);
 *
 * Every string returned through `out` is owned by the caller and must be freed
 * with tpt_free_string, on success and on error.
 *
 * This header is written by hand and checked by the Rust tests in
 * src/ffi.rs. Keep it in step with those functions.
 */
#ifndef TPT_PROTOCOL_H
#define TPT_PROTOCOL_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define TPT_OK 0
#define TPT_ERR_ARGUMENT 1
#define TPT_ERR_DECODE 2

/* Decodes one frame of the named protocol (for example "modbus", "canopen",
 * "matter-thread"). Returns TPT_OK or an error code, and sets *out. */
int tpt_decode_json(const char *protocol, const uint8_t *bytes, size_t len, char **out);

/* Frees a string returned through tpt_decode_json. NULL is accepted. */
void tpt_free_string(char *s);

#ifdef __cplusplus
}
#endif

#endif /* TPT_PROTOCOL_H */
