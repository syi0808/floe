#ifndef FLOE_FFI_H
#define FLOE_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct FloeHandle FloeHandle;
typedef struct FloeNativeHostLane FloeNativeHostLane;

FloeHandle *floe_core_open(const char *database_path, char **error_json_out);
FloeHandle *floe_core_open_default(const char *support_directory, char **error_json_out);
/* Read admitted host identity in a schema 1 response envelope. On success,
 * data contains person_id, device_id and runtime_epoch. Free the returned
 * string with floe_string_free, including error envelopes. */
char *floe_core_identity(FloeHandle *handle);
char *floe_core_command_v2(FloeHandle *handle, const char *request_json);
char *floe_core_query_v2(FloeHandle *handle, const char *request_json);
char *floe_core_events_v2(FloeHandle *handle, const char *request_json);
/* Acquire on the core owner thread before its serial product work. Move only
 * this independent lane to the native callback worker. Command/query accept
 * schema 2 native_host.* envelopes; product commands are rejected. A retained
 * lane rejects calls after core closure. Free once after its final call; free
 * interrupts its outstanding acquisitions and never frees/aliases the core. */
FloeNativeHostLane *floe_native_host_acquire(FloeHandle *handle, char **error_json_out);
char *floe_native_host_command_v2(FloeNativeHostLane *lane, const char *request_json);
char *floe_native_host_query_v2(FloeNativeHostLane *lane, const char *request_json);
void floe_native_host_free(FloeNativeHostLane *lane);
uint32_t floe_protocol_version(void);
void floe_string_free(char *value);
void floe_core_free(FloeHandle *handle);

#ifdef __cplusplus
}
#endif

#endif
