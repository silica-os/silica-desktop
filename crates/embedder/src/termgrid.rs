//! Terminal emulation on the embedder's side (`spawn {grid: true}` on `silica/pty`): the
//! pty output is parsed here with `alacritty_terminal`, and the app only receives the
//! visible rows that changed, once per frame, as runs of text with the same attributes.
//! Parsing runs on a thread of its own ([`GridWorker`]): neither the app's UI thread nor the
//! embedder's main thread parses escape sequences, so a flood of output cannot starve frames
//! or input.
//!
//! The message, `grid {id, cols, rows, full, lines: [{y, runs: [[text, fg, bg, flags, cells,
//! link?], …]}], cursor: {x, y, visible, shape}, modes, …}`; the optional keys appear when
//! they have something new to say:
//! - `title`: the window title set by the program (OSC 0/2, the title stack);
//! - `scroll: {offset, history}`: lines scrolled back from the bottom, lines in the scrollback;
//! - `selection: [[y, x0, x1], …]`: the selected cells of the visible rows (empty: none);
//! - `marks: [y, …]`: the visible rows that start a prompt (OSC 133 A, "Mark");
//! - `cwd`: the working directory the shell reported (OSC 7);
//! - `command: {running, exit?}`: the shell integration (OSC 133 C and D);
//! - `bell: true`; `clipboard: [{set: text, selection} | {get: true, selection}, …]` (OSC 52).
//!
//! Colours are 0xRRGGBB, -1 for the theme's default foreground or background, and
//! `PALETTE + i` for the entry `i` (0–255) of the app's palette. `flags` bits: 1 bold,
//! 2 italic, 4 underline, 8 inverse, 16 dim, 32 strikeout, 64 curly underline, 128 hidden,
//! 256 wide. `cells` is the width of the run in cells; `link` the OSC 8 target, if any.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Row, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::{Cell, Flags, LineLength};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{
    ClipboardType, Config, Osc52, Term, TermDamage, TermMode, test::TermSize,
};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor, Rgb};
use serde_json::{Value, json};

/// Lines kept above the screen by default.
pub const SCROLLBACK: usize = 10_000;
/// The most lines the scrollback can be set to.
pub const MAX_SCROLLBACK: usize = 1_000_000;

/// Colours `PALETTE + i` are the entries of the app's palette.
pub const PALETTE: i64 = 0x0100_0000;

/// The zero-width characters that mark a row (they travel with the row as it scrolls, is
/// reflowed or leaves the screen): a prompt starts here, the output of a command starts here.
const PROMPT_MARK: char = '\u{10FF01}';
const OUTPUT_MARK: char = '\u{10FF02}';

/// The most matches a search returns.
const MAX_MATCHES: usize = 10_000;
/// The longest OSC payload kept (OSC 7, OSC 133).
const MAX_OSC: usize = 4096;

#[derive(Clone, Default)]
struct Listener {
    events: Arc<Mutex<Vec<Event>>>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

/// Bytes parsed per lock of the grid: the frame can take the lock between slices.
const SLICE: usize = 16 * 1024;

/// What a session is configured with (`spawn`, `configure`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Lines kept above the screen.
    pub scrollback: usize,
    /// OSC 52: whether programs may write to (and read from) the clipboard.
    pub clipboard_write: bool,
    pub clipboard_read: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scrollback: SCROLLBACK,
            clipboard_write: true,
            clipboard_read: false,
        }
    }
}

impl Options {
    fn limit(self) -> usize {
        self.scrollback.min(MAX_SCROLLBACK)
    }

    fn config(self) -> Config {
        Config {
            scrolling_history: self.limit(),
            osc52: match (self.clipboard_write, self.clipboard_read) {
                (false, false) => Osc52::Disabled,
                (true, false) => Osc52::OnlyCopy,
                (false, true) => Osc52::OnlyPaste,
                (true, true) => Osc52::CopyPaste,
            },
            ..Config::default()
        }
    }
}

/// A [`TermGrid`] fed by a parser thread.
pub struct GridWorker {
    grid: Arc<Mutex<TermGrid>>,
    input: mpsc::Sender<Vec<u8>>,
    queued: Arc<AtomicUsize>,
    /// The main thread wants the grid: the parser lets it in between slices (a std Mutex is
    /// not fair, and the parser would otherwise take it back at once).
    waiting: Arc<AtomicUsize>,
}

impl GridWorker {
    pub fn with_options(cols: u16, rows: u16, options: Options) -> std::io::Result<Self> {
        let grid = Arc::new(Mutex::new(TermGrid::with_options(cols, rows, options)));
        let queued = Arc::new(AtomicUsize::new(0));
        let (input, chunks) = mpsc::channel::<Vec<u8>>();
        let waiting = Arc::new(AtomicUsize::new(0));
        let (shared, count, wait) = (grid.clone(), queued.clone(), waiting.clone());
        std::thread::Builder::new()
            .name("silica-term".to_owned())
            .spawn(move || {
                // Ends when the session drops its sender.
                for chunk in chunks {
                    for slice in chunk.chunks(SLICE) {
                        while wait.load(Ordering::Acquire) > 0 {
                            std::thread::yield_now();
                        }
                        if let Ok(mut grid) = shared.lock() {
                            grid.advance(slice);
                        }
                    }
                    count.fetch_sub(chunk.len(), Ordering::Relaxed);
                }
            })?;
        Ok(Self {
            grid,
            input,
            queued,
            waiting,
        })
    }

    /// Hands output to the parser thread.
    pub fn feed(&self, bytes: Vec<u8>) {
        let len = bytes.len();
        self.queued.fetch_add(len, Ordering::Relaxed);
        if self.input.send(bytes).is_err() {
            self.queued.fetch_sub(len, Ordering::Relaxed);
        }
    }

    /// Bytes handed over and not parsed yet.
    pub fn queued(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }

    /// Runs `f` on the grid, between two slices of the parser.
    pub fn with<R>(&self, f: impl FnOnce(&mut TermGrid) -> R) -> Option<R> {
        self.waiting.fetch_add(1, Ordering::AcqRel);
        let result = self.grid.lock().ok().map(|mut grid| f(&mut grid));
        self.waiting.fetch_sub(1, Ordering::AcqRel);
        result
    }

    pub fn resize(&self, cols: u16, rows: u16, cell: (u16, u16)) {
        self.with(|grid| {
            grid.cell_size = (cell.0.max(1), cell.1.max(1));
            grid.resize(cols, rows);
        });
    }

    /// The frame message (if anything changed) and the replies for the program.
    pub fn frame(&self, id: u32) -> (Option<Value>, Vec<u8>) {
        self.with(|grid| {
            let frame = grid.frame(id);
            (frame, grid.take_replies())
        })
        .unwrap_or_default()
    }
}

/// Where the byte stream is in an escape sequence the grid reads itself (alacritty ignores
/// the OSC it does not know, and DEC private mode 9).
#[derive(Debug, Default)]
enum Scan {
    #[default]
    Ground,
    Esc,
    Csi(Vec<u8>),
    Osc(Vec<u8>),
    OscEsc(Vec<u8>),
}

/// A sequence the grid reads itself.
enum Seq {
    X10(bool),
    Osc(Vec<u8>),
}

/// Formats the clipboard's text as the OSC 52 answer.
type ClipboardFormat = Arc<dyn Fn(&str) -> String + Sync + Send + 'static>;

/// A row counted from the top of the whole buffer (scrollback and screen).
type Abs = usize;

pub struct TermGrid {
    term: Term<Listener>,
    parser: Processor,
    events: Arc<Mutex<Vec<Event>>>,
    options: Options,
    /// A cell in pixels, for the window size reports.
    cell_size: (u16, u16),
    scan: Scan,
    /// Bytes the terminal answers to the program (device attributes, cursor position…).
    replies: Vec<u8>,
    title: Option<String>,
    full: bool,
    /// The rows as last sent, to skip the ones that did not really change.
    sent: Vec<Value>,
    cursor: Value,
    modes: u32,
    /// DEC private mode 9 (X10 mouse reports), which alacritty does not know.
    x10_mouse: bool,
    last_scroll: (usize, usize),
    last_selection: Value,
    last_marks: Value,
    cwd: Option<String>,
    cwd_new: bool,
    bell: bool,
    clipboard: Vec<Value>,
    loads: VecDeque<ClipboardFormat>,
    /// Where the prompt that OSC 133 A announced started: (line, scrollback size then).
    prompt_at: Option<(i32, usize)>,
    running: bool,
    command_changed: bool,
    last_exit: Option<i32>,
}

impl TermGrid {
    #[cfg(test)]
    pub fn new(cols: u16, rows: u16) -> Self {
        Self::with_options(cols, rows, Options::default())
    }

    pub fn with_options(cols: u16, rows: u16, options: Options) -> Self {
        let listener = Listener::default();
        let events = listener.events.clone();
        let size = TermSize::new(usize::from(cols.max(1)), usize::from(rows.max(1)));
        Self {
            term: Term::new(options.config(), &size, listener),
            parser: Processor::new(),
            events,
            options,
            cell_size: (8, 16),
            scan: Scan::Ground,
            replies: Vec::new(),
            title: None,
            full: true,
            sent: Vec::new(),
            cursor: Value::Null,
            modes: 0,
            x10_mouse: false,
            last_scroll: (0, 0),
            last_selection: Value::Null,
            last_marks: Value::Null,
            cwd: None,
            cwd_new: false,
            bell: false,
            clipboard: Vec::new(),
            loads: VecDeque::new(),
            prompt_at: None,
            running: false,
            command_changed: false,
            last_exit: None,
        }
    }

    /// Feeds program output to the emulator. Sequences the grid handles itself (OSC 7, OSC
    /// 133, DEC mode 9) split the stream, so that the cursor is where the program put it
    /// when they are read.
    pub fn advance(&mut self, bytes: &[u8]) {
        let mut start = 0;
        let mut i = 0;
        while let Some(&byte) = bytes.get(i) {
            let scan = std::mem::take(&mut self.scan);
            let (next, done) = Self::step(scan, byte);
            self.scan = next;
            i += 1;
            if let Some(done) = done {
                // Everything up to and including the terminator first.
                self.parser
                    .advance(&mut self.term, bytes.get(start..i).unwrap_or(&[]));
                start = i;
                self.drain_events();
                self.sequence(done);
            } else if matches!(self.scan, Scan::Ground) {
                // Nothing in progress: skip to the next ESC.
                let rest = bytes.get(i..).unwrap_or(&[]);
                i += rest.iter().position(|&b| b == 0x1b).unwrap_or(rest.len());
            }
        }
        self.parser
            .advance(&mut self.term, bytes.get(start..).unwrap_or(&[]));
        self.drain_events();
    }

    /// One byte of the scanner: the next state and, at the end of a sequence of interest, it.
    fn step(scan: Scan, byte: u8) -> (Scan, Option<Seq>) {
        match scan {
            Scan::Ground => (
                if byte == 0x1b {
                    Scan::Esc
                } else {
                    Scan::Ground
                },
                None,
            ),
            Scan::Esc => match byte {
                b']' => (Scan::Osc(Vec::new()), None),
                b'[' => (Scan::Csi(Vec::new()), None),
                0x1b => (Scan::Esc, None),
                _ => (Scan::Ground, None),
            },
            Scan::Csi(mut params) => match byte {
                0x40..=0x7e => {
                    let seq = match (params.as_slice(), byte) {
                        (b"?9", b'h') => Some(Seq::X10(true)),
                        (b"?9", b'l') => Some(Seq::X10(false)),
                        _ => None,
                    };
                    (Scan::Ground, seq)
                }
                0x20..=0x3f if params.len() < 16 => {
                    params.push(byte);
                    (Scan::Csi(params), None)
                }
                0x1b => (Scan::Esc, None),
                _ => (Scan::Ground, None),
            },
            Scan::Osc(mut payload) => match byte {
                0x07 => (Scan::Ground, Some(Seq::Osc(payload))),
                0x1b => (Scan::OscEsc(payload), None),
                // CAN and SUB abort the string.
                0x18 | 0x1a => (Scan::Ground, None),
                _ => {
                    if payload.len() < MAX_OSC {
                        payload.push(byte);
                    }
                    (Scan::Osc(payload), None)
                }
            },
            Scan::OscEsc(payload) => match byte {
                b'\\' => (Scan::Ground, Some(Seq::Osc(payload))),
                b']' => (Scan::Osc(Vec::new()), None),
                _ => (Scan::Ground, None),
            },
        }
    }

    fn sequence(&mut self, seq: Seq) {
        match seq {
            Seq::X10(on) => self.x10_mouse = on,
            Seq::Osc(payload) => self.osc(&payload),
        }
    }

    fn osc(&mut self, payload: &[u8]) {
        let text = String::from_utf8_lossy(payload);
        let (code, rest) = text.split_once(';').unwrap_or((&text, ""));
        match code {
            "7" => {
                if let Some(path) = file_url_path(rest)
                    && self.cwd.as_deref() != Some(path.as_str())
                {
                    self.cwd = Some(path);
                    self.cwd_new = true;
                }
            }
            "133" => self.shell_mark(rest),
            _ => {}
        }
    }

    /// OSC 133: A prompt start, B prompt end, C command output start, D command finished.
    fn shell_mark(&mut self, rest: &str) {
        let mut parts = rest.split(';');
        let kind = parts.next().unwrap_or("");
        let cursor = self.term.grid().cursor.point;
        let history = self.term.history_size();
        match kind {
            "A" => {
                self.prompt_at = Some((cursor.line.0, history));
                self.set_mark(cursor.line, PROMPT_MARK);
                if self.running {
                    self.running = false;
                    self.command_changed = true;
                }
            }
            "B" => {
                let line = self.prompt_line(cursor.line, history);
                self.set_mark(line, PROMPT_MARK);
            }
            "C" => {
                let line = self.prompt_line(cursor.line, history);
                self.set_mark(line, PROMPT_MARK);
                self.prompt_at = None;
                // The row above the cursor holds the command: the output starts below it.
                if cursor.line.0 > self.term.topmost_line().0 {
                    self.set_mark(Line(cursor.line.0 - 1), OUTPUT_MARK);
                }
                self.running = true;
                self.last_exit = None;
                self.command_changed = true;
            }
            "D" => {
                self.running = false;
                self.last_exit = parts.next().and_then(|c| c.parse().ok());
                self.command_changed = true;
            }
            _ => {}
        }
    }

    /// The row where the prompt announced by the last A started, given it may have scrolled.
    fn prompt_line(&self, fallback: Line, history: usize) -> Line {
        let Some((line, then)) = self.prompt_at else {
            return fallback;
        };
        let full = history >= self.options.limit();
        let moved = history.saturating_sub(then) as i32;
        let line = Line(line - moved);
        if full || line.0 < self.term.topmost_line().0 || line.0 > fallback.0 {
            fallback
        } else {
            line
        }
    }

    fn set_mark(&mut self, line: Line, mark: char) {
        if line.0 < self.term.topmost_line().0 || line.0 > self.term.bottommost_line().0 {
            return;
        }
        let cell = &mut self.term.grid_mut()[line][Column(0)];
        if !cell.zerowidth().is_some_and(|z| z.contains(&mark)) {
            cell.push_zerowidth(mark);
        }
    }

    fn drain_events(&mut self) {
        let events: Vec<Event> = match self.events.lock() {
            Ok(mut events) => events.drain(..).collect(),
            Err(_) => Vec::new(),
        };
        for event in events {
            match event {
                Event::PtyWrite(text) => self.replies.extend_from_slice(text.as_bytes()),
                Event::Title(title) => self.title = Some(title),
                Event::ResetTitle => self.title = Some(String::new()),
                Event::Bell => self.bell = true,
                Event::ClipboardStore(kind, text) => {
                    if self.clipboard.len() < 16 {
                        self.clipboard.push(json!({
                            "set": text,
                            "selection": kind == ClipboardType::Selection,
                        }));
                    }
                }
                Event::ClipboardLoad(kind, format) => {
                    if self.loads.len() < 4 {
                        self.loads.push_back(format);
                        self.clipboard.push(json!({
                            "get": true,
                            "selection": kind == ClipboardType::Selection,
                        }));
                    }
                }
                Event::ColorRequest(index, format) => {
                    let colors = *self.term.colors();
                    let rgb = (index < 269)
                        .then(|| colors[index])
                        .flatten()
                        .unwrap_or_else(|| default_rgb(index));
                    self.replies.extend_from_slice(format(rgb).as_bytes());
                }
                Event::TextAreaSizeRequest(format) => {
                    let size = WindowSize {
                        num_lines: self.term.screen_lines().min(usize::from(u16::MAX)) as u16,
                        num_cols: self.term.columns().min(usize::from(u16::MAX)) as u16,
                        cell_width: self.cell_size.0,
                        cell_height: self.cell_size.1,
                    };
                    self.replies.extend_from_slice(format(size).as_bytes());
                }
                _ => {}
            }
        }
    }

    /// What the terminal must write back to the program.
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }

    /// The program asked for the clipboard (OSC 52) and the app answered: what to write.
    pub fn clipboard_reply(&mut self, text: &str) -> Vec<u8> {
        self.loads
            .pop_front()
            .map(|format| format(text).into_bytes())
            .unwrap_or_default()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let size = TermSize::new(usize::from(cols.max(1)), usize::from(rows.max(1)));
        self.term.resize(size);
        self.full = true;
    }

    /// Applies new options: the scrollback size and what OSC 52 may do.
    pub fn configure(&mut self, options: Options) {
        self.options = options;
        self.term.set_options(options.config());
        self.full = true;
    }

    pub fn options(&self) -> Options {
        self.options
    }

    #[cfg(test)]
    pub fn history_size(&self) -> usize {
        self.term.history_size()
    }

    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// The mode bits (see [`mode_bits`]).
    pub fn modes(&self) -> u32 {
        mode_bits(*self.term.mode(), self.x10_mouse)
    }

    /// The working directory the shell reported with OSC 7.
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// A command runs between OSC 133 C and D.
    #[cfg(test)]
    pub fn command_running(&self) -> bool {
        self.running
    }

    // ---- scrolling --------------------------------------------------------------------

    /// Scrolls the view: lines, a page, to the top or the bottom, to a row or a prompt.
    pub fn scroll(&mut self, by: ScrollBy) {
        let scroll = match by {
            ScrollBy::Lines(n) => Scroll::Delta(n),
            ScrollBy::Page(true) => Scroll::PageUp,
            ScrollBy::Page(false) => Scroll::PageDown,
            ScrollBy::Top => Scroll::Top,
            ScrollBy::Bottom => Scroll::Bottom,
            ScrollBy::Reveal(abs) => match self.reveal_delta(abs) {
                Some(delta) => Scroll::Delta(delta),
                None => return,
            },
            ScrollBy::Prompt(previous) => {
                let top = self.top_abs();
                let marks = self.mark_rows(PROMPT_MARK);
                let target = if previous {
                    marks.iter().rev().find(|&&m| m < top).copied()
                } else {
                    marks.iter().find(|&&m| m > top).copied()
                };
                match target {
                    Some(abs) => {
                        let offset = self.term.history_size().saturating_sub(abs);
                        Scroll::Delta(offset as i32 - self.display_offset() as i32)
                    }
                    None if previous => return,
                    None => Scroll::Bottom,
                }
            }
        };
        self.term.scroll_display(scroll);
    }

    /// The scroll delta that shows the row `abs` (centred when it is out of view).
    fn reveal_delta(&self, abs: Abs) -> Option<i32> {
        let top = self.top_abs();
        let rows = self.term.screen_lines();
        if abs >= top && abs < top + rows {
            return None;
        }
        let want_top = abs.saturating_sub(rows / 2);
        let offset = self.term.history_size().saturating_sub(want_top);
        Some(offset as i32 - self.display_offset() as i32)
    }

    /// The row at the top of the view.
    fn top_abs(&self) -> Abs {
        self.term
            .history_size()
            .saturating_sub(self.display_offset())
    }

    // ---- selection --------------------------------------------------------------------

    fn viewport_point(&self, x: usize, y: usize) -> Point {
        let x = x.min(self.term.columns().saturating_sub(1));
        let y = y.min(self.term.screen_lines().saturating_sub(1));
        Point::new(Line(y as i32 - self.display_offset() as i32), Column(x))
    }

    pub fn select_start(&mut self, kind: SelectionType, x: usize, y: usize, right: bool) {
        let point = self.viewport_point(x, y);
        let side = if right { Side::Right } else { Side::Left };
        self.term.selection = Some(Selection::new(kind, point, side));
    }

    pub fn select_update(&mut self, x: usize, y: usize, right: bool) {
        let point = self.viewport_point(x, y);
        let side = if right { Side::Right } else { Side::Left };
        if let Some(selection) = &mut self.term.selection {
            selection.update(point, side);
        }
    }

    pub fn select_clear(&mut self) {
        self.term.selection = None;
    }

    /// Selects the whole buffer, scrollback included.
    pub fn select_all(&mut self) {
        let top = self.term.topmost_line();
        let bottom = self.term.bottommost_line();
        let last = self.term.last_column();
        let mut selection = Selection::new(
            SelectionType::Simple,
            Point::new(top, Column(0)),
            Side::Left,
        );
        selection.update(Point::new(bottom, last), Side::Right);
        self.term.selection = Some(selection);
    }

    /// Selects the output of the last command: between the output mark above the live
    /// prompt and that prompt (or the end of the text).
    pub fn select_between_marks(&mut self) -> bool {
        let history = self.term.history_size() as i32;
        let cursor_abs = (self.term.grid().cursor.point.line.0 + history).max(0) as usize;
        let mut marks: Vec<Abs> = self.mark_rows(PROMPT_MARK);
        marks.extend(self.mark_rows(OUTPUT_MARK));
        marks.sort_unstable();
        marks.dedup();
        let hi = marks.iter().find(|&&m| m >= cursor_abs).copied();
        let Some(lo) = marks.iter().copied().rfind(|&m| hi.is_none_or(|h| m < h)) else {
            return false;
        };
        let end = match hi {
            Some(h) => h.saturating_sub(1),
            None => self.last_text_abs(),
        };
        if end <= lo {
            return false;
        }
        let first = Point::new(Line(lo as i32 + 1 - history), Column(0));
        let last = Point::new(Line(end as i32 - history), self.term.last_column());
        let mut selection = Selection::new(SelectionType::Simple, first, Side::Left);
        selection.update(last, Side::Right);
        self.term.selection = Some(selection);
        true
    }

    /// The last row that holds text.
    fn last_text_abs(&self) -> Abs {
        let history = self.term.history_size() as i32;
        let grid = self.term.grid();
        let mut line = self.term.bottommost_line().0;
        while line > self.term.topmost_line().0 && grid[Line(line)].line_length().0 == 0 {
            line -= 1;
        }
        (line + history).max(0) as usize
    }

    /// The selected text, with soft-wrapped lines joined.
    pub fn selection_text(&self) -> Option<String> {
        self.term
            .selection_to_string()
            .map(|text| strip_marks(&text))
    }

    /// The text of the whole buffer (scrollback and screen), trailing blanks trimmed.
    pub fn text(&self) -> String {
        let start = Point::new(self.term.topmost_line(), Column(0));
        let end = Point::new(self.term.bottommost_line(), self.term.last_column());
        strip_marks(&self.term.bounds_to_string(start, end))
            .trim_end()
            .to_owned()
    }

    /// The selection as spans of the visible rows: `[y, x0, x1]`.
    fn selection_spans(&self) -> Value {
        let Some(range) = self
            .term
            .selection
            .as_ref()
            .and_then(|s| s.to_range(&self.term))
        else {
            return json!([]);
        };
        let rows = self.term.screen_lines();
        let last = self.term.columns().saturating_sub(1);
        let offset = self.display_offset() as i32;
        let mut spans = Vec::new();
        for y in 0..rows {
            let line = y as i32 - offset;
            if line < range.start.line.0 || line > range.end.line.0 {
                continue;
            }
            let (x0, x1) = if range.is_block {
                (
                    range.start.column.0.min(range.end.column.0),
                    range.start.column.0.max(range.end.column.0),
                )
            } else {
                (
                    if line == range.start.line.0 {
                        range.start.column.0
                    } else {
                        0
                    },
                    if line == range.end.line.0 {
                        range.end.column.0
                    } else {
                        last
                    },
                )
            };
            spans.push(json!([y, x0.min(last), x1.min(last)]));
        }
        Value::Array(spans)
    }

    // ---- marks ------------------------------------------------------------------------

    fn has_mark(&self, line: i32, mark: char) -> bool {
        self.term.grid()[Line(line)][Column(0)]
            .zerowidth()
            .is_some_and(|z| z.contains(&mark))
    }

    /// The rows marked with `mark`, from the top of the buffer, in order.
    fn mark_rows(&self, mark: char) -> Vec<Abs> {
        let history = self.term.history_size() as i32;
        (self.term.topmost_line().0..=self.term.bottommost_line().0)
            .filter(|&line| self.has_mark(line, mark))
            .map(|line| (line + history).max(0) as usize)
            .collect()
    }

    /// Marks the row of the cursor (Mark, ⌘U).
    pub fn mark_cursor_row(&mut self) {
        let line = self.term.grid().cursor.point.line;
        self.set_mark(line, PROMPT_MARK);
    }

    /// Removes the nearest mark at or above the cursor (Remove Mark).
    pub fn unmark(&mut self) -> bool {
        let cursor = self.term.grid().cursor.point.line.0;
        for line in (self.term.topmost_line().0..=cursor).rev() {
            if !self.has_mark(line, PROMPT_MARK) {
                continue;
            }
            let cell = &mut self.term.grid_mut()[Line(line)][Column(0)];
            let kept: Vec<char> = cell
                .zerowidth()
                .unwrap_or_default()
                .iter()
                .copied()
                .filter(|&c| c != PROMPT_MARK)
                .collect();
            cell.extra = None;
            for c in kept {
                cell.push_zerowidth(c);
            }
            return true;
        }
        false
    }

    /// The visible rows that start a prompt.
    fn visible_marks(&self) -> Value {
        let offset = self.display_offset() as i32;
        let marks: Vec<Value> = (0..self.term.screen_lines())
            .filter(|&y| self.has_mark(y as i32 - offset, PROMPT_MARK))
            .map(|y| json!(y))
            .collect();
        Value::Array(marks)
    }

    /// Clears the scrollback above the nearest prompt mark above the cursor.
    pub fn clear_to_previous_mark(&mut self) -> bool {
        let history = self.term.history_size();
        let cursor_abs = (self.term.grid().cursor.point.line.0 + history as i32).max(0) as usize;
        let Some(mark) = self
            .mark_rows(PROMPT_MARK)
            .into_iter()
            .rfind(|&m| m < cursor_abs)
        else {
            return false;
        };
        self.drop_history_before(mark)
    }

    /// Drops the lines of the scrollback above the row `abs` (nothing of the screen).
    fn drop_history_before(&mut self, abs: Abs) -> bool {
        let history = self.term.history_size();
        let keep = history.saturating_sub(abs);
        if keep >= history {
            return false;
        }
        let max = self.options.limit();
        self.term.selection = None;
        let grid = self.term.grid_mut();
        grid.update_history(keep);
        grid.update_history(max);
        self.full = true;
        true
    }

    /// Clears the scrollback.
    pub fn clear_scrollback(&mut self) {
        self.drop_history_before(self.term.history_size());
    }

    /// Clears the scrollback and the screen above the cursor (the cursor's row becomes the
    /// first one).
    pub fn clear_to_start(&mut self) {
        self.clear_scrollback();
        let line = self.term.grid().cursor.point.line.0;
        if line > 0 && !self.term.mode().contains(TermMode::ALT_SCREEN) {
            // Scroll the rows above the cursor away, move the cursor up with them, drop them.
            self.feed_internal(format!("\x1b[{line}S\x1b[{line}A").as_bytes());
            self.clear_scrollback();
        }
    }

    /// Pushes the screen into the scrollback, leaving a blank one (Clear Screen).
    pub fn clear_screen(&mut self) {
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        let rows = self.term.screen_lines();
        self.feed_internal(format!("\x1b[{rows}S\x1b[H").as_bytes());
    }

    /// Soft reset (DECSTR) or full reset (RIS) with the scrollback and marks gone.
    pub fn reset(&mut self, hard: bool) {
        if hard {
            self.feed_internal(b"\x1bc");
            self.clear_scrollback();
            self.running = false;
            self.prompt_at = None;
        } else {
            self.feed_internal(b"\x1b[!p");
        }
        self.full = true;
    }

    /// Bytes for the emulator that did not come from the program.
    fn feed_internal(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
        self.drain_events();
        self.full = true;
    }

    // ---- search -----------------------------------------------------------------------

    /// Every match of `query`, in order from the top of the buffer: each is a list of
    /// `[row, x0, x1]` pieces (rows from the top of the buffer, `x1` included), more than
    /// one when the match crosses a soft wrap.
    pub fn search(&self, query: &str, case_sensitive: bool) -> Value {
        let fold = |c: char| {
            if case_sensitive {
                c
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        };
        let needle: Vec<char> = query.chars().map(fold).collect();
        if needle.is_empty() {
            return json!([]);
        }
        let history = self.term.history_size() as i32;
        let grid = self.term.grid();
        let last = self.term.last_column().0;
        let mut matches: Vec<Value> = Vec::new();
        // The characters of the logical line so far, with the cells each one is in.
        let mut chars: Vec<char> = Vec::new();
        let mut cells: Vec<(i32, usize, usize)> = Vec::new();
        let flush = |chars: &mut Vec<char>,
                     cells: &mut Vec<(i32, usize, usize)>,
                     matches: &mut Vec<Value>| {
            let n = needle.len();
            let mut i = 0;
            while i + n <= chars.len() && matches.len() < MAX_MATCHES {
                if chars.get(i..i + n) == Some(needle.as_slice()) {
                    let mut pieces: Vec<(i32, usize, usize)> = Vec::new();
                    for &(line, x0, x1) in cells.get(i..i + n).unwrap_or(&[]) {
                        match pieces.last_mut() {
                            Some(p) if p.0 == line => p.2 = p.2.max(x1),
                            _ => pieces.push((line, x0, x1)),
                        }
                    }
                    matches.push(Value::Array(
                        pieces
                            .into_iter()
                            .map(|(line, x0, x1)| json!([(line + history).max(0), x0, x1]))
                            .collect(),
                    ));
                    i += n;
                } else {
                    i += 1;
                }
            }
            chars.clear();
            cells.clear();
        };
        for line in self.term.topmost_line().0..=self.term.bottommost_line().0 {
            let row = &grid[Line(line)];
            let wraps = row[Column(last)].flags.contains(Flags::WRAPLINE);
            // A wrapped row is full: its blanks are text. Otherwise the trailing blanks go.
            let length = if wraps {
                last + 1
            } else {
                row.line_length().0.min(last + 1)
            };
            for x in 0..length {
                let cell = &row[Column(x)];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                let c = if cell.c == '\0' { ' ' } else { cell.c };
                chars.push(fold(c));
                let width = usize::from(cell.flags.contains(Flags::WIDE_CHAR));
                cells.push((line, x, (x + width).min(last)));
            }
            if !wraps {
                flush(&mut chars, &mut cells, &mut matches);
            }
        }
        flush(&mut chars, &mut cells, &mut matches);
        Value::Array(matches)
    }

    // ---- frames -----------------------------------------------------------------------

    /// The `grid` message for what changed since the last call; None if nothing did.
    pub fn frame(&mut self, id: u32) -> Option<Value> {
        // A synchronized update (DEC mode 2026) a program never ended must not freeze the view.
        if self.parser.sync_bytes_count() > 0
            && self
                .parser
                .sync_timeout()
                .sync_timeout()
                .is_some_and(|at| at <= Instant::now())
        {
            self.parser.stop_sync(&mut self.term);
            self.drain_events();
        }
        let rows = self.term.screen_lines();
        let cols = self.term.columns();
        let offset = self.display_offset();
        let history = self.term.history_size();
        let scroll = (offset, history);
        let full = std::mem::take(&mut self.full);
        // Scrolled back (or just back): damage is of the live screen, so look at every row;
        // the rows already sent are skipped below.
        let everything = full || offset > 0 || scroll.0 != self.last_scroll.0;
        let damaged: Vec<usize> = match (everything, self.term.damage()) {
            (true, _) | (_, TermDamage::Full) => (0..rows).collect(),
            (false, TermDamage::Partial(lines)) => lines.map(|l| l.line).collect(),
        };
        if full {
            self.sent.clear();
        }
        self.sent.resize(rows, Value::Null);
        let colors = *self.term.colors();
        let mut lines = Vec::new();
        {
            let grid = self.term.grid();
            for y in damaged.into_iter().filter(|&y| y < rows) {
                let row = &grid[Line(y as i32 - offset as i32)];
                let runs = row_runs(row, cols, &colors);
                // alacritty marks the cursor's rows damaged on every frame: send real changes
                // only.
                if self.sent.get(y) != Some(&runs) {
                    lines.push(json!({ "y": y, "runs": runs.clone() }));
                    if let Some(slot) = self.sent.get_mut(y) {
                        *slot = runs;
                    }
                }
            }
        }
        self.term.reset_damage();
        let cursor = self.term.renderable_content().cursor;
        let state = json!({
            "x": cursor.point.column.0,
            "y": cursor.point.line.0 + offset as i32,
            "visible": cursor.shape != CursorShape::Hidden,
            "shape": match cursor.shape {
                CursorShape::Beam => "beam",
                CursorShape::Underline => "underline",
                _ => "block",
            },
        });
        let modes = self.modes();
        let selection = self.selection_spans();
        let marks = self.visible_marks();
        let mut extra = serde_json::Map::new();
        if let Some(title) = self.title.take() {
            extra.insert("title".to_owned(), Value::String(title));
        }
        if scroll != self.last_scroll || full {
            extra.insert(
                "scroll".to_owned(),
                json!({ "offset": offset, "history": history }),
            );
        }
        if selection != self.last_selection {
            extra.insert("selection".to_owned(), selection.clone());
        }
        if marks != self.last_marks {
            extra.insert("marks".to_owned(), marks.clone());
        }
        if self.cwd_new
            && let Some(cwd) = &self.cwd
        {
            extra.insert("cwd".to_owned(), Value::String(cwd.clone()));
        }
        self.cwd_new = false;
        if std::mem::take(&mut self.command_changed) {
            let mut command = json!({ "running": self.running });
            if let (Some(code), Some(object)) = (self.last_exit, command.as_object_mut()) {
                object.insert("exit".to_owned(), json!(code));
            }
            extra.insert("command".to_owned(), command);
        }
        if std::mem::take(&mut self.bell) {
            extra.insert("bell".to_owned(), Value::Bool(true));
        }
        if !self.clipboard.is_empty() {
            extra.insert(
                "clipboard".to_owned(),
                Value::Array(std::mem::take(&mut self.clipboard)),
            );
        }
        let moved = self.cursor != state || self.modes != modes;
        if lines.is_empty() && !moved && extra.is_empty() && !full {
            return None;
        }
        self.cursor = state.clone();
        self.modes = modes;
        self.last_scroll = scroll;
        self.last_selection = selection;
        self.last_marks = marks;
        let mut message = json!({
            "id": id,
            "cols": cols,
            "rows": rows,
            "full": full,
            "lines": lines,
            "cursor": state,
            "modes": modes,
        });
        if let Some(object) = message.as_object_mut() {
            object.extend(extra);
        }
        Some(message)
    }
}

/// How [`TermGrid::scroll`] moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollBy {
    /// Lines towards the older text (negative: towards the newer).
    Lines(i32),
    /// A page towards the older text (true) or the newer.
    Page(bool),
    Top,
    Bottom,
    /// Shows the row (from the top of the buffer), centred if it was out of view.
    Reveal(usize),
    /// The previous (true) or next prompt mark, at the top of the view.
    Prompt(bool),
}

/// The path of a `file://host/path` URL, percent-decoded; None if it is not one.
fn file_url_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let path = rest.get(rest.find('/')?..)?;
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%'
            && let Some(hex) = path.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b);
        i += 1;
    }
    let path = String::from_utf8(out).ok()?;
    (!path.is_empty() && !path.contains('\0')).then_some(path)
}

/// Text without the marker characters.
fn strip_marks(text: &str) -> String {
    text.chars()
        .filter(|&c| c != PROMPT_MARK && c != OUTPUT_MARK)
        .collect()
}

type Key = (i64, i64, u32, Option<String>);

/// The runs of a row: `[text, fg, bg, flags, cells, link?]` for each stretch of cells with
/// the same attributes, without the default blanks at the end.
fn row_runs(row: &Row<Cell>, cols: usize, colors: &Colors) -> Value {
    let mut runs: Vec<Value> = Vec::new();
    let mut text = String::new();
    let mut cells = 0usize;
    let mut key: Option<Key> = None;
    for x in 0..cols {
        let cell = &row[Column(x)];
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            // The second half of a wide character is part of its cell.
            continue;
        }
        let attrs: Key = (
            resolve(cell.fg, colors),
            resolve(cell.bg, colors),
            flag_bits(cell.flags),
            cell.hyperlink().map(|l| l.uri().to_owned()),
        );
        if key.as_ref().is_some_and(|k| *k != attrs)
            && let Some((fg, bg, flags, link)) = key.take()
        {
            runs.push(run(std::mem::take(&mut text), fg, bg, flags, cells, link));
            cells = 0;
        }
        key = Some(attrs);
        if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) || cell.c == '\0' {
            text.push(' ');
        } else {
            text.push(cell.c);
        }
        if let Some(zw) = cell.zerowidth() {
            text.extend(zw.iter().filter(|&&c| c != PROMPT_MARK && c != OUTPUT_MARK));
        }
        cells += if cell.flags.contains(Flags::WIDE_CHAR) {
            2
        } else {
            1
        };
    }
    if let Some((fg, bg, flags, link)) = key {
        if fg == -1 && bg == -1 && flags & !256 == 0 && link.is_none() {
            let trimmed = text.trim_end_matches(' ');
            if !trimmed.is_empty() {
                let blanks = text.chars().count() - trimmed.chars().count();
                runs.push(run(
                    trimmed.to_owned(),
                    fg,
                    bg,
                    flags,
                    cells.saturating_sub(blanks),
                    None,
                ));
            }
        } else {
            runs.push(run(text, fg, bg, flags, cells, link));
        }
    }
    Value::Array(runs)
}

fn run(text: String, fg: i64, bg: i64, flags: u32, cells: usize, link: Option<String>) -> Value {
    match link {
        Some(link) => json!([text, fg, bg, flags, cells, link]),
        None => json!([text, fg, bg, flags, cells]),
    }
}

/// Mode bits the app needs to encode input: 1 application cursor keys, 2 bracketed paste,
/// 4 mouse reporting (any), 8 SGR mouse, 16 focus events, 32 alternate screen, 64 app keypad,
/// 128 report drags (1002), 256 report all motion (1003), 512 UTF-8 mouse (1005), 1024 X10
/// mouse (9), 2048 alternate scroll (1007).
fn mode_bits(mode: TermMode, x10: bool) -> u32 {
    let mut bits = 0;
    for (flag, bit) in [
        (TermMode::APP_CURSOR, 1),
        (TermMode::BRACKETED_PASTE, 2),
        (TermMode::SGR_MOUSE, 8),
        (TermMode::FOCUS_IN_OUT, 16),
        (TermMode::ALT_SCREEN, 32),
        (TermMode::APP_KEYPAD, 64),
        (TermMode::MOUSE_DRAG, 128),
        (TermMode::MOUSE_MOTION, 256),
        (TermMode::UTF8_MOUSE, 512),
        (TermMode::ALTERNATE_SCROLL, 2048),
    ] {
        if mode.contains(flag) {
            bits |= bit;
        }
    }
    if mode.intersects(TermMode::MOUSE_MODE) || x10 {
        bits |= 4;
    }
    if x10 {
        bits |= 1024;
    }
    bits
}

fn flag_bits(flags: Flags) -> u32 {
    let mut bits = 0;
    for (flag, bit) in [
        (Flags::BOLD, 1),
        (Flags::ITALIC, 2),
        (Flags::UNDERLINE, 4),
        (Flags::INVERSE, 8),
        (Flags::DIM, 16),
        (Flags::STRIKEOUT, 32),
        (Flags::UNDERCURL, 64),
        (Flags::HIDDEN, 128),
        (Flags::WIDE_CHAR, 256),
    ] {
        if flags.contains(flag) {
            bits |= bit;
        }
    }
    bits
}

fn rgb(c: Rgb) -> i64 {
    (i64::from(c.r) << 16) | (i64::from(c.g) << 8) | i64::from(c.b)
}

/// A cell colour: 0xRRGGBB, -1 for the theme's default, or `PALETTE + i` (an entry of the
/// app's palette, unless a program redefined it with OSC 4).
fn resolve(color: Color, colors: &Colors) -> i64 {
    match color {
        Color::Spec(c) => rgb(c),
        Color::Named(NamedColor::Foreground | NamedColor::Background) => -1,
        Color::Named(name) => palette(named_index(name), colors),
        Color::Indexed(i) => palette(Some(usize::from(i)), colors),
    }
}

/// The palette entry of a named colour; None for the ones without (cursor, bright/dim
/// foreground).
fn named_index(name: NamedColor) -> Option<usize> {
    let n = name as usize;
    match n {
        0..=15 => Some(n),
        // DimBlack…DimWhite.
        259..=266 => Some(n - 259),
        _ => None,
    }
}

fn palette(index: Option<usize>, colors: &Colors) -> i64 {
    let Some(index) = index else {
        return -1;
    };
    if index < 256
        && let Some(c) = colors[index]
    {
        return rgb(c);
    }
    PALETTE + index as i64
}

/// The colour of palette entry `index` when a program asks for it (OSC 4/10/11 queries).
fn default_rgb(index: usize) -> Rgb {
    let value = match index {
        0..=15 => {
            const BASE: [u32; 16] = [
                0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5,
                0x7f7f7f, 0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff,
            ];
            BASE.get(index).copied().unwrap_or(0)
        }
        16..=231 => {
            let i = index as u32 - 16;
            let level = |v: u32| if v == 0 { 0 } else { 55 + v * 40 };
            (level(i / 36) << 16) | (level(i / 6 % 6) << 8) | level(i % 6)
        }
        232..=255 => {
            let v = 8 + (index as u32 - 232) * 10;
            (v << 16) | (v << 8) | v
        }
        // Foreground.
        256 => 0x000000,
        // Background.
        257 => 0xffffff,
        _ => 0x808080,
    };
    Rgb {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(message: &Value, y: usize) -> Vec<Value> {
        message["lines"]
            .as_array()
            .and_then(|lines| lines.iter().find(|l| l["y"] == y))
            .and_then(|l| l["runs"].as_array())
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn text_and_colours_become_runs() {
        let mut grid = TermGrid::new(20, 4);
        grid.advance(b"ab\x1b[31mcd\x1b[0m\r\n\x1b[48;2;1;2;3mX");
        let message = grid.frame(7).unwrap();
        assert_eq!(message["id"], 7);
        assert_eq!(message["full"], true);
        let first = runs(&message, 0);
        assert_eq!(first[0], json!(["ab", -1, -1, 0, 2]));
        // Red is the app's palette entry 1.
        assert_eq!(first[1], json!(["cd", PALETTE + 1, -1, 0, 2]));
        assert_eq!(runs(&message, 1)[0], json!(["X", -1, 0x010203, 0, 1]));
        assert_eq!(message["cursor"]["y"], 1);
        // Nothing new: nothing to send.
        assert!(grid.frame(7).is_none());
        // Only the changed row next time.
        grid.advance(b"!");
        let message = grid.frame(7).unwrap();
        assert_eq!(message["full"], false);
        assert_eq!(message["lines"].as_array().map(Vec::len), Some(1));
        assert!(grid.frame(7).is_none());
    }

    #[test]
    fn wide_characters_count_two_cells() {
        let mut grid = TermGrid::new(10, 2);
        grid.advance("a日b".as_bytes());
        let message = grid.frame(1).unwrap();
        let row = runs(&message, 0);
        let total: u64 = row.iter().filter_map(|r| r[4].as_u64()).sum();
        assert_eq!(total, 4);
        let text: String = row.iter().filter_map(|r| r[0].as_str()).collect();
        assert_eq!(text, "a日b");
    }

    #[test]
    fn palette_overrides_and_indexed_colours() {
        let mut grid = TermGrid::new(10, 2);
        grid.advance(b"\x1b[38;5;196mA\x1b[38;5;3mB\x1b]4;3;rgb:11/22/33\x07\x1b[38;5;3mC");
        let message = grid.frame(1).unwrap();
        let row = runs(&message, 0);
        assert_eq!(row[0][1], json!(PALETTE + 196));
        // A palette entry a program redefines changes the cells that use it, as in xterm.
        assert_eq!(row[1][0], "BC");
        assert_eq!(row[1][1], json!(0x112233));
    }

    #[test]
    fn device_attributes_are_answered_and_titles_reported() {
        let mut grid = TermGrid::new(10, 2);
        grid.advance(b"\x1b[c\x1b]0;hello\x07");
        assert!(!grid.take_replies().is_empty());
        assert_eq!(grid.frame(1).unwrap()["title"], "hello");
    }

    #[test]
    fn window_reports_are_answered() {
        let mut grid = TermGrid::new(30, 10);
        grid.advance(b"\x1b[18t");
        assert_eq!(
            String::from_utf8_lossy(&grid.take_replies()),
            "\x1b[8;10;30t"
        );
        grid.advance(b"\x1b[6n");
        assert_eq!(String::from_utf8_lossy(&grid.take_replies()), "\x1b[1;1R");
    }

    #[test]
    fn modes_follow_the_program() {
        let mut grid = TermGrid::new(10, 2);
        grid.advance(b"\x1b[?1h\x1b[?2004h\x1b[?1049h");
        let modes = grid.frame(1).unwrap()["modes"].as_u64().unwrap_or(0);
        assert_eq!(modes & (1 | 2 | 32), 1 | 2 | 32);
    }

    #[test]
    fn mouse_modes_are_told_apart() {
        let mut grid = TermGrid::new(10, 2);
        let bits = |grid: &mut TermGrid, input: &[u8]| {
            grid.advance(input);
            grid.modes()
        };
        assert_eq!(bits(&mut grid, b"\x1b[?1000h") & (4 | 128 | 256 | 1024), 4);
        assert_eq!(
            bits(&mut grid, b"\x1b[?1000l\x1b[?1002h") & (4 | 128 | 256),
            4 | 128
        );
        assert_eq!(
            bits(&mut grid, b"\x1b[?1002l\x1b[?1003h") & (4 | 128 | 256),
            4 | 256
        );
        assert_eq!(bits(&mut grid, b"\x1b[?1003l\x1b[?1006h") & (4 | 8), 8);
        assert_eq!(bits(&mut grid, b"\x1b[?1004h") & 16, 16);
        assert_eq!(bits(&mut grid, b"\x1b[?9h") & (4 | 1024), 4 | 1024);
        assert_eq!(bits(&mut grid, b"\x1b[?9l") & (4 | 1024), 0);
        assert_eq!(bits(&mut grid, b"\x1b[?1005h") & 512, 512);
    }

    fn fill(grid: &mut TermGrid, n: usize) {
        for i in 0..n {
            grid.advance(format!("line {i}\r\n").as_bytes());
        }
    }

    #[test]
    fn scrolling_back_shows_the_scrollback() {
        let mut grid = TermGrid::new(20, 4);
        fill(&mut grid, 10);
        grid.frame(1);
        assert_eq!(grid.history_size(), 7);
        grid.scroll(ScrollBy::Lines(3));
        let message = grid.frame(1).unwrap();
        assert_eq!(message["scroll"], json!({"offset": 3, "history": 7}));
        assert_eq!(runs(&message, 0)[0][0], "line 4");
        grid.scroll(ScrollBy::Top);
        let message = grid.frame(1).unwrap();
        assert_eq!(message["scroll"]["offset"], 7);
        assert_eq!(runs(&message, 0)[0][0], "line 0");
        grid.scroll(ScrollBy::Bottom);
        assert_eq!(grid.frame(1).unwrap()["scroll"]["offset"], 0);
        grid.scroll(ScrollBy::Page(true));
        assert!(grid.display_offset() > 0);
    }

    #[test]
    fn output_keeps_a_scrolled_view_in_place() {
        let mut grid = TermGrid::new(20, 4);
        fill(&mut grid, 10);
        grid.scroll(ScrollBy::Lines(2));
        let message = grid.frame(1).unwrap();
        let before = runs(&message, 0)[0][0].clone();
        grid.advance(b"more\r\n");
        let message = grid.frame(1);
        assert_eq!(grid.display_offset(), 3);
        // The rows on show did not change, so none was sent again.
        assert!(
            message.is_none_or(|m| m["lines"].as_array().is_none_or(Vec::is_empty)),
            "the view stays put"
        );
        assert_eq!(before, "line 5");
    }

    #[test]
    fn selection_covers_words_lines_and_blocks() {
        let mut grid = TermGrid::new(20, 4);
        grid.advance(b"hello world\r\nsecond line\r\nthird");
        grid.frame(1);
        grid.select_start(SelectionType::Semantic, 7, 0, false);
        let spans = grid.frame(1).unwrap()["selection"].clone();
        assert_eq!(spans, json!([[0, 6, 10]]));
        assert_eq!(grid.selection_text().as_deref(), Some("world"));
        grid.select_start(SelectionType::Lines, 3, 1, false);
        assert_eq!(grid.selection_text().as_deref(), Some("second line\n"));
        grid.select_start(SelectionType::Block, 1, 0, false);
        grid.select_update(3, 1, true);
        assert_eq!(grid.selection_text().as_deref(), Some("ell\neco"));
        grid.select_start(SelectionType::Simple, 0, 0, false);
        grid.select_update(4, 0, true);
        assert_eq!(grid.selection_text().as_deref(), Some("hello"));
        grid.select_clear();
        assert_eq!(grid.frame(1).unwrap()["selection"], json!([]));
        assert!(grid.selection_text().is_none());
        grid.select_all();
        assert!(grid.selection_text().unwrap_or_default().contains("third"));
    }

    #[test]
    fn soft_wrapped_lines_are_joined_when_copied() {
        let mut grid = TermGrid::new(5, 4);
        grid.advance(b"abcdefgh\r\nx");
        grid.select_all();
        assert_eq!(
            grid.selection_text().as_deref().map(str::trim_end),
            Some("abcdefgh\nx")
        );
    }

    #[test]
    fn hyperlinks_come_with_their_runs() {
        let mut grid = TermGrid::new(30, 2);
        grid.advance(b"a\x1b]8;;https://example.org/x\x07link\x1b]8;;\x07b");
        let message = grid.frame(1).unwrap();
        let row = runs(&message, 0);
        assert_eq!(row.len(), 3);
        assert_eq!(
            row[1],
            json!(["link", -1, -1, 0, 4, "https://example.org/x"])
        );
        assert_eq!(row[2][0], "b");
    }

    #[test]
    fn working_directory_bell_and_clipboard_are_reported() {
        let mut grid = TermGrid::new(20, 2);
        grid.advance(b"\x1b]7;file://host/home/me/my%20dir\x07\x07");
        // "hello" in base64.
        grid.advance(b"\x1b]52;c;aGVsbG8=\x07");
        let message = grid.frame(1).unwrap();
        assert_eq!(message["cwd"], "/home/me/my dir");
        assert_eq!(message["bell"], true);
        assert_eq!(
            message["clipboard"],
            json!([{"set": "hello", "selection": false}])
        );
        assert!(grid.frame(1).is_none());
    }

    #[test]
    fn clipboard_reads_need_the_option_and_an_answer() {
        let mut grid = TermGrid::new(20, 2);
        grid.advance(b"\x1b]52;c;?\x07");
        assert!(grid.frame(1).is_some_and(|m| m.get("clipboard").is_none()));
        let mut grid = TermGrid::with_options(
            20,
            2,
            Options {
                clipboard_read: true,
                ..Options::default()
            },
        );
        grid.advance(b"\x1b]52;c;?\x07");
        let message = grid.frame(1).unwrap();
        assert_eq!(
            message["clipboard"],
            json!([{"get": true, "selection": false}])
        );
        let reply = grid.clipboard_reply("hi");
        assert_eq!(String::from_utf8_lossy(&reply), "\x1b]52;c;aGk=\x07");
        assert!(
            grid.clipboard_reply("again").is_empty(),
            "one answer per question"
        );
    }

    #[test]
    fn writing_to_the_clipboard_can_be_turned_off() {
        let mut grid = TermGrid::with_options(
            20,
            2,
            Options {
                clipboard_write: false,
                ..Options::default()
            },
        );
        grid.advance(b"\x1b]52;c;aGVsbG8=\x07");
        assert!(grid.frame(1).unwrap().get("clipboard").is_none());
    }

    fn prompt(grid: &mut TermGrid, command: &str, output: &str) {
        grid.advance(b"\x1b]133;A\x07$ \x1b]133;B\x07");
        grid.advance(command.as_bytes());
        grid.advance(b"\r\n\x1b]133;C\x07");
        grid.advance(output.as_bytes());
        grid.advance(b"\x1b]133;D;0\x07");
    }

    #[test]
    fn shell_marks_survive_scrolling_and_drive_navigation() {
        let mut grid = TermGrid::new(20, 5);
        for i in 0..6 {
            prompt(
                &mut grid,
                &format!("cmd{i}"),
                &format!("out{i}\r\nmore{i}\r\n"),
            );
        }
        grid.advance(b"\x1b]133;A\x07$ \x1b]133;B\x07");
        let message = grid.frame(1).unwrap();
        assert_eq!(message["command"], json!({"running": false, "exit": 0}));
        assert!(message["marks"].as_array().is_some_and(|m| !m.is_empty()));
        assert_eq!(grid.mark_rows(PROMPT_MARK).len(), 7);
        // Previous prompt from the bottom: the view moves to put it at the top.
        grid.scroll(ScrollBy::Prompt(true));
        let message = grid.frame(1).unwrap();
        let first = runs(&message, 0)[0][0].as_str().unwrap_or("").to_owned();
        assert!(first.starts_with("$ cmd"), "{first}");
        grid.scroll(ScrollBy::Prompt(true));
        let message = grid.frame(1).unwrap();
        let second = runs(&message, 0)[0][0].as_str().unwrap_or("").to_owned();
        assert!(
            second.starts_with("$ cmd") && second < first,
            "{second} {first}"
        );
        for _ in 0..8 {
            grid.scroll(ScrollBy::Prompt(false));
        }
        assert_eq!(grid.display_offset(), 0);
        // The marker never shows up in the text.
        assert!(!grid.text().contains(PROMPT_MARK));
    }

    #[test]
    fn a_running_command_is_reported() {
        let mut grid = TermGrid::new(20, 5);
        grid.advance(b"\x1b]133;A\x07$ \x1b]133;B\x07sleep\r\n\x1b]133;C\x07");
        assert_eq!(grid.frame(1).unwrap()["command"], json!({"running": true}));
        assert!(grid.command_running());
        grid.advance(b"\x1b]133;D;130\x07");
        assert_eq!(
            grid.frame(1).unwrap()["command"],
            json!({"running": false, "exit": 130})
        );
    }

    #[test]
    fn the_output_of_the_last_command_can_be_selected() {
        let mut grid = TermGrid::new(20, 10);
        prompt(&mut grid, "one", "first\r\n");
        prompt(&mut grid, "two", "second\r\nthird\r\n");
        grid.advance(b"\x1b]133;A\x07$ \x1b]133;B\x07");
        assert!(grid.select_between_marks());
        assert_eq!(
            grid.selection_text()
                .map(|t| t.trim().to_owned())
                .as_deref(),
            Some("second\nthird")
        );
    }

    #[test]
    fn sequences_split_across_reads_are_still_read() {
        let mut grid = TermGrid::new(20, 2);
        for part in [&b"\x1b]7;file:///tm"[..], b"p/x\x1b", b"\\"] {
            grid.advance(part);
        }
        assert_eq!(grid.cwd(), Some("/tmp/x"));
    }

    #[test]
    fn search_finds_text_across_the_scrollback_and_wraps() {
        let mut grid = TermGrid::new(10, 3);
        grid.advance(b"Alpha beta\r\nxxxxxxxxxxALPHA\r\nend\r\nalpha");
        let found = grid.search("alpha", false);
        let found = found.as_array().cloned().unwrap_or_default();
        assert_eq!(found.len(), 3);
        assert_eq!(found[0], json!([[0, 0, 4]]));
        assert_eq!(grid.search("alpha", true).as_array().map(Vec::len), Some(1));
        // A match across a soft wrap comes in two pieces.
        let wrapped = grid.search("xALP", false);
        assert_eq!(wrapped, json!([[[1, 9, 9], [2, 0, 2]]]));
        assert_eq!(grid.search("", false), json!([]));
    }

    #[test]
    fn scrolling_to_a_match_centres_it() {
        let mut grid = TermGrid::new(20, 4);
        fill(&mut grid, 30);
        grid.frame(1);
        grid.scroll(ScrollBy::Reveal(2));
        let top = grid.top_abs();
        assert!((top..top + 4).contains(&2));
        grid.scroll(ScrollBy::Reveal(2));
        assert_eq!(grid.top_abs(), top, "already visible: no scrolling");
    }

    #[test]
    fn scrollback_can_be_cleared_trimmed_and_resized() {
        let mut grid = TermGrid::new(20, 4);
        fill(&mut grid, 20);
        assert!(grid.history_size() > 10);
        grid.clear_scrollback();
        assert_eq!(grid.history_size(), 0);
        // The limit still holds afterwards.
        fill(&mut grid, 20);
        assert!(grid.history_size() > 10);
        grid.configure(Options {
            scrollback: 5,
            ..Options::default()
        });
        assert_eq!(grid.history_size(), 5);
        fill(&mut grid, 20);
        assert_eq!(grid.history_size(), 5);
    }

    #[test]
    fn clearing_to_a_mark_keeps_the_text_below_it() {
        let mut grid = TermGrid::new(20, 4);
        for i in 0..5 {
            prompt(&mut grid, &format!("c{i}"), &format!("o{i}\r\n"));
        }
        grid.advance(b"\x1b]133;A\x07$ \x1b]133;B\x07");
        assert!(grid.clear_to_previous_mark());
        let text = grid.text();
        assert!(text.contains("$ c4"));
        assert!(!text.contains("$ c0"));
    }

    #[test]
    fn clear_to_start_leaves_the_cursor_row_on_top() {
        let mut grid = TermGrid::new(20, 5);
        fill(&mut grid, 12);
        grid.advance(b"$ ");
        grid.clear_to_start();
        assert_eq!(grid.history_size(), 0);
        let message = grid.frame(1).unwrap();
        assert_eq!(message["cursor"]["y"], 0);
        assert_eq!(runs(&message, 0)[0][0], "$");
    }

    #[test]
    fn clear_screen_pushes_the_screen_into_the_scrollback() {
        let mut grid = TermGrid::new(20, 4);
        grid.advance(b"one\r\ntwo\r\n");
        grid.clear_screen();
        let message = grid.frame(1).unwrap();
        assert_eq!(message["cursor"]["y"], 0);
        assert!(runs(&message, 0).is_empty());
        assert!(grid.text().contains("one"));
    }

    #[test]
    fn reset_restores_the_screen() {
        let mut grid = TermGrid::new(20, 4);
        grid.advance(b"\x1b[?1h\x1b[31mred\r\n");
        grid.reset(true);
        assert_eq!(grid.modes() & 1, 0);
        assert!(grid.text().is_empty());
    }

    #[test]
    fn a_synchronized_update_never_ended_does_not_freeze_the_view() {
        let mut grid = TermGrid::new(20, 2);
        grid.advance(b"\x1b[?2026hhidden");
        assert!(grid.frame(1).is_some_and(|m| {
            m["lines"]
                .as_array()
                .is_none_or(|l| l.iter().all(|l| l["runs"] == json!([])))
        }));
        std::thread::sleep(std::time::Duration::from_millis(200));
        let message = grid.frame(1).unwrap();
        assert_eq!(runs(&message, 0)[0][0], "hidden");
    }

    #[test]
    fn manual_marks_can_be_added_and_removed() {
        let mut grid = TermGrid::new(20, 4);
        grid.advance(b"abc");
        grid.mark_cursor_row();
        assert_eq!(grid.frame(1).unwrap()["marks"], json!([0]));
        assert!(grid.unmark());
        assert_eq!(grid.frame(1).unwrap()["marks"], json!([]));
        assert!(!grid.unmark());
    }

    #[test]
    fn file_urls_are_decoded() {
        assert_eq!(file_url_path("file://h/a%20b/c").as_deref(), Some("/a b/c"));
        assert_eq!(file_url_path("file:///x").as_deref(), Some("/x"));
        assert_eq!(file_url_path("http://h/x"), None);
        assert_eq!(file_url_path("file://nopath"), None);
    }

    #[test]
    fn indexed_colours_are_not_baked_in() {
        let colors = Colors::default();
        assert_eq!(palette(Some(1), &colors), PALETTE + 1);
        assert_eq!(palette(None, &colors), -1);
        assert_eq!(default_rgb(196), Rgb { r: 255, g: 0, b: 0 });
    }
}
