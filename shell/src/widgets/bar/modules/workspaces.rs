use std::sync::{Mutex, LazyLock};
use futures_signals::signal::SignalExt as _;
use gtk::prelude::*;
use crate::scss;
use crate::services::compositor;
use crate::utils::gesture;
use super::super::base::BarModule;

const SHOWN_WORKSPACES: usize = 10;
const WORKSPACE_WIDTH: f64 = 13.0;
const WORKSPACE_HEIGHT: f64 = 13.0;
const WORKSPACE_Y: f64 = 5.0;
const WORKSPACE_PADDING: f64 = 1.0;
/// How visible the color of a workspace without windows is, from 0.0 to 1.0
const EMPTY_WORKSPACE_ALPHA: f64 = 0.3;

static WORKSPACE_MASK: LazyLock<Mutex<WorkspaceMask>> = LazyLock::new(|| Mutex::new(WorkspaceMask::default()));

#[derive(Clone, Default)]
struct WorkspaceMask {
    pub mask: u32
}

impl WorkspaceMask {
    pub fn update(&mut self) {
        let workspaces = compositor::COMPOSITOR.workspaces.get_cloned();
        let active_workspace = compositor::COMPOSITOR.active_workspace.get_cloned();
        
        self.mask = if let (Some(workspaces), Some(active_workspace)) = (workspaces, active_workspace) {
            let offset = (((active_workspace.id - 1) as f64 / SHOWN_WORKSPACES as f64).floor() * SHOWN_WORKSPACES as f64) as i32;
            let mut mask = 0;

            for workspace in &workspaces {
                if workspace.id > offset && workspace.id <= offset + SHOWN_WORKSPACES as i32 && workspace.windows > 0 {
                    mask |= 1 << (workspace.id - offset);
                }
            }

            mask
        } else {
            0
        };
    }
}

pub fn new() -> BarModule {
    let style_provider = gtk::CssProvider::new();

    view! {
        workspaces_click_gesture = gesture::on_primary_down(|_, x, _| {
            let ws = (((x - 5.0 - WORKSPACE_PADDING) / (WORKSPACE_WIDTH + WORKSPACE_PADDING)) + 1.0).floor() as i32;

            if ws >= 1
                && ws <= SHOWN_WORKSPACES as i32
                && let Some(active_workspace) = compositor::COMPOSITOR.active_workspace.get_cloned()
            {
                let clamped = ((active_workspace.id - 1) / 10) * 10 + ws;
                compositor::switch_workspace(clamped);
            }
        }),

        workspaces_scroll_gesture = gesture::on_vertical_scroll(|dy| {
            let delta = if dy < 0.0 { -1 } else { 1 };
            compositor::switch_workspace_relative(delta);
        }),

        workspaces_drawing_area = gtk::DrawingArea {
            set_css_classes: &["bar-workspaces-drawingarea"],

            set_draw_func: {
                move |area, cr, _, _| {
                    area.set_size_request((SHOWN_WORKSPACES as i32 + 1) * WORKSPACE_WIDTH as i32, 16);

                    let active_ws: f64 = area.pango_context().font_description()
                        .map_or(1.0, |desc| desc.size() as f64 / gtk::pango::SCALE as f64);

                    // draw workspace squares
                    for i in 0..SHOWN_WORKSPACES+1 {
                        let workspace_x = (i as f64 - 1.0).mul_add(WORKSPACE_WIDTH + WORKSPACE_PADDING, WORKSPACE_PADDING);
                        let occupied = WORKSPACE_MASK.lock().unwrap().mask & (1 << i) != 0;

                        // Every workspace has a color of its own, empty ones are a dim version of it
                        let color = scss::accent_color(i.saturating_sub(1))
                            .map(|accent| (accent, if occupied { 1.0 } else { EMPTY_WORKSPACE_ALPHA }))
                            .or_else(|| scss::get_color(if occupied {
                                "foreground-color-primary"
                            } else {
                                "foreground-color-quinary"
                            }).map(|color| (color, 1.0)));

                        if let Some((color, strength)) = color {
                            cr.set_source_rgba(
                                color.red as f64 / 255.0,
                                color.green as f64 / 255.0,
                                color.blue as f64 / 255.0,
                                color.alpha as f64 / 255.0 * strength
                            );

                            cr.rectangle(
                                ((workspace_x + (WORKSPACE_WIDTH / 4.0)) + 2.0).ceil(),
                                ((WORKSPACE_Y + (WORKSPACE_HEIGHT / 4.0)) + 1.0).ceil(),
                                ((WORKSPACE_WIDTH / 2.0) - 4.0).ceil(),
                                ((WORKSPACE_HEIGHT / 2.0) - 4.0).ceil()
                            );

                            let _ = cr.fill();
                        }
                    }

                    // draw active workspace, in the color of the workspace it is on (or closest to,
                    // while it moves)
                    let active_slot = (active_ws.round().max(1.0) as usize) - 1;
                    if let Some(color) = scss::accent_color(active_slot).or_else(|| scss::get_color("foreground-color-primary")) {
                        let active_workspace_x = (active_ws - 1.0).mul_add(WORKSPACE_WIDTH + WORKSPACE_PADDING, WORKSPACE_PADDING) + 1.0;

                        cr.set_source_rgba(
                            color.red as f64 / 255.0,
                            color.green as f64 / 255.0,
                            color.blue as f64 / 255.0,
                            color.alpha as f64 / 255.0
                        );

                        cr.rectangle(
                            ((active_workspace_x + (WORKSPACE_WIDTH / 4.0)) - 1.0).ceil(),
                            ((WORKSPACE_Y + (WORKSPACE_HEIGHT / 4.0)) - 1.0).ceil(),
                            (WORKSPACE_WIDTH / 2.0).ceil(),
                            (WORKSPACE_HEIGHT / 2.0).ceil()
                        );

                        let _ = cr.fill();
                    }
                }
            }
        },

        workspaces_box = gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 0,
            
            append: &workspaces_drawing_area
        }
    }

    workspaces_drawing_area.style_context().add_provider(&style_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

    glib::spawn_future_local({
        let workspaces_drawing_area = workspaces_drawing_area.clone();
        signal_cloned!(compositor::COMPOSITOR.workspaces, (_) {
            WORKSPACE_MASK.lock().unwrap().update();
            workspaces_drawing_area.queue_draw();
        })
    });

    glib::spawn_future_local(signal_cloned!(compositor::COMPOSITOR.active_workspace, (active) {
        WORKSPACE_MASK.lock().unwrap().update();
            
        if let Some(active) = active {
            style_provider.load_from_data(&format!(
                ".bar-workspaces-drawingarea {{ font-size: {}px; }}",
                ((active.id - 1) % SHOWN_WORKSPACES as i32) + 1
            ));
        }

        workspaces_drawing_area.queue_draw();
    }));

    let module = BarModule::builder()
        .minimal_widget(&workspaces_box)
        .build();
    module.add_css_class("bar-workspaces");
    module.add_controller(workspaces_click_gesture);
    module.add_controller(workspaces_scroll_gesture);
    module
}