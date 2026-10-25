#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <objc/runtime.h>

@interface ACAKeyboardInvalidation : NSObject
@property(atomic) BOOL invalidated;
@end
@implementation ACAKeyboardInvalidation
@end

typedef struct {
    pid_t pid;
    AXUIElementRef window;
    AXUIElementRef element;
    AXObserverRef observer;
    CFTypeRef activationObserver;
    CFTypeRef sleepObserver;
    CFTypeRef sessionObserver;
    CFTypeRef invalidation;
} ACATarget;

static CFTypeRef attribute(AXUIElementRef element, CFStringRef name) {
    CFTypeRef value = NULL;
    if (AXUIElementCopyAttributeValue(element, name, &value) != kAXErrorSuccess) return NULL;
    return value;
}

void aca_keyboard_target_free(void *raw) {
    ACATarget *target = raw;
    if (!target) return;
    if (target->observer) {
        CFRunLoopRemoveSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(target->observer), kCFRunLoopCommonModes);
        CFRelease(target->observer);
    }
    for (id token in @[target->activationObserver ? (__bridge id)target->activationObserver : NSNull.null,
                       target->sleepObserver ? (__bridge id)target->sleepObserver : NSNull.null,
                       target->sessionObserver ? (__bridge id)target->sessionObserver : NSNull.null]) {
        if (token != NSNull.null) [NSWorkspace.sharedWorkspace.notificationCenter removeObserver:token];
    }
    if (target->invalidation) CFRelease(target->invalidation);
    if (target->activationObserver) CFRelease(target->activationObserver);
    if (target->sleepObserver) CFRelease(target->sleepObserver);
    if (target->sessionObserver) CFRelease(target->sessionObserver);
    if (target->window) CFRelease(target->window);
    if (target->element) CFRelease(target->element);
    free(target);
}

static void focus_changed(AXObserverRef observer, AXUIElementRef element, CFStringRef notification, void *context) {
    (void)observer; (void)element; (void)notification;
    ACAKeyboardInvalidation *status = (__bridge id)((ACATarget *)context)->invalidation;
    status.invalidated = YES;
}
static ACATarget *capture_identity(void) {
    if (!AXIsProcessTrusted()) return NULL;
    NSRunningApplication *front = NSWorkspace.sharedWorkspace.frontmostApplication;
    if (!front || front.processIdentifier == getpid()) return NULL;
    AXUIElementRef app = AXUIElementCreateApplication(front.processIdentifier);
    AXUIElementSetMessagingTimeout(app, 0.08);
    ACATarget *target = calloc(1, sizeof(ACATarget));
    target->pid = front.processIdentifier;
    target->window = (AXUIElementRef)attribute(app, kAXFocusedWindowAttribute);
    target->element = (AXUIElementRef)attribute(app, kAXFocusedUIElementAttribute);
    CFRelease(app);
    if (!target->window || !target->element) {
        aca_keyboard_target_free(target);
        return NULL;
    }
    AXUIElementSetMessagingTimeout(target->element, 0.08);
    CFTypeRef role = attribute(target->element, kAXRoleAttribute);
    CFTypeRef subrole = attribute(target->element, kAXSubroleAttribute);
    bool text = role && (CFEqual(role, kAXTextFieldRole) || CFEqual(role, kAXTextAreaRole) || CFEqual(role, kAXComboBoxRole));
    bool secure = subrole && CFEqual(subrole, CFSTR("AXSecureTextField"));
    if (role) CFRelease(role);
    if (subrole) CFRelease(subrole);
    if (!text || secure) {
        aca_keyboard_target_free(target);
        return NULL;
    }
    return target;
}

void *aca_keyboard_target_capture(void) {
    ACATarget *target = capture_identity();
    if (!target) return NULL;
    ACAKeyboardInvalidation *status = [ACAKeyboardInvalidation new];
    target->invalidation = CFBridgingRetain(status);
    AXUIElementRef app = AXUIElementCreateApplication(target->pid);
    if (AXObserverCreate(target->pid, focus_changed, &target->observer) != kAXErrorSuccess ||
        AXObserverAddNotification(target->observer, app, kAXFocusedUIElementChangedNotification, target) != kAXErrorSuccess ||
        AXObserverAddNotification(target->observer, app, kAXFocusedWindowChangedNotification, target) != kAXErrorSuccess) {
        CFRelease(app);
        aca_keyboard_target_free(target);
        return NULL;
    }
    CFRelease(app);
    CFRunLoopAddSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(target->observer), kCFRunLoopCommonModes);
    NSNotificationCenter *center = NSWorkspace.sharedWorkspace.notificationCenter;
    pid_t pid = target->pid;
    target->activationObserver = CFBridgingRetain([center addObserverForName:NSWorkspaceDidActivateApplicationNotification object:nil queue:nil usingBlock:^(NSNotification *note) {
        NSRunningApplication *app = note.userInfo[NSWorkspaceApplicationKey];
        if (app.processIdentifier != pid) status.invalidated = YES;
    }]);
    target->sleepObserver = CFBridgingRetain([center addObserverForName:NSWorkspaceWillSleepNotification object:nil queue:nil usingBlock:^(NSNotification *note) {
        (void)note; status.invalidated = YES;
    }]);
    target->sessionObserver = CFBridgingRetain([center addObserverForName:NSWorkspaceSessionDidResignActiveNotification object:nil queue:nil usingBlock:^(NSNotification *note) {
        (void)note; status.invalidated = YES;
    }]);
    return target;
}

bool aca_keyboard_target_valid(void *raw) {
    ACATarget *target = raw;
    if (!target || ((__bridge ACAKeyboardInvalidation *)target->invalidation).invalidated) return false;
    ACATarget *current = capture_identity();
    bool valid = current && target && target->pid == current->pid &&
        CFEqual(target->window, current->window) && CFEqual(target->element, current->element);
    aca_keyboard_target_free(current);
    return valid;
}

bool aca_keyboard_target_send(void *raw, const uint16_t *text, size_t length, uint16_t key) {
    ACATarget *target = raw;
    if (!aca_keyboard_target_valid(target)) return false;
    CGEventRef down = CGEventCreateKeyboardEvent(NULL, key, true);
    CGEventRef up = CGEventCreateKeyboardEvent(NULL, key, false);
    if (!down || !up) {
        if (down) CFRelease(down);
        if (up) CFRelease(up);
        return false;
    }
    CGEventSetFlags(down, 0);
    CGEventSetFlags(up, 0);
    if (length) {
        CGEventKeyboardSetUnicodeString(down, length, text);
        CGEventKeyboardSetUnicodeString(up, length, text);
    }
    // Posting has no delivery acknowledgment. Never retry an ambiguous result.
    CGEventPostToPid(target->pid, down);
    CGEventPostToPid(target->pid, up);
    CFRelease(down);
    CFRelease(up);
    return true;
}

static IMP originalCanBecomeKey;
static IMP originalCanBecomeMain;
static BOOL is_keyboard_panel(NSWindow *window) {
    return [window.title isEqualToString:@"ACA keyboard"] || [window.title isEqualToString:@"ACA keyboard experiment"];
}
static BOOL keyboard_can_become_key(id window, SEL selector) {
    return is_keyboard_panel(window) ? NO : ((BOOL (*)(id, SEL))originalCanBecomeKey)(window, selector);
}
static BOOL keyboard_can_become_main(id window, SEL selector) {
    return is_keyboard_panel(window) ? NO : ((BOOL (*)(id, SEL))originalCanBecomeMain)(window, selector);
}
bool aca_keyboard_panel_prepare(const char *title) {
    NSString *name = [NSString stringWithUTF8String:title];
    for (NSWindow *window in NSApp.windows) {
        if (![window.title isEqualToString:name] || ![window isKindOfClass:NSPanel.class]) continue;
        if (!originalCanBecomeKey) {
            Class panel = object_getClass(window);
            originalCanBecomeKey = class_getMethodImplementation(panel, @selector(canBecomeKeyWindow));
            originalCanBecomeMain = class_getMethodImplementation(panel, @selector(canBecomeMainWindow));
            class_replaceMethod(panel, @selector(canBecomeKeyWindow), (IMP)keyboard_can_become_key, "c@:");
            class_replaceMethod(panel, @selector(canBecomeMainWindow), (IMP)keyboard_can_become_main, "c@:");
        }
        ((NSPanel *)window).becomesKeyOnlyIfNeeded = YES;
        ((NSPanel *)window).hidesOnDeactivate = NO;
        return YES;
    }
    return NO;
}

const char *aca_keyboard_qa_data_root(void) {
    NSBundle *bundle = NSBundle.mainBundle;
    if (![bundle.bundleIdentifier containsString:@".verification."]) return NULL;
    NSString *root = [bundle objectForInfoDictionaryKey:@"ACAIsolatedDataRoot"];
    return root.length ? root.UTF8String : "";
}

void aca_keyboard_pointer(double *x, double *y) {
    CGEventRef event = CGEventCreate(NULL);
    CGPoint point = event ? CGEventGetLocation(event) : CGPointZero;
    if (event) CFRelease(event);
    *x = point.x;
    *y = point.y;
}

void aca_keyboard_panel_move(const char *title, double x, double y) {
    NSString *name = [NSString stringWithUTF8String:title];
    for (NSWindow *window in NSApp.windows) {
        if ([window.title isEqualToString:name] && [window isKindOfClass:NSPanel.class]) {
            CGFloat top = NSScreen.screens.firstObject.frame.size.height;
            [window setFrameTopLeftPoint:NSMakePoint(x, top - y)];
            return;
        }
    }
}
