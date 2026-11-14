// Standalone native regression test; see README for the compile/run command.
#import <Cocoa/Cocoa.h>
#import <CoreGraphics/CoreGraphics.h>
#include <assert.h>

extern void notely_pre_init(void);
extern void notely_setup_window(void *window);
extern void notely_focus_window(void *window);

static void change_space(void) {
    [[[NSWorkspace sharedWorkspace] notificationCenter]
        postNotificationName:NSWorkspaceActiveSpaceDidChangeNotification object:nil];
    [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
}

int main(void) {
    @autoreleasepool {
        notely_pre_init();
        NSWindow *window = [[NSWindow alloc]
            initWithContentRect:NSMakeRect(100, 100, 200, 100)
                     styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskResizable)
                       backing:NSBackingStoreBuffered defer:NO];
        void *ptr = (__bridge void *)window;

        [window orderFrontRegardless];
        notely_setup_window(ptr);
        assert([window isVisible]);
        assert([window styleMask] & NSWindowStyleMaskResizable);
        [window setContentSize:NSMakeSize(620, 480)];
        assert([window contentRectForFrameRect:[window frame]].size.width == 620);
        assert([window level] == NSFloatingWindowLevel);
        assert([window level] < CGWindowLevelForKey(kCGDockWindowLevelKey));
        assert([window level] < CGWindowLevelForKey(kCGPopUpMenuWindowLevelKey));
        assert([window collectionBehavior] & NSWindowCollectionBehaviorFullScreenAuxiliary);
        // Native/Tauri callers can lower the level; there must be no hidden clamp.
        [window setLevel:NSNormalWindowLevel];
        assert([window level] == NSNormalWindowLevel);
        notely_setup_window(ptr);
        assert([window level] == NSFloatingWindowLevel);

        [window orderOut:nil];
        assert(![window isVisible]);
        // Each of these previously had a path that could reveal the overlay.
        change_space();
        assert(![window isVisible]);
        notely_setup_window(ptr);
        assert(![window isVisible]);
        notely_focus_window(ptr);
        assert(![window isVisible]);

        [window orderFrontRegardless];
        notely_focus_window(ptr);
        assert([window isVisible]);
        change_space();
        assert([window isVisible]);
        [window orderOut:nil];
        puts("PASS: overlay stays below Dock/menus; window levels are not clamped; fullscreen flags and hide/show are preserved.");
    }
    return 0;
}
