#ifndef FLOE_FFI_H
#define FLOE_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct FloeHandle FloeHandle;

FloeHandle *floe_core_open(const char *database_path, char **error_json_out);
char *floe_core_load_day(FloeHandle *handle, const char *request_json);
char *floe_core_execute(FloeHandle *handle, const char *request_json);
uint32_t floe_protocol_version(void);
void floe_string_free(char *value);
void floe_core_free(FloeHandle *handle);

#ifdef __cplusplus
}
#endif

#endif
