use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
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
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *mut CFMachPort,
        order: i32,
    ) -> *mut CFRunLoopSource;
    fn CFMachPortInvalidate(port: *mut CFMachPort);
    fn CFRelease(cf: *const c_void);

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

    static kCFRunLoopCommonModes: *const c_void;
}

const KCG_HID_EVENT_TAP: u32 = 0;
const KCG_SESSION_EVENT_TAP: u32 = 1;
const KCG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const KCG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;
const KCG_EVENT_KEY_DOWN: u32 = 10;
const KCG_EVENT_KEY_UP: u32 = 11;
const KCG_EVENT_FLAGS_CHANGED: u32 = 12;
const KCG_EVENT_MASK_FOR_ALL_KEYBOARD_EVENTS: u64 =
    (1 << KCG_EVENT_KEY_DOWN) | (1 << KCG_EVENT_KEY_UP) | (1 << KCG_EVENT_FLAGS_CHANGED);

static BLOCKING_ACTIVE: AtomicBool = AtomicBool::new(false);
static mut EVENT_TAP: *mut CFMachPort = ptr::null_mut();
static mut RUN_LOOP_SOURCE: *mut CFRunLoopSource = ptr::null_mut();

extern "C" fn event_tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    _user_info: *mut c_void,
) -> *mut c_void {
    if (event_type == KCG_EVENT_KEY_DOWN
        || event_type == KCG_EVENT_KEY_UP
        || event_type == KCG_EVENT_FLAGS_CHANGED)
        && BLOCKING_ACTIVE.load(Ordering::Relaxed)
    {
        ptr::null_mut()
    } else {
        event
    }
}

struct KeyboardBlockerApp {
    is_blocking: bool,
    start_time: Option<Instant>,
    status_message: String,
    permission_checked: bool,
    has_permissions: bool,
}

impl Default for KeyboardBlockerApp {
    fn default() -> Self {
        Self {
            is_blocking: false,
            start_time: None,
            status_message: "Ready to block keyboard".to_string(),
            permission_checked: false,
            has_permissions: false,
        }
    }
}

impl KeyboardBlockerApp {
    fn check_permissions(&mut self) {
        if !self.permission_checked {
            unsafe {
                self.has_permissions = CGPreflightListenEventAccess();
                self.permission_checked = true;

                if !self.has_permissions {
                    self.status_message = "❌ Accessibility permissions required".to_string();
                    CGRequestListenEventAccess();
                }
            }
        }
    }

    fn create_event_tap(&self) -> bool {
        if !self.has_permissions {
            return false;
        }

        unsafe {
            EVENT_TAP = CGEventTapCreate(
                KCG_HID_EVENT_TAP,
                KCG_HEAD_INSERT_EVENT_TAP,
                KCG_EVENT_TAP_OPTION_DEFAULT,
                KCG_EVENT_MASK_FOR_ALL_KEYBOARD_EVENTS,
                event_tap_callback,
                ptr::null_mut(),
            );

            if EVENT_TAP.is_null() {
                EVENT_TAP = CGEventTapCreate(
                    KCG_SESSION_EVENT_TAP,
                    KCG_HEAD_INSERT_EVENT_TAP,
                    KCG_EVENT_TAP_OPTION_DEFAULT,
                    KCG_EVENT_MASK_FOR_ALL_KEYBOARD_EVENTS,
                    event_tap_callback,
                    ptr::null_mut(),
                );
            }

            if !EVENT_TAP.is_null() {
                RUN_LOOP_SOURCE = CFMachPortCreateRunLoopSource(ptr::null(), EVENT_TAP, 0);
                if !RUN_LOOP_SOURCE.is_null() {
                    let run_loop = CFRunLoopGetCurrent();
                    CFRunLoopAddSource(run_loop, RUN_LOOP_SOURCE, kCFRunLoopCommonModes);
                    return true;
                }
            }
        }

        false
    }

    fn start_blocking(&mut self) {
        if self.create_event_tap() {
            unsafe {
                CGEventTapEnable(EVENT_TAP, true);
            }

            BLOCKING_ACTIVE.store(true, Ordering::Relaxed);
            self.is_blocking = true;
            self.start_time = Some(Instant::now());
            self.status_message = "KEYBOARD BLOCKED".to_string();
        } else {
            self.status_message = "Failed to create event tap - check permissions".to_string();
        }
    }

    fn stop_blocking(&mut self) {
        BLOCKING_ACTIVE.store(false, Ordering::Relaxed);

        unsafe {
            if !EVENT_TAP.is_null() {
                CGEventTapEnable(EVENT_TAP, false);
                CFMachPortInvalidate(EVENT_TAP);
                CFRelease(EVENT_TAP as *const c_void);
                EVENT_TAP = ptr::null_mut();
            }

            if !RUN_LOOP_SOURCE.is_null() {
                CFRelease(RUN_LOOP_SOURCE as *const c_void);
                RUN_LOOP_SOURCE = ptr::null_mut();
            }
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
