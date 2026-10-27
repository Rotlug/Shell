use gtk::prelude::*;

use crate::services::mpris::{self, LoopStatus, PlaybackStatus};
use crate::utils::gesture;
use super::progress;

pub static SEEK_STEP_MICROSECONDS: i64 = 5_000_000;

fn format_artist_list(artists: &[String]) -> String {
    artists.join(", ")
}

fn format_duration(microseconds: i64) -> String {
    let total_seconds = microseconds / 1_000_000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{}:{:02}", minutes, seconds)
}

fn get_background_css() -> Option<String> {
    if let Some(default_player) = mpris::get_default_player()
        && let Some(art_url) = default_player.metadata.art_url
    {
        let css = format!(
            ".bar-mpris-extended-background {{ background-image: url('{}'); }}",
            art_url.replace('\'', "\\'")
        );

        Some(css)
    } else {
        None
    }
}

fn default_mpris_player() -> gtk::Box {
    let background_style_provider = gtk::CssProvider::new();
    let progress_bar = progress::ProgressBar::new();

    view! {
        previous_button = gtk::Button {
            set_css_classes: &["bar-mpris-button"],
            set_label: "skip_previous",
            set_hexpand: false,
            connect_clicked => |_| {
                mpris::previous();
            }
        },

        play_pause_button = gtk::Button {
            set_css_classes: &["bar-mpris-button"],
            set_label: if mpris::get_default_player().is_some_and(|p| p.playback_status == PlaybackStatus::Playing) {
                "pause"
            } else {
                "play_arrow"
            },
            set_hexpand: false,
            connect_clicked => |_| {
                mpris::play_pause();
            }
        },

        next_button = gtk::Button {
            set_css_classes: &["bar-mpris-button"],
            set_label: "skip_next",
            set_hexpand: false,
            connect_clicked => |_| {
                mpris::next();
            }
        },

        loop_button = gtk::Button {
            set_css_classes: &["bar-mpris-button"],
            set_label: "repeat",
            set_hexpand: false,
            connect_clicked => |_| {
                let Some(player) = mpris::get_default_player() else {
                    return warn!("No MPRIS player available to change loop status");
                };

                let new_status = match player.loop_status {
                    LoopStatus::None => LoopStatus::Playlist,
                    LoopStatus::Playlist => LoopStatus::Track,
                    LoopStatus::Track => LoopStatus::None,
                };
                
                mpris::set_loop_status(new_status);
            }
        },

        shuffle_button = gtk::Button {
            set_css_classes: &["bar-mpris-button"],
            set_label: "shuffle",
            set_hexpand: false,
            connect_clicked => |_| {
                let Some(player) = mpris::get_default_player() else {
                    return warn!("No MPRIS player available to toggle shuffle");
                };

                let new_shuffle = !player.shuffle;
                
                mpris::set_shuffle(new_shuffle);
            }
        },

        background = gtk::Box {
            set_css_classes: &["bar-mpris-extended-background"],
            set_hexpand: true,
            set_vexpand: true
        },

        buttons = gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            set_halign: gtk::Align::Start,
            set_valign: gtk::Align::End,

            append: &previous_button,
            append: &play_pause_button,
            append: &next_button,
            append: &loop_button,
            append: &shuffle_button,
        },

        progress = gtk::Label {
            set_css_classes: &["bar-mpris-extended-progress"],
            set_hexpand: true,
            set_xalign: 1.0,
            set_label: "0:00 / 0:00",
        },

        controls = gtk::CenterBox {
            set_css_classes: &["bar-mpris-extended-controls"],
            set_orientation: gtk::Orientation::Horizontal,
            set_hexpand: true,

            set_start_widget: Some(&buttons),
            set_end_widget: Some(&progress),
        },

        metadata_artist = gtk::Label {
            set_css_classes: &["bar-mpris-extended-artist"],
            set_hexpand: true,
            set_xalign: 0.0,
            set_ellipsize: gtk::pango::EllipsizeMode::End,
        },

        metadata_title = gtk::Label {
            set_css_classes: &["bar-mpris-extended-title"],
            set_hexpand: true,
            set_xalign: 0.0,
            set_ellipsize: gtk::pango::EllipsizeMode::End,
        },

        metadata_album = gtk::Label {
            set_css_classes: &["bar-mpris-extended-album"],
            set_hexpand: true,
            set_xalign: 0.0,
            set_ellipsize: gtk::pango::EllipsizeMode::End,
        },

        metadata = gtk::Box {
            set_css_classes: &["bar-mpris-extended-metadata"],
            set_orientation: gtk::Orientation::Vertical,
            set_hexpand: true,
            set_vexpand: true,

            append: &metadata_artist,
            append: &metadata_title,
            append: &metadata_album,
        },

        over = gtk::Box {
            set_css_classes: &["bar-mpris-extended-box"],
            set_orientation: gtk::Orientation::Vertical,
            set_hexpand: true,

            append: &metadata,
            append: &progress_bar.drawing_area,
            append: &controls,
        },

        player_overlay = gtk::Overlay {
            set_child: Some(&background),
            set_overflow: gtk::Overflow::Hidden,
            add_overlay: &over,
        },

        no_players_widget = gtk::Label {
            set_css_classes: &["bar-mpris-extended-no-players"],
            set_label: "No players",
            set_hexpand: true,
            set_xalign: 0.5
        },

        widget = gtk::Box {
            set_css_classes: &["bar-mpris-extended-container"],
            set_hexpand: true,

            append: &no_players_widget,
            append: &player_overlay,
        },
    }

    background.style_context().add_provider(&background_style_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

    progress.add_controller(gesture::on_vertical_scroll(|delta| {
        let seek_amount = if delta < 0.0 {
            SEEK_STEP_MICROSECONDS
        } else {
            -SEEK_STEP_MICROSECONDS
        };

        mpris::seek(seek_amount);
    }));

    mpris::subscribe_to_default_player_changes(clone!(
        #[weak] progress,
        #[weak] progress_bar,
        move |_| {
            if let Some(default_player) = mpris::get_default_player() {
                no_players_widget.hide();
                player_overlay.show();

                let is_playing = default_player.playback_status == PlaybackStatus::Playing;
                play_pause_button.set_label(if is_playing { "pause" } else { "play_arrow" });

                if default_player.shuffle {
                    shuffle_button.set_css_classes(&["bar-mpris-button", "toggled"]);
                } else {
                    shuffle_button.set_css_classes(&["bar-mpris-button"]);
                }

                loop_button.set_label(if default_player.loop_status == LoopStatus::Track {"repeat_one"} else {"repeat"});
                if default_player.loop_status != LoopStatus::None {
                    loop_button.set_css_classes(&["bar-mpris-button", "toggled"]);
                } else {
                    loop_button.set_css_classes(&["bar-mpris-button"]);
                }

                if let (position, Some(duration)) = (mpris::get_position(), default_player.metadata.length) {
                    progress.set_label(&format!("{} / {}", format_duration(position), format_duration(duration)));
                    progress_bar.set_position(position as f64);
                    progress_bar.set_duration(duration as f64);
                }

                // update metadata labels
                metadata_title.set_label(default_player.metadata.title.as_deref().unwrap_or("Unknown Title"));
                metadata_artist.set_label(&format_artist_list(&default_player.metadata.artist.unwrap_or_default()));
                metadata_album.set_label(default_player.metadata.album.as_deref().unwrap_or("Unknown Album"));

                // set the background of background box to the album art if any
                if let Some(css) = get_background_css() {
                    background_style_provider.load_from_data(&css);
                } else {
                    background_style_provider.load_from_data("");
                }
            } else {
                no_players_widget.show();
                player_overlay.hide();
                background_style_provider.load_from_data("");
            }
        }
    ));

    // run a future that changes the progress bar value every second if playing
    glib::spawn_future_local(async move {
        loop {
            glib::source::idle_add_local_once(clone!(
                #[weak] progress,
                #[weak] progress_bar,
                move || {
                    if let Some(player) = mpris::get_default_player()
                        && player.playback_status == PlaybackStatus::Playing
                        && let Some(duration) = player.metadata.length
                    {
                        let position = mpris::get_position();
                        progress.set_label(&format!("{} / {}", format_duration(position), format_duration(duration)));
                        progress_bar.set_position(position as f64);
                        progress_bar.set_duration(duration as f64);
                    }
                }
            ));

            glib::timeout_future(std::time::Duration::from_millis(500)).await;
        }
    });

    widget
}

pub fn extended() -> gtk::Box {
    view! {
        widget = gtk::Box {
            set_css_classes: &["bar-mpris-extended"],
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            append: &default_mpris_player(),
        },
    }

    widget
}