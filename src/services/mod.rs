pub mod date_time;
pub mod mpris;
pub mod sysstats;
pub mod hyprland;
pub mod compositor;
pub mod tray;
pub mod wireplumber;
pub mod notifications;
pub mod networkmanager;
pub mod clipboard;
pub mod idle_inhibitor;

pub async fn activate_all() {
    date_time::activate();
    mpris::activate();
    sysstats::activate();
    compositor::activate();
    tray::activate();
    wireplumber::activate();
    notifications::activate();
    //networkmanager::activate();
    networkmanager::status::activate();
    clipboard::activate();
    idle_inhibitor::activate().await;
}