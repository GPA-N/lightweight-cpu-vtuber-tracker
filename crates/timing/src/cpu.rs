use std::num::NonZeroUsize;
use std::time::Instant;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// Process CPU time in 100 ns units (user + kernel).
fn process_cpu_100ns() -> windows::core::Result<u64> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: the pseudo-handle from GetCurrentProcess needs no close; all out pointers are valid locals.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )?;
    }
    let ticks = |t: FILETIME| (t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64;
    Ok(ticks(kernel) + ticks(user))
}

/// Average process CPU percent between `start` and `percent` calls.
/// 100.0 means one core fully busy.
pub struct CpuSampler {
    cpu_100ns: u64,
    wall: Instant,
}

impl CpuSampler {
    pub fn start() -> windows::core::Result<Self> {
        Ok(Self {
            cpu_100ns: process_cpu_100ns()?,
            wall: Instant::now(),
        })
    }

    pub fn percent(&self) -> windows::core::Result<f64> {
        let cpu_secs = (process_cpu_100ns()? - self.cpu_100ns) as f64 / 1e7;
        let wall_secs = self.wall.elapsed().as_secs_f64();
        if wall_secs <= 0.0 {
            return Ok(0.0);
        }
        Ok(cpu_secs / wall_secs * 100.0)
    }

    /// Same as `percent`, divided by the logical core count.
    pub fn percent_of_logical_cores(&self) -> windows::core::Result<f64> {
        let cores = std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1);
        Ok(self.percent()? / cores as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::CpuSampler;
    use std::time::{Duration, Instant};

    #[test]
    fn cpu_busy_loop_reads_high_and_sleep_reads_low() {
        let busy = CpuSampler::start().unwrap();
        let t = Instant::now();
        let mut x: u64 = 0;
        while t.elapsed() < Duration::from_secs(1) {
            x = x.wrapping_add(1);
            std::hint::black_box(x);
        }
        let busy_percent = busy.percent().unwrap();
        assert!(busy_percent > 50.0, "busy loop read {busy_percent}%");

        let idle = CpuSampler::start().unwrap();
        std::thread::sleep(Duration::from_secs(1));
        let idle_percent = idle.percent().unwrap();
        assert!(idle_percent < 10.0, "sleep read {idle_percent}%");
    }
}
