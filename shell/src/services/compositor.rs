//! Compositor-neutral abstraction over Hyprland and niri.
//!
//! Widgets consume [`COMPOSITOR`] and the dispatch helpers below instead of
//! talking to either compositor's IPC directly. `submap` is a Hyprland-only
//! concept (keybind "grab" submap) and is simply never set on niri, which
//! has no equivalent - consumers of it degrade gracefully (the keybinds OSD/
//! quicktoggle just never activate under niri).
use std::sync::LazyLock;
use futures_signals::signal::Mutable;
use gdk::prelude::MonitorExt as _;
use hyprland::data::{Client, Monitor as HyprMonitor, Workspace as HyprWorkspace, Workspaces as HyprWorkspaces};
use hyprland::event_listener::AsyncEventListener;
use hyprland::shared::{HyprData as _, HyprDataActive as _, HyprDataActiveOptional as _};
use hyprland::dispatch::{Dispatch, DispatchType, WorkspaceIdentifierWithSpecial};
use niri_ipc::{Action, Event as NiriEvent, Request as NiriRequest, Response as NiriResponse, WorkspaceReferenceArg};
use niri_ipc::socket::Socket as NiriSocket;

use crate::utils::display;

#[derive(Debug, Clone, Default)]
pub struct CompositorClient {
    pub class: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CompositorWorkspace {
    pub id: i32,
    pub windows: u32,
}

#[derive(Default)]
pub struct Compositor {
    pub active_client: Mutable<Option<CompositorClient>>,
    pub active_workspace: Mutable<Option<CompositorWorkspace>>,
    pub workspaces: Mutable<Option<Vec<CompositorWorkspace>>>,
    /// Hyprland-only: the active keybind submap, if any non-default one is active.
    pub submap: Mutable<Option<String>>,
}

pub static COMPOSITOR: LazyLock<Compositor> = LazyLock::new(Compositor::default);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Hyprland,
    Niri,
}

pub fn detect_backend() -> Backend {
    if std::env::var_os(niri_ipc::socket::SOCKET_PATH_ENV).is_some() {
        Backend::Niri
    } else {
        Backend::Hyprland
    }
}

pub fn activate() {
    match detect_backend() {
        Backend::Hyprland => activate_hyprland(),
        Backend::Niri => activate_niri(),
    }
}

/// Switch to the workspace with the given index/id (1-based on both backends).
pub fn switch_workspace(id: i32) {
    match detect_backend() {
        Backend::Hyprland => {
            let _ = Dispatch::call(DispatchType::Workspace(WorkspaceIdentifierWithSpecial::Id(id)));
        },

        Backend::Niri => {
            niri_send(NiriRequest::Action(Action::FocusWorkspace {
                reference: WorkspaceReferenceArg::Index(id.clamp(0, i32::from(u8::MAX)) as u8),
            }));
        },
    }
}

/// Switch to the workspace `delta` steps away. Only `-1`/`1` are meaningful on niri,
/// which only exposes single-step up/down actions (no arbitrary relative jumps).
pub fn switch_workspace_relative(delta: i32) {
    match detect_backend() {
        Backend::Hyprland => {
            let _ = Dispatch::call(DispatchType::Workspace(WorkspaceIdentifierWithSpecial::RelativeMonitorIncludingEmpty(delta)));
        },

        Backend::Niri => {
            let action = if delta < 0 { Action::FocusWorkspaceUp {} } else { Action::FocusWorkspaceDown {} };
            niri_send(NiriRequest::Action(action));
        },
    }
}

pub fn get_active_monitor() -> Option<gdk::Monitor> {
    let geometry = match detect_backend() {
        Backend::Hyprland => {
            let monitor = HyprMonitor::get_active().ok()?;
            (monitor.x, monitor.y, i32::from(monitor.width), i32::from(monitor.height))
        },

        Backend::Niri => {
            let mut socket = NiriSocket::connect().ok()?;
            let Ok(Ok(NiriResponse::FocusedOutput(Some(output)))) = socket.send(NiriRequest::FocusedOutput) else {
                return None;
            };
            let logical = output.logical?;
            (logical.x, logical.y, logical.width as i32, logical.height as i32)
        },
    };

    for m in display::get_all_monitors(&gdk::Display::default()?) {
        let g = m.geometry();

        if g.x() == geometry.0 && g.y() == geometry.1 && g.width() == geometry.2 && g.height() == geometry.3 {
            return Some(m);
        }
    }

    None
}

// --- Hyprland backend --------------------------------------------------

fn refresh_hypr_active_client() {
    COMPOSITOR.active_client.set(Client::get_active().ok().flatten().map(|c| CompositorClient {
        class: c.class,
        title: c.title,
    }));
}

fn refresh_hypr_active_workspace() {
    COMPOSITOR.active_workspace.set(HyprWorkspace::get_active().ok().map(|w| CompositorWorkspace {
        id: w.id,
        windows: u32::from(w.windows),
    }));
    refresh_hypr_active_client();
}

fn refresh_hypr_workspaces() {
    COMPOSITOR.workspaces.set(HyprWorkspaces::get().ok().map(|workspaces| {
        workspaces.into_iter().map(|w| CompositorWorkspace {
            id: w.id,
            windows: u32::from(w.windows),
        }).collect()
    }));
}

fn refresh_hypr_submap(new_submap: Option<String>) {
    if let Some(submap) = new_submap {
        COMPOSITOR.submap.set((!submap.is_empty() && submap != "default").then_some(submap));
    } else if let Some(submap) = super::hyprland::call_hyprctl("submap") {
        COMPOSITOR.submap.set((!submap.is_empty() && submap != "default").then_some(submap));
    }
}

fn activate_hyprland() {
    refresh_hypr_active_workspace();
    refresh_hypr_workspaces();
    refresh_hypr_submap(None);

    tokio::spawn(async move {
        let mut event_listener = AsyncEventListener::new();

        event_listener.add_window_opened_handler(|_| Box::pin(async { refresh_hypr_active_client(); refresh_hypr_workspaces(); }));
        event_listener.add_window_closed_handler(|_| Box::pin(async { refresh_hypr_active_client() }));
        event_listener.add_active_window_changed_handler(|_| Box::pin(async { refresh_hypr_active_client() }));
        event_listener.add_float_state_changed_handler(|_| Box::pin(async { refresh_hypr_active_client() }));
        event_listener.add_window_title_changed_handler(|_| Box::pin(async { refresh_hypr_active_client() }));
        event_listener.add_fullscreen_state_changed_handler(|_| Box::pin(async { refresh_hypr_active_client() }));
        event_listener.add_workspace_added_handler(|_| Box::pin(async { refresh_hypr_workspaces() }));
        event_listener.add_workspace_deleted_handler(|_| Box::pin(async { refresh_hypr_workspaces() }));
        event_listener.add_workspace_moved_handler(|_| Box::pin(async { refresh_hypr_workspaces() }));
        event_listener.add_workspace_changed_handler(|_| Box::pin(async { refresh_hypr_active_workspace() }));
        event_listener.add_active_monitor_changed_handler(|_| Box::pin(async { refresh_hypr_active_workspace() }));
        event_listener.add_sub_map_changed_handler(|new_submap| Box::pin(async { refresh_hypr_submap(Some(new_submap)) }));

        let _ = event_listener.start_listener_async().await;
    });
}

// --- niri backend --------------------------------------------------

fn niri_send(request: NiriRequest) {
    match NiriSocket::connect() {
        Ok(mut socket) => {
            if let Err(err) = socket.send(request) {
                error!(%err, "Failed to send niri IPC request");
            }
        },

        Err(err) => error!(%err, "Failed to connect to niri socket"),
    }
}

fn refresh_niri_state(windows: &[niri_ipc::Window], workspaces: &[niri_ipc::Workspace]) {
    let window_count = |workspace_id: u64| windows.iter().filter(|w| w.workspace_id == Some(workspace_id)).count() as u32;

    let active_window = windows.iter().find(|w| w.is_focused);
    COMPOSITOR.active_client.set(active_window.map(|w| CompositorClient {
        class: w.app_id.clone().unwrap_or_default(),
        title: w.title.clone().unwrap_or_default(),
    }));

    let active_workspace = workspaces.iter().find(|w| w.is_focused);
    COMPOSITOR.active_workspace.set(active_workspace.map(|w| CompositorWorkspace {
        id: i32::from(w.idx),
        windows: window_count(w.id),
    }));

    COMPOSITOR.workspaces.set(Some(workspaces.iter().map(|w| CompositorWorkspace {
        id: i32::from(w.idx),
        windows: window_count(w.id),
    }).collect()));
}

fn activate_niri() {
    std::thread::spawn(move || {
        let mut windows: Vec<niri_ipc::Window> = Vec::new();
        let mut workspaces: Vec<niri_ipc::Workspace> = Vec::new();

        let mut socket = match NiriSocket::connect() {
            Ok(socket) => socket,
            Err(err) => {
                error!(%err, "Failed to connect to niri socket, compositor integration will not be available");
                return;
            },
        };

        match socket.send(NiriRequest::EventStream) {
            Ok(Ok(NiriResponse::Handled)) => {},
            other => {
                error!(?other, "niri did not acknowledge the event stream request");
                return;
            },
        }

        let mut read_event = socket.read_events();

        loop {
            match read_event() {
                Ok(NiriEvent::WorkspacesChanged { workspaces: new }) => workspaces = new,
                Ok(NiriEvent::WindowsChanged { windows: new }) => windows = new,

                Ok(NiriEvent::WindowOpenedOrChanged { window }) => {
                    if let Some(existing) = windows.iter_mut().find(|w| w.id == window.id) {
                        *existing = window;
                    } else {
                        windows.push(window);
                    }
                },

                Ok(NiriEvent::WindowClosed { id }) => windows.retain(|w| w.id != id),

                Ok(NiriEvent::WindowFocusChanged { id }) => {
                    for w in &mut windows {
                        w.is_focused = Some(w.id) == id;
                    }
                },

                Ok(NiriEvent::WorkspaceActivated { id, focused }) => {
                    let target_output = workspaces.iter().find(|w| w.id == id).and_then(|w| w.output.clone());

                    for w in &mut workspaces {
                        if w.id == id {
                            w.is_active = true;
                            w.is_focused = focused;
                        } else {
                            if w.output == target_output {
                                w.is_active = false;
                            }
                            if focused {
                                w.is_focused = false;
                            }
                        }
                    }
                },

                // Other event kinds (layout, keyboard layout, overview, config reload, etc.)
                // don't map onto anything this shell's widgets currently track.
                Ok(_) => continue,

                Err(err) => {
                    error!(%err, "niri event stream read failed, compositor integration stopping");
                    break;
                },
            }

            refresh_niri_state(&windows, &workspaces);
        }
    });
}
