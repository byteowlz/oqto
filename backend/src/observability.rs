//! Host observability helpers.

use anyhow::{Context, Result};
use serde::Serialize;
use tokio::fs;

#[derive(Debug, Clone)]
pub struct CpuTimes {
    total: u64,
    idle: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostMetrics {
    pub cpu_percent: f64,
    pub mem_total_bytes: u64,
    pub mem_used_bytes: u64,
    pub mem_available_bytes: u64,
}

#[derive(Debug, Clone)]
struct MemInfo {
    total_bytes: u64,
    available_bytes: u64,
}

pub async fn read_host_metrics(prev_cpu: Option<CpuTimes>) -> Result<(HostMetrics, CpuTimes)> {
    let stat_contents = fs::read_to_string("/proc/stat")
        .await
        .context("reading /proc/stat")?;
    let mem_contents = fs::read_to_string("/proc/meminfo")
        .await
        .context("reading /proc/meminfo")?;

    let current_cpu = parse_cpu_times(&stat_contents)?;
    let mem_info = parse_meminfo(&mem_contents)?;
    let cpu_percent = compute_cpu_percent(prev_cpu.as_ref(), &current_cpu);

    let used_bytes = mem_info
        .total_bytes
        .saturating_sub(mem_info.available_bytes);

    Ok((
        HostMetrics {
            cpu_percent,
            mem_total_bytes: mem_info.total_bytes,
            mem_used_bytes: used_bytes,
            mem_available_bytes: mem_info.available_bytes,
        },
        current_cpu,
    ))
}

fn parse_cpu_times(contents: &str) -> Result<CpuTimes> {
    let line = contents
        .lines()
        .find(|line| line.starts_with("cpu "))
        .context("missing cpu line in /proc/stat")?;

    let mut parts = line.split_whitespace();
    let _ = parts.next();
    let values: Vec<u64> = parts
        .map(|value| value.parse::<u64>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("parsing cpu times")?;

    if values.len() < 4 {
        return Err(anyhow::anyhow!("cpu stats line missing expected fields"));
    }

    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    let total = values.iter().sum();

    Ok(CpuTimes { total, idle })
}

fn parse_meminfo(contents: &str) -> Result<MemInfo> {
    let mut total_kb = None;
    let mut available_kb = None;

    for line in contents.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = parse_meminfo_kb(line);
        } else if line.starts_with("MemAvailable:") {
            available_kb = parse_meminfo_kb(line);
        }
    }

    let total_kb = total_kb.context("missing MemTotal in /proc/meminfo")?;
    let available_kb = available_kb.context("missing MemAvailable in /proc/meminfo")?;

    Ok(MemInfo {
        total_bytes: total_kb.saturating_mul(1024),
        available_bytes: available_kb.saturating_mul(1024),
    })
}

fn parse_meminfo_kb(line: &str) -> Option<u64> {
    line.split_whitespace().nth(1)?.parse::<u64>().ok()
}

fn compute_cpu_percent(prev: Option<&CpuTimes>, current: &CpuTimes) -> f64 {
    let Some(prev) = prev else {
        return 0.0;
    };

    let total_delta = current.total.saturating_sub(prev.total);
    let idle_delta = current.idle.saturating_sub(prev.idle);
    if total_delta == 0 {
        return 0.0;
    }

    let busy_delta = total_delta.saturating_sub(idle_delta);
    (busy_delta as f64 / total_delta as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_times() {
        let sample = "cpu  2255 34 2290 22625563 6290 127 456 0 0 0\ncpu0 1132 17 1441 11311771 3675 0 227 0 0 0\n";
        let parsed = parse_cpu_times(sample).unwrap();
        assert!(parsed.total > 0);
        assert!(parsed.idle > 0);
    }

    #[test]
    fn test_parse_meminfo() {
        let sample = "\
MemTotal:       16384256 kB
MemFree:         123456 kB
MemAvailable:    999999 kB
Buffers:          65432 kB
";
        let info = parse_meminfo(sample).unwrap();
        assert_eq!(info.total_bytes, 16384256 * 1024);
        assert_eq!(info.available_bytes, 999999 * 1024);
    }

    #[test]
    fn test_compute_cpu_percent() {
        let prev = CpuTimes {
            total: 100,
            idle: 40,
        };
        let current = CpuTimes {
            total: 200,
            idle: 60,
        };

        let percent = compute_cpu_percent(Some(&prev), &current);
        assert!((percent - 80.0).abs() < 0.01);
    }
}
