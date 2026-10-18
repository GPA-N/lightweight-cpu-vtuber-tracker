use std::time::Duration;

use crate::stats;

/// One benchmark run's numbers. None prints "n/a".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub median_latency_ms: Option<f64>,
    pub p95_latency_ms: Option<f64>,
    pub delivered_camera_fps: Option<f64>,
    pub tracking_fps: Option<f64>,
    pub render_fps: Option<f64>,
    pub peak_working_set_mb: Option<f64>,
    pub peak_private_mb: Option<f64>,
    pub dxgi_local_mb: Option<f64>,
    pub dxgi_nonlocal_mb: Option<f64>,
    pub onnx_threads: Option<u32>,
    pub avg_cpu_percent: Option<f64>,
    pub cpu_model: String,
    pub ram_mb: Option<f64>,
    pub power_state: String,
}

impl Report {
    pub fn from_samples(
        latencies: &[Duration],
        delivered_camera_fps: Option<f64>,
        tracking_fps: Option<f64>,
        render_fps: Option<f64>,
    ) -> Self {
        Report {
            median_latency_ms: stats::median(latencies).map(ms),
            p95_latency_ms: stats::p95(latencies).map(ms),
            delivered_camera_fps,
            tracking_fps,
            render_fps,
            ..Default::default()
        }
    }

    pub fn to_text(&self) -> String {
        let mut s = String::new();
        s += &line("median tracking latency", self.median_latency_ms, "ms");
        s += &line("p95 tracking latency", self.p95_latency_ms, "ms");
        s += &line("delivered camera FPS", self.delivered_camera_fps, "fps");
        s += &line("tracking FPS", self.tracking_fps, "fps");
        s += &line("render FPS", self.render_fps, "fps");
        s += &line("peak working set", self.peak_working_set_mb, "MB");
        s += &line("peak private bytes", self.peak_private_mb, "MB");
        s += &line("DXGI local video memory", self.dxgi_local_mb, "MB");
        s += &line("DXGI non-local video memory", self.dxgi_nonlocal_mb, "MB");
        let threads = self.onnx_threads.map_or("n/a".to_string(), |n| n.to_string());
        s += &format!("ONNX Runtime thread count: {threads}\n");
        s += &line("average CPU", self.avg_cpu_percent, "%");
        s += &format!("CPU model: {}\n", self.cpu_model);
        s += &line("RAM", self.ram_mb, "MB");
        s += &format!("power state: {}\n", self.power_state);
        s += "\n";
        s += &format!(
            "median tracking latency < 50 ms: {}\n",
            verdict(self.median_latency_ms, |v| v < 50.0)
        );
        s += &format!("render FPS >= 30: {}\n", verdict(self.render_fps, |v| v >= 30.0));
        s += &format!(
            "peak working set <= 1024 MB: {}\n",
            verdict(self.peak_working_set_mb, |v| v <= 1024.0)
        );
        s
    }
}

fn ms(d: Duration) -> f64 {
    d.as_micros() as f64 / 1000.0
}

fn line(label: &str, value: Option<f64>, unit: &str) -> String {
    match value {
        Some(v) => format!("{label}: {v:.1} {unit}\n"),
        None => format!("{label}: n/a\n"),
    }
}

fn verdict(value: Option<f64>, pass: impl Fn(f64) -> bool) -> &'static str {
    match value {
        Some(v) if pass(v) => "PASS",
        Some(_) => "FAIL",
        None => "n/a",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Report {
        Report {
            median_latency_ms: Some(12.3),
            p95_latency_ms: Some(20.1),
            delivered_camera_fps: Some(30.0),
            tracking_fps: Some(28.5),
            render_fps: Some(60.0),
            peak_working_set_mb: Some(512.0),
            peak_private_mb: Some(600.0),
            dxgi_local_mb: Some(300.0),
            dxgi_nonlocal_mb: Some(100.0),
            onnx_threads: Some(7),
            avg_cpu_percent: Some(45.0),
            cpu_model: "Test CPU".into(),
            ram_mb: Some(16384.0),
            power_state: "plugged in".into(),
        }
    }

    #[test]
    fn report_snapshot_full() {
        let expected = "median tracking latency: 12.3 ms\n\
p95 tracking latency: 20.1 ms\n\
delivered camera FPS: 30.0 fps\n\
tracking FPS: 28.5 fps\n\
render FPS: 60.0 fps\n\
peak working set: 512.0 MB\n\
peak private bytes: 600.0 MB\n\
DXGI local video memory: 300.0 MB\n\
DXGI non-local video memory: 100.0 MB\n\
ONNX Runtime thread count: 7\n\
average CPU: 45.0 %\n\
CPU model: Test CPU\n\
RAM: 16384.0 MB\n\
power state: plugged in\n\
\n\
median tracking latency < 50 ms: PASS\n\
render FPS >= 30: PASS\n\
peak working set <= 1024 MB: PASS\n";
        assert_eq!(full().to_text(), expected);
    }

    #[test]
    fn report_missing_fields_print_na() {
        let text = Report::default().to_text();
        assert!(text.contains("median tracking latency: n/a\n"));
        assert!(text.contains("ONNX Runtime thread count: n/a\n"));
        assert!(text.contains("median tracking latency < 50 ms: n/a\n"));
        assert!(text.contains("render FPS >= 30: n/a\n"));
        assert!(text.contains("peak working set <= 1024 MB: n/a\n"));
    }

    #[test]
    fn report_latency_boundary() {
        let mut r = full();
        r.median_latency_ms = Some(49.9);
        assert!(r.to_text().contains("median tracking latency < 50 ms: PASS\n"));
        r.median_latency_ms = Some(50.0);
        assert!(r.to_text().contains("median tracking latency < 50 ms: FAIL\n"));
    }

    #[test]
    fn report_render_fps_boundary() {
        let mut r = full();
        r.render_fps = Some(30.0);
        assert!(r.to_text().contains("render FPS >= 30: PASS\n"));
        r.render_fps = Some(29.9);
        assert!(r.to_text().contains("render FPS >= 30: FAIL\n"));
    }

    #[test]
    fn report_peak_working_set_boundary() {
        let mut r = full();
        r.peak_working_set_mb = Some(1024.0);
        assert!(r.to_text().contains("peak working set <= 1024 MB: PASS\n"));
        r.peak_working_set_mb = Some(1024.1);
        assert!(r.to_text().contains("peak working set <= 1024 MB: FAIL\n"));
    }

    #[test]
    fn from_samples_uses_median_and_p95() {
        let xs: Vec<Duration> = [10u64, 20, 30].iter().map(|&m| Duration::from_millis(m)).collect();
        let r = Report::from_samples(&xs, Some(30.0), None, Some(60.0));
        assert_eq!(r.median_latency_ms, Some(20.0));
        assert_eq!(r.p95_latency_ms, Some(30.0));
        assert_eq!(r.delivered_camera_fps, Some(30.0));
        assert_eq!(r.tracking_fps, None);
        assert_eq!(r.render_fps, Some(60.0));
    }

    #[test]
    fn from_samples_empty_is_none() {
        let r = Report::from_samples(&[], None, None, None);
        assert_eq!(r.median_latency_ms, None);
        assert_eq!(r.p95_latency_ms, None);
    }
}
