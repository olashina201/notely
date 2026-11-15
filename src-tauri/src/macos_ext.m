#import <Cocoa/Cocoa.h>
#import <objc/runtime.h>

@interface NotelyPanel : NSPanel
@end

@implementation NotelyPanel
- (BOOL)canBecomeKeyWindow {
    return YES;
}
- (BOOL)canBecomeMainWindow {
    return YES;
}
@end

static void (*orig_sendEvent)(id, SEL, NSEvent *) = NULL;
static void (*orig_setLevel)(id, SEL, NSInteger) = NULL;
static void (*orig_setCollectionBehavior)(id, SEL, NSUInteger) = NULL;

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

static void notely_setLevel(id self, SEL _cmd, NSInteger level) {
    // Prevent Tauri/Tao from resetting the level back to floating (3 or 5).
    // Level 1000 (kCGScreenSaverWindowLevel) is required to float above native macOS fullscreen spaces.
    if (level < 1000) {
        level = 1000;
    }
    if (orig_setLevel) {
        orig_setLevel(self, _cmd, level);
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

        // 1. Morph window class to NotelyPanel (subclass of NSPanel)
        Class panelClass = [NotelyPanel class];
        if ([win class] != panelClass) {
            object_setClass(win, panelClass);
        }
        NotelyPanel *panel = (NotelyPanel *)win;

        FILE *f = fopen("/tmp/notely.log", "a");
        if (f) {
            fprintf(f, "[macos_ext] notely_setup_window morphed win=%p to NotelyPanel (isPanel=%d)\n",
                    panel, (int)[panel isKindOfClass:[NSPanel class]]);
            fclose(f);
        }

        static dispatch_once_t onceToken;
        dispatch_once(&onceToken, ^{
            Method mSend = class_getInstanceMethod(panelClass, @selector(sendEvent:));
            if (mSend) {
                const char *types = method_getTypeEncoding(mSend);
                orig_sendEvent = (void (*)(id, SEL, NSEvent *))method_getImplementation(mSend);
                if (!class_addMethod(panelClass, @selector(sendEvent:), (IMP)notely_sendEvent, types)) {
                    class_replaceMethod(panelClass, @selector(sendEvent:), (IMP)notely_sendEvent, types);
                }
            }

            Method mLevel = class_getInstanceMethod(panelClass, @selector(setLevel:));
            if (mLevel) {
                const char *types = method_getTypeEncoding(mLevel);
                orig_setLevel = (void (*)(id, SEL, NSInteger))method_getImplementation(mLevel);
                if (!class_addMethod(panelClass, @selector(setLevel:), (IMP)notely_setLevel, types)) {
                    class_replaceMethod(panelClass, @selector(setLevel:), (IMP)notely_setLevel, types);
                }
            }

            Method mBehavior = class_getInstanceMethod(panelClass, @selector(setCollectionBehavior:));
            if (mBehavior) {
                const char *types = method_getTypeEncoding(mBehavior);
                orig_setCollectionBehavior = (void (*)(id, SEL, NSUInteger))method_getImplementation(mBehavior);
                if (!class_addMethod(panelClass, @selector(setCollectionBehavior:), (IMP)notely_setCollectionBehavior, types)) {
                    class_replaceMethod(panelClass, @selector(setCollectionBehavior:), (IMP)notely_setCollectionBehavior, types);
                }
            }
        });

        // 2. Configure panel properties
        [panel setFloatingPanel:YES];
        [panel setBecomesKeyOnlyIfNeeded:NO];
        [panel setMovable:YES];
        [panel setMovableByWindowBackground:YES];
        [panel setStyleMask:[panel styleMask] | NSWindowStyleMaskNonactivatingPanel];

        NSWindowCollectionBehavior behavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
                                              NSWindowCollectionBehaviorFullScreenAuxiliary |
                                              NSWindowCollectionBehaviorIgnoresCycle;
        [panel setCollectionBehavior:behavior];

        [panel setLevel:1000];
        [panel setHidesOnDeactivate:NO];

        [panel orderFrontRegardless];

        // 3. Keep-alive observer for Space transitions (fullscreen apps like Antigravity IDE and Google Chrome)
        static id spaceObserver = nil;
        if (!spaceObserver) {
            spaceObserver = [[[NSWorkspace sharedWorkspace] notificationCenter]
                addObserverForName:NSWorkspaceActiveSpaceDidChangeNotification
                            object:nil
                             queue:[NSOperationQueue mainQueue]
                        usingBlock:^(NSNotification * _Nonnull __unused note) {
                            if ([panel isVisible]) {
                                [panel orderFrontRegardless];
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
        if (![win isVisible]) return;
        [NSApp activateIgnoringOtherApps:YES];
        [win makeKeyAndOrderFront:nil];
        [win makeKeyWindow];
    }
}
