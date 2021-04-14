use std::io::Write;

use crossterm::queue;
use crossterm::style;
use crossterm::Result;

#[derive(Debug, Clone, Copy)]
pub enum Priority {
    Basic,
    Mark,
    Selection,
    Cursor,
}

#[derive(Debug, Clone)]
pub struct PrioritizedStyle {
    pub style: style::ContentStyle,
    pub priority: Priority,
}

#[derive(Debug, Clone, Default)]
pub struct StylingCommand {
    start: Option<PrioritizedStyle>,
    mid: Option<PrioritizedStyle>,
    end: Option<PrioritizedStyle>,
}

impl StylingCommand {
    pub fn start_style(&self) -> Option<&style::ContentStyle> {
        self.start.as_ref().map(|x| &x.style)
    }
    pub fn mid_style(&self) -> Option<&style::ContentStyle> {
        self.mid.as_ref().map(|x| &x.style)
    }
    pub fn end_style(&self) -> Option<&style::ContentStyle> {
        self.end.as_ref().map(|x| &x.style)
    }
    pub fn with_start_style(self, style: PrioritizedStyle) -> StylingCommand {
        StylingCommand {
            start: Some(style),
            ..self
        }
    }
    pub fn with_mid_style(self, style: PrioritizedStyle) -> StylingCommand {
        StylingCommand {
            mid: Some(style),
            ..self
        }
    }
    pub fn with_end_style(self, style: PrioritizedStyle) -> StylingCommand {
        StylingCommand {
            end: Some(style),
            ..self
        }
    }
}

pub fn queue_style(stdout: &mut impl Write, style: &style::ContentStyle) -> Result<()> {
    if let Some(fg) = style.foreground_color {
        queue!(stdout, style::SetForegroundColor(fg))?;
    }
    if let Some(bg) = style.background_color {
        queue!(stdout, style::SetBackgroundColor(bg))?;
    }
    if !style.attributes.is_empty() {
        queue!(stdout, style::SetAttributes(style.attributes))?;
    }
    Ok(())
}
