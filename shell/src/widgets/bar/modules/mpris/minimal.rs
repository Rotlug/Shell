use std::cell::RefCell;
use gtk::prelude::*;

use crate::services::mpris;
use color::{dominant::dominant_color, models::Rgba};

const ALBUM_ART_WIDTH: i32 = 24; // Expected width of the album art image
const ALBUM_ART_HEIGHT: i32 = 24; // Expected height of the album art image
const WIDGET_WIDTH: i32 = 175;
const MAX_TRACK_WIDTH: i32 = WIDGET_WIDTH - ALBUM_ART_WIDTH;

/// How big the cover is made to look for its dominant color: a lot less than it is, which is
/// quicker and doesn't change what the dominant color is.
const COLOR_SAMPLE_SIZE: i32 = 32;

/// The pixels of the cover, scaled down.
fn sample_pixels(pixbuf: &gtk::gdk_pixbuf::Pixbuf) -> Vec<Rgba> {
    let Some(sample) = pixbuf.scale_simple(COLOR_SAMPLE_SIZE, COLOR_SAMPLE_SIZE, gtk::gdk_pixbuf::InterpType::Bilinear) else {
        return Vec::new();
    };

    let bytes = sample.read_pixel_bytes();
    let channels = sample.n_channels() as usize;
    let rowstride = sample.rowstride() as usize;
    let has_alpha = sample.has_alpha();

    (0..sample.height() as usize)
        .flat_map(|y| (0..sample.width() as usize).map(move |x| y * rowstride + x * channels))
        .filter_map(|offset| Some(Rgba {
            red: *bytes.get(offset)?,
            green: *bytes.get(offset + 1)?,
            blue: *bytes.get(offset + 2)?,
            alpha: if has_alpha { *bytes.get(offset + 3)? } else { 255 },
        }))
        .collect()
}

/// Shows the title in `color`, or in the color of the styles if there is none.
fn set_title_color(title: &gtk::Label, color: Option<Rgba>) {
    let attributes = color.map(|Rgba { red, green, blue, .. }| {
        let attributes = gtk::pango::AttrList::new();
        // Pango colors have 16 bits per channel
        attributes.insert(gtk::pango::AttrColor::new_foreground(
            u16::from(red) * 257,
            u16::from(green) * 257,
            u16::from(blue) * 257,
        ));
        attributes
    });

    title.set_attributes(attributes.as_ref());
}

fn get_mpris_player_label_text() -> String {
    mpris::get_default_player()
        .map_or_else(|| "No players".to_owned(), |player| player.metadata.title.unwrap_or("No title".to_owned()))
}

pub fn minimal() -> gtk::Box {
    let current_art_url = RefCell::new(" ".to_owned());

    view! {
        no_players_widget = gtk::Label {
            set_css_classes: &["bar-mpris-track"],
            set_label: "No players",
            set_hexpand: true,
            set_xalign: 0.5
        },

        current_track = gtk::Label {
            set_css_classes: &["bar-mpris-track"],
            set_label: &get_mpris_player_label_text(),
            set_hexpand: true,
            set_xalign: 0.5,
            set_ellipsize: gtk::pango::EllipsizeMode::End
        },

        current_album_art = gtk::Image {
            set_width_request: ALBUM_ART_WIDTH,
            set_height_request: ALBUM_ART_HEIGHT,
            set_pixel_size: ALBUM_ART_WIDTH,
        },

        paused_overlay = gtk::CenterBox {
            set_css_classes: &["bar-mpris-paused-overlay"],
            set_center_widget: Some(&gtk::Label::new(Some("▶"))),
            set_visible: mpris::get_default_player().is_some_and(|p| p.playback_status != mpris::PlaybackStatus::Playing),
        },

        album_overlay = gtk::Overlay {
            set_child: Some(&current_album_art),
            add_overlay: &paused_overlay,
        },

        players_widget = gtk::Box {
            set_hexpand: false,

            append: &album_overlay,

            libadwaita::Clamp {
                set_child: Some(&current_track),
                set_width_request: MAX_TRACK_WIDTH,
                set_maximum_size: MAX_TRACK_WIDTH,
                set_unit: libadwaita::LengthUnit::Px
            }
        },

        widget = gtk::Box {
            set_hexpand: false,
            set_width_request: WIDGET_WIDTH,

            append: &no_players_widget,
            append: &players_widget
        },
    }

    mpris::subscribe_to_default_player_changes(move |_| {
        if let Some(default_player) = mpris::get_default_player() {
            no_players_widget.hide();
            players_widget.show();

            current_track.set_label(&get_mpris_player_label_text());
            paused_overlay.set_visible(default_player.playback_status != mpris::PlaybackStatus::Playing);

            let make_blank_art = || {
                // No cover, no color to take from it
                set_title_color(&current_track, None);

                // Create blank pixbuf filled with color #0D0D0D
                let blank_pixbuf = gtk::gdk_pixbuf::Pixbuf::new(
                    gtk::gdk_pixbuf::Colorspace::Rgb,
                    true,
                    8,
                    ALBUM_ART_WIDTH,
                    ALBUM_ART_HEIGHT
                );

                if let Some(blank_pixbuf) = blank_pixbuf {
                    blank_pixbuf.fill(0x0D0D_0DFF);
                    current_album_art.set_from_pixbuf(Some(&blank_pixbuf));
                } else {
                    error!("Failed to create blank pixbuf for album art");
                }
            };

            if let Some(art_url) = default_player.metadata.art_url {
                if *current_art_url.borrow() != *art_url {
                    *current_art_url.borrow_mut() = art_url.clone();

                    // URL-decode the album art URL
                    let art_url = match urlencoding::decode(&art_url.replace("file://", "")) {
                        Ok(decoded) => decoded.into_owned(),
                        Err(e) => {
                            error!(%e, "Failed to decode album art URL");
                            set_title_color(&current_track, None);
                            return;
                        }
                    };

                    // Make pixbuf from album art
                    match gtk::gdk_pixbuf::Pixbuf::from_file(&*art_url) {
                        Ok(pixbuf) => if let Some(scaled_pixbuf) = pixbuf.scale_simple(
                            ALBUM_ART_WIDTH,
                            ALBUM_ART_HEIGHT,
                            gtk::gdk_pixbuf::InterpType::Tiles
                        ) {
                            set_title_color(&current_track, dominant_color(sample_pixels(&pixbuf)));
                            current_album_art.set_from_pixbuf(Some(&scaled_pixbuf));
                        } else {
                            warn!(%art_url, "Failed to scale album art from file");
                            make_blank_art();
                        },

                        Err(e) => {
                            warn!(%art_url, %e, "Failed to load album art from file");
                            make_blank_art();
                        }
                    }
                }
            } else if !(*current_art_url.borrow()).is_empty() {
                current_art_url.borrow_mut().clear();
                make_blank_art();
            }
        } else {
            players_widget.hide();
            no_players_widget.show();
        }
    });

    widget
}