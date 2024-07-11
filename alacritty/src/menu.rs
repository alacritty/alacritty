use core::fmt;

use crate::{config::Action, event::Event};
use muda::{
    accelerator::{Accelerator, Code, Modifiers}, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu
};
use winit::event_loop::EventLoopBuilder;
#[cfg(target_os = "macos")]
use winit::platform::macos::EventLoopBuilderExtMacOS;
#[cfg(target_os = "windows")]
use winit::platform::windows::EventLoopBuilderExtWindows;

pub struct MenuBar {
    menu: Menu
}

impl MenuBar {
    pub fn new() -> MenuBar {
        MenuBar { menu: Menu::new() }
    }

    pub fn setup_event_loop(&mut self, event_loop_builder: &mut EventLoopBuilder<Event>) {
        #[cfg(target_os = "windows")]
        {
            let menu = self.menu.clone();
            event_loop_builder.with_msg_hook(move |msg| {
                use windows_sys::Win32::UI::WindowsAndMessaging::{TranslateAcceleratorW, MSG};
                unsafe {
                    let msg = msg as *const MSG;
                    let translated = TranslateAcceleratorW((*msg).hwnd, menu.haccel(), msg);
                    translated == 1
                }
            });
        }

        #[cfg(target_os = "macos")]
        event_loop_builder.with_default_menu(false);
    }

    pub fn init(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let app_menu = Submenu::new("App", true);
            let _ = self.menu.append(&app_menu);
            let _ = app_menu.append_items(&[
                &PredefinedMenuItem::about(None, None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ]);
        }

        let new_window_item = &MenuItem::with_id(
            MenuAction::CreateNewWindow,
            "New Window",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyN)),
        );        

        let new_tab_item = &MenuItem::with_id(
            MenuAction::CreateNewTab,
            "New Tab",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyT)),
        );

        let shell_menu =
            Submenu::with_items("&Shell", true, &[new_window_item, new_tab_item]).unwrap();

        let _ = self.menu.append(&shell_menu);

        #[cfg(target_os = "windows")]
        self.menu.init_for_hwnd(window_hwnd);
        #[cfg(target_os = "linux")]
        self.menu.init_for_gtk_window(&gtk_window, Some(&vertical_gtk_box));
        #[cfg(target_os = "macos")]
        self.menu.init_for_nsapp();

        MenuEvent::set_event_handler(Some(MenuBar::handle_event));
    }

    fn handle_event(event: MenuEvent) {
        match event.id {
            _ if event.id == MenuId::from(MenuAction::CreateNewTab) => {
                
            },
            _ if event.id == MenuId::from(MenuAction::CreateNewWindow) => {
                
            },
            _ => {}
        }
    }
    
}

#[derive(Debug)]
enum MenuAction {
    CreateNewTab,
    CreateNewWindow
}

impl fmt::Display for MenuAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)        
    }
}
