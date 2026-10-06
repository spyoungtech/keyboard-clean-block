use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eframe::egui;

#[repr(C)]
pub struct CFRunLoop {
    _private: [u8; 0],
}

#[repr(C)]
pub struct CFRunLoopSource {
    _private: [u8; 0],
}

#[repr(C)]
pub struct CFMachPort {
    _private: [u8; 0],
}

#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CFRunLoopGetCurrent() -> *mut CFRunLoop;
    fn CFRunLoopAddSource(rl: *mut CFRunLoop, source: *mut CFRunLoopSource, mode: *const c_void);
    fn CFRunLoopRemoveSource(rl: *mut CFRunLoop, source: *mut CFRunLoopSource, mode: *const c_void);
    fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source_handled: bool) -> i32;
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *mut CFMachPort,
        order: i32,
    ) -> *mut CFRunLoopSource;
    fn CFMachPortInvalidate(port: *mut CFMachPort);
    fn CFRelease(cf: *const c_void);
    static kCFRunLoopDefaultMode: *const c_void;

    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
        user_info: *mut c_void,
    ) -> *mut CFMachPort;
    fn CGEventTapEnable(tap: *mut CFMachPort, enable: bool);
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGPreflightPostEventAccess() -> bool;
    fn CGRequestPostEventAccess() -> bool;
}

const KCG_HID_EVENT_TAP: u32 = 0;
const KCG_SESSION_EVENT_TAP: u32 = 1;
const KCG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const KCG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;
const KCG_EVENT_KEY_DOWN: u32 = 10;
const KCG_EVENT_KEY_UP: u32 = 11;
const KCG_EVENT_FLAGS_CHANGED: u32 = 12;
const KCG_EVENT_SYSTEM_DEFINED: u32 = 14; // For media/volume keys
// Sent to the callback when macOS disables the tap (callback too slow, or user input)
const KCG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;
const KCG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFFFFFF;
const KCG_EVENT_MASK_FOR_ALL_KEYBOARD_EVENTS: u64 = (1 << KCG_EVENT_KEY_DOWN)
    | (1 << KCG_EVENT_KEY_UP)
    | (1 << KCG_EVENT_FLAGS_CHANGED)
    | (1 << KCG_EVENT_SYSTEM_DEFINED); // Add system defined events

static BLOCKING_ACTIVE: AtomicBool = AtomicBool::new(false);
static EVENT_TAP: AtomicPtr<CFMachPort> = AtomicPtr::new(ptr::null_mut());

extern "C" fn event_tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    _user_info: *mut c_void,
) -> *mut c_void {
    if event_type == KCG_EVENT_TAP_DISABLED_BY_TIMEOUT
        || event_type == KCG_EVENT_TAP_DISABLED_BY_USER_INPUT
    {
        // macOS turned the tap off; turn it back on or all further input gets through
        let tap = EVENT_TAP.load(Ordering::Relaxed);
        if !tap.is_null() && BLOCKING_ACTIVE.load(Ordering::Relaxed) {
            unsafe { CGEventTapEnable(tap, true) };
        }
        return event;
    }

    if (event_type == KCG_EVENT_KEY_DOWN ||
        event_type == KCG_EVENT_KEY_UP ||
        event_type == KCG_EVENT_FLAGS_CHANGED ||
        event_type == KCG_EVENT_SYSTEM_DEFINED) &&
        BLOCKING_ACTIVE.load(Ordering::Relaxed)
    {
        ptr::null_mut()
    } else {
        event
    }
}

unsafe fn create_tap(location: u32) -> *mut CFMachPort {
    unsafe {
        CGEventTapCreate(
            location,
            KCG_HEAD_INSERT_EVENT_TAP,
            KCG_EVENT_TAP_OPTION_DEFAULT,
            KCG_EVENT_MASK_FOR_ALL_KEYBOARD_EVENTS,
            event_tap_callback,
            ptr::null_mut(),
        )
    }
}

/// Runs the event tap on its own thread and run loop, so a busy UI thread
/// can't delay the callback long enough for macOS to disable the tap.
fn spawn_tap_thread(stop: Arc<AtomicBool>) -> Option<JoinHandle<()>> {
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || unsafe {
        // Try HID event tap first, fall back to session event tap
        let mut tap = create_tap(KCG_HID_EVENT_TAP);
        if tap.is_null() {
            tap = create_tap(KCG_SESSION_EVENT_TAP);
        }
        if tap.is_null() {
            let _ = tx.send(false);
            return;
        }

        let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
        if source.is_null() {
            CFMachPortInvalidate(tap);
            CFRelease(tap as *const c_void);
            let _ = tx.send(false);
            return;
        }

        let run_loop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(run_loop, source, kCFRunLoopDefaultMode);
        EVENT_TAP.store(tap, Ordering::Relaxed);
        CGEventTapEnable(tap, true);
        let _ = tx.send(true);

        while !stop.load(Ordering::Relaxed) {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);
        }

        EVENT_TAP.store(ptr::null_mut(), Ordering::Relaxed);
        CGEventTapEnable(tap, false);
        CFRunLoopRemoveSource(run_loop, source, kCFRunLoopDefaultMode);
        CFMachPortInvalidate(tap);
        CFRelease(source as *const c_void);
        CFRelease(tap as *const c_void);
    });

    if rx.recv().unwrap_or(false) {
        Some(handle)
    } else {
        let _ = handle.join();
        None
    }
}

struct KeyboardBlockerApp {
    is_blocking: bool,
    start_time: Option<Instant>,
    status_message: String,
    permission_checked: bool,
    has_permissions: bool,
    tap_stop: Arc<AtomicBool>,
    tap_thread: Option<JoinHandle<()>>,
}

impl Default for KeyboardBlockerApp {
    fn default() -> Self {
        Self {
            is_blocking: false,
            start_time: None,
            status_message: "Ready to block keyboard".to_string(),
            permission_checked: false,
            has_permissions: false,
            tap_stop: Arc::new(AtomicBool::new(false)),
            tap_thread: None,
        }
    }
}

impl KeyboardBlockerApp {
    fn check_permissions(&mut self) {
        // Blocking (not just observing) events needs both Input Monitoring and Accessibility
        let granted = unsafe { CGPreflightListenEventAccess() && CGPreflightPostEventAccess() };

        if !self.permission_checked {
            self.permission_checked = true;
            if !granted {
                self.status_message = "Accessibility permissions required".to_string();
                unsafe {
                    CGRequestListenEventAccess();
                    CGRequestPostEventAccess();
                }
            }
        } else if granted && !self.has_permissions {
            self.status_message = "Ready to block keyboard".to_string();
        }

        self.has_permissions = granted;
    }

    fn start_blocking(&mut self) {
        if !self.has_permissions {
            return;
        }

        let stop = Arc::new(AtomicBool::new(false));
        BLOCKING_ACTIVE.store(true, Ordering::Relaxed);

        if let Some(handle) = spawn_tap_thread(stop.clone()) {
            self.tap_stop = stop;
            self.tap_thread = Some(handle);
            self.is_blocking = true;
            self.start_time = Some(Instant::now());
            self.status_message = "KEYBOARD BLOCKED".to_string();
        } else {
            BLOCKING_ACTIVE.store(false, Ordering::Relaxed);
            self.status_message = "Failed to create event tap - check permissions".to_string();
        }
    }

    fn stop_blocking(&mut self) {
        BLOCKING_ACTIVE.store(false, Ordering::Relaxed);
        self.tap_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.tap_thread.take() {
            let _ = handle.join();
        }

        self.is_blocking = false;
        self.start_time = None;
        self.status_message = "Keyboard input restored".to_string();
    }

    fn get_remaining_time(&self) -> u64 {
        if let Some(start_time) = self.start_time {
            let elapsed = start_time.elapsed().as_secs();
            if elapsed >= 30 {
                0
            } else {
                30 - elapsed
            }
        } else {
            0
        }
    }
}

impl eframe::App for KeyboardBlockerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_permissions();

        if self.is_blocking && self.get_remaining_time() == 0 {
            self.stop_blocking();
        }

        ctx.request_repaint_after(Duration::from_secs(1));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);

                ui.heading("Keyboard Clean Block");
                ui.add_space(20.0);

                ui.label(&self.status_message);
                ui.add_space(10.0);

                if self.is_blocking {
                    let remaining = self.get_remaining_time();
                    ui.label(format!("Time remaining: {} seconds", remaining));
                } else if self.has_permissions {
                    ui.label("Ready to block keyboard input");
                } else {
                    ui.label("Grant accessibility permissions in System Preferences");
                }
                ui.add_space(20.0);

                let button_text = if self.is_blocking {
                    "Stop Blocking"
                } else {
                    "Start Blocking (30s)"
                };

                let button_enabled = self.has_permissions;

                ui.add_enabled_ui(button_enabled, |ui| {
                    if ui.button(button_text).clicked() {
                        if self.is_blocking {
                            self.stop_blocking();
                        } else {
                            self.start_blocking();
                        }
                    }
                });

                ui.add_space(30.0);

                ui.separator();
                ui.add_space(10.0);

                ui.label("Instructions:");
                ui.label("• Grant accessibility permissions when prompted");
                ui.label("• Click 'Start Blocking' to disable keyboard for 30 seconds");
                ui.label("• Perfect for cleaning your keyboard safely");
                ui.label("• Click 'Stop Blocking' to restore input early");

                if !self.has_permissions {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);
                    ui.label("To grant permissions:");
                    ui.label("1. Go to System Preferences → Security & Privacy");
                    ui.label("2. Click Privacy → Accessibility");
                    ui.label("3. Add this app and check the box");
                }
            });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.is_blocking {
            self.stop_blocking();
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 500.0])
            .with_resizable(false)
            .with_icon(std::sync::Arc::new(egui::IconData::default())),
        ..Default::default()
    };

    eframe::run_native(
        "Keyboard Clean Block",
        options,
        Box::new(|_cc| Ok(Box::new(KeyboardBlockerApp::default()))),
    )
}
