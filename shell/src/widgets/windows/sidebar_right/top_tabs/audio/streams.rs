use std::rc::Rc;
use std::cell::RefCell;
use gtk::prelude::*;

use crate::services::wireplumber::{self, Node};
use crate::widgets::common::slider_list::{SliderList, SliderRow};

pub struct AudioStream {
    pub node: Node,
    pub row: SliderRow,
    pub mute_button: gtk::Button,
}

impl AudioStream {
    pub fn new(node: Node) -> Self {
        let node_id = node.id;

        let mute_button = gtk::Button::new();
        mute_button.set_css_classes(&["audio-stream-mute-button"]);
        mute_button.set_halign(gtk::Align::End);
        mute_button.connect_clicked(move |_| {
            let mute = wireplumber::get_node(node_id).map(|n| n.mute).unwrap_or(false);
            wireplumber::set_node_mute(node_id, !mute);
        });

        let row = SliderRow::new(
            &node.description,
            &node.name,
            node.volume as f64,
            move |volume| wireplumber::set_node_volume(node_id, volume as f32),
            Some(&mute_button),
        );

        let stream = Self {
            node,
            row,
            mute_button,
        };
        stream.update_mute_button();
        stream
    }

    fn update_mute_button(&self) {
        if self.node.mute {
            self.mute_button.set_label("volume_off");
            self.mute_button.add_css_class("muted");
        } else {
            self.mute_button.set_label("volume_up");
            self.mute_button.remove_css_class("muted");
        }
    }

    pub fn update_from(&mut self, node: &Node) {
        self.node = node.clone();
        self.row.update(&node.description, &node.name, node.volume as f64);
        self.update_mute_button();
    }
}

pub struct AudioStreams {
    pub streams: Rc<RefCell<Vec<AudioStream>>>,
    list: SliderList,
    pub root: gtk::ScrolledWindow,
}

impl Default for AudioStreams {
    fn default() -> Self {
        let list = SliderList::default();

        Self {
            streams: Rc::new(RefCell::new(Vec::new())),
            root: list.root.clone(),
            list,
        }
    }
}

impl AudioStreams {
    pub fn add_stream(&self, stream: Node) {
        let stream = AudioStream::new(stream);
        self.list.append(&stream.row);
        self.streams.borrow_mut().push(stream);
    }

    pub fn remove_stream(&self, stream: &Node) {
        let Some(index) = self.streams.borrow().iter().position(|s| s.node.id == stream.id) else {
            return;
        };

        let stream = self.streams.borrow_mut().remove(index);
        self.list.remove(&stream.row);
    }

    pub fn update_stream(&self, id: i32) {
        let Some(index) = self.streams.borrow().iter().position(|s| s.node.id == id) else {
            return;
        };

        let Some(node) = wireplumber::get_node(id) else {
            return;
        };

        self.streams.borrow_mut()[index].update_from(&node);
    }
}
