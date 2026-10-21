use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub app_version: &'static str,
    pub input_monitoring: String,
    pub secure_input_enabled: bool,
    pub devices: Vec<Device>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Device {
    pub name: String,
    pub transport: String,
    pub vendor_id: i64,
    pub product_id: i64,
    pub battery_percent: Option<u8>,
    pub source: Option<String>,
    pub battery_elements: Vec<BatteryElement>,
    pub access_blocked: bool,
    pub error: Option<String>,
}

impl Snapshot {
    pub fn new(input_monitoring: &str, secure_input_enabled: bool) -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION"),
            input_monitoring: input_monitoring.into(),
            secure_input_enabled,
            devices: vec![],
            error: None,
        }
    }

    pub fn needs_access_settings(&self) -> bool {
        matches!(self.input_monitoring.as_str(), "denied" | "not_requested")
            || (!self.secure_input_enabled && self.devices.iter().any(|d| d.access_blocked))
    }

    pub fn menu_title(&self) -> String {
        if self.input_monitoring == "checking" {
            return "…".into();
        }
        if self.error.is_some() {
            return "Error".into();
        }
        match self.devices.as_slice() {
            [] => "Offline".into(),
            [device] => device
                .battery_percent
                .map_or_else(|| "—".into(), |n| format!("{n}%")),
            _ => "Keychron".into(),
        }
    }

    pub fn summary(&self) -> &str {
        if self.input_monitoring == "checking" {
            "Reading keyboard battery…"
        } else if self.error.is_some() {
            "Could not read keyboard information"
        } else if self.devices.is_empty() {
            "Connect your Keychron keyboard"
        } else if self.input_monitoring != "granted" {
            "Enable Battery Access to read your keyboard"
        } else if self.secure_input_enabled {
            "Battery reads paused by macOS Secure Input"
        } else if self.devices.iter().any(|d| d.battery_percent.is_some()) {
            "Battery level reported by your keyboard"
        } else if self.devices.iter().any(|d| d.access_blocked) {
            "macOS blocked battery access — review Input Monitoring"
        } else if self
            .devices
            .iter()
            .any(|d| d.error.is_some() || d.battery_elements.iter().any(|e| e.read_error.is_some()))
        {
            "Unable to read battery data — see diagnostics"
        } else if self.devices.iter().any(|d| !d.battery_elements.is_empty()) {
            "Waiting for the keyboard to report its battery"
        } else {
            "This keyboard exposes no readable HID battery field"
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BatteryElement {
    pub usage_page: u32,
    pub usage: u32,
    pub report_id: u32,
    pub logical_min: i64,
    pub logical_max: i64,
    pub raw_value: Option<i64>,
    pub timestamp: Option<u64>,
    pub read_error: Option<String>,
}

/// A zero-valued cache entry with no timestamp is not a measurement.
pub fn measured_percent(raw: i64, min: i64, max: i64, timestamp: u64) -> Option<u8> {
    if timestamp == 0 || max <= min || raw < min || raw > max {
        return None;
    }
    let percent = ((raw as i128 - min as i128) * 100 + (max as i128 - min as i128) / 2)
        / (max as i128 - min as i128);
    u8::try_from(percent).ok().filter(|n| *n <= 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(percent: Option<u8>) -> Snapshot {
        Snapshot {
            devices: vec![Device {
                name: "Keychron K2".into(),
                transport: "Bluetooth".into(),
                vendor_id: 1452,
                product_id: 591,
                battery_percent: percent,
                source: None,
                battery_elements: vec![],
                access_blocked: false,
                error: None,
            }],
            ..Snapshot::new("granted", false)
        }
    }

    #[test]
    fn unavailable_and_empty_battery_have_distinct_displays() {
        assert_eq!(snapshot(None).menu_title(), "—");
        assert_eq!(snapshot(Some(0)).menu_title(), "0%");
    }

    #[test]
    fn permission_and_disconnect_have_actionable_states() {
        let mut state = snapshot(None);
        state.input_monitoring = "denied".into();
        assert_eq!(
            state.summary(),
            "Enable Battery Access to read your keyboard"
        );
        state.devices.clear();
        assert_eq!(state.menu_title(), "Offline");
        assert_eq!(state.summary(), "Connect your Keychron keyboard");
    }

    #[test]
    fn read_error_is_not_presented_as_unsupported_hardware() {
        let mut state = snapshot(None);
        state.devices[0].error = Some("IOKit error".into());
        assert_eq!(
            state.summary(),
            "Unable to read battery data — see diagnostics"
        );
        state.error = Some("Manager error".into());
        assert_eq!(state.menu_title(), "Error");
    }

    #[test]
    fn secure_input_is_distinct_from_a_missing_permission_and_recovers() {
        let mut state = snapshot(None);
        state.secure_input_enabled = true;
        assert_eq!(
            state.summary(),
            "Battery reads paused by macOS Secure Input"
        );
        assert!(!state.needs_access_settings());

        state.secure_input_enabled = false;
        state.devices[0].battery_percent = Some(75);
        assert_eq!(state.menu_title(), "75%");
        assert_eq!(state.summary(), "Battery level reported by your keyboard");
    }

    #[test]
    fn denied_hid_read_offers_settings_even_when_preflight_is_granted() {
        let mut state = snapshot(None);
        state.devices[0].access_blocked = true;
        assert!(state.needs_access_settings());
        assert_eq!(
            state.summary(),
            "macOS blocked battery access — review Input Monitoring"
        );

        state.secure_input_enabled = true;
        assert!(!state.needs_access_settings());
        assert_eq!(
            state.summary(),
            "Battery reads paused by macOS Secure Input"
        );

        state.input_monitoring = "denied".into();
        assert!(state.needs_access_settings());
        assert_eq!(
            state.summary(),
            "Enable Battery Access to read your keyboard"
        );
    }

    #[test]
    fn failed_element_read_is_not_presented_as_waiting_for_a_report() {
        let mut state = snapshot(None);
        state.devices[0].battery_elements.push(BatteryElement {
            usage_page: 6,
            usage: 32,
            report_id: 3,
            logical_min: 0,
            logical_max: 100,
            raw_value: None,
            timestamp: None,
            read_error: Some("IOKit 0xe00002d6".into()),
        });
        assert_eq!(
            state.summary(),
            "Unable to read battery data — see diagnostics"
        );
        state.devices[0].battery_elements[0].read_error = None;
        assert_eq!(
            state.summary(),
            "Waiting for the keyboard to report its battery"
        );
    }

    #[test]
    fn distinguishes_empty_cache_from_real_empty_battery() {
        assert_eq!(measured_percent(0, 0, 100, 0), None);
        assert_eq!(measured_percent(0, 0, 100, 42), Some(0));
    }

    #[test]
    fn rejects_invalid_reports_and_scales_declared_range() {
        assert_eq!(measured_percent(101, 0, 100, 1), None);
        assert_eq!(measured_percent(-1, 0, 100, 1), None);
        assert_eq!(measured_percent(1, 0, 0, 1), None);
        assert_eq!(measured_percent(128, 0, 255, 1), Some(50));
        assert_eq!(measured_percent(75, 0, 100, 1), Some(75));
    }
}
