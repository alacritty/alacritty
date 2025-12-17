use objc2::runtime::AnyObject;
use objc2_foundation::{NSDictionary, NSString, NSUserDefaults, ns_string};

pub mod locale;
pub mod proc;

pub fn disable_autofill() {
    unsafe {
        NSUserDefaults::standardUserDefaults().registerDefaults(
            &NSDictionary::<NSString, AnyObject>::from_slices(
                &[ns_string!("NSAutoFillHeuristicControllerEnabled")],
                &[ns_string!("NO")],
            ),
        );
    }
}

// Implement dock menu using Objective-C runtime.
pub fn setup_dock_menu() {
    use objc2::ffi::sel_registerName;
    use objc2::ffi::{class_addMethod, object_getClass};
    use objc2::{class, msg_send};
    use std::ffi::CString;

    unsafe {
        // Get NSApplication.
        let ns_app_class = class!(NSApplication);
        let app: *mut AnyObject = msg_send![ns_app_class, sharedApplication];

        // Get the current delegate (set by Winit).
        let delegate: *mut AnyObject = msg_send![app, delegate];

        if delegate.is_null() {
            return;
        }

        // Get the class of the existing delegate.
        let delegate_class = object_getClass(delegate.cast());

        // Add applicationDockMenu: method to the existing delegate class.
        let method_name = CString::new("applicationDockMenu:").unwrap();
        let sel = sel_registerName(method_name.as_ptr()).expect("Failed to register selector");

        // Method signature: id (self, SEL, id).
        let types = CString::new("@@:@").unwrap();

        class_addMethod(
            delegate_class.cast_mut(),
            sel,
            std::mem::transmute::<*const (), unsafe extern "C-unwind" fn()>(
                application_dock_menu_impl as *const (),
            ),
            types.as_ptr(),
        );
    }
}

// C function that implements applicationDockMenu: delegate method.
extern "C" fn application_dock_menu_impl(
    _self: *mut AnyObject,
    _cmd: *const std::os::raw::c_void,
    _app: *mut AnyObject,
) -> *mut AnyObject {
    use objc2::{class, msg_send, sel};

    unsafe {
        // Get NSApplication to access windows.
        let ns_app_class = class!(NSApplication);
        let app: *mut AnyObject = msg_send![ns_app_class, sharedApplication];

        // Create menu.
        let ns_menu_class = class!(NSMenu);
        let menu: *mut AnyObject = msg_send![ns_menu_class, alloc];
        let menu: *mut AnyObject = msg_send![menu, init];

        // Get windows array.
        let windows: *mut AnyObject = msg_send![app, windows];
        let count: usize = msg_send![windows, count];

        // Add each window to menu.
        for i in 0..count {
            let window: *mut AnyObject = msg_send![windows, objectAtIndex: i];
            let title: *mut AnyObject = msg_send![window, title];

            // Check if title is not empty.
            let length: usize = msg_send![title, length];
            if length > 0 {
                // Create menu item.
                let ns_menu_item_class = class!(NSMenuItem);
                let item: *mut AnyObject = msg_send![ns_menu_item_class, alloc];
                let item: *mut AnyObject = msg_send![item, init];

                let _: () = msg_send![item, setTitle: title];
                let _: () = msg_send![item, setTarget: window];

                let selector = sel!(makeKeyAndOrderFront:);
                let _: () = msg_send![item, setAction: selector];

                let _: () = msg_send![menu, addItem: item];
            }
        }

        // Add separator if we have windows.
        if count > 0 {
            let separator_class = class!(NSMenuItem);
            let separator: *mut AnyObject = msg_send![separator_class, separatorItem];
            let _: () = msg_send![menu, addItem: separator];
        }

        // Add "New Window" item.
        let ns_menu_item_class = class!(NSMenuItem);
        let new_item: *mut AnyObject = msg_send![ns_menu_item_class, alloc];
        let new_item: *mut AnyObject = msg_send![new_item, init];

        let new_window_title = ns_string!("New Window");
        let _: () = msg_send![new_item, setTitle: new_window_title];
        let _: () = msg_send![menu, addItem: new_item];

        menu
    }
}
