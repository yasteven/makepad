//! Request-correlated Android device services. No process-wide network rebinding.
use crate::{cx::Cx, cx_api::CxOsOp};
use std::sync::atomic::{AtomicUsize, Ordering};

pub const CLIPBOARD_TEXT_LIMIT: usize = 65_536;
pub const NETWORK_SNAPSHOT_LIMIT: usize = 131_072;
const REQUEST_LIMIT: usize = 16;
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct ClipboardResult {
    pub request_id: u64,
    /// Copy succeeds with None; paste succeeds with Some(text).
    pub result: Result<Option<String>, String>,
}

impl std::fmt::Debug for ClipboardResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardResult").field("request_id", &self.request_id)
            .field("success", &self.result.is_ok()).finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct NetworkSnapshotResult {
    pub request_id: u64,
    /// JSON array: handle (decimal string), interface, transports, addresses,
    /// routes, default, internet and validated. This is an observation only.
    pub result: Result<String, String>,
}

fn reserve(request_id: u64) -> Result<(), String> {
    if request_id == 0 { return Err("Device service request ID must be nonzero".into()); }
    IN_FLIGHT.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n|
        (n < REQUEST_LIMIT).then_some(n + 1))
        .map(|_| ()).map_err(|_| "Device service queue is full; retry later".into())
}
pub(crate) fn release_request() {
    let _ = IN_FLIGHT.fetch_update(Ordering::AcqRel, Ordering::Acquire,
        |n| n.checked_sub(1));
}

impl Cx {
    /// Enqueue a user-initiated copy. Do not clear source data until its matching
    /// successful ClipboardResult arrives in Actions.
    pub fn request_clipboard_copy(&mut self, request_id: u64, text: &str) -> Result<(), String> {
        if text.len() > CLIPBOARD_TEXT_LIMIT { return Err("Clipboard text exceeds 64 KiB".into()); }
        reserve(request_id)?;
        self.platform_ops.push_back(CxOsOp::AndroidDeviceService { request_id, operation: 0, text: text.into() });
        Ok(())
    }
    /// Enqueue a foreground, user-initiated plain-text paste.
    pub fn request_clipboard_paste(&mut self, request_id: u64) -> Result<(), String> {
        reserve(request_id)?;
        self.platform_ops.push_back(CxOsOp::AndroidDeviceService { request_id, operation: 1, text: String::new() });
        Ok(())
    }
    pub fn request_network_snapshot(&mut self, request_id: u64) -> Result<(), String> {
        reserve(request_id)?;
        self.platform_ops.push_back(CxOsOp::AndroidDeviceService { request_id, operation: 2, text: String::new() });
        Ok(())
    }
}

/// Bind one unconnected socket to a currently available Android Network handle.
/// Call on the socket's owner worker before connect. Failure never falls back to
/// another network. This does not change the process default or other sockets.
pub fn bind_socket_to_network(fd: i32, handle: u64) -> std::io::Result<()> {
    if fd < 0 || handle == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput,
            "A valid socket and explicit nonzero Android network handle are required"));
    }
    #[link(name = "android")]
    extern "C" { fn android_setsocknetwork(network: u64, fd: i32) -> i32; }
    if unsafe { android_setsocknetwork(handle, fd) } == 0 { Ok(()) }
    else { Err(std::io::Error::last_os_error()) }
}
