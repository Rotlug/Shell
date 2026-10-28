use std::{cell::{Cell, RefCell}, f64, rc::Rc};
use gtk::prelude::*;

use crate::scss;
use crate::utils::gesture;
use crate::services::mpris;
use super::extended::SEEK_STEP_MICROSECONDS;
use super::super::super::base::BarModule;

static WAVE_AMPLITUDE_FACTOR: f64 = 10.0;
static WAVE_SPEED: f64 = 3.0;
static WAVE_LINE_WIDTH: f64 = 3.0;

#[derive(Debug, Clone, glib::Downgrade)]
pub struct ProgressBar {
    pub drawing_area: gtk::DrawingArea,
    pub position: Rc<RefCell<f64>>,
    pub duration: Rc<RefCell<f64>>,
}

impl ProgressBar {
    pub fn new() -> Self {
        let frametime = Rc::new(RefCell::new(0.0));
        let seek_position: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
        let position = Rc::new(RefCell::new(0.0));
        let duration = Rc::new(RefCell::new(0.0));
        let drawing_area = gtk::DrawingArea::new();
        drawing_area.set_css_classes(&["bar-mpris-extended-progress-bar"]);
        drawing_area.set_content_width(200);
        drawing_area.set_content_height(24);

        // draw cool animated wavey progress bar
        drawing_area.set_draw_func(clone!(
            #[strong] position,
            #[strong] seek_position,
            #[strong] duration,
            #[strong] frametime,
            move |_, cr, width, height| {
                let pos: f64 = seek_position.borrow().map_or_else(|| *position.borrow(), |seek_pos| seek_pos);
                let dur: f64 = *duration.borrow();
                let time: f64 = *frametime.borrow();
                if dur <= 0.0 {
                    return;
                }

                let progress = (pos / dur).clamp(0.0, 1.0);
                let v_center = height as f64 / 2.0;
                let wave_amplitude = height as f64 / WAVE_AMPLITUDE_FACTOR;
                let wave_offset = |x: f64| time.mul_add(WAVE_SPEED, x / 20.0).sin() * wave_amplitude;

                let stroke = |alpha: f64, to: i32| {
                    let (r, g, b) = scss::get_color("foreground-color-primary").map_or(
                        (226.0, 226.0, 226.0), 
                        |wave_color| (wave_color.red as f64, wave_color.green as f64, wave_color.blue as f64)
                    );

                    cr.set_source_rgba(r / 255.0, g / 255.0, b / 255.0, alpha);
                    cr.set_line_width(WAVE_LINE_WIDTH);
                    cr.move_to(0.0, v_center);
                    for x in 0..=to {
                        cr.line_to(x as f64, v_center + wave_offset(x as f64));
                    }
                    let _ = cr.stroke();
                };

                let progress_width = (width as f64 * progress) as i32;
                stroke(0.3, width); // background wave
                stroke(1.0, progress_width); // foreground wave
            }
        ));
        
        // The wave is only animated while the module is expanded. A tick callback keeps the
        // frame clock (and the redrawing of the whole bar) running for as long as it exists,
        // even when what it draws can't be seen, so it is only there when it is needed.
        let tick: Rc<RefCell<Option<gtk::TickCallbackId>>> = Rc::new(RefCell::new(None));
        let set_animated = Rc::new(clone!(
            #[weak] drawing_area,
            #[strong] tick,
            move |animated: bool| {
                let mut tick = tick.borrow_mut();

                if animated && tick.is_none() {
                    *tick = Some(drawing_area.add_tick_callback(clone!(
                        #[strong] frametime,
                        move |drawing_area, clock| {
                            *frametime.borrow_mut() = clock.frame_time() as f64 / 1_000_000.0;
                            drawing_area.queue_draw();
                            glib::ControlFlow::Continue
                        }
                    )));
                } else if !animated && let Some(id) = tick.take() {
                    id.remove();
                }
            }
        ));

        let connected = Cell::new(false);
        drawing_area.connect_realize(move |drawing_area| {
            if connected.replace(true) {
                return;
            }

            let Some(module) = drawing_area.ancestor(BarModule::static_type()).and_downcast::<BarModule>() else {
                return;
            };

            set_animated(module.expanded());
            module.connect_expanded_notify(clone!(
                #[strong] set_animated,
                move |module| set_animated(module.expanded())
            ));
        });

        let primary_held = Rc::new(RefCell::new(false));
        drawing_area.add_controller(gesture::on_primary_down(clone!(
            #[strong] primary_held,
            #[strong] seek_position,
            #[strong] duration,
            #[weak] drawing_area,
            move |_, x, _| {
                *primary_held.borrow_mut() = true;

                let alloc = drawing_area.allocation();
                let rel_x = x.clamp(0.0, alloc.width() as f64);
                let new_pos = (rel_x / alloc.width() as f64) * (*duration.borrow());
                *seek_position.borrow_mut() = Some(new_pos);
                drawing_area.queue_draw();
            }
        )));

        drawing_area.add_controller(gesture::on_motion(clone!(
            #[strong] primary_held,
            #[strong] seek_position,
            #[strong] duration,
            #[weak] drawing_area,
            move |x, _| {
                if *primary_held.borrow() {
                    let alloc = drawing_area.allocation();
                    let rel_x = x.clamp(0.0, alloc.width() as f64);
                    let new_pos = (rel_x / alloc.width() as f64) * (*duration.borrow());
                    *seek_position.borrow_mut() = Some(new_pos);
                    drawing_area.queue_draw();
                }
            }
        )));

        drawing_area.add_controller(gesture::on_primary_up(clone!(
            #[strong] position,
            #[strong] duration,
            #[weak] drawing_area,
            move |_, _, _| {
                if *primary_held.borrow() {
                    let Some(seek_pos) = *seek_position.borrow() else {
                        return;
                    };

                    // Nowhere to seek to in a track without a length
                    let track_length = *duration.borrow();
                    if track_length <= 0.0 {
                        *seek_position.borrow_mut() = None;
                        *primary_held.borrow_mut() = false;
                        return;
                    }

                    let alloc = drawing_area.allocation();
                    let rel_x = ((seek_pos / track_length) * alloc.width() as f64).clamp(0.0, alloc.width() as f64);
                    let new_pos = (rel_x / alloc.width() as f64) * track_length;

                    // Where to go, not how far to go from where we think it is: that is a little
                    // behind, and different every time
                    mpris::set_position(new_pos as i64);

                    *position.borrow_mut() = new_pos;
                    *seek_position.borrow_mut() = None;
                    drawing_area.queue_draw();
                    *primary_held.borrow_mut() = false;
                }
            }
        )));

        drawing_area.add_controller(gesture::on_vertical_scroll(|delta| {
            let seek_amount = if delta < 0.0 {
                SEEK_STEP_MICROSECONDS
            } else {
                -SEEK_STEP_MICROSECONDS
            };

            mpris::seek(seek_amount);
        }));

        Self {
            drawing_area,
            position,
            duration,
        }
    }

    pub fn set_position(&self, position: f64) {
        *self.position.borrow_mut() = position;
        self.drawing_area.queue_draw();
    }

    pub fn set_duration(&self, duration: f64) {
        *self.duration.borrow_mut() = duration;
        self.drawing_area.queue_draw();
    }
}