#include "floe_ffi.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int count, char **arguments) {
    if (count != 2) return 64;
    char *error = NULL;
    FloeHandle *core = floe_core_open(arguments[1], &error);
    if (!core) {
        if (error) { puts(error); floe_string_free(error); }
        return 1;
    }
    char operation[32];
    char *request = malloc(1048578);
    int result = request ? 0 : 74;
    while (request && fgets(operation, sizeof(operation), stdin)) {
        operation[strcspn(operation, "\n")] = '\0';
        if (!fgets(request, 1048578, stdin) || !strchr(request, '\n')) {
            result = 65;
            break;
        }
        char *response = NULL;
        if (strcmp(operation, "command_v2") == 0) {
            response = floe_core_command_v2(core, request);
        } else if (strcmp(operation, "query_v2") == 0) {
            response = floe_core_query_v2(core, request);
        } else if (strcmp(operation, "events_v2") == 0) {
            response = floe_core_events_v2(core, request);
        } else {
            result = 64;
            break;
        }
        if (!response) {
            result = 1;
            break;
        }
        puts(response);
        fflush(stdout);
        floe_string_free(response);
    }
    floe_core_free(core);
    free(request);
    return result;
}
