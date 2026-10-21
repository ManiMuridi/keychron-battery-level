//! Read only documented HID battery usages. Never write a device report.
use std::{ffi::c_void, ptr};

use core_foundation::{
    base::{CFType, TCFType},
    number::CFNumber,
    string::CFString,
};

use crate::model::{BatteryElement, Device, Snapshot, measured_percent};

type Ref = *const c_void;
const NOT_PERMITTED: u32 = 0xe000_02e2;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

pub fn secure_input_enabled(_mtm: objc2::MainThreadMarker) -> bool {
    // SAFETY: Carbon marks this read-only API as not thread safe. The marker
    // confines every call to the main thread; its return type is Boolean (u8).
    unsafe { IsSecureEventInputEnabled() != 0 }
}

fn read_error(result: i32) -> String {
    if result as u32 == NOT_PERMITTED {
        "kIOReturnNotPermitted (IOKit 0xe00002e2)".into()
    } else {
        format!("IOKit 0x{:08x}", result as u32)
    }
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOHIDManagerCreate(allocator: Ref, options: u32) -> Ref;
    fn IOHIDManagerSetDeviceMatching(manager: Ref, matching: Ref);
    fn IOHIDManagerCopyDevices(manager: Ref) -> Ref;
    fn IOHIDDeviceGetProperty(device: Ref, key: Ref) -> Ref;
    fn IOHIDDeviceOpen(device: Ref, options: u32) -> i32;
    fn IOHIDDeviceClose(device: Ref, options: u32) -> i32;
    fn IOHIDDeviceCopyMatchingElements(device: Ref, matching: Ref, options: u32) -> Ref;
    fn IOHIDElementGetUsagePage(element: Ref) -> u32;
    fn IOHIDElementGetUsage(element: Ref) -> u32;
    fn IOHIDElementGetType(element: Ref) -> u32;
    fn IOHIDElementGetReportID(element: Ref) -> u32;
    fn IOHIDElementGetLogicalMin(element: Ref) -> isize;
    fn IOHIDElementGetLogicalMax(element: Ref) -> isize;
    fn IOHIDDeviceGetValue(device: Ref, element: Ref, value: *mut Ref) -> i32;
    fn IOHIDDeviceGetValueWithOptions(
        device: Ref,
        element: Ref,
        value: *mut Ref,
        options: u32,
    ) -> i32;
    fn IOHIDValueGetIntegerValue(value: Ref) -> isize;
    fn IOHIDValueGetTimeStamp(value: Ref) -> u64;
    fn IOHIDCheckAccess(request_type: u32) -> u32;
    fn IOHIDRequestAccess(request_type: u32) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: Ref);
    fn CFSetGetCount(set: Ref) -> isize;
    fn CFSetGetValues(set: Ref, values: *mut Ref);
    fn CFArrayGetCount(array: Ref) -> isize;
    fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
}

/// Owns a +1 Core Foundation reference returned by a Create or Copy API.
struct Owned(Ref);

/// A non-exclusive open; the system continues receiving keyboard input.
struct OpenDevice(Ref);

impl Drop for OpenDevice {
    fn drop(&mut self) {
        // SAFETY: Constructed only after a successful open on a retained device.
        unsafe { IOHIDDeviceClose(self.0, 0) };
    }
}

pub fn request_access() -> bool {
    // SAFETY: Requests the documented ListenEvent permission through macOS.
    unsafe { IOHIDRequestAccess(1) }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: This handle was returned at +1 and is released only here.
            unsafe { CFRelease(self.0) };
        }
    }
}

fn property(device: Ref, name: &str) -> Option<CFType> {
    let key = CFString::new(name);
    // SAFETY: The device is kept alive by the enclosing device set. Get-rule
    // wrapping retains the borrowed property before returning it.
    unsafe {
        let value = IOHIDDeviceGetProperty(device, key.as_CFTypeRef());
        (!value.is_null()).then(|| CFType::wrap_under_get_rule(value))
    }
}

fn string_property(device: Ref, name: &str) -> Option<String> {
    property(device, name)?
        .downcast::<CFString>()
        .map(|s| s.to_string())
}

fn number_property(device: Ref, name: &str) -> Option<i64> {
    property(device, name)?.downcast::<CFNumber>()?.to_i64()
}

pub fn read_snapshot(filter: &str, secure_input_enabled: bool) -> Snapshot {
    snapshot_with_options(filter, false, secure_input_enabled)
}

pub fn probe_snapshot(filter: &str, secure_input_enabled: bool) -> Snapshot {
    snapshot_with_options(filter, true, secure_input_enabled)
}

fn snapshot_with_options(filter: &str, refresh: bool, secure_input_enabled: bool) -> Snapshot {
    // SAFETY: Constant 1 is kIOHIDRequestTypeListenEvent. Checking does not ask
    // for access or change the user's privacy settings.
    let access = unsafe { IOHIDCheckAccess(1) };
    let mut snapshot = Snapshot::new(
        match access {
            0 => "granted",
            1 => "denied",
            _ => "not_requested",
        },
        secure_input_enabled,
    );

    // SAFETY: All raw handles below come from IOKit. Their containing CF
    // collections outlive all borrows, and each Create/Copy result is released.
    unsafe {
        let manager = Owned(IOHIDManagerCreate(ptr::null(), 0));
        if manager.0.is_null() {
            snapshot.error = Some("Could not create the HID manager.".into());
            return snapshot;
        }
        IOHIDManagerSetDeviceMatching(manager.0, ptr::null());
        let devices = Owned(IOHIDManagerCopyDevices(manager.0));
        if devices.0.is_null() {
            return snapshot;
        }
        let mut handles = vec![ptr::null(); CFSetGetCount(devices.0) as usize];
        CFSetGetValues(devices.0, handles.as_mut_ptr());
        for device in handles {
            let name = string_property(device, "Product").unwrap_or_default();
            if !name.to_lowercase().contains(&filter.to_lowercase()) {
                continue;
            }
            let mut found = Device {
                name,
                transport: string_property(device, "Transport").unwrap_or_default(),
                vendor_id: number_property(device, "VendorID").unwrap_or(0),
                product_id: number_property(device, "ProductID").unwrap_or(0),
                battery_percent: None,
                source: None,
                battery_elements: vec![],
                access_blocked: false,
                error: None,
            };
            // Opening only the selected keyboard avoids requesting access to
            // unrelated devices. Permission is requested from the menu action.
            if access != 0 {
                found.error = Some(
                    "Input Monitoring permission is required to read HID battery data.".into(),
                );
                snapshot.devices.push(found);
                continue;
            }
            // Secure Input protects the whole keyboard interface, including
            // battery fields. Wait for it to end instead of retrying HID reads.
            if snapshot.secure_input_enabled {
                found.error =
                    Some("Battery reads are paused while macOS Secure Input is active.".into());
                snapshot.devices.push(found);
                continue;
            }
            let result = IOHIDDeviceOpen(device, 0);
            if result != 0 {
                found.access_blocked = result as u32 == NOT_PERMITTED;
                found.error = Some(format!(
                    "Could not open the keyboard: {}",
                    read_error(result)
                ));
                snapshot.devices.push(found);
                continue;
            }
            let _open_device = OpenDevice(device);
            let elements = Owned(IOHIDDeviceCopyMatchingElements(device, ptr::null(), 0));
            if elements.0.is_null() {
                found.error = Some("macOS did not expose the keyboard's HID elements.".into());
            } else {
                for index in 0..CFArrayGetCount(elements.0) {
                    let element = CFArrayGetValueAtIndex(elements.0, index);
                    let page = IOHIDElementGetUsagePage(element);
                    let usage = IOHIDElementGetUsage(element);
                    // Generic Device / Battery Strength, Battery System /
                    // Absolute State Of Charge. Other fields aren't percentages.
                    if !matches!((page, usage), (0x06, 0x20) | (0x85, 0x65)) {
                        continue;
                    }
                    if !(1..=3).contains(&IOHIDElementGetType(element)) {
                        continue;
                    }
                    let mut reading = BatteryElement {
                        usage_page: page,
                        usage,
                        report_id: IOHIDElementGetReportID(element),
                        logical_min: IOHIDElementGetLogicalMin(element) as i64,
                        logical_max: IOHIDElementGetLogicalMax(element) as i64,
                        raw_value: None,
                        timestamp: None,
                        read_error: None,
                    };
                    let mut value = ptr::null();
                    // Request the input report declared by this battery element.
                    // This is a standard GET_REPORT, not a vendor output command.
                    let result = if refresh {
                        IOHIDDeviceGetValueWithOptions(device, element, &mut value, 0x0002_0000)
                    } else {
                        IOHIDDeviceGetValue(device, element, &mut value)
                    };
                    if result == 0 && !value.is_null() {
                        let raw = IOHIDValueGetIntegerValue(value) as i64;
                        let timestamp = IOHIDValueGetTimeStamp(value);
                        reading.raw_value = Some(raw);
                        reading.timestamp = Some(timestamp);
                        if let Some(percent) = measured_percent(
                            raw,
                            reading.logical_min,
                            reading.logical_max,
                            timestamp,
                        ) {
                            found.battery_percent = Some(percent);
                            found.source = Some(format!(
                                "HID battery strength, report {}",
                                reading.report_id
                            ));
                        }
                    } else {
                        found.access_blocked |= result as u32 == NOT_PERMITTED;
                        reading.read_error = Some(if result == 0 {
                            "macOS returned no HID value.".into()
                        } else {
                            read_error(result)
                        });
                    }
                    found.battery_elements.push(reading);
                }
            }
            snapshot.devices.push(found);
        }
    }
    snapshot.devices.sort_by(|a, b| a.name.cmp(&b.name));
    snapshot
}
