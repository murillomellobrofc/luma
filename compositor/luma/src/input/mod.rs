pub mod grabs;
mod keyboard;
mod pointer;
pub(crate) mod shortcuts;

use smithay::backend::input::{InputBackend, InputEvent};

use crate::state::LumaState;

impl LumaState {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => keyboard::handle_key::<I>(self, event),
            InputEvent::PointerMotionAbsolute { event, .. } => {
                pointer::handle_motion_absolute::<I>(self, event)
            }
            InputEvent::PointerButton { event, .. } => pointer::handle_button::<I>(self, event),
            InputEvent::PointerAxis { event, .. } => pointer::handle_axis::<I>(self, event),
            // PointerMotion (delta), Touch, Switch, etc — chegam quando rolar
            // backend nativo (libinput). Winit nested manda só PointerMotionAbsolute.
            _ => {}
        }
    }
}
