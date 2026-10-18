use std::time::Duration;

// Nearest-rank percentile: sort ascending, take the element at rank ceil(p/100 * n), 1-based. None when empty.
pub fn percentile(durations: &[Duration], p: f64) -> Option<Duration> {
    if durations.is_empty() {
        return None;
    }
    let mut sorted = durations.to_vec();
    sorted.sort();
    let rank = (p / 100.0 * sorted.len() as f64).ceil() as usize;
    let index = rank.clamp(1, sorted.len()) - 1;
    Some(sorted[index])
}

pub fn median(durations: &[Duration]) -> Option<Duration> {
    percentile(durations, 50.0)
}

pub fn p95(durations: &[Duration]) -> Option<Duration> {
    percentile(durations, 95.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ms(v: &[u64]) -> Vec<Duration> {
        v.iter().map(|&x| Duration::from_millis(x)).collect()
    }

    #[test]
    fn stats_known_list() {
        // Unsorted on purpose: 10..=100 ms in steps of 10.
        let xs = ms(&[70, 10, 100, 30, 50, 90, 20, 80, 60, 40]);
        assert_eq!(median(&xs), Some(Duration::from_millis(50)));
        assert_eq!(p95(&xs), Some(Duration::from_millis(100)));
    }

    #[test]
    fn stats_single_value() {
        let xs = ms(&[7]);
        assert_eq!(median(&xs), Some(Duration::from_millis(7)));
        assert_eq!(p95(&xs), Some(Duration::from_millis(7)));
    }

    #[test]
    fn stats_empty_list_is_none() {
        let xs: Vec<Duration> = Vec::new();
        assert_eq!(median(&xs), None);
        assert_eq!(p95(&xs), None);
    }
}
