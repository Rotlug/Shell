pub mod date_time;
pub mod mpris;
pub mod sysstats;
pub mod hyprland;
pub mod tray;
pub mod wireplumber;
pub mod apps;
pub mod calculator;
pub mod notifications;
pub mod networkmanager;
pub mod clipboard;
pub mod idle_inhibitor;

pub async fn activate_all() {
    date_time::activate();
    mpris::activate();
    sysstats::activate();
    hyprland::activate();
    tray::activate();
    wireplumber::activate();
    apps::activate().await;
    calculator::activate();
    notifications::activate();
    //networkmanager::activate();
    clipboard::activate();
    idle_inhibitor::activate().await;
}