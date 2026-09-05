#include "floe_ffi.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int count, char **arguments) {
    if (count != 4) return 64;
    FILE *file = fopen(arguments[3], "rb");
    if (!file) return 66;
    if (fseek(file, 0, SEEK_END) != 0) return 74;
    long length = ftell(file);
    if (length < 0 || length > 1048576) return 65;
    rewind(file);
    char *request = calloc((size_t)length + 1, 1);
    if (!request || fread(request, 1, (size_t)length, file) != (size_t)length) return 74;
    fclose(file);
    char *error = NULL;
    FloeHandle *core = floe_core_open(arguments[1], &error);
    if (!core) {
        if (error) { puts(error); floe_string_free(error); }
        free(request);
        return 1;
    }
    char *response = strcmp(arguments[2], "action") == 0
        ? floe_core_calendar_actions(core, request)
        : strcmp(arguments[2], "load") == 0
            ? floe_core_load_day(core, request) : floe_core_execute(core, request);
    if (response) { puts(response); floe_string_free(response); }
    floe_core_free(core);
    free(request);
    return response ? 0 : 1;
}
