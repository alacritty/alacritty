use objc2::MainThreadMarker;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
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

/// Add a window menu, which macOS populates with its window and tab management items.
pub fn add_window_menu() {
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = app.mainMenu() else { return };

    let window_menu = NSMenu::initWithTitle(mtm.alloc(), ns_string!("Window"));
    let window_item = NSMenuItem::new(mtm);
    window_item.setSubmenu(Some(&window_menu));
    main_menu.addItem(&window_item);

    app.setWindowsMenu(Some(&window_menu));
}
