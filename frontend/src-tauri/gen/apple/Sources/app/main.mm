#include "bindings/bindings.h"
#import <UIKit/UIKit.h>

// Import Swift bridging header for KeyboardAccessoryDisabler
#if __has_include("octo-Swift.h")
#import "octo-Swift.h"
#elif __has_include("app-Swift.h")
#import "app-Swift.h"
#endif

int main(int argc, char * argv[]) {
	@autoreleasepool {
		// Disable iOS keyboard accessory bar before starting the app
		[KeyboardAccessoryDisabler disable];
	}
	ffi::start_app();
	return 0;
}
