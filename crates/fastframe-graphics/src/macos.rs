//! The dialog as an AppKit alert.

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSApplicationActivationOptions,
    NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{NSString, NSURL};

use crate::Message;

pub(crate) fn show(message: &Message) -> bool {
    let Some(main_thread) = MainThreadMarker::new() else {
        log::warn!("the error dialog can only be shown from the main thread");
        return false;
    };
    // An app whose window never opened may not be active yet; the alert
    // would open behind the window that was in front.
    let _ = NSApplication::sharedApplication(main_thread);
    NSRunningApplication::currentApplication()
        .activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    let alert = NSAlert::new(main_thread);
    alert.setAlertStyle(NSAlertStyle::Critical);
    alert.setMessageText(&NSString::from_str(&message.title));
    alert.setInformativeText(&NSString::from_str(&message.body));
    if message.asks_for_help {
        alert.addButtonWithTitle(&NSString::from_str("Open Help"));
        alert.addButtonWithTitle(&NSString::from_str("Close"));
    }
    alert.runModal() == NSAlertFirstButtonReturn && message.asks_for_help
}

pub(crate) fn open_url(url: &str) {
    let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) else {
        log::warn!("the help page's address is not a URL");
        return;
    };
    if !NSWorkspace::sharedWorkspace().openURL(&url) {
        log::warn!("could not open the help page");
    }
}
