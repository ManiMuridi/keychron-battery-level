//! Native AppKit UI. All Objective-C objects stay on the main thread.
use std::cell::{Cell, OnceCell, RefCell};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSFont, NSImage,
    NSMenu, NSMenuItem, NSPasteboard, NSPasteboardTypeString, NSStatusBar, NSStatusItem,
    NSWorkspace,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSString, NSTimer, NSURL,
    ns_string,
};

use crate::{hid, model::Snapshot};

#[derive(Default)]
struct AppState {
    status_item: OnceCell<Retained<NSStatusItem>>,
    timer: OnceCell<Retained<NSTimer>>,
    snapshot: RefCell<Option<Snapshot>>,
    pending: RefCell<Option<PendingRead>>,
    next_refresh: Cell<Option<Instant>>,
}

struct PendingRead {
    receiver: Receiver<Snapshot>,
    started: Instant,
    timeout_shown: bool,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The delegate and all
    // its UI state are confined to the main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppState]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let item = NSStatusBar::systemStatusBar().statusItemWithLength(-1.0);
            if let Some(button) = item.button(self.mtm()) {
                let icon = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    ns_string!("keyboard"), Some(ns_string!("Keychron Battery")),
                );
                if let Some(icon) = icon {
                    icon.setTemplate(true);
                    button.setImage(Some(&icon));
                }
                button.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(12.0, 0.0)));
            }
            self.ivars().status_item.set(item).expect("status item initialized once");
            self.render(Snapshot::new("checking", hid::secure_input_enabled(self.mtm())));
            self.refresh();
            // SAFETY: tick: is implemented below with the NSTimer signature.
            let timer = unsafe {
                NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                    1.0, self, sel!(tick:), None, true,
                )
            };
            timer.setTolerance(0.2);
            self.ivars().timer.set(timer).expect("timer initialized once");
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _notification: &NSNotification) {
            if let Some(timer) = self.ivars().timer.get() {
                timer.invalidate();
            }
        }
    }

    impl AppDelegate {
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            self.poll_read();
            let secure_input_changed = self.ivars().snapshot.borrow().as_ref()
                .is_some_and(|snapshot| snapshot.secure_input_enabled != hid::secure_input_enabled(self.mtm()));
            if secure_input_changed || self.ivars().next_refresh.get().is_some_and(|next| Instant::now() >= next) {
                self.refresh();
            }
        }

        #[unsafe(method(refreshBattery:))]
        fn refresh_battery(&self, _sender: &NSMenuItem) {
            self.refresh();
        }

        #[unsafe(method(enableBatteryAccess:))]
        fn enable_battery_access(&self, _sender: &NSMenuItem) {
            hid::request_access();
            open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent");
            self.refresh();
        }

        #[unsafe(method(showSecureInputHelp:))]
        fn show_secure_input_help(&self, _sender: &NSMenuItem) {
            let alert = NSAlert::new(self.mtm());
            alert.setMessageText(ns_string!("Battery reads paused by macOS"));
            alert.setInformativeText(ns_string!("Secure Input protects the keyboard while you enter passwords. It can also block the battery reading, even when Input Monitoring is enabled.\n\nFinish entering your password and close any password dialogs. Battery reads resume automatically when Secure Input ends.\n\nIf this remains after unlocking your Mac, try locking and unlocking again. If it persists, save your work and restart macOS."));
            alert.addButtonWithTitle(ns_string!("OK"));
            alert.runModal();
        }

        #[unsafe(method(copyDiagnostics:))]
        fn copy_diagnostics(&self, _sender: &NSMenuItem) {
            if let Some(snapshot) = self.ivars().snapshot.borrow().as_ref()
                && let Ok(json) = serde_json::to_string_pretty(snapshot) {
                    let pasteboard = NSPasteboard::generalPasteboard();
                    pasteboard.clearContents();
                    // SAFETY: Apple's exported string constant is immutable.
                    pasteboard.setString_forType(&NSString::from_str(&json), unsafe { NSPasteboardTypeString });
            }
        }

        #[unsafe(method(openBluetooth:))]
        fn open_bluetooth(&self, _sender: &NSMenuItem) {
            open_url("x-apple.systempreferences:com.apple.BluetoothSettings");
        }

        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: &NSMenuItem) {
            let alert = NSAlert::new(self.mtm());
            alert.setMessageText(ns_string!("Keychron Battery"));
            alert.setInformativeText(&NSString::from_str(&format!(
                "A native Rust menu bar app.\n\nReads standard HID battery fields from connected Keychron keyboards. Some models declare a battery field but never send a reading; those display —. Hold Fn+B on the keyboard for its built-in estimate.\n\nmacOS requires Input Monitoring access for the keyboard interface. This app requests only standard battery reports; it does not record keystrokes or change keyboard settings.\n\nVersion {}", env!("CARGO_PKG_VERSION")
            )));
            alert.addButtonWithTitle(ns_string!("OK"));
            alert.runModal();
        }

        #[unsafe(method(quitApp:))]
        fn quit_app(&self, _sender: &NSMenuItem) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppState::default());
        // SAFETY: NSObject init is the correct initializer for this superclass.
        unsafe { msg_send![super(this), init] }
    }

    fn item(&self, title: &str, action: Option<Sel>) -> Retained<NSMenuItem> {
        // SAFETY: All action selectors supplied here are implemented above with
        // the NSMenuItem signature. The application retains the delegate.
        unsafe {
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.mtm()),
                &NSString::from_str(title),
                action,
                ns_string!(""),
            );
            item.setTarget(Some(self));
            item.setEnabled(action.is_some());
            item
        }
    }

    fn refresh(&self) {
        if self.ivars().pending.borrow().is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.ivars().next_refresh.set(None);
        let secure_input_enabled = hid::secure_input_enabled(self.mtm());
        // Device reads can wait on Bluetooth. Exactly one worker is allowed at
        // a time, including when the user repeatedly clicks Refresh.
        let worker = std::thread::Builder::new()
            .name("battery-reader".into())
            .spawn(move || {
                let snapshot = hid::probe_snapshot("Keychron", secure_input_enabled);
                let _ = sender.send(snapshot);
            });
        match worker {
            Ok(_) => {
                *self.ivars().pending.borrow_mut() = Some(PendingRead {
                    receiver,
                    started: Instant::now(),
                    timeout_shown: false,
                });
            }
            Err(error) => {
                self.show_error(format!("Could not start the battery reader: {error}"));
                self.ivars()
                    .next_refresh
                    .set(Some(Instant::now() + Duration::from_secs(30)));
            }
        }
    }

    fn poll_read(&self) {
        let mut pending = self.ivars().pending.borrow_mut();
        let Some(read) = pending.as_mut() else { return };
        match read.receiver.try_recv() {
            Ok(snapshot) => {
                pending.take();
                drop(pending);
                // A lock or password dialog may change access during the read.
                // Discard that result and read again with the current state.
                if snapshot.secure_input_enabled != hid::secure_input_enabled(self.mtm()) {
                    self.refresh();
                    return;
                }
                self.render(snapshot);
                self.ivars()
                    .next_refresh
                    .set(Some(Instant::now() + Duration::from_secs(30)));
            }
            Err(TryRecvError::Disconnected) => {
                pending.take();
                drop(pending);
                self.show_error("The battery reader stopped unexpectedly.".into());
                self.ivars()
                    .next_refresh
                    .set(Some(Instant::now() + Duration::from_secs(30)));
            }
            Err(TryRecvError::Empty) => {
                if read.started.elapsed() > Duration::from_secs(10) && !read.timeout_shown {
                    read.timeout_shown = true;
                    drop(pending);
                    // Remove the old percentage while a read is stalled. Keep
                    // the worker alive so a late result can still recover.
                    self.show_error(
                        "The keyboard is taking longer than expected to respond.".into(),
                    );
                }
            }
        }
    }

    fn show_error(&self, error: String) {
        self.render(Snapshot {
            error: Some(error),
            ..Snapshot::new("unknown", hid::secure_input_enabled(self.mtm()))
        });
    }

    fn render(&self, snapshot: Snapshot) {
        let Some(status_item) = self.ivars().status_item.get() else {
            return;
        };
        if let Some(button) = status_item.button(self.mtm()) {
            button.setTitle(&NSString::from_str(&format!(" {}", snapshot.menu_title())));
            button.setToolTip(Some(&NSString::from_str(snapshot.summary())));
        }
        let menu = NSMenu::new(self.mtm());
        menu.setAutoenablesItems(false);
        menu.addItem(&self.item("Keychron Battery", None));
        menu.addItem(&NSMenuItem::separatorItem(self.mtm()));
        for device in &snapshot.devices {
            let value = device
                .battery_percent
                .map_or_else(|| "Unavailable".into(), |n| format!("{n}%"));
            menu.addItem(&self.item(
                &format!("{} · {} — {value}", device.name, device.transport),
                None,
            ));
        }
        menu.addItem(&self.item(snapshot.summary(), None));
        if snapshot.devices.iter().any(|d| d.battery_percent.is_none())
            && snapshot.input_monitoring == "granted"
        {
            menu.addItem(&self.item("Hold Fn+B for the keyboard's battery estimate", None));
        }
        menu.addItem(&NSMenuItem::separatorItem(self.mtm()));
        if snapshot.secure_input_enabled {
            menu.addItem(&self.item("Secure Input Help…", Some(sel!(showSecureInputHelp:))));
        }
        if snapshot.needs_access_settings() {
            let title = if snapshot.input_monitoring == "granted" {
                "Review Battery Access…"
            } else {
                "Enable Battery Access…"
            };
            menu.addItem(&self.item(title, Some(sel!(enableBatteryAccess:))));
        }
        menu.addItem(&self.item("Refresh Battery", Some(sel!(refreshBattery:))));
        menu.addItem(&self.item("Copy Diagnostics", Some(sel!(copyDiagnostics:))));
        menu.addItem(&self.item("Bluetooth Settings…", Some(sel!(openBluetooth:))));
        menu.addItem(&NSMenuItem::separatorItem(self.mtm()));
        menu.addItem(&self.item("About Keychron Battery", Some(sel!(showAbout:))));
        let quit = self.item("Quit", Some(sel!(quitApp:)));
        quit.setKeyEquivalent(ns_string!("q"));
        menu.addItem(&quit);
        status_item.setMenu(Some(&menu));
        *self.ivars().snapshot.borrow_mut() = Some(snapshot);
    }
}

fn open_url(url: &str) {
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

pub fn run() {
    let mtm = MainThreadMarker::new().expect("AppKit must start on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
