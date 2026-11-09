//! X-ray Shell — a small terminal emulator window.
//!
//! How it works:
//!   1. PTY (portable-pty): starts your shell (bash/zsh) inside a fake terminal.
//!   2. Parser (vt100):     turns the shell's output bytes + color codes into a grid of cells.
//!   3. Window (eframe):    draws that grid and sends your key presses back to the shell.

use eframe::egui::{self, Color32, Event, FontId, Key, Modifiers, TextFormat, text::LayoutJob};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const FONT_SIZE: f32 = 14.0;
const BG: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
const FG: Color32 = Color32::from_rgb(0xd4, 0xd4, 0xd4);
const SCROLLBACK_LINES: usize = 5000;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn main() -> eframe::Result {
  let options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default()
      .with_title("X-ray Shell")
      .with_inner_size([900.0, 560.0]),
    ..Default::default()
  };
  eframe::run_native(
    "X-ray Shell",
    options,
    Box::new(|cc| Ok(Box::new(TerminalApp::new(cc.egui_ctx.clone())?))),
  )
}

struct TerminalApp {
  /// Screen state: grid of characters, colors, cursor, scrollback.
  parser: Arc<Mutex<vt100::Parser>>,
  /// Our side of the PTY (used for resizing).
  master: Box<dyn MasterPty + Send>,
  /// Bytes written here arrive at the shell as keyboard input.
  writer: Box<dyn Write + Send>,
  /// The running shell process.
  child: Box<dyn Child + Send + Sync>,
  /// Set by the reader thread when the shell exits.
  exited: Arc<AtomicBool>,
  /// Current terminal size in (rows, cols).
  size: (u16, u16),
  /// Leftover mouse-wheel movement smaller than one line.
  scroll_acc: f32,
}

impl TerminalApp {
  fn new(ctx: egui::Context) -> Result<Self, BoxError> {
    let size = (24, 80);

    // 1. Open a PTY and start the user's shell inside it.
    let pair = native_pty_system().openpty(pty_size(size))?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let mut cmd = CommandBuilder::new(shell);
    cmd.env("TERM", "xterm-256color");
    if let Ok(dir) = std::env::current_dir() {
      cmd.cwd(dir);
    }
    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave); // only the child needs the slave side

    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;

    // 2. Read shell output on a background thread and feed it to the parser.
    let parser = Arc::new(Mutex::new(vt100::Parser::new(size.0, size.1, SCROLLBACK_LINES)));
    let exited = Arc::new(AtomicBool::new(false));
    {
      let parser = parser.clone();
      let exited = exited.clone();
      std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
          match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
              parser.lock().unwrap().process(&buf[..n]);
              ctx.request_repaint(); // new output → redraw the window
            }
          }
        }
        exited.store(true, Ordering::SeqCst);
        ctx.request_repaint();
      });
    }

    Ok(Self {
      parser,
      master: pair.master,
      writer,
      child,
      exited,
      size,
      scroll_acc: 0.0,
    })
  }

  /// Send keyboard / paste events to the shell.
  fn handle_input(&mut self, ctx: &egui::Context) {
    let (app_cursor, bracketed) = {
      let p = self.parser.lock().unwrap();
      (p.screen().application_cursor(), p.screen().bracketed_paste())
    };

    let mut out: Vec<u8> = Vec::new();
    for event in ctx.input(|i| i.events.clone()) {
      match event {
        Event::Text(text) => out.extend_from_slice(text.as_bytes()),
        Event::Paste(text) => {
          if bracketed {
            out.extend_from_slice(b"\x1b[200~");
          }
          out.extend_from_slice(text.replace('\n', "\r").as_bytes());
          if bracketed {
            out.extend_from_slice(b"\x1b[201~");
          }
        }
        // egui turns Ctrl+C / Ctrl+X into Copy / Cut; the shell wants the raw control codes.
        Event::Copy => out.push(0x03),
        Event::Cut => out.push(0x18),
        Event::Key { key, pressed: true, modifiers, .. } => {
          if let Some(bytes) = key_to_bytes(key, modifiers, app_cursor) {
            out.extend_from_slice(&bytes);
          }
        }
        _ => {}
      }
    }

    if !out.is_empty() {
      self.parser.lock().unwrap().set_scrollback(0); // typing jumps back to the bottom
      let _ = self.writer.write_all(&out);
      let _ = self.writer.flush();
    }
  }

  /// Keep the PTY and parser size in sync with the window size.
  fn resize(&mut self, rows: u16, cols: u16) {
    if (rows, cols) == self.size {
      return;
    }
    self.size = (rows, cols);
    self.parser.lock().unwrap().set_size(rows, cols);
    let _ = self.master.resize(pty_size(self.size));
  }

  /// Mouse wheel scrolls through the scrollback history.
  fn handle_scroll(&mut self, ui: &egui::Ui, row_h: f32) {
    self.scroll_acc += ui.input(|i| i.raw_scroll_delta.y);
    let lines = (self.scroll_acc / row_h).trunc();
    if lines != 0.0 {
      self.scroll_acc -= lines * row_h;
      let mut p = self.parser.lock().unwrap();
      let current = p.screen().scrollback() as isize;
      p.set_scrollback((current + lines as isize).max(0) as usize);
    }
  }
}

impl eframe::App for TerminalApp {
  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // Close the window when the shell exits (e.g. you typed `exit`).
    if self.exited.load(Ordering::SeqCst) {
      ctx.send_viewport_cmd(egui::ViewportCommand::Close);
      return;
    }

    self.handle_input(ctx);

    let panel = egui::Frame::none().fill(BG).inner_margin(6.0);
    egui::CentralPanel::default().frame(panel).show(ctx, |ui| {
      let font = FontId::monospace(FONT_SIZE);
      let (cell_w, cell_h) = ui.fonts(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));

      // How many rows/cols fit in the window?
      let avail = ui.available_size();
      let cols = ((avail.x / cell_w).floor() as u16).max(2);
      let rows = ((avail.y / cell_h).floor() as u16).max(2);
      self.resize(rows, cols);
      self.handle_scroll(ui, cell_h);

      let (rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
      let painter = ui.painter_at(rect);
      let parser = self.parser.lock().unwrap();
      let screen = parser.screen();

      // Draw each row as one text "job", grouping cells with the same style.
      for row in 0..rows {
        let mut job = LayoutJob::default();
        let mut run = String::new();
        let mut run_fmt: Option<TextFormat> = None;

        for col in 0..cols {
          let Some(cell) = screen.cell(row, col) else { break };
          if cell.is_wide_continuation() {
            continue; // second half of a wide char (e.g. emoji / CJK)
          }
          let fmt = cell_format(cell, &font);
          let text = cell.contents();
          let text = if text.is_empty() { " ".to_string() } else { text };

          if run_fmt.as_ref() != Some(&fmt) {
            if let Some(prev) = run_fmt.take() {
              job.append(&std::mem::take(&mut run), 0.0, prev);
            }
            run_fmt = Some(fmt);
          }
          run.push_str(&text);
        }
        if let Some(fmt) = run_fmt {
          job.append(&run, 0.0, fmt);
        }

        let galley = ui.fonts(|f| f.layout_job(job));
        let pos = rect.min + egui::vec2(0.0, row as f32 * cell_h);
        painter.galley(pos, galley, FG);
      }

      // Block cursor (hidden while scrolled back or when the app hides it, e.g. vim).
      if !screen.hide_cursor() && screen.scrollback() == 0 {
        let (cr, cc) = screen.cursor_position();
        let min = rect.min + egui::vec2(cc as f32 * cell_w, cr as f32 * cell_h);
        painter.rect_filled(
          egui::Rect::from_min_size(min, egui::vec2(cell_w, cell_h)),
          0.0,
          Color32::from_rgba_unmultiplied(220, 220, 220, 140),
        );
      }
    });
  }
}

impl Drop for TerminalApp {
  fn drop(&mut self) {
    let _ = self.child.kill();
  }
}

fn pty_size((rows, cols): (u16, u16)) -> PtySize {
  PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

/// Text style (colors, underline, italic) for one cell.
fn cell_format(cell: &vt100::Cell, font: &FontId) -> TextFormat {
  let mut fg = to_color32(cell.fgcolor(), cell.bold()).unwrap_or(FG);
  let mut bg = to_color32(cell.bgcolor(), false);
  if cell.inverse() {
    (fg, bg) = (bg.unwrap_or(BG), Some(fg));
  }
  TextFormat {
    font_id: font.clone(),
    color: fg,
    background: bg.unwrap_or(Color32::TRANSPARENT),
    italics: cell.italic(),
    underline: if cell.underline() {
      egui::Stroke::new(1.0_f32, fg)
    } else {
      egui::Stroke::NONE
    },
    ..Default::default()
  }
}

/// Convert a terminal color to an egui color. `None` means "use the default".
fn to_color32(color: vt100::Color, bold: bool) -> Option<Color32> {
  match color {
    vt100::Color::Default => None,
    vt100::Color::Idx(i) => Some(ansi_256(if bold && i < 8 { i + 8 } else { i })),
    vt100::Color::Rgb(r, g, b) => Some(Color32::from_rgb(r, g, b)),
  }
}

/// The standard xterm 256-color palette.
fn ansi_256(i: u8) -> Color32 {
  const BASE: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00), (0xcd, 0x31, 0x31), (0x0d, 0xbc, 0x79), (0xe5, 0xe5, 0x10),
    (0x24, 0x72, 0xc8), (0xbc, 0x3f, 0xbc), (0x11, 0xa8, 0xcd), (0xe5, 0xe5, 0xe5),
    (0x66, 0x66, 0x66), (0xf1, 0x4c, 0x4c), (0x23, 0xd1, 0x8b), (0xf5, 0xf5, 0x43),
    (0x3b, 0x8e, 0xea), (0xd6, 0x70, 0xd6), (0x29, 0xb8, 0xdb), (0xff, 0xff, 0xff),
  ];
  match i {
    0..=15 => {
      let (r, g, b) = BASE[i as usize];
      Color32::from_rgb(r, g, b)
    }
    16..=231 => {
      let i = i - 16;
      let level = |x: u8| if x == 0 { 0 } else { 55 + x * 40 };
      Color32::from_rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
    }
    232..=255 => {
      let v = 8 + (i - 232) * 10;
      Color32::from_rgb(v, v, v)
    }
  }
}

/// Bytes a real terminal sends for special keys.
fn key_to_bytes(key: Key, m: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
  // Ctrl+A .. Ctrl+Z → control codes 0x01 .. 0x1A (Ctrl+D = exit, Ctrl+L = clear, ...)
  if m.ctrl && !m.alt {
    let name = key.name().as_bytes();
    if name.len() == 1 && name[0].is_ascii_alphabetic() {
      return Some(vec![name[0].to_ascii_uppercase() - b'A' + 1]);
    }
  }

  // Arrow keys use a different prefix when apps like vim/less ask for "application cursor" mode.
  let cursor = |c: char| {
    if app_cursor { format!("\x1bO{c}") } else { format!("\x1b[{c}") }
  };

  let s = match key {
    Key::Enter => "\r".to_string(),
    Key::Backspace => "\x7f".to_string(),
    Key::Tab if m.shift => "\x1b[Z".to_string(),
    Key::Tab => "\t".to_string(),
    Key::Escape => "\x1b".to_string(),
    Key::ArrowUp => cursor('A'),
    Key::ArrowDown => cursor('B'),
    Key::ArrowRight => cursor('C'),
    Key::ArrowLeft => cursor('D'),
    Key::Home => cursor('H'),
    Key::End => cursor('F'),
    Key::Insert => "\x1b[2~".to_string(),
    Key::Delete => "\x1b[3~".to_string(),
    Key::PageUp => "\x1b[5~".to_string(),
    Key::PageDown => "\x1b[6~".to_string(),
    Key::F1 => "\x1bOP".to_string(),
    Key::F2 => "\x1bOQ".to_string(),
    Key::F3 => "\x1bOR".to_string(),
    Key::F4 => "\x1bOS".to_string(),
    _ => return None,
  };
  Some(s.into_bytes())
}
