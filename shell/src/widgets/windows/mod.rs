pub mod popup;
pub mod sidebar_right;

use std::any::Any;

use crate::APP_LOCAL;

pub trait GmsWindow: Any {
    fn show(&self);
    fn hide(&self);
    fn toggle(&self) -> bool;
    fn is_visible(&self) -> bool;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl dyn GmsWindow {
    pub fn downcast_ref<T: GmsWindow>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }

    pub fn downcast_mut<T: GmsWindow>(&mut self) -> Option<&mut T> {
        self.as_any_mut().downcast_mut::<T>()
    }
}

pub fn with<F, R>(window: &str, callback: F) -> Option<R>
where
    F: FnOnce(&dyn GmsWindow) -> R,
{
    APP_LOCAL.with(|app| {
        let windows = app.windows.borrow();
        let borrow_attempt = windows.get(window);

        borrow_attempt.map(|win| callback(win.as_ref()))
    })
}


pub fn hide(window: &str) {
    with(window, |win| win.hide());
}

pub fn toggle(window: &str) -> bool {
    with(window, |win| win.toggle()).unwrap_or(false)
}

pub fn hide_all_popups() {
    APP_LOCAL.with(|app| {
        for window in app.windows.borrow().values() {
            if window.downcast_ref::<popup::PopupWindow>().is_some() {
                window.hide();
            }
        }
    });
}


