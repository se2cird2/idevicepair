// Jackson Coxson

use idevice::provider::{TcpProvider, UsbmuxdProvider};
use std::ffi::CStr;
use std::os::raw::c_char;

use crate::{IdeviceErrorCode, usbmuxd::UsbmuxdAddrHandle, util};

pub struct TcpProviderHandle(pub TcpProvider);
pub struct UsbmuxdProviderHandle(pub UsbmuxdProvider);

/// Creates a TCP provider for idevice
///
/// # Arguments
/// * [`ip`] - The sockaddr IP to connect to
/// * [`pairing_file`] - The pairing file handle to use
/// * [`label`] - The label to use with the connection
/// * [`provider`] - A pointer to a newly allocated provider
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `ip` must be a valid sockaddr
/// `pairing_file` must never be used again
/// `label` must be a valid Cstr
/// `provider` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_tcp_provider_new(
    ip: *const libc::sockaddr,
    pairing_file: *mut crate::pairing_file::IdevicePairingFile,
    label: *const c_char,
    provider: *mut *mut TcpProviderHandle,
) -> IdeviceErrorCode {
    let addr = match util::c_addr_to_rust(ip) {
        Ok(i) => i,
        Err(e) => {
            return e;
        }
    };
    let label = match unsafe { CStr::from_ptr(label) }.to_str() {
        Ok(l) => l.to_string(),
        Err(e) => {
            log::error!("Invalid label string: {e:?}");
            return IdeviceErrorCode::InvalidString;
        }
    };

    let pairing_file = unsafe { Box::from_raw(pairing_file) };
    let t = TcpProvider {
        addr,
        pairing_file: pairing_file.0,
        label,
    };

    let boxed = Box::new(TcpProviderHandle(t));
    unsafe { *provider = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Frees a TcpProvider handle
///
/// # Arguments
/// * [`provider`] - The provider handle to free
///
/// # Safety
/// `provider` must be a valid pointer to a TcpProvider handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcp_provider_free(provider: *mut TcpProviderHandle) {
    if !provider.is_null() {
        log::debug!("Freeing TCP provider");
        unsafe { drop(Box::from_raw(provider)) };
    }
}

/// Creates a usbmuxd provider for idevice
///
/// # Arguments
/// * [`addr`] - The UsbmuxdAddr handle to connect to
/// * [`tag`] - The tag returned in usbmuxd responses
/// * [`udid`] - The UDID of the device to connect to
/// * [`device_id`] - The muxer ID of the device to connect to
/// * [`pairing_file`] - The pairing file handle to use
/// * [`label`] - The label to use with the connection
/// * [`provider`] - A pointer to a newly allocated provider
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `addr` must be a valid pointer to UsbmuxdAddrHandle created by this library, and never used again
/// `udid` must be a valid CStr
/// `pairing_file` must never be used again
/// `label` must be a valid Cstr
/// `provider` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn usbmuxd_provider_new(
    addr: *mut UsbmuxdAddrHandle,
    tag: u32,
    udid: *const c_char,
    device_id: u32,
    label: *const c_char,
    provider: *mut *mut UsbmuxdProviderHandle,
) -> IdeviceErrorCode {
    let udid = match unsafe { CStr::from_ptr(udid) }.to_str() {
        Ok(u) => u.to_string(),
        Err(e) => {
            log::error!("Invalid UDID string: {e:?}");
            return IdeviceErrorCode::InvalidArgument;
        }
    };

    let label = match unsafe { CStr::from_ptr(label) }.to_str() {
        Ok(l) => l.to_string(),
        Err(e) => {
            log::error!("Invalid UDID string: {e:?}");
            return IdeviceErrorCode::InvalidArgument;
        }
    };

    let addr = unsafe { Box::from_raw(addr) }.0;

    let p = UsbmuxdProvider {
        addr,
        tag,
        udid,
        device_id,
        label,
    };

    let boxed = Box::new(UsbmuxdProviderHandle(p));
    unsafe { *provider = Box::into_raw(boxed) };

    IdeviceErrorCode::IdeviceSuccess
}

/// Frees a UsbmuxdProvider handle
///
/// # Arguments
/// * [`provider`] - The provider handle to free
///
/// # Safety
/// `provider` must be a valid pointer to a UsbmuxdProvider handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn usbmuxd_provider_free(provider: *mut UsbmuxdProviderHandle) {
    if !provider.is_null() {
        unsafe { drop(Box::from_raw(provider)) };
    }
}
