use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use log::debug;

use crate::event::{Event, EventType};
use alacritty_terminal::thread;
use tray_icon::icon::{BadIcon, Icon};
use tray_icon::{TrayIcon, TrayIconBuilder};
use winit::event_loop::EventLoopProxy;

pub fn setup_keyboard_hook(proxy: EventLoopProxy<Event>) -> Result<(), global_hotkey::Error> {
    // initialize the hotkeys manager
    let manager = GlobalHotKeyManager::new().unwrap();

    // construct the hotkey
    let hotkey = HotKey::new(Some(Modifiers::ALT), Code::Space);

    // register it
    manager.register(hotkey)?;

    thread::spawn_named("globalHook", move || loop {
        if let Ok(_) = GlobalHotKeyEvent::receiver().recv() {
            match proxy.send_event(Event::new(EventType::GlobalShortcut, None)) {
                Ok(_) => {
                    debug!("Event sent to proxy");
                },
                Err(error) => {
                    debug!("Failed to send event to proxy: {}", error);
                },
            };
        }
    });
    Ok(())
}

pub fn setup_tray() -> Result<TrayIcon, BadIcon> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/extra/tray.ico");
    let icon = Icon::from_path(path, None)?;

    Ok(TrayIconBuilder::new().with_icon(icon).with_tooltip("alacritty-visor").build().unwrap())
}
