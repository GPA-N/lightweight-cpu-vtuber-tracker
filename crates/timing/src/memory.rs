use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::GetCurrentProcess;

const BYTES_PER_MB: f64 = 1_048_576.0;

/// Memory of the current process, in megabytes (1 MB = 1,048,576 bytes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessMemory {
    pub working_set_mb: f64,
    pub peak_working_set_mb: f64,
    pub peak_private_mb: f64,
}

/// Reads the current process working set and peak working set / peak private bytes.
pub fn process_memory() -> windows::core::Result<ProcessMemory> {
    let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
    let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    c.cb = cb;
    // SAFETY: c is a valid writable PROCESS_MEMORY_COUNTERS_EX and cb is its exact size, as PSAPI requires.
    unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut c as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            cb,
        )?;
    }
    Ok(ProcessMemory {
        working_set_mb: c.WorkingSetSize as f64 / BYTES_PER_MB,
        peak_working_set_mb: c.PeakWorkingSetSize as f64 / BYTES_PER_MB,
        peak_private_mb: c.PeakPagefileUsage as f64 / BYTES_PER_MB,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_values_are_positive_and_peak_bounds_current() {
        let m = process_memory().unwrap();
        assert!(m.working_set_mb > 0.0);
        assert!(m.peak_working_set_mb > 0.0);
        assert!(m.peak_private_mb > 0.0);
        assert!(m.peak_working_set_mb >= m.working_set_mb);
    }
}
