use cocoa::{
    appkit::{NSMenu, NSMenuItem},
    base::{nil, selector},
    foundation::{NSAutoreleasePool, NSString},
};

pub fn init() {
    unsafe {
        let menubar = NSMenu::new(nil);
        let app_menu_item = NSMenuItem::new(nil).autorelease();

        menubar.addItem_(app_menu_item);

        let title = NSString::alloc(nil).init_str("Test menubar title");
        let key = NSString::alloc(nil).init_str("");
        menubar.addItemWithTitle_action_keyEquivalent(title, selector("TestMenubarSelector:"), key);

        let app_menu = NSMenu::new(nil).autorelease();
        let title = NSString::alloc(nil).init_str("Test app menu title");
        let key = NSString::alloc(nil).init_str("");
        app_menu.addItemWithTitle_action_keyEquivalent(title, selector("TestAppMenuSelector:"), key);
    }
}

