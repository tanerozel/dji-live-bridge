#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <SystemExtensions/SystemExtensions.h>

static NSString *const DJICameraExtensionIdentifier = @"com.djilivebridge.desktop.camera";

typedef void (*DJIExtensionEventCallback)(const char *event, const char *message);
static DJIExtensionEventCallback gEventCallback = NULL;

void dji_set_camera_extension_callback(DJIExtensionEventCallback callback) {
    gEventCallback = callback;
}

static void fireEvent(const char *event, NSString *message) {
    if (gEventCallback) {
        gEventCallback(event, message.UTF8String ?: "");
    }
}

@interface DJISystemExtensionDelegate : NSObject <OSSystemExtensionRequestDelegate>
@end

@implementation DJISystemExtensionDelegate

- (OSSystemExtensionReplacementAction)request:(OSSystemExtensionRequest *)request
                    actionForReplacingExtension:(OSSystemExtensionProperties *)existing
                                  withExtension:(OSSystemExtensionProperties *)extension {
    return OSSystemExtensionReplacementActionReplace;
}

- (void)requestNeedsUserApproval:(OSSystemExtensionRequest *)request {
    NSLog(@"DJI Live Bridge camera extension is waiting for user approval");
    dispatch_async(dispatch_get_main_queue(), ^{
        NSURL *url = [NSURL URLWithString:@"x-apple.systempreferences:com.apple.preference.security?Privacy_SystemExtensions"];
        [[NSWorkspace sharedWorkspace] openURL:url];
    });
    fireEvent("needs_approval", @"System Settings opened — allow DJI Live Bridge Camera under Privacy & Security.");
}

- (void)request:(OSSystemExtensionRequest *)request didFinishWithResult:(OSSystemExtensionRequestResult)result {
    NSLog(@"DJI Live Bridge camera extension request finished: %ld", (long)result);
    if (result == OSSystemExtensionRequestCompleted) {
        fireEvent("activated", @"Camera extension activated successfully.");
    } else if (result == OSSystemExtensionRequestWillCompleteAfterReboot) {
        fireEvent("needs_reboot", @"Camera extension will activate after reboot.");
    }
}

- (void)request:(OSSystemExtensionRequest *)request didFailWithError:(NSError *)error {
    NSLog(@"DJI Live Bridge camera extension request failed: %@", error);
    NSString *message = [NSString stringWithFormat:@"code=%ld domain=%@ desc=%@",
        (long)error.code, error.domain, error.localizedDescription];
    fireEvent("failed", message);
}

@end

static DJISystemExtensionDelegate *cameraExtensionDelegate;

void dji_request_camera_extension_activation(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        NSString *bundlePath = [[NSBundle mainBundle] bundlePath];
        NSString *extPath = [bundlePath stringByAppendingPathComponent:
            @"Contents/Library/SystemExtensions/com.djilivebridge.desktop.camera.systemextension"];
        BOOL exists = [[NSFileManager defaultManager] fileExistsAtPath:extPath];
        if (!exists) {
            fireEvent("failed", [NSString stringWithFormat:@"Pre-check: extension not found. mainBundle=%@ extPath=%@", bundlePath, extPath]);
            return;
        }
        cameraExtensionDelegate = [DJISystemExtensionDelegate new];
        OSSystemExtensionRequest *request =
            [OSSystemExtensionRequest activationRequestForExtension:DJICameraExtensionIdentifier
                                                               queue:dispatch_get_main_queue()];
        request.delegate = cameraExtensionDelegate;
        [[OSSystemExtensionManager sharedManager] submitRequest:request];
    });
}
