#include "bindings/bindings.h"
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

// Returns nil to hide the keyboard accessory view
static UIView* nilInputAccessoryView(id self, SEL _cmd) {
	return nil;
}

// Disables the iOS keyboard accessory bar (Previous/Next/Done) for WKWebView
static void disableKeyboardAccessoryBar(void) {
	// Find WKContentView class (private class in WebKit)
	Class WKContentViewClass = NSClassFromString(@"WKContentView");
	if (!WKContentViewClass) {
		NSLog(@"[KeyboardAccessory] WKContentView class not found");
		return;
	}
	
	// Get the original inputAccessoryView selector
	SEL originalSelector = @selector(inputAccessoryView);
	Method originalMethod = class_getInstanceMethod(WKContentViewClass, originalSelector);
	
	if (!originalMethod) {
		NSLog(@"[KeyboardAccessory] Could not get inputAccessoryView method");
		return;
	}
	
	// Replace the implementation with our nil-returning function
	method_setImplementation(originalMethod, (IMP)nilInputAccessoryView);
	NSLog(@"[KeyboardAccessory] Successfully disabled keyboard accessory bar");
}

int main(int argc, char * argv[]) {
	@autoreleasepool {
		// Disable iOS keyboard accessory bar before starting the app
		disableKeyboardAccessoryBar();
	}
	ffi::start_app();
	return 0;
}
