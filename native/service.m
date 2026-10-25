// Narrow macOS adapter. Rust owns HID, framing, deadlines and all product logic.
#import <Foundation/Foundation.h>
#import <ServiceManagement/ServiceManagement.h>
#import <Security/Security.h>
#import <Security/AuthSession.h>
#import <SystemConfiguration/SystemConfiguration.h>
#import <xpc/xpc.h>
#import <sys/socket.h>
#import <unistd.h>
#import <fcntl.h>

static NSString *const Plist = @"dev.facundo.ReadyForAirCapture.plist";
static const char *Service = "dev.facundo.ReadyForAirCapture";
extern void aca_reader_session(int fd, uint32_t uid, uint32_t session);
extern bool aca_accept_handshake(uint64_t protocol, const char *build);
extern const char *aca_build_id(void);

static void error_text(char *out, size_t cap, NSString *text) {
    if (cap) snprintf(out, cap, "%s", text.UTF8String ?: "Unknown service error");
}

int aca_service_status(void) {
    @autoreleasepool { return (int)[SMAppService daemonServiceWithPlistName:Plist].status; }
}
int aca_service_register(char *error, size_t cap) {
    @autoreleasepool {
        NSError *problem = nil;
        BOOL ok = [[SMAppService daemonServiceWithPlistName:Plist] registerAndReturnError:&problem];
        if (!ok) error_text(error, cap, problem.description);
        return ok ? 0 : -1;
    }
}
int aca_service_unregister(char *error, size_t cap) {
    @autoreleasepool {
        // Completion, unlike the synchronous variant, confirms that the old process died.
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        __block NSError *problem = nil;
        [[SMAppService daemonServiceWithPlistName:Plist] unregisterWithCompletionHandler:^(NSError *e) {
            problem = e;
            dispatch_semaphore_signal(done);
        }];
        if (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 15 * NSEC_PER_SEC))) {
            error_text(error, cap, @"Service removal did not finish. Check status before retrying; do not replace the app yet.");
            return -1;
        }
        if (problem) { error_text(error, cap, problem.description); return -1; }
        return 0;
    }
}
void aca_service_settings(void) { [SMAppService openSystemSettingsLoginItems]; }

// UID narrows the session only AFTER XPC authenticates the signed peer.
bool aca_session_active(uint32_t uid, uint32_t session) {
    uid_t console = 0; gid_t group = 0;
    CFStringRef user = SCDynamicStoreCopyConsoleUser(NULL, &console, &group);
    if (user) CFRelease(user);
    SessionAttributeBits bits = 0;
    return uid != 0 && console == uid &&
        SessionGetInfo(session, NULL, &bits) == errSecSuccess &&
        (bits & sessionHasGraphicAccess) && !(bits & (sessionIsRemote | sessionIsRoot));
}

static NSString *peer_requirement(NSString *identifier) {
    SecCodeRef code = NULL;
    CFDictionaryRef info = NULL;
    if (SecCodeCopySelf(kSecCSDefaultFlags, &code)) return nil;
    OSStatus result = SecCodeCopySigningInformation(code, kSecCSSigningInformation, &info);
    CFRelease(code);
    if (result) return nil;
    NSString *team = [(__bridge NSDictionary *)info objectForKey:(__bridge NSString *)kSecCodeInfoTeamIdentifier];
    // A hash-only/ad-hoc build may be inspected, but can never register/connect as a trusted product.
    NSCharacterSet *invalid = [[NSCharacterSet characterSetWithCharactersInString:@"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"] invertedSet];
    NSString *requirement = nil;
    if (team.length == 10 && [team rangeOfCharacterFromSet:invalid].location == NSNotFound) {
        requirement = [NSString stringWithFormat:@"anchor apple generic and certificate leaf[subject.OU] = \"%@\" and identifier \"%@\"", team, identifier];
    }
    CFRelease(info);
    return requirement;
}
int aca_stable_identity(void) {
    @autoreleasepool { return peer_requirement(@"dev.facundo.ReadyForAirCapture") != nil; }
}

@interface ACAChannel : NSObject {
@public
    int monitor;
    xpc_connection_t connection;
    NSString *failure;
    int received;
    bool abandoned;
}
- (void)interrupt;
@end
@implementation ACAChannel
- (instancetype)init { if ((self = [super init])) { monitor = -1; received = -1; } return self; }
- (void)interrupt { @synchronized(self) { if (monitor >= 0) shutdown(monitor, SHUT_RDWR); } }
- (void)dealloc { if (monitor >= 0) close(monitor); if (received >= 0) close(received); }
@end

void *aca_channel_connect(int *fd, char *error, size_t cap) {
    @autoreleasepool {
        NSString *requirement = peer_requirement(@"dev.facundo.ReadyForAirCapture");
        if (!requirement) { error_text(error, cap, @"This development build has no stable Apple signing identity. Build a signed ACA.app before enabling its service."); return NULL; }
        ACAChannel *link = [ACAChannel new];
        xpc_connection_t conn = xpc_connection_create_mach_service(Service, NULL, XPC_CONNECTION_MACH_SERVICE_PRIVILEGED);
        link->connection = conn;
        if (xpc_connection_set_peer_code_signing_requirement(conn, requirement.UTF8String)) {
            error_text(error, cap, @"Cannot enforce reader signing requirement"); return NULL;
        }
        xpc_connection_set_event_handler(conn, ^(xpc_object_t event) {
            if (xpc_get_type(event) == XPC_TYPE_ERROR) {
                [link interrupt];
                @synchronized(link) { link->connection = nil; }
            }
        });
        xpc_connection_resume(conn);
        xpc_object_t request = xpc_dictionary_create(NULL, NULL, 0);
        xpc_dictionary_set_uint64(request, "protocol", 3);
        xpc_dictionary_set_string(request, "build", aca_build_id());
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        xpc_connection_send_message_with_reply(conn, request, dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^(xpc_object_t reply) {
            @synchronized(link) {
                if (!link->abandoned) {
                    if (xpc_get_type(reply) != XPC_TYPE_DICTIONARY || xpc_connection_get_euid(conn) != 0) {
                        link->failure = @"The authenticated ACA service is unavailable. Check background approval and retry.";
                    } else if (xpc_dictionary_get_string(reply, "error")) {
                        link->failure = @(xpc_dictionary_get_string(reply, "error"));
                    } else if (!aca_accept_handshake(xpc_dictionary_get_uint64(reply, "protocol"), xpc_dictionary_get_string(reply, "build"))) {
                        link->failure = @"The reader is from another ACA build. Remove its service before enabling this version.";
                    } else {
                        link->received = xpc_dictionary_dup_fd(reply, "channel");
                        if (link->received < 0 || fcntl(link->received, F_SETFD, FD_CLOEXEC) < 0) link->failure = @"Reader returned no safe capture channel";
                    }
                }
            }
            dispatch_semaphore_signal(done);
        });
        long timeout = dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC));
        @synchronized(link) {
            if (timeout || link->failure || link->received < 0) {
                link->abandoned = true;
                xpc_connection_cancel(conn);
                error_text(error, cap, link->failure ?: @"Reader connection timed out. Retry explicitly when ready.");
                return NULL;
            }
            *fd = link->received; link->received = -1;
            link->monitor = fcntl(*fd, F_DUPFD_CLOEXEC, 0);
        }
        return (__bridge_retained void *)link;
    }
}
void aca_channel_drop(void *opaque) {
    ACAChannel *link = (__bridge_transfer ACAChannel *)opaque;
    [link interrupt];
    @synchronized(link) { if (link->connection) xpc_connection_cancel(link->connection); }
}

int aca_service_run(char *error, size_t cap) {
    @autoreleasepool {
        NSString *requirement = peer_requirement(@"dev.facundo.ReadyForAirStudio");
        if (geteuid() != 0 || !requirement) {
            error_text(error, cap, @"The reader must be launched by the signed ACA background service."); return -1;
        }
        dispatch_queue_t queue = dispatch_queue_create("dev.facundo.aca.reader.admission", DISPATCH_QUEUE_SERIAL);
        xpc_connection_t listener = xpc_connection_create_mach_service(Service, queue, XPC_CONNECTION_MACH_SERVICE_LISTENER);
        if (xpc_connection_set_peer_code_signing_requirement(listener, requirement.UTF8String)) {
            error_text(error, cap, @"Cannot enforce client signing requirement"); return -1;
        }
        __block bool occupied = false;
        xpc_connection_set_event_handler(listener, ^(xpc_object_t event) {
            if (xpc_get_type(event) != XPC_TYPE_CONNECTION) return;
            xpc_connection_t peer = event;
            ACAChannel *link = [ACAChannel new];
            link->connection = peer;
            __block bool admitted = false;
            xpc_connection_set_target_queue(peer, queue);
            if (xpc_connection_set_peer_code_signing_requirement(peer, requirement.UTF8String)) {
                xpc_connection_cancel(peer); link->connection = nil; return;
            }
            xpc_connection_set_event_handler(peer, ^(xpc_object_t message) {
                if (xpc_get_type(message) == XPC_TYPE_ERROR) {
                    [link interrupt]; link->connection = nil; return;
                }
                if (xpc_get_type(message) != XPC_TYPE_DICTIONARY) { xpc_connection_cancel(peer); return; }
                xpc_object_t reply = xpc_dictionary_create_reply(message);
                if (!reply) { xpc_connection_cancel(peer); return; }
                uint32_t uid = xpc_connection_get_euid(peer), session = xpc_connection_get_asid(peer);
                const char *rejection = NULL;
                if (xpc_dictionary_get_count(message) != 2) rejection = "Invalid connection request";
                else if (!aca_session_active(uid, session)) rejection = "Only the active local graphical user may capture the remote.";
                else if (!aca_accept_handshake(xpc_dictionary_get_uint64(message, "protocol"), xpc_dictionary_get_string(message, "build"))) rejection = "Reader version mismatch. Remove the service before enabling this ACA build.";
                else if (occupied || admitted) rejection = "The remote reader is already owned by an ACA session. Close it before retrying.";
                int pair[2] = {-1, -1};
                if (!rejection && socketpair(AF_UNIX, SOCK_STREAM, 0, pair)) rejection = "Cannot create capture channel";
                if (rejection) {
                    xpc_dictionary_set_string(reply, "error", rejection);
                    xpc_connection_send_message(peer, reply);
                    return;
                }
                occupied = true; admitted = true;
                link->monitor = fcntl(pair[0], F_DUPFD_CLOEXEC, 0);
                fcntl(pair[0], F_SETFD, FD_CLOEXEC);
                fcntl(pair[1], F_SETFD, FD_CLOEXEC);
                xpc_dictionary_set_uint64(reply, "protocol", 3);
                xpc_dictionary_set_string(reply, "build", aca_build_id());
                xpc_dictionary_set_fd(reply, "channel", pair[1]);
                xpc_connection_send_message(peer, reply);
                close(pair[1]);
                int owned = pair[0];
                dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
                    aca_reader_session(owned, uid, session); // Rust owns and closes this descriptor.
                    dispatch_async(queue, ^{
                        [link interrupt];
                        if (link->connection) xpc_connection_cancel(link->connection);
                        link->connection = nil;
                        occupied = false;
                    });
                });
            });
            xpc_connection_resume(peer);
        });
        xpc_connection_resume(listener);
        dispatch_main();
    }
}
