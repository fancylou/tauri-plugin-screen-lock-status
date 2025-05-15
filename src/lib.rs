#[cfg(target_os = "linux")]
use zbus::{blocking::Connection, dbus_proxy};

#[cfg(target_os = "windows")]
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::System::{
        LibraryLoader::*,
        RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_ALL_SESSIONS},
    },
    Win32::UI::Input::KeyboardAndMouse::GetActiveWindow,
    Win32::UI::WindowsAndMessaging::*,
};

#[cfg(target_os = "macos")]
extern crate core_foundation;
#[cfg(target_os = "macos")]
extern crate core_graphics;

#[cfg(target_os = "macos")]
use core_foundation::{base::TCFType, base::ToVoid, string::CFString, dictionary::CFDictionary};

use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, WebviewWindow
};
use tauri::Emitter;

#[cfg(target_os = "macos")]
extern "C" {
    fn CGSessionCopyCurrentDictionary() -> core_foundation::dictionary::CFDictionaryRef;
}

//auto gen code
#[cfg(target_os = "linux")]
#[dbus_proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1/session/auto"
)]
trait Session {
    /// LockedHint property
    #[dbus_proxy(property)]
    fn locked_hint(&self) -> zbus::Result<bool>;
}

#[cfg(target_os = "windows")]
fn register_session_notification(hwnd: HWND) {
    unsafe {
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_ALL_SESSIONS);
    }
}

#[cfg(target_os = "windows")]
extern "system" fn wndproc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message as u32 {
            _ => DefWindowProcA(window, message, wparam, lparam),
        }
    }
}

pub static WINDOW_TAURI: OnceLock<WebviewWindow> = OnceLock::new();

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    #[cfg(target_os = "windows")]
    {
        thread::spawn(|| unsafe {
            println!("Start new thread...");
            let instance = GetModuleHandleA(None).unwrap();
            debug_assert!(instance.0 != 0);

            let window_class = s!("window");

            let wc = WNDCLASSA {
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hInstance: instance.into(),
                lpszClassName: window_class,

                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),
                ..Default::default()
            };

            let atom = RegisterClassA(&wc);
            debug_assert!(atom != 0);

            CreateWindowExA(
                WINDOW_EX_STYLE::default(),
                window_class,
                s!("Window"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                instance,
                Some(std::ptr::null()),
            );

            let hwnd = GetActiveWindow();

            ShowWindow(*&hwnd, SW_HIDE);

            let mut message = MSG::default();
            register_session_notification(hwnd);
            while GetMessageA(&mut message, HWND(0), 0, 0).into() {
                if message.message == WM_WTSSESSION_CHANGE {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);

                    match message.wParam.0 as u32 {
                        WTS_SESSION_LOCK => {
                            let _ = WINDOW_TAURI
                                .get()
                                .expect("Error get WINDOW_TAURI")
                                .emit(
                                    "window_screen_lock_status://change_session_status",
                                    "lock",
                                );
                            println!("Locked");
                        }
                        WTS_SESSION_UNLOCK => {
                            let _ = WINDOW_TAURI
                                .get()
                                .expect("Error get WINDOW_TAURI")
                                .emit(
                                    "window_screen_lock_status://change_session_status",
                                    "unlock",
                                );
                            println!("Unlocked");
                        }
                        _ => {}
                    }
                }
                thread::sleep(Duration::from_millis(1000));
            }
        });
    }

    #[cfg(target_os = "linux")]
    {
        thread::spawn(move || {
            println!("Start new thread...");
            let mut flg = false;
            loop {
                let conn = Connection::system().unwrap();
                let proxy = SessionProxyBlocking::new(&conn).unwrap();

                let mut property = proxy.receive_locked_hint_changed();

                match property.next() {
                    Some(pro) => {
                        let current_property = pro.get().unwrap();
                        if flg != current_property {
                            flg = current_property;

                            let window = WINDOW_TAURI.get();

                            match window {
                                Some(_) => {
                                    if flg == true {
                                        let _ = window.expect("Error get WINDOW_TAURI").emit(
                                            "window_screen_lock_status://change_session_status",
                                            "lock",
                                        );
                                        println!("Locked");
                                    } else {
                                        let _ = window.expect("Error get WINDOW_TAURI").emit(
                                            "window_screen_lock_status://change_session_status",
                                            "unlock",
                                        );
                                        println!("Unlocked");
                                    }
                                }
                                None => break,
                            }
                        }
                    }
                    None => break,
                }
                thread::sleep(Duration::from_millis(1000));
            }
        });
    }

    #[cfg(target_os = "macos")]
    {
        thread::spawn(move || {
            println!("Start new thread...");
            let mut flg = false;
            loop {
                unsafe {
                    let session_dictionary_ref = CGSessionCopyCurrentDictionary();
                    let session_dictionary: CFDictionary =
                        CFDictionary::wrap_under_create_rule(session_dictionary_ref);
                    let current_session_property;
                    match session_dictionary
                        .find(CFString::new("CGSSessionScreenIsLocked").to_void())
                    {
                        None => current_session_property = false,
                        Some(_) => current_session_property = true,
                    }
                    if flg != current_session_property {
                        flg = current_session_property;
                        let window = WINDOW_TAURI.get();

                        match window {
                            Some(_) => {
                                if current_session_property == true {
                                    let _ = window.expect("Error get WINDOW_TAURI").emit(
                                        "window_screen_lock_status://change_session_status",
                                        "lock",
                                    );
                                    println!("Locked");
                                } else {
                                    let _ = window.expect("Error get WINDOW_TAURI").emit(
                                        "window_screen_lock_status://change_session_status",
                                        "unlock",
                                    );
                                    println!("Unlocked");
                                }
                            }
                            None => break,
                        }
                    }
                    thread::sleep(Duration::from_millis(1000));
                }
            }
        });
    }
    Builder::new("window_screen_lock_status").build()
}
