// Jackson Coxson

use std::ffi::{CString, c_char};

use crate::core_device_proxy::AdapterHandle;
use crate::{IdeviceErrorCode, RUNTIME};

/// Connects the adapter to a specific port
///
/// # Arguments
/// * [`handle`] - The adapter handle
/// * [`port`] - The port to connect to
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_connect(
    handle: *mut AdapterHandle,
    port: u16,
) -> IdeviceErrorCode {
    if handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let adapter = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { adapter.connect(port).await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => {
            log::error!("Adapter connect failed: {}", e);
            IdeviceErrorCode::AdapterIOFailed
        }
    }
}

/// Enables PCAP logging for the adapter
///
/// # Arguments
/// * [`handle`] - The adapter handle
/// * [`path`] - The path to save the PCAP file (null-terminated string)
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `path` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_pcap(
    handle: *mut AdapterHandle,
    path: *const c_char,
) -> IdeviceErrorCode {
    if handle.is_null() || path.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let adapter = unsafe { &mut (*handle).0 };
    let c_str = unsafe { CString::from_raw(path as *mut c_char) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };

    let res = RUNTIME.block_on(async move { adapter.pcap(path_str).await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => {
            log::error!("Adapter pcap failed: {}", e);
            IdeviceErrorCode::AdapterIOFailed
        }
    }
}

/// Closes the adapter connection
///
/// # Arguments
/// * [`handle`] - The adapter handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_close(handle: *mut AdapterHandle) -> IdeviceErrorCode {
    if handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let adapter = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { adapter.close().await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => {
            log::error!("Adapter close failed: {}", e);
            IdeviceErrorCode::AdapterIOFailed
        }
    }
}

/// Sends data through the adapter
///
/// # Arguments
/// * [`handle`] - The adapter handle
/// * [`data`] - The data to send
/// * [`length`] - The length of the data
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `data` must be a valid pointer to at least `length` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_send(
    handle: *mut AdapterHandle,
    data: *const u8,
    length: usize,
) -> IdeviceErrorCode {
    if handle.is_null() || data.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let adapter = unsafe { &mut (*handle).0 };
    let data_slice = unsafe { std::slice::from_raw_parts(data, length) };

    let res = RUNTIME.block_on(async move { adapter.psh(data_slice).await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => {
            log::error!("Adapter send failed: {}", e);
            IdeviceErrorCode::AdapterIOFailed
        }
    }
}

/// Receives data from the adapter
///
/// # Arguments
/// * [`handle`] - The adapter handle
/// * [`data`] - Pointer to a buffer where the received data will be stored
/// * [`length`] - Pointer to store the actual length of received data
/// * [`max_length`] - Maximum number of bytes that can be stored in `data`
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `data` must be a valid pointer to at least `max_length` bytes
/// `length` must be a valid pointer to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_recv(
    handle: *mut AdapterHandle,
    data: *mut u8,
    length: *mut usize,
    max_length: usize,
) -> IdeviceErrorCode {
    if handle.is_null() || data.is_null() || length.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let adapter = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { adapter.recv().await });

    match res {
        Ok(received_data) => {
            let received_len = received_data.len();
            if received_len > max_length {
                return IdeviceErrorCode::BufferTooSmall;
            }

            unsafe {
                std::ptr::copy_nonoverlapping(received_data.as_ptr(), data, received_len);
                *length = received_len;
            }

            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => {
            log::error!("Adapter recv failed: {}", e);
            IdeviceErrorCode::AdapterIOFailed
        }
    }
}
