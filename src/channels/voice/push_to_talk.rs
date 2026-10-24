//! SPACE to talk: hold (terminals that report key releases) or press twice.

use crate::adapters::audio::Recording;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags};
use crossterm::{execute, terminal};

/// Whether the terminal can report key releases, so SPACE can be held.
pub fn hold_supported() -> bool {
    terminal::supports_keyboard_enhancement().unwrap_or(false)
}

/// Wait for SPACE, record until it is released (or pressed again on terminals that
/// cannot report key releases). Returns `None` when the user quits.
pub fn record(hold: bool) -> Result<Option<(Vec<f32>, u32)>> {
    struct Raw(bool);
    impl Drop for Raw {
        fn drop(&mut self) {
            if self.0 {
                let _ = execute!(std::io::stdout(), event::PopKeyboardEnhancementFlags);
            }
            let _ = terminal::disable_raw_mode();
        }
    }
    terminal::enable_raw_mode()?;
    if hold {
        execute!(std::io::stdout(), event::PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES))?;
    }
    let _raw = Raw(hold);

    loop {
        if let Event::Key(k) = event::read()? {
            match k.code {
                KeyCode::Char(' ') if k.kind == KeyEventKind::Press => break,
                KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => return Ok(None),
                _ => {}
            }
        }
    }
    let recording = Recording::start()?;
    print!("\r\x1b[2K● listening…");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let stop_on = if hold { KeyEventKind::Release } else { KeyEventKind::Press };
    loop {
        if let Event::Key(k) = event::read()?
            && k.code == KeyCode::Char(' ')
            && k.kind == stop_on
        {
            break;
        }
    }
    print!("\r\x1b[2K");
    Ok(Some(recording.stop()))
}
