//! Audio mixing, backed by the `volume_mixer` crate
//! instead of the old astal-wireplumber C++ FFI bridge - talks to PipeWire
//! directly, no WirePlumber/astal headers needed at build time.
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, OnceLock, RwLock};
use async_broadcast::Receiver;
use volume_mixer::node::{NodeKind, NodeProps};
use volume_mixer::{Command, Event as PwEvent, Mixer, ObjectId};
use tokio::sync::mpsc;

use crate::utils::broadcast::BroadcastChannel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointType {
    Microphone,
    Speaker,
    #[allow(dead_code)]
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub volume: f32,
    pub mute: bool,
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub type_: EndpointType,
    pub is_default: bool,
    pub node: Node,
}

#[derive(Debug, Clone)]
pub enum WpEvent {
    UpdateNode(i32, String),
    UpdateEndpoint(i32, String),
    UpdateDefaultMicrophone(i32),
    UpdateDefaultSpeaker(i32),
    CreateStream(Node),
    RemoveStream(Node),
    CreateRecorder(Node),
    RemoveRecorder(Node),
    CreateMicrophone(Endpoint),
    RemoveMicrophone(Endpoint),
    CreateSpeaker(Endpoint),
    RemoveSpeaker(Endpoint),
}

pub static CHANNEL: LazyLock<BroadcastChannel<WpEvent>> = LazyLock::new(|| BroadcastChannel::new(10));

static STREAMS: OnceLock<RwLock<Vec<Node>>> = OnceLock::new();
static RECORDERS: OnceLock<RwLock<Vec<Node>>> = OnceLock::new();
static ENDPOINTS: OnceLock<RwLock<Vec<Endpoint>>> = OnceLock::new();
static COMMAND_TX: OnceLock<mpsc::Sender<Command>> = OnceLock::new();
static MIXER: OnceLock<Mixer> = OnceLock::new();

fn broadcast(event: WpEvent) {
    CHANNEL.spawn_send(event);
}

fn send_command(command: Command) {
    if let Some(tx) = COMMAND_TX.get() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(command).await;
        });
    }
}

pub fn subscribe() -> Receiver<WpEvent> {
    CHANNEL.subscribe()
}

pub fn subscribe_default_speaker_volume<F>(callback: F)
where
    F: Fn(f32) + 'static
{
    glib::spawn_future_local(async move {
        // Seed from whatever's already resolved - a default speaker can
        // exist before this subscription starts listening (e.g. this UI
        // element is created after startup enumeration already settled).
        if let Some(default_speaker) = get_default_speaker() {
            callback(default_speaker.node.volume);
        }

        let mut receiver = subscribe();
        while receiver.recv().await.is_ok() {
            // Re-resolve the current default on every event rather than
            // matching specific variants/ids - which default sink/source
            // metadata actually resolves to, and when, isn't reliably
            // orderable against node enumeration at startup, so trying to
            // special-case "the right" event is fragile. Recomputing is
            // cheap and always converges to the right value regardless.
            if let Some(default_speaker) = get_default_speaker() {
                callback(default_speaker.node.volume);
            }
        }
    });
}

pub fn get_node(id: i32) -> Option<Node> {
    STREAMS.get()?.read().ok()?.iter().find(|n| n.id == id).cloned()
        .or_else(|| RECORDERS.get()?.read().ok()?.iter().find(|n| n.id == id).cloned())
}

pub fn get_endpoint(id: i32) -> Option<Endpoint> {
    ENDPOINTS.get()?.read().ok()?.iter().find(|e| e.node.id == id).cloned()
}

pub fn get_default_speaker() -> Option<Endpoint> {
    ENDPOINTS.get()?.read().ok()?.iter().find(|e| e.is_default && e.type_ == EndpointType::Speaker).cloned()
}

pub fn get_default_microphone() -> Option<Endpoint> {
    ENDPOINTS.get()?.read().ok()?.iter().find(|e| e.is_default && e.type_ == EndpointType::Microphone).cloned()
}

/// Sets a node's volume (0.0-1.5, same range the old astal-backed code used).
pub fn set_node_volume(id: i32, volume: f32) {
    send_command(Command::ChangeVolume(ObjectId(id as u32), volume));
}

/// Sets a node's mute state.
pub fn set_node_mute(id: i32, mute: bool) {
    send_command(Command::Mute(ObjectId(id as u32), mute));
}

/// Makes the given microphone/speaker node the default input/output.
pub fn set_default(id: i32) {
    let Some(endpoint) = get_endpoint(id) else { return };

    send_command(match endpoint.type_ {
        EndpointType::Microphone => Command::ChangeDefaultSource(ObjectId(id as u32)),
        _ => Command::ChangeDefaultSink(ObjectId(id as u32)),
    });
}

/// Mirrors volume_mixer's own `Display for NodeProps` priority: nick > app name >
/// description > raw node name, so there's always something sensible to show.
fn display_name(props: &NodeProps) -> String {
    props.nick.clone()
        .or_else(|| props.app_name.clone())
        .or_else(|| props.description.clone())
        .unwrap_or_else(|| props.name.clone())
}

fn node_from_props(props: &NodeProps) -> Node {
    Node {
        id: props.id.0 as i32,
        name: props.name.clone(),
        description: display_name(props),
        volume: 0.0,
        mute: false,
    }
}

fn handle_node_added(props_by_id: &mut HashMap<ObjectId, Arc<NodeProps>>, props: Arc<NodeProps>) {
    props_by_id.insert(props.id, props.clone());

    match props.kind {
        NodeKind::Sink | NodeKind::Source => {
            let endpoint = Endpoint {
                type_: if props.kind == NodeKind::Source { EndpointType::Microphone } else { EndpointType::Speaker },
                is_default: false,
                node: node_from_props(&props),
            };

            if let Some(endpoints) = ENDPOINTS.get() && let Ok(mut endpoints) = endpoints.write() {
                endpoints.push(endpoint.clone());
            }

            broadcast(match endpoint.type_ {
                EndpointType::Microphone => WpEvent::CreateMicrophone(endpoint),
                _ => WpEvent::CreateSpeaker(endpoint),
            });
        },

        NodeKind::AppOut => {
            let node = node_from_props(&props);
            if let Some(streams) = STREAMS.get() && let Ok(mut streams) = streams.write() {
                streams.push(node.clone());
            }
            broadcast(WpEvent::CreateStream(node));
        },

        NodeKind::AppIn => {
            let node = node_from_props(&props);
            if let Some(recorders) = RECORDERS.get() && let Ok(mut recorders) = recorders.write() {
                recorders.push(node.clone());
            }
            broadcast(WpEvent::CreateRecorder(node));
        },
    }
}

fn handle_node_removed(props_by_id: &mut HashMap<ObjectId, Arc<NodeProps>>, id: ObjectId) {
    let Some(props) = props_by_id.remove(&id) else { return };
    let raw_id = id.0 as i32;

    match props.kind {
        NodeKind::Sink | NodeKind::Source => {
            let removed = ENDPOINTS.get().and_then(|endpoints| {
                let mut endpoints = endpoints.write().ok()?;
                let index = endpoints.iter().position(|e| e.node.id == raw_id)?;
                Some(endpoints.remove(index))
            });

            if let Some(endpoint) = removed {
                broadcast(match endpoint.type_ {
                    EndpointType::Microphone => WpEvent::RemoveMicrophone(endpoint),
                    _ => WpEvent::RemoveSpeaker(endpoint),
                });
            }
        },

        NodeKind::AppOut => {
            let removed = STREAMS.get().and_then(|streams| {
                let mut streams = streams.write().ok()?;
                let index = streams.iter().position(|n| n.id == raw_id)?;
                Some(streams.remove(index))
            });

            if let Some(node) = removed {
                broadcast(WpEvent::RemoveStream(node));
            }
        },

        NodeKind::AppIn => {
            let removed = RECORDERS.get().and_then(|recorders| {
                let mut recorders = recorders.write().ok()?;
                let index = recorders.iter().position(|n| n.id == raw_id)?;
                Some(recorders.remove(index))
            });

            if let Some(node) = removed {
                broadcast(WpEvent::RemoveRecorder(node));
            }
        },
    }
}

fn handle_property_changed(id: ObjectId, property_name: &str, update: impl Fn(&mut Node)) {
    let raw_id = id.0 as i32;

    if let Some(endpoints) = ENDPOINTS.get()
        && let Ok(mut endpoints) = endpoints.write()
        && let Some(endpoint) = endpoints.iter_mut().find(|e| e.node.id == raw_id)
    {
        update(&mut endpoint.node);
        broadcast(WpEvent::UpdateEndpoint(raw_id, property_name.to_owned()));
        return;
    }

    for store in [STREAMS.get(), RECORDERS.get()].into_iter().flatten() {
        if let Ok(mut nodes) = store.write()
            && let Some(node) = nodes.iter_mut().find(|n| n.id == raw_id)
        {
            update(node);
            broadcast(WpEvent::UpdateNode(raw_id, property_name.to_owned()));
            return;
        }
    }
}

fn handle_default_changed(id: ObjectId, endpoint_type: EndpointType) {
    let raw_id = id.0 as i32;

    if let Some(endpoints) = ENDPOINTS.get() && let Ok(mut endpoints) = endpoints.write() {
        for e in endpoints.iter_mut().filter(|e| e.type_ == endpoint_type) {
            e.is_default = e.node.id == raw_id;
        }
    }

    broadcast(match endpoint_type {
        EndpointType::Microphone => WpEvent::UpdateDefaultMicrophone(raw_id),
        _ => WpEvent::UpdateDefaultSpeaker(raw_id),
    });
}

pub fn activate() {
    let _ = STREAMS.set(RwLock::new(Vec::new()));
    let _ = RECORDERS.set(RwLock::new(Vec::new()));
    let _ = ENDPOINTS.set(RwLock::new(Vec::new()));

    let (command_tx, command_rx) = mpsc::channel(16);
    let _ = COMMAND_TX.set(command_tx);

    let (mixer, mut event_rx) = Mixer::new(command_rx);
    let _ = MIXER.set(mixer);

    tokio::spawn(async move {
        let mut props_by_id: HashMap<ObjectId, Arc<NodeProps>> = HashMap::new();

        while let Some(event) = event_rx.recv().await {
            match event {
                PwEvent::NodeAdded(props) => handle_node_added(&mut props_by_id, props),
                PwEvent::NodeRemoved(id) => handle_node_removed(&mut props_by_id, id),
                PwEvent::NodeVolumeChanged(id, volume) => handle_property_changed(id, "volume", move |n| n.volume = volume),
                PwEvent::NodeMuted(id, mute) => handle_property_changed(id, "mute", move |n| n.mute = mute),
                PwEvent::DefaultSourceChanged(id) => handle_default_changed(id, EndpointType::Microphone),
                PwEvent::DefaultSinkChanged(id) => handle_default_changed(id, EndpointType::Speaker),
            }
        }
    });
}
