use std::collections::HashMap;

use crossterm::event::{Event, KeyEvent, KeyModifiers};

#[derive(Debug, PartialEq, Clone)]
pub struct KeyMap<T: Copy> {
    pub maps: HashMap<KeyEvent, T>,
}

impl<T: Copy> KeyMap<T> {
    pub fn event_to_action(&self, evt: &Event) -> Option<T> {
        if let Event::Key(evt) = evt {
            self.maps
                .get(&KeyEvent {
                    modifiers: evt.modifiers & !KeyModifiers::SHIFT,
                    ..*evt
                })
                .copied()
        } else {
            None
        }
    }
}

macro_rules! normalized_char {
    ($ch:expr) => {
        /*if $ch.is_ascii_uppercase() {
            KeyEvent {
                code: KeyCode::Char($ch),
                modifiers: KeyModifiers::SHIFT,
            }
        } else {*/
        KeyEvent {
            code: KeyCode::Char($ch),
            modifiers: KeyModifiers::NONE,
        }
        /*}*/
    };
}

macro_rules! k {
    ($map:ident, ($ch:expr => $act:expr)) => {
        $map.insert(normalized_char!($ch), $act);
    };

    ($map:ident, (alt $ch:expr => $act:expr)) => {
        let mut norm = normalized_char!($ch);
        norm.modifiers |= KeyModifiers::ALT;
        $map.insert(norm, $act);
    };

    ($map:ident, (ctrl $ch:expr => $act:expr)) => {
        let mut norm = normalized_char!($ch);
        norm.modifiers |= KeyModifiers::CONTROL;
        $map.insert(norm, $act);
    };

    ($map:ident, (key $key:path => $act:expr)) => {
        $map.insert(
            KeyEvent {
                code: $key,
                modifiers: KeyModifiers::NONE,
            },
            $act,
        );
    };
}

macro_rules! keys {
	($($mapping:tt),*) => {
    	{
        	let mut map = HashMap::new();
    		$(k!(map, $mapping);)*
    		map
    	}
	}
}
