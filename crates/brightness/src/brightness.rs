use std::{
    collections::HashMap,
    sync::mpsc::{Receiver as CommandReceiver, Sender as CommandSender, channel as command_channel},
};

use tokio::sync::mpsc::{Receiver, channel};

use crate::{MonitorId, backlight, ddc, event::Event};

/// Asks a worker to set the brightness of a monitor.
#[derive(Debug)]
pub(crate) struct Command {
    pub id: MonitorId,
    /// From 0.0 to 1.0
    pub brightness: f64,
}

/// Takes the commands that are already waiting after `first`, keeping only the newest one per
/// monitor. Dragging a slider sends far more of them than a monitor can keep up with.
pub(crate) fn coalesce(first: Command, rx: &CommandReceiver<Command>) -> Vec<Command> {
    let mut latest: HashMap<MonitorId, f64> = HashMap::new();
    let mut order = vec![first.id.clone()];
    latest.insert(first.id, first.brightness);

    while let Ok(command) = rx.try_recv() {
        if latest.insert(command.id.clone(), command.brightness).is_none() {
            order.push(command.id);
        }
    }

    order
        .into_iter()
        .filter_map(|id| latest.remove(&id).map(|brightness| Command { id, brightness }))
        .collect()
}

/// Finds the monitors of the machine, and changes their brightness.
///
/// The monitors are handled by threads of their own, since talking to them is blocking and
/// DDC/CI in particular is slow. They stop once this is dropped.
pub struct Brightness {
    backlight_tx: CommandSender<Command>,
    ddc_tx: CommandSender<Command>,
}

impl Brightness {
    /// Starts looking for monitors. Returns a `Receiver` which can be used to listen to them
    /// and their brightness: the monitors that are there already are reported first.
    ///
    /// # Errors
    /// Errors when the threads that handle the monitors can't be started.
    pub fn new() -> crate::Result<(Self, Receiver<Event>)> {
        let (event_tx, event_rx) = channel(64);
        let (backlight_tx, backlight_rx) = command_channel();
        let (ddc_tx, ddc_rx) = command_channel();

        backlight::spawn(backlight_rx, event_tx.clone())?;
        ddc::spawn(ddc_rx, event_tx)?;

        Ok((Self { backlight_tx, ddc_tx }, event_rx))
    }

    /// Sets the brightness of a monitor, from 0.0 to 1.0. The change is reported through an
    /// `Event::Brightness` once it happened.
    ///
    /// Internal panels never go fully dark, there would be no way to tell what is going on.
    pub fn set_brightness(&self, id: &MonitorId, brightness: f64) {
        let command = Command {
            id: id.clone(),
            brightness: if brightness.is_nan() { 0.0 } else { brightness.clamp(0.0, 1.0) },
        };

        let tx = if backlight::owns(id) { &self.backlight_tx } else { &self.ddc_tx };

        // Only fails once the thread is gone, and then there is nothing to set
        let _ = tx.send(command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(id: &str, brightness: f64) -> Command {
        Command { id: MonitorId(id.to_string()), brightness }
    }

    #[test]
    fn keeps_the_newest_command_per_monitor() {
        let (tx, rx) = command_channel();
        tx.send(command("a", 0.2)).unwrap();
        tx.send(command("b", 0.9)).unwrap();
        tx.send(command("a", 0.3)).unwrap();

        let coalesced = coalesce(command("a", 0.1), &rx);

        assert_eq!(coalesced.len(), 2);
        assert_eq!(coalesced[0].id.as_str(), "a");
        assert!((coalesced[0].brightness - 0.3).abs() < f64::EPSILON);
        assert_eq!(coalesced[1].id.as_str(), "b");
        assert!((coalesced[1].brightness - 0.9).abs() < f64::EPSILON);
    }
}
