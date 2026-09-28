use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub input_monitoring: String,
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
    pub error: Option<String>,
}

impl Snapshot {
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
        } else if self.devices.iter().any(|d| d.battery_percent.is_some()) {
            "Battery level reported by your keyboard"
        } else if self.devices.iter().any(|d| d.error.is_some()) {
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
            input_monitoring: "granted".into(),
            devices: vec![Device {
                name: "Keychron K2".into(),
                transport: "Bluetooth".into(),
                vendor_id: 1452,
                product_id: 591,
                battery_percent: percent,
                source: None,
                battery_elements: vec![],
                error: None,
            }],
            error: None,
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
