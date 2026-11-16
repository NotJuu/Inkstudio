//! Attach the X11 backend to a real window (run under Xvfb in CI):
//!   Xvfb :99 & DISPLAY=:99 cargo run -p ink-tablet --example x11_attach
#[cfg(all(unix, not(target_os = "macos")))]
fn main() {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, CreateWindowAux, WindowClass};
    let (conn, screen) = x11rb::connect(None).expect("X server");
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id().unwrap();
    conn.create_window(0, win, root, 0, 0, 200, 200, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new()).unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();
    let q = ink_tablet::Queue::default();
    let b = ink_tablet::x11::Backend::attach(win, q.clone());
    println!("XInput2 backend attached: {}", b.is_some());
    assert!(b.is_some());
}
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn main() {}
