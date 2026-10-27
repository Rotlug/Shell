mod minimal;
mod extended;
mod progress;

use gtk::prelude::*;

use crate::services::mpris;
use crate::utils::gesture;
use super::super::base::BarModule;

const VOLUME_STEP: f64 = 0.05;

pub fn new() -> BarModule {
    let module = BarModule::with_widgets(&minimal::minimal(), &extended::extended());
    module.add_css_class("bar-mpris");

    module.add_controller(gesture::on_middle_down(clone!(
        #[weak] module,
        move |_, _, _| if !module.expanded() {
            mpris::play_pause();
        }
    )));

    module.add_controller(gesture::on_secondary_down(clone!(
        #[weak] module,
        move |_, _, _| if !module.expanded() {
            mpris::next();
        }
    )));

    module.add_controller(gesture::on_vertical_scroll(clone!(
        #[weak] module,
        move |delta_y| if !module.expanded() {
            let step = if delta_y < 0.0 {
                VOLUME_STEP
            } else {
                -VOLUME_STEP
            };

            mpris::adjust_volume(step);
        }
    )));

    module
}