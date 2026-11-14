#import <Cocoa/Cocoa.h>
#import <CoreGraphics/CoreGraphics.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>

bool notely_screen_permission(void) { return CGPreflightScreenCaptureAccess(); }
bool notely_request_screen_permission(void) { return CGRequestScreenCaptureAccess(); }
bool notely_modern_capture_available(void) {
    if (@available(macOS 14.0, *)) return true;
    return false;
}
void notely_capture_free(void *memory) { free(memory); }

// Called on a Rust blocking worker, never on the main thread. Completion blocks
// own their state even if the bounded semaphore wait times out.
int notely_capture_image(uint32_t displayID, uint32_t windowID,
                         double x, double y, double width, double height,
                         unsigned char **bytes, size_t *length,
                         uint32_t *pixelWidth, uint32_t *pixelHeight) {
    if ([NSThread isMainThread]) return 5;
    if (@available(macOS 14.0, *)) {
        @autoreleasepool {
            dispatch_semaphore_t done = dispatch_semaphore_create(0);
            __block NSData *jpeg = nil;
            __block int result = 3;
            __block uint32_t outputWidth = 0, outputHeight = 0;
            [SCShareableContent getShareableContentExcludingDesktopWindows:NO onScreenWindowsOnly:YES
                completionHandler:^(SCShareableContent *content, NSError *error) {
                    if (error || !content) {
                        result = [error.domain isEqualToString:SCStreamErrorDomain] && error.code == SCStreamErrorUserDeclined ? 1 : 3;
                        dispatch_semaphore_signal(done); return;
                    }
                    SCContentFilter *filter = nil;
                    CGSize size = CGSizeZero;
                    if (windowID) {
                        for (SCWindow *window in content.windows) {
                            if (window.windowID == windowID) {
                                filter = [[SCContentFilter alloc] initWithDesktopIndependentWindow:window];
                                size = window.frame.size;
                                break;
                            }
                        }
                    } else {
                        for (SCDisplay *display in content.displays) {
                            if (display.displayID != displayID) continue;
                            NSMutableArray<SCRunningApplication *> *excluded = [NSMutableArray array];
                            for (SCRunningApplication *application in content.applications) {
                                if (application.processID == getpid()) [excluded addObject:application];
                            }
                            filter = [[SCContentFilter alloc] initWithDisplay:display excludingApplications:excluded exceptingWindows:@[]];
                            size = display.frame.size;
                            break;
                        }
                    }
                    if (!filter || size.width <= 0 || size.height <= 0) {
                        result = 4; dispatch_semaphore_signal(done); return;
                    }
                    SCStreamConfiguration *config = [[SCStreamConfiguration alloc] init];
                    if (width > 0 && height > 0) {
                        config.sourceRect = CGRectMake(x, y, width, height);
                        size = CGSizeMake(width, height);
                    }
                    // Capture at most 1920px wide, preserving the selected region's aspect ratio.
                    double scale = windowID ? 2.0 : (double)CGDisplayPixelsWide(displayID) / MAX(1.0, CGDisplayBounds(displayID).size.width);
                    config.width = (size_t)MAX(1.0, MIN(1920.0, size.width * scale));
                    config.height = (size_t)MAX(1.0, round(config.width * size.height / size.width));
                    config.showsCursor = NO;
                    [SCScreenshotManager captureImageWithFilter:filter configuration:config
                        completionHandler:^(CGImageRef image, NSError *captureError) {
                            if (!captureError && image) {
                                outputWidth = (uint32_t)CGImageGetWidth(image);
                                outputHeight = (uint32_t)CGImageGetHeight(image);
                                NSBitmapImageRep *bitmap = [[NSBitmapImageRep alloc] initWithCGImage:image];
                                jpeg = [bitmap representationUsingType:NSBitmapImageFileTypeJPEG properties:@{NSImageCompressionFactor: @0.75}];
                                result = jpeg.length > 0 ? 0 : 3;
                            } else if ([captureError.domain isEqualToString:SCStreamErrorDomain] && captureError.code == SCStreamErrorUserDeclined) {
                                result = 1;
                            }
                            dispatch_semaphore_signal(done);
                        }];
                }];
            if (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC))) return 2;
            if (result != 0) return result;
            *bytes = malloc(jpeg.length);
            if (!*bytes) return 3;
            memcpy(*bytes, jpeg.bytes, jpeg.length);
            *length = jpeg.length;
            *pixelWidth = outputWidth;
            *pixelHeight = outputHeight;
            return 0;
        }
    }
    // macOS 13 uses the existing xcap backend after the permission check.
    if (!CGPreflightScreenCaptureAccess()) return 1;
    return 6;
}
