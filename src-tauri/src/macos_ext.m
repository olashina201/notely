#import <Cocoa/Cocoa.h>
#import <objc/runtime.h>

static void (*orig_sendEvent)(id, SEL, NSEvent *) = NULL;
static void (*orig_setCollectionBehavior)(id, SEL, NSUInteger) = NULL;

static BOOL notely_yes(id __unused self, SEL __unused _cmd) {
    return YES;
}

static void notely_sendEvent(id self, SEL _cmd, NSEvent *event) {
    if ([event type] == NSEventTypeLeftMouseDown || [event type] == NSEventTypeRightMouseDown) {
        if (![(NSWindow *)self isKeyWindow]) {
            [NSApp activateIgnoringOtherApps:YES];
            [(NSWindow *)self makeKeyWindow];
        }
    }
    if (orig_sendEvent) {
        orig_sendEvent(self, _cmd, event);
    }
}

static void notely_setCollectionBehavior(id self, SEL _cmd, NSUInteger behavior) {
    // Ensure auxiliary fullscreen behavior is never stripped by Tauri window calls
    behavior |= (NSWindowCollectionBehaviorCanJoinAllSpaces |
                 NSWindowCollectionBehaviorFullScreenAuxiliary |
                 NSWindowCollectionBehaviorIgnoresCycle);
    if (orig_setCollectionBehavior) {
        orig_setCollectionBehavior(self, _cmd, behavior);
    }
}

void notely_setup_window(void *ptr) {
    if (!ptr) return;
    @autoreleasepool {
        NSWindow *win = (__bridge NSWindow *)ptr;
        Class cls = [win class];

        FILE *f = fopen("/tmp/notely.log", "a");
        if (f) {
            fprintf(f, "[macos_ext] notely_setup_window configuring win=%p class=%s\n", win, class_getName(cls));
            fclose(f);
        }

        // 1. canBecomeKeyWindow & canBecomeMainWindow on cls
        Method mKey = class_getInstanceMethod(cls, @selector(canBecomeKeyWindow));
        if (mKey) {
            const char *types = method_getTypeEncoding(mKey);
            if (!class_addMethod(cls, @selector(canBecomeKeyWindow), (IMP)notely_yes, types)) {
                class_replaceMethod(cls, @selector(canBecomeKeyWindow), (IMP)notely_yes, types);
            }
        }
        Method mMain = class_getInstanceMethod(cls, @selector(canBecomeMainWindow));
        if (mMain) {
            const char *types = method_getTypeEncoding(mMain);
            if (!class_addMethod(cls, @selector(canBecomeMainWindow), (IMP)notely_yes, types)) {
                class_replaceMethod(cls, @selector(canBecomeMainWindow), (IMP)notely_yes, types);
            }
        }

        static dispatch_once_t onceToken;
        dispatch_once(&onceToken, ^{
            // 2. sendEvent:
            Method mSend = class_getInstanceMethod(cls, @selector(sendEvent:));
            if (mSend) {
                const char *types = method_getTypeEncoding(mSend);
                orig_sendEvent = (void (*)(id, SEL, NSEvent *))method_getImplementation(mSend);
                if (!class_addMethod(cls, @selector(sendEvent:), (IMP)notely_sendEvent, types)) {
                    class_replaceMethod(cls, @selector(sendEvent:), (IMP)notely_sendEvent, types);
                }
            }

            // 4. setCollectionBehavior:
            Method mBehavior = class_getInstanceMethod(cls, @selector(setCollectionBehavior:));
            if (mBehavior) {
                const char *types = method_getTypeEncoding(mBehavior);
                orig_setCollectionBehavior = (void (*)(id, SEL, NSUInteger))method_getImplementation(mBehavior);
                if (!class_addMethod(cls, @selector(setCollectionBehavior:), (IMP)notely_setCollectionBehavior, types)) {
                    class_replaceMethod(cls, @selector(setCollectionBehavior:), (IMP)notely_setCollectionBehavior, types);
                }
            }
        });

        // 5. Add NSWindowStyleMaskNonactivatingPanel (128 = 0x80) to styleMask:
        // Essential on macOS: WindowServer requires NonactivatingPanel styleMask for auxiliary windows to join fullscreen spaces
        [win setStyleMask:[win styleMask] | NSWindowStyleMaskNonactivatingPanel];

        // 6. Set collection behavior:
        NSWindowCollectionBehavior behavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
                                              NSWindowCollectionBehaviorFullScreenAuxiliary |
                                              NSWindowCollectionBehaviorIgnoresCycle;
        [win setCollectionBehavior:behavior];

        // Float above ordinary app windows, below the Dock, menus and OS prompts.
        // Fullscreen visibility comes from collection behavior, not a screensaver-level override.
        [win setLevel:NSFloatingWindowLevel];
        [win setHidesOnDeactivate:NO];

        // Configuring window levels must never undo an explicit hide or steal focus.

        // 9. Observe space transitions: whenever user enters or leaves fullscreen apps or switches spaces,
        // re-order front so Notely is immediately visible on the active space
        static id spaceObserver = nil;
        if (!spaceObserver) {
            spaceObserver = [[[NSWorkspace sharedWorkspace] notificationCenter]
                addObserverForName:NSWorkspaceActiveSpaceDidChangeNotification
                            object:nil
                             queue:[NSOperationQueue mainQueue]
                        usingBlock:^(NSNotification * _Nonnull __unused note) {
                            if ([win isVisible]) {
                                [win orderFrontRegardless];
                            }
                        }];
        }
    }
}

void notely_pre_init(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
    }
}

void notely_focus_window(void *ptr) {
    if (!ptr) return;
    @autoreleasepool {
        NSWindow *win = (__bridge NSWindow *)ptr;
        // A queued webview focus request must not reveal an overlay hidden meanwhile.
        if (![win isVisible]) return;
        [NSApp activateIgnoringOtherApps:YES];
        [win makeKeyAndOrderFront:nil];
        [win makeKeyWindow];
    }
}
