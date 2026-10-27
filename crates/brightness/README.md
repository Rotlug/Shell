# brightness

Brightness of the monitors of the machine, internal and external:

* Internal (laptop) panels through `/sys/class/backlight`. Setting the brightness goes
  through logind (`org.freedesktop.login1.Session.SetBrightness`), which lets a regular user
  do it in their session.
* External monitors through DDC/CI over I2C (`/dev/i2c-*`). Needs the `i2c-dev` kernel module,
  and permission to open the devices (usually the `i2c` group, or a udev rule).

Monitors, and their brightness, are reported over a channel as they come, go and change:

```rust
let (brightness, mut rx) = Brightness::new()?;

while let Some(event) = rx.recv().await {
    match event {
        Event::Added(monitor) => brightness.set_brightness(&monitor.id, 0.5),
        _ => println!("{event:?}"),
    }
}
```
