use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Device {
    ip: String,
    mac: String,
    iface: String,
    state: String,
    last_seen: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Event {
    event_type: String,
    ip: String,
    old_mac: Option<String>,
    new_mac: Option<String>,
    mac: Option<String>,
    timestamp: i64,
}

#[derive(Debug, Clone, Default)]
struct CoreState {
    devices: HashMap<String, Device>,
    ignored: Vec<String>,
    interval_ms: u64,
    running: bool,
    do_sweep: bool,
}

#[tokio::main]
async fn main() {
    eprintln!("[ionguard-core] NEMESIS ENGINE LAN Monitor started");

    let state = Arc::new(Mutex::new(CoreState {
        interval_ms: 3000,
        ..Default::default()
    }));

    let (evt_tx, mut evt_rx) = mpsc::channel::<Event>(100);

    // stdout JSON emitter
    let emitter = tokio::spawn(async move {
        while let Some(evt) = evt_rx.recv().await {
            let line = serde_json::to_string(&evt).unwrap();
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            let _ = writeln!(lock, "{}", line);
            let _ = lock.flush();
            drop(lock);
        }
    });

    // monitoring loop
    let mon_state = Arc::clone(&state);
    let mon_tx = evt_tx.clone();
    let monitor = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_millis(100));
        let mut last_poll = Instant::now() - Duration::from_secs(60);

        loop {
            ticker.tick().await;

            let (should_run, interval_ms, should_sweep) = {
                let s = mon_state.lock().await;
                (s.running, s.interval_ms, s.do_sweep)
            };

            if !should_run {
                continue;
            }

            if should_sweep {
                let sweep_devices = active_sweep().await;
                let mut s = mon_state.lock().await;
                s.do_sweep = false;
                for dev in sweep_devices {
                    s.devices.insert(dev.ip.clone(), dev);
                }
            }

            let now = Instant::now();
            if now.duration_since(last_poll) < Duration::from_millis(interval_ms) {
                continue;
            }
            last_poll = now;

            let current = read_neighbors().await;
            let mut s = mon_state.lock().await;
            let ignored: Vec<String> = s.ignored.clone();

            // detect new / changed
            for (ip, dev) in &current {
                if ignored.contains(ip) {
                    continue;
                }
                if let Some(old) = s.devices.get(ip) {
                    if old.mac != dev.mac && !dev.mac.is_empty() && dev.mac != "00:00:00:00:00:00" {
                        let _ = mon_tx.send(Event {
                            event_type: "mac_change".into(),
                            ip: ip.clone(),
                            old_mac: Some(old.mac.clone()),
                            new_mac: Some(dev.mac.clone()),
                            mac: None,
                            timestamp: chrono::Utc::now().timestamp(),
                        }).await;
                    }
                } else {
                    let _ = mon_tx.send(Event {
                        event_type: "new_device".into(),
                        ip: ip.clone(),
                        old_mac: None,
                        new_mac: None,
                        mac: Some(dev.mac.clone()),
                        timestamp: chrono::Utc::now().timestamp(),
                    }).await;
                }
            }

            // detect gone
            let old_ips: Vec<String> = s.devices.keys().cloned().collect();
            for ip in old_ips {
                if ignored.contains(&ip) {
                    continue;
                }
                if !current.contains_key(&ip) {
                    let old_mac = s.devices.get(&ip).map(|d| d.mac.clone());
                    let _ = mon_tx.send(Event {
                        event_type: "device_gone".into(),
                        ip: ip.clone(),
                        old_mac: old_mac.clone(),
                        new_mac: None,
                        mac: old_mac,
                        timestamp: chrono::Utc::now().timestamp(),
                    }).await;
                }
            }

            s.devices = current;
        }
    });

    // stdin command reader
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    while let Some(Ok(line)) = lines.next() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("").to_uppercase();

        match cmd.as_str() {
            "START" => {
                let mut s = state.lock().await;
                s.running = true;
            }
            "STOP" => {
                let mut s = state.lock().await;
                s.running = false;
            }
            "INTERVAL" => {
                if let Some(val) = parts.next() {
                    if let Ok(ms) = val.parse::<u64>() {
                        let mut s = state.lock().await;
                        s.interval_ms = ms.max(500);
                    }
                }
            }
            "IGNORE" => {
                let rest = line.trim_start_matches("IGNORE ").trim();
                let ips: Vec<String> = rest.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let mut s = state.lock().await;
                s.ignored = ips;
            }
            "SWEEP" => {
                let mut s = state.lock().await;
                s.do_sweep = true;
            }
            "STATUS" => {
                let s = state.lock().await;
                let _ = writeln!(io::stdout(), "{{\"status\":\"ok\",\"running\":{},\"devices\":{},\"interval\":{}}}", s.running, s.devices.len(), s.interval_ms);
            }
            _ => {}
        }
    }

    monitor.abort();
    emitter.abort();
}

async fn read_neighbors() -> HashMap<String, Device> {
    let mut devices = HashMap::new();

    // Try ip neigh first
    if let Ok(out) = Command::new("ip").args(["neigh", "show"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let ip = parts[0].to_string();
                let mut mac = String::new();
                let mut iface = String::new();
                let mut state = String::new();

                for (i, &part) in parts.iter().enumerate() {
                    if part == "dev" && i + 1 < parts.len() {
                        iface = parts[i + 1].to_string();
                    }
                    if part == "lladdr" && i + 1 < parts.len() {
                        mac = parts[i + 1].to_string();
                    }
                    if part == "REACHABLE" || part == "STALE" || part == "DELAY" || part == "PROBE" || part == "FAILED" || part == "INCOMPLETE" || part == "PERMANENT" {
                        state = part.to_string();
                    }
                }

                if mac.is_empty() || mac == "00:00:00:00:00:00" {
                    continue;
                }

                devices.insert(ip.clone(), Device {
                    ip,
                    mac,
                    iface,
                    state: state.to_lowercase(),
                    last_seen: chrono::Utc::now().timestamp(),
                });
            }
        }
    }

    // Fallback to /proc/net/arp
    if devices.is_empty() {
        if let Ok(text) = tokio::fs::read_to_string("/proc/net/arp").await {
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let ip = parts[0].to_string();
                    let mac = parts[3].to_string();
                    let iface = parts.get(5).unwrap_or(&"").to_string();

                    if mac == "00:00:00:00:00:00" || mac.len() != 17 {
                        continue;
                    }

                    devices.insert(ip.clone(), Device {
                        ip,
                        mac,
                        iface,
                        state: "reachable".into(),
                        last_seen: chrono::Utc::now().timestamp(),
                    });
                }
            }
        }
    }

    devices
}

async fn active_sweep() -> Vec<Device> {
    let mut found = vec![];

    // Get local network range from ip command
    let mut network = String::from("192.168.1.0/24");
    if let Ok(out) = Command::new("ip").args(["-4", ""]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("inet ") && !line.contains("lo") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(addr) = parts.get(1) {
                    network = addr.to_string();
                    break;
                }
            }
        }
    }

    // Simple ping sweep using fping if available, otherwise skip
    if Command::new("which").arg("fping").output().map(|o| o.status.success()).unwrap_or(false) {
        if let Ok(out) = Command::new("fping")
            .args(["-a", "-g", &network, "-q", "-i", "10", "-r", "1"])
            .stderr(Stdio::null())
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let ip = line.trim().to_string();
                if !ip.is_empty() {
                    // Try to get MAC for this IP
                    if let Ok(arp_out) = Command::new("ip").args(["neigh", "show", &ip]).output() {
                        let arp_text = String::from_utf8_lossy(&arp_out.stdout);
                        let mut mac = String::new();
                        for part in arp_text.split_whitespace() {
                            if part.contains(':') && part.len() == 17 {
                                mac = part.to_string();
                                break;
                            }
                        }
                        if !mac.is_empty() {
                            found.push(Device {
                                ip: ip.clone(),
                                mac,
                                iface: "sweep".into(),
                                state: "reachable".into(),
                                last_seen: chrono::Utc::now().timestamp(),
                            });
                        }
                    }
                }
            }
        }
    }

    found
}
