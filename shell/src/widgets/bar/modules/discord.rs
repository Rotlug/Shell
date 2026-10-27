use gtk::prelude::*;

use crate::services::discord::{self, Event, User, avatar};
use super::super::base::BarModule;

/// Shown until the avatar has been downloaded (and if that never works out)
const PLACEHOLDER_COLOR: u32 = 0x0D0D_0DFF;

struct Participant {
    name: String,
    widget: gtk::Image,
}

struct Participants {
    box_: gtk::Box,
    participants: Vec<Participant>,
}

impl Participants {
    fn add(&mut self, user: User) {
        view! {
            widget = gtk::Image {
                set_css_classes: &["bar-discord-avatar"],
                set_pixel_size: avatar::SIZE as i32,
                set_tooltip_text: Some(&user.name),
            }
        }

        if let Some(placeholder) = gtk::gdk_pixbuf::Pixbuf::new(
            gtk::gdk_pixbuf::Colorspace::Rgb,
            true,
            8,
            avatar::SIZE as i32,
            avatar::SIZE as i32,
        ) {
            placeholder.fill(PLACEHOLDER_COLOR);
            widget.set_from_pixbuf(Some(&placeholder));
        }

        glib::spawn_future_local(clone!(
            #[weak] widget,
            async move {
                // The user may be gone by the time the picture is there, and that is fine
                if let Some(avatar) = avatar::load(user.avatar).await {
                    widget.set_from_pixbuf(Some(&pixbuf_from(avatar)));
                }
            }
        ));

        self.box_.append(&widget);
        self.participants.push(Participant { name: user.name, widget });
        self.update_visibility();
    }

    fn remove(&mut self, name: &str) {
        if let Some(index) = self.participants.iter().position(|participant| participant.name == name) {
            let participant = self.participants.remove(index);
            self.box_.remove(&participant.widget);
            self.update_visibility();
        }
    }

    fn update_visibility(&self) {
        self.box_.set_visible(!self.participants.is_empty());
    }
}

fn pixbuf_from(avatar: avatar::Avatar) -> gtk::gdk_pixbuf::Pixbuf {
    let size = avatar::SIZE as i32;

    gtk::gdk_pixbuf::Pixbuf::from_mut_slice(
        avatar.pixels,
        gtk::gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        size,
        size,
        size * 4,
    )
}

pub fn new() -> BarModule {
    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    box_.set_visible(false);
    box_.set_hexpand(false);

    let mut participants = Participants {
        box_: box_.clone(),
        participants: Vec::new(),
    };

    glib::spawn_future_local(async move {
        // Subscribe before looking at who is there already, so nothing falls in between
        let mut receiver = discord::subscribe();

        for user in discord::users() {
            participants.add(user);
        }

        while let Ok(event) = receiver.recv().await {
            match event {
                Event::UserJoined(user) => participants.add(user),
                Event::UserLeft(name) => participants.remove(&name),
            }
        }
    });

    let module = BarModule::builder()
        .minimal_widget(&box_)
        .build();
    module.add_css_class("bar-discord");
    module
}
