import UIKit
import WebKit

/// Disables the iOS keyboard accessory bar (Previous/Next/Done) for WKWebView
/// This extension swizzles WKContentView to return nil for inputAccessoryView
@objc public class KeyboardAccessoryDisabler: NSObject {
    
    @objc public static func disable() {
        // Find WKContentView class (private class in WebKit)
        guard let WKContentViewClass: AnyClass = NSClassFromString("WKContentView") else {
            print("[KeyboardAccessoryDisabler] WKContentView class not found")
            return
        }
        
        // Get the original inputAccessoryView selector
        let originalSelector = #selector(getter: UIResponder.inputAccessoryView)
        
        // Get our replacement method
        let swizzledSelector = #selector(KeyboardAccessoryDisabler.nilInputAccessoryView)
        
        guard let originalMethod = class_getInstanceMethod(WKContentViewClass, originalSelector),
              let swizzledMethod = class_getInstanceMethod(KeyboardAccessoryDisabler.self, swizzledSelector) else {
            print("[KeyboardAccessoryDisabler] Could not get methods for swizzling")
            return
        }
        
        // Add our method to WKContentView
        let didAddMethod = class_addMethod(
            WKContentViewClass,
            swizzledSelector,
            method_getImplementation(swizzledMethod),
            method_getTypeEncoding(swizzledMethod)
        )
        
        if didAddMethod {
            // Get the newly added method
            guard let newMethod = class_getInstanceMethod(WKContentViewClass, swizzledSelector) else {
                print("[KeyboardAccessoryDisabler] Could not get added method")
                return
            }
            // Exchange implementations
            method_exchangeImplementations(originalMethod, newMethod)
            print("[KeyboardAccessoryDisabler] Successfully disabled keyboard accessory bar")
        } else {
            print("[KeyboardAccessoryDisabler] Failed to add method - may already be swizzled")
        }
    }
    
    @objc func nilInputAccessoryView() -> UIView? {
        return nil
    }
}
