use std::ffi::c_void;

use windows::core::w;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    PluggedIn,
    OnBattery,
    Unknown,
}

/// CPU model name from the registry. Empty string if it cannot be read.
pub fn cpu_model() -> String {
    let mut buf = vec![0u16; 256];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: buf and bytes are live for the call, and bytes holds buf's size in bytes.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0"),
            w!("ProcessorNameString"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut bytes as *mut u32),
        )
    };
    if rc != ERROR_SUCCESS {
        return String::new();
    }
    let len = (bytes as usize / 2).min(buf.len());
    String::from_utf16_lossy(&buf[..len])
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_string()
}

/// Total physical RAM in megabytes. 0 if it cannot be read.
pub fn total_ram_mb() -> u64 {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: status is live and dwLength is set to its size, as the API requires.
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_err() {
        return 0;
    }
    status.ullTotalPhys / (1024 * 1024)
}

/// Current power source: plugged in, on battery, or unknown.
pub fn power_state() -> PowerState {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: status is a live, properly sized SYSTEM_POWER_STATUS.
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return PowerState::Unknown;
    }
    match status.ACLineStatus {
        0 => PowerState::OnBattery,
        1 => PowerState::PluggedIn,
        _ => PowerState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_reports_cpu_model_and_ram() {
        assert!(!cpu_model().is_empty());
        assert!(total_ram_mb() > 0);
    }

    #[test]
    fn hardware_reports_power_state() {
        let _ = power_state();
    }
}
