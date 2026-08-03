use objc2::MainThreadMarker;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSApplication, NSMenu};
use objc2_foundation::{NSDictionary, NSString, NSUserDefaults, ns_string};

pub mod locale;
pub mod proc;

/// Clear all native menu shortcuts to ensure keyboard events reach Alacritty bindings.
pub fn clear_menu_shortcuts() {
    let mtm = MainThreadMarker::new().expect("menu shortcuts must be cleared on the main thread");
    let application = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = application.mainMenu() else {
        return;
    };

    clear_menu_shortcuts_from(&main_menu);
}

fn clear_menu_shortcuts_from(menu: &NSMenu) {
    for index in 0..menu.numberOfItems() {
        let Some(item) = menu.itemAtIndex(index) else {
            continue;
        };

        item.setKeyEquivalent(ns_string!(""));

        if let Some(submenu) = item.submenu() {
            clear_menu_shortcuts_from(&submenu);
        }
    }
}

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
