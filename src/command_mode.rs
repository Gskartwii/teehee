use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::modes::normal::Normal;
use super::modes::quitting;
use super::view::view_options::{DirtyBytes, ViewOptions};
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

    pub fn quit(buf: &mut Buffers, options: &mut ViewOptions, _: &str) -> ModeTransition {
        if buf.iter().any(|x| x.dirty && x.path.is_some()) {
            options.info = Some("unsaved changes! Run :wq or :q! instead.".into());
            ModeTransition::new_mode(Normal::new())
        } else {
            ModeTransition::new_mode(quitting::Quitting {})
        }
    }

    pub fn force_quit(_: &mut Buffers, _: &mut ViewOptions, _: &str) -> ModeTransition {
        ModeTransition::new_mode(quitting::Quitting {})
    }

    pub fn write(buf: &mut Buffers, options: &mut ViewOptions, filename: &str) -> ModeTransition {
        let path = if filename.is_empty() {
            buf.current().path.as_ref().map(|p| p.as_path())
        } else {
            Some(std::path::Path::new(&filename))
        };

        if let Some(path) = path {
            if let Err(e) = fs::write(&path, buf.current().data.slice_to_cow(..)) {
                options.info = Some(format!("write failed: {}", e));
                return ModeTransition::new_mode(Normal::new());
            }

            let owned_path = path.to_owned();
            let buf_mut = buf.current_mut();
            buf_mut.dirty = false;
            buf_mut.update_path_if_missing(owned_path);
            ModeTransition::new_mode(Normal::new())
        } else {
            options.info = Some("buffer has no path".into());
            ModeTransition::new_mode(Normal::new())
        }
    }

    pub fn write_all(buffers: &mut Buffers, options: &mut ViewOptions, _: &str) -> ModeTransition {
        for buf in buffers.iter_mut() {
            if let Some(path) = buf.path.as_ref() {
                if let Err(e) = fs::write(&path, buf.data.slice_to_cow(..)) {
                    options.info = Some(format!("write failed: {}", e));
                    return ModeTransition::new_mode(Normal::new());
                }
                buf.dirty = false;
            }
        }
        ModeTransition::new_mode(Normal::new())
    }

    pub fn write_quit(buffers: &mut Buffers, options: &mut ViewOptions, _: &str) -> ModeTransition {
        for buf in buffers.iter_mut() {
            if let Some(path) = buf.path.as_ref() {
                if let Err(e) = fs::write(&path, buf.data.slice_to_cow(..)) {
                    options.info = Some(format!("write failed: {}", e));
                    return ModeTransition::new_mode(Normal::new());
                }
                buf.dirty = false;
            }
        }
        ModeTransition::new_mode(quitting::Quitting {})
    }

    pub fn edit(
        buffers: &mut Buffers,
        options: &mut ViewOptions,
        filename: &str,
    ) -> ModeTransition {
        let result = buffers.switch_buffer(filename);
        if let Err(e) = result {
            options.info = Some(e.to_string());
            return ModeTransition::new_mode(Normal::new());
        }
        options.make_dirty(DirtyBytes::ChangeLength);
        ModeTransition::new_mode(Normal::new())
    }

    pub fn delete_buffer(
        buffers: &mut Buffers,
        options: &mut ViewOptions,
        _: &str,
    ) -> ModeTransition {
        if buffers.current().dirty && buffers.current().path.is_some() {
            options.info = Some("buffer is dirty, use :db! if you're sure".to_string());
            return ModeTransition::new_mode(Normal::new());
        }
        buffers.delete_current();
        options.make_dirty(DirtyBytes::ChangeLength);
        ModeTransition::new_mode(Normal::new())
    }

    pub fn force_delete_buffer(
        buffers: &mut Buffers,
        options: &mut ViewOptions,
        _: &str,
    ) -> ModeTransition {
        buffers.delete_current();
        options.make_dirty(DirtyBytes::ChangeLength);
        ModeTransition::new_mode(Normal::new())
    }
}

type CommandHandler = fn(&mut Buffers, &mut ViewOptions, &str) -> ModeTransition;

macro_rules! make_commands {
    ($($string:tt => $cmd:ident,)*) => {
        hashmap![
            $($string.to_string() => (cmd::$cmd as CommandHandler),)*
        ]
    }
}

fn default_commands() -> HashMap<String, CommandHandler> {
    make_commands![
        "q" => quit,
        "quit" => quit,
        "q!" => force_quit,
        "quit!" => force_quit,
        "w" => write,
        "write" => write,
        "wq" => write_quit,
        "wa" => write_all,
        "write-all" => write_all,
        "e" => edit,
        "edit" => edit,
        "db" => delete_buffer,
        "delete-buffer" => delete_buffer,
        "db!" => force_delete_buffer,
        "delete-buffer!" => force_delete_buffer,
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

    fn finish(&self, buffers: &mut Buffers, options: &mut ViewOptions) -> ModeTransition {
        let (name, rest) = self
            .command
            .split_at(self.command.find(' ').unwrap_or(self.command.len()));
        if let Some(handler) = DEFAULT_COMMANDS.get(name) {
            handler(buffers, options, if rest.len() == 0 { rest } else { &rest[1..] })
        } else {
            options.info = Some(format!("Unknown command {}", name));
            ModeTransition::new_mode(Normal::new())
        }
    }
}

impl Mode for Command {
    fn name(&self) -> Cow<'static, str> {
        "COMMAND".into()
    }

    fn transition(
        self: Box<Self>,
        evt: &Event,
        buffers: &mut Buffers,
        options: &mut ViewOptions,
    ) -> ModeTransition {
        if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
            let mut cursor = self.cursor;
            let mut command = self.command.to_owned();

            match action {
                Action::RemoveLast if cursor != 0 => {
                    command.remove(cursor - 1);
                    cursor -= 1;
                }
                Action::RemoveLast => return ModeTransition::new_mode(*self),
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
                Action::Cancel => return ModeTransition::new_mode(Normal::new()),
                Action::Finish => return self.finish(buffers, options),
            }
            ModeTransition::new_mode(Command { command, cursor })
        } else if let Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers,
        }) = evt
        {
            if !(*modifiers & !KeyModifiers::SHIFT).is_empty() {
                return ModeTransition::not_handled(*self);
            }
            let mut command = self.command.to_owned();
            let mut cursor = self.cursor;
            command.insert(cursor, *ch);
            cursor += 1;
            ModeTransition::new_mode(Command { command, cursor })
        } else {
            ModeTransition::not_handled(*self)
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
