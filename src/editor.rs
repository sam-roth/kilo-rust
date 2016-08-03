use std::default::Default;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{io, fs};

use libc;
use low_level;
use read_key;
use syntax;

fn uclamp(a: isize) -> usize {
    if a < 0 {
        0
    } else {
        a as usize
    }
}

fn isprint(b: u8) -> bool {
    unsafe {
        if libc::isprint(b as libc::c_int) != 0 {
            true
        } else {
            false
        }
    }
}

/// Returns the first character index of the start of a substring
/// searching from a given character index.
fn find_char(s: &str, query: &str, from_char: usize) -> Option<usize> {
    let from_byte_index = s.char_indices().nth(from_char);
    if let Some((from_byte_index, _)) = from_byte_index {
        let slice = &s[from_byte_index..];

        if let Some(match_byte_index) = slice.find(query) {
            let match_char_index = slice.char_indices()
                .position(|(i, _)| i == match_byte_index)
                .expect("byte did not correspond to char");

            return Some(match_char_index + from_char);
        }
    }

    None
}

/// Returns the last character index of the start of a substring
/// searching backwards from a given character index.
fn rfind_char(s: &str, query: &str, to_char: usize) -> Option<usize> {
    let to_byte_index = s.char_indices().nth(to_char);
    if let Some((to_byte_index, _)) = to_byte_index {
        let slice = &s[..to_byte_index];

        if let Some(match_byte_index) = slice.rfind(query) {
            let match_char_index = slice.char_indices()
                .position(|(i, _)| i == match_byte_index)
                .expect("byte did not correspond to char");

            return Some(match_char_index);
        }
    }

    None
}

#[derive(Debug, Eq, PartialEq, Default, Clone, Copy)]
struct Pos {
    x: usize,
    y: usize,
}

fn pos(x: usize, y: usize) -> Pos {
    Pos {x: x, y: y}
}

#[derive(Debug, Eq, PartialEq, Default, Clone, Copy)]
struct Delta {
    dx: isize,
    dy: isize,
}

fn delta(dx: isize, dy: isize) -> Delta {
    Delta {dx: dx, dy: dy}
}

#[derive(Debug)]
struct StatusMessage {
    text: String,
    time: Instant,
}

fn get_window_size() -> io::Result<Pos> {
    let mut ws = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let ioctl_rv = unsafe {
        libc::ioctl(1, libc::TIOCGWINSZ, &mut ws)
    };

    if ioctl_rv == -1 {
        return Err(io::Error::last_os_error());
    }

    Ok(Pos {x: ws.ws_col as usize,
            y: ws.ws_row as usize})
}

#[derive(Default)]
struct Row {
    text: String,
    render: String,
    highlight: Option<syntax::HighlightResult>,
}

impl Row {
    fn new() -> Row {
        Default::default()
    }

    fn update(&mut self, text: String) {
        self.text = text;
        self.render.clear();

        for ch in self.text.chars() {
            if ch == '\t' {
                self.render.push(' ');

                while self.render.len() % 8 != 0 {
                    self.render.push(' ');
                }
            } else {
                self.render.push(ch);
            }
        }

        self.highlight = None;
    }
}

#[derive(Default)]
pub struct Editor {
    cursor: Pos,
    // offset is always positive so use Pos, not Delta
    offset: Pos,
    screen: Pos,

    orig_termios: Option<libc::termios>,

    rows: Vec<Row>,

    // dirty: bool,

    file_path: Option<PathBuf>,
    status_msg: Option<StatusMessage>,
    syntax: Option<syntax::Syntax>,
}

impl Editor {
    pub fn new() -> io::Result<Editor> {
        let mut screen = get_window_size()?;
        screen.y -= 2;          // for status bar

        let mut result: Editor = Default::default();
        result.screen = screen;
        result.cursor.x = 1;
        result.syntax = Some(syntax::make_rust_syntax());

        Ok(result)
    }

    pub fn open(&mut self, path: &Path) -> io::Result<()> {
        let file = fs::File::open(path)?;
        let reader = io::BufReader::new(file);

        self.rows.clear();

        for line in reader.lines() {
            let row = Row::new();
            self.rows.push(row);
            let new_index = self.rows.len() - 1;
            self.update_row(new_index, line?);
        }

        self.file_path = Some(PathBuf::from(path));

        Ok(())
    }

    fn save(&mut self) -> io::Result<()> {
        use std::io::Write;

        let path = self.file_path.clone().unwrap();
        let file = fs::File::create(path)?;
        let mut writer = io::BufWriter::new(file);

        for row in &self.rows {
            let text: &str = &row.text;
            writer.write_all(text.as_bytes())?;
            writer.write_all(b"\n")?;
        }

        Ok(())
    }

    pub fn enable_raw_mode(&mut self) -> io::Result<()> {
        // "Raw mode: 1960 magic shit" -- Antirez

        use libc::*;

        let fd = STDIN_FILENO;

        if let Some(_) = self.orig_termios {
            return Ok(());      // already in raw mode
        }

        let mut raw = low_level::get_termios(fd)?;
        let orig_termios = raw.clone();

        if unsafe { isatty(fd) } == 0 {
            panic!("stdin is not a TTY");
        }

        // Input modes: no break, no CR -> newline, no parity check,
        // no strip char, no start/stop output control
        raw.c_iflag &= !(BRKINT | ICRNL | INPCK | ISTRIP | IXON);
        // Output modes: disable post-processing
        raw.c_oflag &= !OPOST;
        // Control modes: set 8-bit chars
        raw.c_cflag |= CS8;
        // Local modes: no echoing, not canonical, no extended functions,
        // no signal chars (^Z, ^C)
        raw.c_lflag &= !(ECHO | ICANON | IEXTEN | ISIG);
        // Return each byte or zero on timeout
        raw.c_cc[VMIN] = 0;
        // 1 decisecond timeout
        raw.c_cc[VTIME] = 1;

        low_level::set_termios(fd, TCSAFLUSH, &raw)?;

        self.orig_termios = Some(orig_termios);

        Ok(())
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        if let Some(cooked) = self.orig_termios {
            low_level::set_termios(libc::STDIN_FILENO, libc::TCSAFLUSH, &cooked)?;
            self.orig_termios = None;
        }

        Ok(())
    }

    fn visual_cursor_position(&self) -> Pos {
        match self.rows.get(self.cursor.y) {
            Some(row) => {
                let mut x = 0;

                for ch in row.text.chars()
                        .skip(self.offset.x)
                        .take(self.screen.x)
                        .take(self.cursor.x) {
                    if ch == '\t' {
                        x += 1;

                        while x % 8 != 0 {
                            x += 1;
                        }
                    } else {
                        x += 1;
                    }
                }

                Pos {y: self.cursor.y - self.offset.y, x: x}
            },
            None => Pos {y: self.cursor.y - self.offset.y, x: 0},
        }
    }

    fn row_needs_rehighlight(&self, index: usize) -> bool {
        if let None = self.syntax {
            return false;
        }

        if index == 0 {
            return if let Some(_) = self.rows[index].highlight {
                false
            } else {
                true
            };
        }

        if let Some(ref hl_line) = self.rows[index].highlight {
            if let Some(ref hl_above) = self.rows[index - 1].highlight {
                hl_line.initial_state != hl_above.ending_state
            } else {
                panic!("row highlighted before above row");
            }
        } else {
            true
        }
    }

    fn update_row(&mut self, mut index: usize, text: String) {
        self.rows[index].update(text);

        if let Some(ref syntax) = self.syntax {
            loop {
                if index >= self.rows.len() {
                    break;
                }

                if self.row_needs_rehighlight(index) {
                    let init_state =
                        if index > 0 {
                            if let Some(ref hl) = self.rows[index - 1].highlight {
                                hl.ending_state
                            } else {
                                syntax::Highlight::Normal
                            }
                        } else {
                            syntax::Highlight::Normal
                        };
                    let row = &mut self.rows[index];
                    let highlight_res = syntax.highlight(init_state, &row.render);
                    row.highlight = Some(highlight_res);
                }

                index += 1;
            }
        }
    }

    pub fn refresh_screen(&mut self) -> io::Result<()> {
        let mut buf: Vec<u8> = vec![];

        buf.extend(b"\x1b[?25l"); // Hide cursor
        buf.extend(b"\x1b[H");    // Go home

        for y in 0..self.screen.y {
            let row_index = self.offset.y + y;

            if row_index >= self.rows.len() {
                buf.extend(b"~\x1b[0K\r\n"); // CSI 0 K = Erase from cursor to EOL
                continue;
            }

            let row = &self.rows[row_index];

            let trimmed_row: String = row.render.chars()
                .skip(self.offset.x)
                .take(self.screen.x)
                .collect();

            if let Some(ref highlight) = row.highlight {
                let mut current_color = None;

                let trimmed_highlight = highlight.highlight.iter()
                    .skip(self.offset.x)
                    .take(self.screen.x);

                for (ch, hl) in trimmed_row.chars().zip(trimmed_highlight) {
                    let color = hl.color();
                    let color_changed = match current_color {
                        None => true,
                        Some(c) if c != color => true,
                        _ => false,
                    };

                    if color_changed {
                        buf.extend(format!("\x1b[{}m", color).as_bytes());
                        current_color = Some(color);
                    }

                    buf.extend(ch.encode_utf8().as_slice());
                }
                buf.extend(b"\x1b[0m");
            } else {
                buf.extend(trimmed_row.as_bytes());
            }
            buf.extend(b"\x1b[0K\r\n");
        }

        // Create two lines for status.

        // First line:
        buf.extend(b"\x1b[0K"); // CSI 0 K = Erase from cursor to EOL
        buf.extend(b"\x1b[7m"); // CSI 7 m = Use inverse video
        let cursor_fix = self.fixup(self.cursor);

        let path_string: String;

        if let Some(ref p) = self.file_path {
            path_string = p.to_string_lossy().into_owned();
        } else {
            path_string = String::from("<unsaved>");
        }

        let left = format!(
            "{:<.20}:{}:{} - {} lines {}",
            &path_string,
            cursor_fix.y + 1,
            cursor_fix.x,
            self.rows.len(),
            "");

        buf.extend(format!("{:<width$}",
                           left,
                           width = self.screen.x).as_bytes());

        buf.extend(b"\x1b[0m\r\n"); // Reset char attributes

        // Second line:
        buf.extend(b"\x1b[0K"); // CSI 0 K = Erase from cursor to EOL

        if let &Some(ref status_msg) = &self.status_msg {
            buf.extend(format!("{:<.width$}",
                               status_msg.text,
                               width = self.screen.x).as_bytes());
        }

        buf.extend(b"\x1b[?25h"); // Make cursor visible again

        let visual_cursor = self.visual_cursor_position();

        buf.extend(b"\x1b[");
        buf.extend(format!("{};{}H", visual_cursor.y + 1, visual_cursor.x + 1).as_bytes());

        let stdout = io::stdout();
        let write: &mut io::Write = &mut stdout.lock();

        write.write_all(&buf)?;
        write.flush()?;

        Ok(())
    }

    pub fn handle_keypress(&mut self, key: read_key::Key) -> bool {
        use read_key::Key::*;
        use read_key::key_codes::*;

        match key {
            Char(CTRL_C) => (),
            Char(CTRL_Q) => return false,
            Char(CTRL_S) =>
                if let Err(e) = self.save() {
                    self.set_status_message(
                        format!("Error: {}", e));
                } else {
                    self.set_status_message("Saved file".to_owned());
                },
            Char(CTRL_F) => {
                if let Err(e) = self.find() {
                    self.set_status_message(
                        format!("Error: {}", e));
                }
            },
            Char(ENTER) | Char(b'\n') =>
                self.insert_newline(),
            Char(BACKSPACE) | Char(CTRL_H) =>
                self.backspace(),
            PageUp | PageDown | ArrowUp | ArrowDown
                | ArrowLeft | ArrowRight =>
                    self.handle_cursor_move_keypress(key),
            Char(CTRL_L) => (),                         // Refresh screen as side effect
            Char(ch) => self.insert_char(ch as char),
            _ => (),                                    // Unknown. Do nothing.
        }

        true
    }

    fn handle_cursor_move_keypress(&mut self, key: read_key::Key) {
        use read_key::Key::*;

        let delta = match key {
            ArrowUp => delta(0, -1),
            ArrowDown => delta(0, 1),
            ArrowLeft => delta(-1, 0),
            ArrowRight => delta(1, 0),
            PageUp => delta(0, -(self.screen.y as isize)),
            PageDown => delta(0, self.screen.y as isize),
            _ => return,
        };

        self.move_cursor_by(delta);
    }

    fn move_cursor_to(&mut self, mut pos: Pos) {
        if pos.y >= self.rows.len() {
            if self.rows.len() == 0 {
                pos.y = 0;
            } else {
                pos.y = self.rows.len() - 1;
            }
        }

        self.scroll_to(pos);

        self.cursor = pos;
    }

    /// Constrain the `x` of the cursor to its line
    fn fixup(&self, pos: Pos) -> Pos {
        let Pos {mut x, y} = pos;

        match self.rows.get(y) {
            Some(row) if x > row.text.len() =>
                x = row.text.len(),
            _ => (),
        }

        Pos {x: x, y: y}
    }

    fn scroll_to(&mut self, pos: Pos) {
        let Pos {y, ..} = pos;

        if y < self.offset.y {
            self.offset.y = y;
        } else if y >= self.offset.y + self.screen.y {
            self.offset.y = y - self.screen.y + 1;
        }
    }

    fn move_cursor_by(&mut self, delta: Delta) {
        let Delta {dx, dy} = delta;

        // Don't pre-fixup `x` unless the user explicitly requests it
        // by moving laterally.
        let Pos {x, y} = if dx != 0 {
            self.fixup(self.cursor)
        } else {
            self.cursor
        };

        let new_curs = pos(uclamp(x as isize + dx),
                           uclamp(y as isize + dy));

        // Again, only post-fixup if this was explicitly requested.
        let new_curs = if dx != 0 {
            self.fixup(new_curs)
        } else {
            new_curs
        };

        self.move_cursor_to(new_curs);
    }

    pub fn set_status_message(&mut self, msg: String) {
        self.status_msg = Some(StatusMessage {
            text: msg,
            time: Instant::now()
        });
    }

    fn ensure_line_exists(&mut self) {
        while self.rows.len() <= self.cursor.y {
            self.rows.push(Row::new());
        }
    }

    fn insert_char(&mut self, ch: char) {
        assert!(ch != '\n', "insert_char called with newline");

        self.ensure_line_exists();

        let cursor_fixup = self.fixup(self.cursor);

        let mut row_text;

        {
            let row: &Row = &self.rows[self.cursor.y];
            row_text = row.text.clone();
        }
        row_text.insert(cursor_fixup.x, ch);

        let row_index = self.cursor.y;
        self.update_row(row_index, row_text);

        self.cursor.x += 1;
    }

    fn insert_newline(&mut self) {
        self.ensure_line_exists();

        let Pos {x, y} = self.fixup(self.cursor);

        let row_left: String;
        let row_right: String;

        {
            let row: &Row = &mut self.rows[y];
            row_left = (&row.text[..x]).to_owned();
            row_right = (&row.text[x..]).to_owned();
        }

        self.update_row(y, row_left);

        let new_row = Row::new();
        let new_row_y = y + 1;

        self.rows.insert(new_row_y, new_row);
        self.update_row(new_row_y, row_right);
        self.move_cursor_to(Pos {x: 0, y: new_row_y});
    }

    fn backspace(&mut self) {
        self.ensure_line_exists();

        let Pos {x, y} = self.fixup(self.cursor);

        if x == 0 {
            if y == 0 {
                return;
            }
            let new_y = y - 1;
            let new_x = self.rows[new_y].text.len();

            let lower_row = self.rows.remove(y);

            let mut new_row_text = self.rows[new_y].text.clone();
            new_row_text.push_str(&lower_row.text);

            self.update_row(new_y, new_row_text);

            self.move_cursor_to(Pos {x: new_x, y: new_y});
        } else {
            let mut row_text = self.rows[y].text.clone();
            row_text.remove(x - 1);
            self.update_row(y, row_text);
            self.move_cursor_to(Pos {x: x - 1, y: y});
        }
    }

    fn find(&mut self) -> io::Result<()> {
        use read_key::Key::*;
        use read_key::key_codes::*;

        let mut query = String::new();
        let mut direction = 0isize;

        let saved_cursor = self.cursor;
        let saved_offset = self.offset;

        let stdin = io::stdin();

        loop {
            self.set_status_message(format!(
                "Search: {} (Use ESC/Arrows/Enter)",
                query));
            self.refresh_screen()?;

            let key = {
                if let Some(k) = read_key::read_escape(&mut stdin.lock())?.interpret() {
                    k
                } else {
                    continue;
                }
            };

            match key {
                Char(CTRL_H) | Char(BACKSPACE) => {
                    let _ = query.pop();
                },
                Esc | Char(ENTER) | Char(b'\n') => {
                    if let Esc = key {
                        self.cursor = saved_cursor;
                        self.offset = saved_offset;
                    }

                    self.set_status_message("".to_owned());
                    break;
                },
                ArrowRight | ArrowDown => {
                    direction = 1;
                },
                ArrowLeft | ArrowUp => {
                    direction = -1;
                },
                Char(ch) if isprint(ch) => {
                    query.push(ch as char);
                },
                _ => (),
            }

            let tmp_cursor = self.cursor;
            let found = if direction >= 0 {
                if direction > 0 {
                    self.move_cursor_by(delta(1, 0));
                }
                self.search_forward(&query)
            } else {
                self.search_backward(&query)
            };

            if !found {
                self.move_cursor_to(tmp_cursor);
            }
        }

        Ok(())
    }

    fn search_forward(&mut self, query: &str) -> bool {
        loop {
            let Pos {x, y} = self.cursor;

            if let Some(match_idx) = find_char(&self.rows[y].text, query, x) {
                self.move_cursor_to(pos(match_idx, y));
                return true;
            }

            if y + 1 == self.rows.len() {
                return false;
            }

            self.move_cursor_to(Pos {x: 0, y: y + 1});
        }
    }


    fn search_backward(&mut self, query: &str) -> bool {
        loop {
            let Pos {x, y} = self.cursor;

            if let Some(match_idx) = rfind_char(&self.rows[y].text, query, x) {
                self.move_cursor_to(Pos {x: match_idx, y: y});
                return true;
            }

            if y == 0 {
                return false;
            }

            let upper_line_len = self.rows[y -  1].text.chars().count();

            self.move_cursor_to(Pos {x: upper_line_len.saturating_sub(1),
                                     y: y - 1});
        }
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        if let Err(e) = self.disable_raw_mode() {
            println!("Warning: Couldn't disable raw mode: {}", e);
        }
    }
}
