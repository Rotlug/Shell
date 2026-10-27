# system_stats

CPU, memory, swap, uptime, CPU temperature, GPU memory and battery of the machine.

Pick the stats you want, and get their changes over a channel:

```rust
let (stats, mut rx) = SystemStats::builder()
    .interval(Duration::from_secs(1))
    .cpu_usage()
    .memory()
    .battery()
    .build()?;

while let Some(event) = rx.recv().await {
    println!("{event:?}");
}
```

Only the stats you ask for are read. `stats.snapshot()` returns the latest value of each.

## Features

* `nvidia` (default): GPU memory of NVIDIA cards, through NVML.
