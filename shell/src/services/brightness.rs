//! The brightness of the monitors, backed by the `brightness` crate.
//!
//! The crate reports monitors and their brightness over a channel; this keeps the monitors
//! that are there, so that whoever starts listening later can catch up, and fans the events out.
use std::sync::{LazyLock, OnceLock, RwLock};
use async_broadcast::Receiver;
use brightness::Brightness;

pub use brightness::event::Event;
pub use brightness::{Monitor, MonitorId, MonitorKind};

use crate::utils::broadcast::BroadcastChannel;

static CHANNEL: LazyLock<BroadcastChannel<Event>> = LazyLock::new(|| BroadcastChannel::new(32));
static MONITORS: LazyLock<RwLock<Vec<Monitor>>> = LazyLock::new(|| RwLock::new(Vec::new()));
static BRIGHTNESS: OnceLock<Brightness> = OnceLock::new();

fn apply(event: &Event) {
    let Ok(mut monitors) = MONITORS.write() else {
        return;
    };

    match event {
        Event::Added(monitor) => {
            monitors.retain(|existing| existing.id != monitor.id);
            monitors.push(monitor.clone());
        },
        Event::Removed(id) => monitors.retain(|monitor| monitor.id != *id),
        Event::Brightness(id, brightness) => if let Some(monitor) = monitors.iter_mut().find(|monitor| monitor.id == *id) {
            monitor.brightness = *brightness;
        },
    }
}

pub fn activate() {
    let (brightness, mut events) = match Brightness::new() {
        Ok(brightness) => brightness,
        Err(e) => return error!(%e, "Failed to start looking for monitors, their brightness will not be available"),
    };

    if BRIGHTNESS.set(brightness).is_err() {
        return;
    }

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            apply(&event);
            CHANNEL.send(event).await;
        }
    });
}

/// Subscribes to monitors coming and going, and their brightness changing. Monitors that are
/// there already are not announced, get those from `monitors`.
pub fn subscribe() -> Receiver<Event> {
    CHANNEL.subscribe()
}

/// The monitors that are there, the ones that are part of the machine first.
pub fn monitors() -> Vec<Monitor> {
    let mut monitors = MONITORS.read().map(|monitors| monitors.clone()).unwrap_or_default();

    monitors.sort_by_key(|monitor| monitor.kind == MonitorKind::External);
    monitors
}

/// Sets the brightness of a monitor, from 0.0 to 1.0.
pub fn set_brightness(id: &MonitorId, brightness: f64) {
    if let Some(service) = BRIGHTNESS.get() {
        service.set_brightness(id, brightness);
    }
}
