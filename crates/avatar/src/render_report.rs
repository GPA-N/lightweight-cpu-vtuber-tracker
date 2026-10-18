//! Render FPS and DXGI video memory measurement for the test report.

use std::time::{Duration, Instant};

use bevy::ecs::resource::Resource;
use bevy::prelude::{App, Last, Plugin, ResMut};
use windows::core::Interface;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter3, IDXGIFactory1, DXGI_MEMORY_SEGMENT_GROUP,
    DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL,
    DXGI_QUERY_VIDEO_MEMORY_INFO,
};

/// Average frames per second from per-frame durations (each entry is one frame interval).
/// None when the list is empty or the total time is zero.
pub fn fps_from_frame_times(times: &[Duration]) -> Option<f64> {
    let total: f64 = times.iter().map(Duration::as_secs_f64).sum();
    if times.is_empty() || total <= 0.0 {
        return None;
    }
    Some(times.len() as f64 / total)
}

/// Counts frame ticks. Frames are the intervals between ticks, so N ticks give N-1 frames.
#[derive(Debug, Default, Resource)]
pub struct FpsMeter {
    first: Option<Instant>,
    last: Option<Instant>,
    ticks: u64,
}

impl FpsMeter {
    pub fn tick(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
        self.ticks += 1;
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Frames (tick intervals) divided by time between first and last tick. None with fewer than 2 ticks.
    pub fn average_fps(&self) -> Option<f64> {
        if self.ticks < 2 {
            return None;
        }
        let elapsed = self.last?.duration_since(self.first?).as_secs_f64();
        if elapsed <= 0.0 {
            return None;
        }
        Some((self.ticks - 1) as f64 / elapsed)
    }
}

/// Video memory in megabytes, from DXGI QueryVideoMemoryInfo (CurrentUsage and Budget).
/// Usage is for the calling process only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DxgiMemory {
    pub local_mb: f64,
    pub non_local_mb: f64,
    pub local_budget_mb: f64,
    pub non_local_budget_mb: f64,
}

/// Source of DXGI video memory numbers.
pub trait VideoMemorySource {
    fn query(&self) -> Option<DxgiMemory>;
}

/// Source used when no DXGI adapter query is wired up. Always None.
pub struct NullSource;

impl VideoMemorySource for NullSource {
    fn query(&self) -> Option<DxgiMemory> {
        None
    }
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// (CurrentUsage MB, Budget MB) for one memory segment group of adapter 0.
fn query_segment(adapter: &IDXGIAdapter3, group: DXGI_MEMORY_SEGMENT_GROUP) -> Option<(f64, f64)> {
    let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
    unsafe { adapter.QueryVideoMemoryInfo(0, group, &mut info) }.ok()?;
    Some((bytes_to_mb(info.CurrentUsage), bytes_to_mb(info.Budget)))
}

/// Current DXGI video memory usage and budget for adapter 0. None when no DXGI adapter
/// or the query is unavailable (for example on a machine without a GPU).
pub fn query_dxgi_memory() -> Option<DxgiMemory> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    let adapter = unsafe { factory.EnumAdapters1(0) }.ok()?;
    let adapter: IDXGIAdapter3 = adapter.cast().ok()?;
    let (local_mb, local_budget_mb) = query_segment(&adapter, DXGI_MEMORY_SEGMENT_GROUP_LOCAL)?;
    let (non_local_mb, non_local_budget_mb) =
        query_segment(&adapter, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL)?;
    Some(DxgiMemory {
        local_mb,
        non_local_mb,
        local_budget_mb,
        non_local_budget_mb,
    })
}

fn tick_fps(mut meter: ResMut<FpsMeter>) {
    meter.tick(Instant::now());
}

/// Ticks `FpsMeter` once per frame in `Last` and exposes it as a resource.
pub struct RenderReportPlugin;

impl Plugin for RenderReportPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FpsMeter>();
        app.add_systems(Last, tick_fps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_sixty_frames_at_16_667_ms_is_60() {
        let times = vec![Duration::from_micros(16_667); 60];
        let fps = fps_from_frame_times(&times).unwrap();
        assert!((fps - 60.0).abs() < 0.1, "fps was {fps}");
    }

    #[test]
    fn fps_mixed_times_uses_total_time() {
        // 10 frames of 10 ms and 10 frames of 30 ms: 20 frames in 400 ms = 50 fps
        let mut times = vec![Duration::from_millis(10); 10];
        times.extend(vec![Duration::from_millis(30); 10]);
        let fps = fps_from_frame_times(&times).unwrap();
        assert!((fps - 50.0).abs() < 1e-9, "fps was {fps}");
    }

    #[test]
    fn fps_empty_or_zero_time_is_none() {
        assert_eq!(fps_from_frame_times(&[]), None);
        assert_eq!(fps_from_frame_times(&[Duration::ZERO; 3]), None);
    }

    #[test]
    fn meter_with_fewer_than_two_ticks_is_none() {
        let mut m = FpsMeter::default();
        assert_eq!(m.average_fps(), None);
        m.tick(Instant::now());
        assert_eq!(m.average_fps(), None);
    }

    #[test]
    fn meter_synthetic_ticks_average_to_60() {
        let start = Instant::now();
        let mut m = FpsMeter::default();
        for i in 0..61u32 {
            m.tick(start + Duration::from_micros(16_667 * u64::from(i)));
        }
        // 61 ticks = 60 frame intervals over 1000.02 ms
        let fps = m.average_fps().unwrap();
        assert!((fps - 60.0).abs() < 0.1, "fps was {fps}");
    }

    #[test]
    fn null_source_returns_none() {
        assert_eq!(NullSource.query(), None);
    }

    #[test]
    fn real_dxgi_query_does_not_panic_and_values_are_sane() {
        // Needs no GPU: returns None when no adapter is available.
        let m = query_dxgi_memory();
        println!("dxgi_memory: {m:?}");
        if let Some(m) = m {
            for v in [m.local_mb, m.non_local_mb, m.local_budget_mb, m.non_local_budget_mb] {
                assert!(v.is_finite() && v >= 0.0, "value was {v}");
            }
        }
    }

    struct FixedSource;
    impl VideoMemorySource for FixedSource {
        fn query(&self) -> Option<DxgiMemory> {
            Some(DxgiMemory {
                local_mb: 300.0,
                non_local_mb: 12.5,
                local_budget_mb: 4096.0,
                non_local_budget_mb: 8192.0,
            })
        }
    }

    #[test]
    fn custom_source_returns_some() {
        let m = FixedSource.query().unwrap();
        assert_eq!(m.local_mb, 300.0);
        assert_eq!(m.non_local_mb, 12.5);
    }

    #[test]
    fn plugin_inserts_meter_and_ticks_each_update() {
        let mut app = App::new();
        app.add_plugins(RenderReportPlugin);
        app.update();
        app.update();
        let meter = app.world().resource::<FpsMeter>();
        assert_eq!(meter.ticks(), 2);
    }
}
