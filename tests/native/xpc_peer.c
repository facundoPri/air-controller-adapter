// Adversarial native fixture: asks for a channel but never opens a device.
// Run unsigned and then signed with the app's team but a DIFFERENT identifier.
#include <xpc/xpc.h>
#include <dispatch/dispatch.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    xpc_connection_t c = xpc_connection_create_mach_service("dev.facundo.ReadyForAirCapture", NULL, XPC_CONNECTION_MACH_SERVICE_PRIVILEGED);
    xpc_connection_set_event_handler(c, ^(xpc_object_t event) { (void)event; });
    xpc_connection_resume(c);
    xpc_object_t request = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_uint64(request, "protocol", 3);
    xpc_dictionary_set_string(request, "build", argv[1]);
    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    __block int result = 1;
    xpc_connection_send_message_with_reply(c, request, dispatch_get_global_queue(0, 0), ^(xpc_object_t reply) {
        if (reply == XPC_ERROR_PEER_CODE_SIGNING_REQUIREMENT) {
            puts("PASS: peer rejected by XPC code-signing requirement"); result = 0;
        } else if (xpc_get_type(reply) == XPC_TYPE_DICTIONARY) {
            int fd = xpc_dictionary_dup_fd(reply, "channel");
            if (fd >= 0) { close(fd); puts("FAIL: unauthorized peer received a channel"); }
            else puts("INCONCLUSIVE: response was not the code-signing rejection");
        } else { char *description = xpc_copy_description(reply); printf("INCONCLUSIVE: %s\n", description); free(description); }
        dispatch_semaphore_signal(done);
    });
    if (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 3 * NSEC_PER_SEC))) puts("INCONCLUSIVE: timeout");
    xpc_connection_cancel(c);
    return result;
}
