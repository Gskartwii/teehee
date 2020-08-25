use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::modes::normal::Normal;
use super::modes::quitting;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;
use maplit::hashmap;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;

pub struct Command {
    pub command: String,
    pub cursor: usize,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    RemoveLast,
    RemoveThis,
    CursorLeft,
    CursorRight,
    Finish,
    Cancel,
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            (key KeyCode::Backspace => Action::RemoveLast),
            (key KeyCode::Delete => Action::RemoveThis),
            (key KeyCode::Enter => Action::Finish),
            (key KeyCode::Esc => Action::Cancel),
            (key KeyCode::Left => Action::CursorLeft),
            (key KeyCode::Right => Action::CursorRight)
        ),
    }
}

mod cmd {
    use super::*;

    pub fn quit(buf: &mut Buffers, _: &str) -> ModeTransition {
        if buf.iter().any(|x| x.dirty && x.path.is_some()) {
            ModeTransition::new_mode_and_info(
                Normal::new(),
                "unsaved changes! Run :wq or :q! instead.".into(),
            )
        } else {
            ModeTransition::new_mode(quitting::Quitting {})
        }
    }

    pub fn force_quit(_: &mut Buffers, _: &str) -> ModeTransition {
        ModeTransition::new_mode(quitting::Quitting {})
    }

    pub fn write(buf: &mut Buffers, _: &str) -> ModeTransition {
        if let Some(path) = buf.current().path.as_ref() {
            if let Err(e) = fs::write(&path, buf.current().data.slice_to_cow(..)) {
                return ModeTransition::new_mode_and_info(
                    Normal::new(),
                    format!("write failed: {}", e),
                );
            }
            buf.current_mut().dirty = false;
            ModeTransition::new_mode(Normal::new())
        } else {
            ModeTransition::new_mode_and_info(Normal::new(), "buffer has no path".into())
        }
    }

    pub fn write_all(buffers: &mut Buffers, _: &str) -> ModeTransition {
        for buf in buffers.iter_mut() {
            if let Some(path) = buf.path.as_ref() {
                if let Err(e) = fs::write(&path, buf.data.slice_to_cow(..)) {
                    return ModeTransition::new_mode_and_info(
                        Normal::new(),
                        format!("write failed: {}", e),
                    );
                }
                buf.dirty = false;
            }
        }
        ModeTransition::new_mode(Normal::new())
    }

    pub fn write_quit(buffers: &mut Buffers, _: &str) -> ModeTransition {
        for buf in buffers.iter_mut() {
            if let Some(path) = buf.path.as_ref() {
                if let Err(e) = fs::write(&path, buf.data.slice_to_cow(..)) {
                    return ModeTransition::new_mode_and_info(
                        Normal::new(),
                        format!("write failed: {}", e),
                    );
                }
                buf.dirty = false;
            }
        }
        ModeTransition::new_mode(quitting::Quitting {})
    }

    pub fn edit(buffers: &mut Buffers, filename: &str) -> ModeTransition {
        let result = buffers.switch_buffer(filename);
        if let Err(e) = result {
            return ModeTransition::new_mode_and_info(Normal::new(), format!("{}", e));
        }
        ModeTransition::new_mode_and_dirty(Normal::new(), DirtyBytes::ChangeLength)
    }

    pub fn delete_buffer(buffers: &mut Buffers, _: &str) -> ModeTransition {
        if buffers.current().dirty && buffers.current().path.is_some() {
            return ModeTransition::new_mode_and_info(
                Normal::new(),
                "buffer is dirty, use :db! if you're sure".to_string(),
            );
        }
        buffers.delete_current();
        ModeTransition::new_mode_and_dirty(Normal::new(), DirtyBytes::ChangeLength)
    }

    pub fn force_delete_buffer(buffers: &mut Buffers, _: &str) -> ModeTransition {
        buffers.delete_current();
        ModeTransition::new_mode_and_dirty(Normal::new(), DirtyBytes::ChangeLength)
    }
}

type CommandHandler = fn(&mut Buffers, &str) -> ModeTransition;

fn default_commands() -> HashMap<String, CommandHandler> {
    hashmap![
        "q".to_string() => cmd::quit as CommandHandler,
        "quit".to_string() => cmd::quit as CommandHandler,
        "q!".to_string() => cmd::force_quit as CommandHandler,
        "quit!".to_string() => cmd::force_quit as CommandHandler,
        "w".to_string() => cmd::write as CommandHandler,
        "write".to_string() => cmd::write as CommandHandler,
        "wq".to_string() => cmd::write_quit as CommandHandler,
        "wa".to_string() => cmd::write_all as CommandHandler,
        "write-all".to_string() => cmd::write_all as CommandHandler,
        "e".to_string() => cmd::edit as CommandHandler,
        "edit".to_string() => cmd::edit as CommandHandler,
        "db".to_string() => cmd::delete_buffer as CommandHandler,
        "delete-buffer".to_string() => cmd::delete_buffer as CommandHandler,
        "db!".to_string() => cmd::force_delete_buffer as CommandHandler,
        "delete-buffer!".to_string() => cmd::force_delete_buffer as CommandHandler,
    ]
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
    static ref DEFAULT_COMMANDS: HashMap<String, CommandHandler> = default_commands();
}

impl Command {
    pub fn new() -> Command {
        Command {
            cursor: 0,
            command: String::new(),
        }
    }

    fn finish(&self, buffers: &mut Buffers) -> ModeTransition {
        let (name, rest) = self
            .command
            .split_at(self.command.find(' ').unwrap_or(self.command.len()));
        if let Some(handler) = DEFAULT_COMMANDS.get(name) {
            handler(buffers, if rest.len() == 0 { rest } else { &rest[1..] })
        } else {
            ModeTransition::new_mode_and_info(Normal::new(), format!("Unknown command {}", name))
        }
    }
}

impl Mode for Command {
    fn name(&self) -> Cow<'static, str> {
        "COMMAND".into()
    }

    fn transition(&self, evt: &Event, buffers: &mut Buffers, _: usize) -> Option<ModeTransition> {
        if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
            let mut cursor = self.cursor;
            let mut command = self.command.to_owned();

            match action {
                Action::RemoveLast if cursor != 0 => {
                    command.remove(cursor - 1);
                    cursor -= 1;
                }
                Action::RemoveLast => return Some(ModeTransition::None),
                Action::RemoveThis => {
                    command.remove(cursor);
                } // Don't move the cursor
                Action::CursorLeft if cursor != 0 => {
                    cursor -= 1;
                }
                Action::CursorLeft => {}
                Action::CursorRight if cursor < command.len() => {
                    cursor += 1;
                }
                Action::CursorRight => {}
                Action::Cancel => return Some(ModeTransition::new_mode(Normal::new())),
                Action::Finish => return Some(self.finish(buffers)),
            }
            Some(ModeTransition::new_mode(Command { command, cursor }))
        } else if let Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers,
        }) = evt
        {
            if !(*modifiers & !KeyModifiers::SHIFT).is_empty() {
                return None;
            }
            let mut command = self.command.to_owned();
            let mut cursor = self.cursor;
            command.insert(cursor, *ch);
            cursor += 1;
            Some(ModeTransition::new_mode(Command { command, cursor }))
        } else {
            None
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
