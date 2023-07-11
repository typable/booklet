use std::env;
use std::fs;
use std::io::Stdout;
use std::io::Write;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use terminal::Action;
use terminal::Clear;
use terminal::Event;
use terminal::KeyCode;
use terminal::KeyEvent;
use terminal::MouseButton;
use terminal::MouseEvent;
use terminal::Retrieved;
use terminal::Terminal;
use terminal::Value;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub bookmarks: Vec<usize>,
    pub markers: Vec<(usize, usize, usize)>,
}

#[derive(Debug, Default)]
pub struct State {
    pub config: Config,
    pub index: usize,
    pub selection: Option<(usize, usize, usize)>,
    pub pad_left: usize,
}

const OFFSET: usize = 15;
const WIDTH: usize = 80;

pub struct Codes;

impl Codes {
    pub const RESET: char = '\u{E000}';
    // italic
    pub const ITALIC: char = '\u{E003}';
    pub const RESET_ITALIC: char = '\u{E023}';
    // underline
    pub const UNDERLINE: char = '\u{E004}';
    pub const RESET_UNDERLINE: char = '\u{E024}';
    // foreground
    pub const RESET_FOREGROUND: char = '\u{E100}';
    pub const FOREGROUND_DEFAULT: char = '\u{E101}';
    // background
    pub const RESET_BACKGROUND: char = '\u{E200}';
    pub const BACKGROUND_MARKER: char = '\u{E201}';
    pub const BACKGROUND_SELECTION: char = '\u{E202}';
}

fn main() {
    if let Err(err) = run() {
        println!("{err}");
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let path = match args.next() {
        Some(path) => path,
        None => return Ok(()),
    };
    let mut index = 0;
    let mut content = read_content(&path)?;
    let mut open = false;
    let mut find_start = false;
    let mut chars = Vec::new();
    for char in content.chars() {
        if char == '_' {
            open = !open;
            if find_start {
                find_start = false;
            }
            if open {
                chars.push(Codes::ITALIC);
            } else {
                chars.push(Codes::RESET_ITALIC);
            }
            continue;
        }
        if char == '\n' && open {
            find_start = true;
            chars.push(Codes::RESET_ITALIC);
        }
        if !char.is_whitespace() && find_start {
            find_start = false;
            chars.push(Codes::ITALIC);
        }
        chars.push(char);
    }
    content = chars.iter().collect();
    let mut config = read_config(&path)?.unwrap_or_default();
    config.bookmarks.sort();
    let mut state = State {
        config,
        ..Default::default()
    };
    let mut term = terminal::stdout();
    term.batch(Action::EnterAlternateScreen)?;
    term.batch(Action::EnableRawMode)?;
    term.batch(Action::HideCursor)?;
    term.batch(Action::EnableMouseCapture)?;
    term.flush_batch()?;
    let (cols, rows) = read_size(&mut term)?.unwrap_or((0, 0));
    let cols = cols as usize;
    let rows = rows as usize;
    let lines = content.lines().collect::<Vec<_>>();
    let line_count = lines.len();
    let pad_left = (cols / 2).saturating_sub(WIDTH / 2);
    state.pad_left = pad_left;
    for bookmark in &state.config.bookmarks {
        if bookmark > &index {
            index = *bookmark;
            break;
        }
    }
    render_page(&mut term, &lines, index, rows, &state)?;
    loop {
        if let Retrieved::Event(Some(event)) = term.get(Value::Event(None))? {
            match event {
                Event::Key(key) => match key.code {
                    KeyCode::Char(char) => match char {
                        'q' => break,
                        'j' => {
                            if index < line_count.saturating_sub(1) {
                                index += 1;
                                render_page(&mut term, &lines, index, rows, &state)?;
                            }
                        }
                        'k' => {
                            if index > 0 {
                                index -= 1;
                                render_page(&mut term, &lines, index, rows, &state)?;
                            }
                        }
                        'g' => loop {
                            if let Some(key) = read_key(&mut term)? {
                                match key.code {
                                    KeyCode::Char(char) => match char {
                                        'g' => {
                                            index = 0;
                                            render_page(&mut term, &lines, index, rows, &state)?;
                                            break;
                                        }
                                        'e' => {
                                            index = line_count.saturating_sub(1);
                                            render_page(&mut term, &lines, index, rows, &state)?;
                                            break;
                                        }
                                        'n' => {
                                            for bookmark in &state.config.bookmarks {
                                                if bookmark > &index {
                                                    index = *bookmark;
                                                    render_page(
                                                        &mut term, &lines, index, rows, &state,
                                                    )?;
                                                    break;
                                                }
                                            }
                                            break;
                                        }
                                        'p' => {
                                            for bookmark in state.config.bookmarks.iter().rev() {
                                                if bookmark < &index {
                                                    index = *bookmark;
                                                    render_page(
                                                        &mut term, &lines, index, rows, &state,
                                                    )?;
                                                    break;
                                                }
                                            }
                                            break;
                                        }
                                        _ => (),
                                    },
                                    KeyCode::Esc => break,
                                    _ => (),
                                }
                            }
                        },
                        'x' => {
                            let pos = index;
                            match state.config.bookmarks.iter().position(|item| item == &pos) {
                                Some(index) => {
                                    state.config.bookmarks.remove(index);
                                }
                                None => {
                                    state.config.bookmarks.push(pos);
                                }
                            }
                            write_config(&path, &state.config)?;
                            render_line(&mut term, OFFSET, &lines, index, &state)?;
                        }
                        'm' => {
                            if let Some(selection) = state.selection {
                                match state
                                    .config
                                    .markers
                                    .iter()
                                    .position(|item| item == &selection)
                                {
                                    Some(index) => {
                                        state.config.markers.remove(index);
                                    }
                                    None => {
                                        state.config.markers.push(selection);
                                    }
                                }
                                write_config(&path, &state.config)?;
                                render_line(&mut term, OFFSET, &lines, index, &state)?;
                            }
                        }
                        _ => (),
                    },
                    KeyCode::Esc => {
                        if state.selection.is_some() {
                            state.selection = None;
                            render_page(&mut term, &lines, index, rows, &state)?;
                        }
                    }
                    _ => (),
                },
                Event::Mouse(MouseEvent::Up(MouseButton::Left, col, row, _)) => {
                    let col = col as usize;
                    let row = row as usize;
                    let pos = (index + row).saturating_sub(OFFSET);
                    if col >= state.pad_left + 8 {
                        let col = col.saturating_sub(state.pad_left).saturating_sub(8);
                        if let Some(line) = lines.get(pos) {
                            let line = line.replace("\x1b[4m", "");
                            let chars = line.chars().collect::<Vec<_>>();
                            if let Some(char) = chars.get(col) {
                                // mark words
                                if char.is_alphabetic() {
                                    let mut start = col;
                                    for i in (0..col).rev() {
                                        if let Some(char) = chars.get(i) {
                                            if !char.is_alphabetic() {
                                                start = i + 1;
                                                break;
                                            }
                                        }
                                        start = 0;
                                    }
                                    let mut end = col;
                                    for i in col..chars.len() {
                                        if let Some(char) = chars.get(i) {
                                            if !char.is_alphabetic() {
                                                end = i;
                                                break;
                                            }
                                        }
                                        end = chars.len();
                                    }
                                    if state.selection != Some((pos, start, end)) {
                                        state.selection = Some((pos, start, end));
                                        render_page(&mut term, &lines, index, rows, &state)?;
                                    }
                                }
                                // mark numbers
                                if char.is_numeric() {
                                    let mut start = col;
                                    for i in (0..col).rev() {
                                        if let Some(char) = chars.get(i) {
                                            if !char.is_numeric() {
                                                start = i + 1;
                                                break;
                                            }
                                        }
                                        start = 0;
                                    }
                                    let mut end = col;
                                    for i in col..chars.len() {
                                        if let Some(char) = chars.get(i) {
                                            if !char.is_numeric() {
                                                end = i;
                                                break;
                                            }
                                        }
                                        end = chars.len();
                                    }
                                    if state.selection != Some((pos, start, end)) {
                                        state.selection = Some((pos, start, end));
                                        render_page(&mut term, &lines, index, rows, &state)?;
                                    }
                                }
                            }
                        }
                    }
                }
                _ => (),
            }
        }
    }
    term.batch(Action::DisableMouseCapture)?;
    term.batch(Action::ShowCursor)?;
    term.batch(Action::DisableRawMode)?;
    term.batch(Action::LeaveAlternateScreen)?;
    term.flush_batch()?;
    Ok(())
}

fn render_page(
    term: &mut Terminal<Stdout>,
    lines: &[&str],
    index: usize,
    rows: usize,
    state: &State,
) -> Result<()> {
    for i in 0..rows {
        term.act(Action::MoveCursorTo(0, i as u16))?;
        term.batch(Action::ClearTerminal(Clear::CurrentLine))?;
        if index + i >= OFFSET {
            let pos = (index + i).saturating_sub(OFFSET);
            if let Some(line) = lines.get(pos) {
                let mut line = line.to_string();
                let line_number = pos;
                let is_bookmarked = state.config.bookmarks.contains(&line_number);
                // insert selections
                if let Some(selection) = &state.selection {
                    let (row, start, end) = selection;
                    if row == &pos {
                        let mut chars = Vec::new();
                        for (i, char) in line.chars().enumerate() {
                            if &i == start {
                                chars.push(Codes::BACKGROUND_SELECTION);
                            }
                            if &i == end {
                                chars.push(Codes::RESET_BACKGROUND);
                            }
                            chars.push(char);
                        }
                        line = chars.iter().collect();
                    }
                }
                // insert markers
                for marker in &state.config.markers {
                    let (row, start, end) = marker;
                    if row == &pos {
                        let mut chars = Vec::new();
                        for (i, char) in line.chars().enumerate() {
                            if &i == start {
                                chars.push(Codes::BACKGROUND_MARKER);
                            }
                            if &i == end {
                                chars.push(Codes::RESET_BACKGROUND);
                            }
                            chars.push(char);
                        }
                        line = chars.iter().collect();
                    }
                }
                let mut slices = Vec::new();
                for char in line.chars() {
                    match char {
                        Codes::RESET => slices.push("\x1b[0m".to_string()),
                        Codes::ITALIC => slices.push("\x1b[3m".to_string()),
                        Codes::RESET_ITALIC => slices.push("\x1b[23m".to_string()),
                        Codes::UNDERLINE => slices.push("\x1b[4m".to_string()),
                        Codes::RESET_UNDERLINE => slices.push("\x1b[24m".to_string()),
                        Codes::BACKGROUND_MARKER => slices.push("\x1b[48;2;90;90;0m".to_string()),
                        Codes::BACKGROUND_SELECTION => {
                            slices.push("\x1b[48;2;100;100;100m".to_string())
                        }
                        Codes::RESET_BACKGROUND => slices.push("\x1b[49m".to_string()),
                        _ => slices.push(char.to_string()),
                    }
                }
                line = slices.join("");
                term.flush_batch()?;
                term.write_all(
                    format!(
                        "{: >pad_left$}{}{: >5} \x1b[0m  {}\x1b[38;2;240;240;240m{line}\x1b[0m",
                        "",
                        if i == OFFSET {
                            "\x1b[38;2;200;200;0m"
                        } else {
                            "\x1b[38;2;130;130;130m"
                        },
                        if line_number % 5 == 0 || i == OFFSET {
                            line_number.to_string()
                        } else {
                            String::default()
                        },
                        if is_bookmarked {
                            "\x1b[48;2;90;90;0m"
                        } else {
                            ""
                        },
                        pad_left = state.pad_left,
                    )
                    .as_bytes(),
                )?;
            }
        }
    }
    term.flush()?;
    Ok(())
}

fn render_line(
    term: &mut Terminal<Stdout>,
    current: usize,
    lines: &[&str],
    index: usize,
    state: &State,
) -> Result<()> {
    let pos = (index + current).saturating_sub(OFFSET);
    if let Some(line) = lines.get(pos) {
        let line_number = index;
        let is_bookmarked = state.config.bookmarks.contains(&line_number);
        term.batch(Action::MoveCursorTo(0, current as u16))?;
        term.batch(Action::ClearTerminal(Clear::CurrentLine))?;
        term.flush_batch()?;
        term.write_all(
            format!(
                "{: >pad_left$}{}{: >5} \x1b[0m  {}\x1b[38;2;240;240;240m{line}\x1b[0m",
                "",
                if current == OFFSET {
                    "\x1b[38;2;200;200;0m"
                } else {
                    "\x1b[38;2;130;130;130m"
                },
                if line_number % 5 == 0 || current == OFFSET {
                    line_number.to_string()
                } else {
                    String::default()
                },
                if is_bookmarked {
                    "\x1b[48;2;90;90;0m"
                } else {
                    ""
                },
                pad_left = state.pad_left,
            )
            .as_bytes(),
        )?;
    }
    term.flush()?;
    Ok(())
}

fn read_key(term: &mut Terminal<Stdout>) -> Result<Option<KeyEvent>> {
    if let Retrieved::Event(Some(Event::Key(key))) = term.get(Value::Event(None))? {
        return Ok(Some(key));
    }
    Ok(None)
}

fn read_size(term: &mut Terminal<Stdout>) -> Result<Option<(u16, u16)>> {
    if let Retrieved::TerminalSize(cols, rows) = term.get(Value::TerminalSize)? {
        return Ok(Some((cols, rows)));
    }
    Ok(None)
}

fn read_content(path: &str) -> Result<String> {
    let content = fs::read_to_string(path)?;
    Ok(content)
}

fn read_config(path: &str) -> Result<Option<Config>> {
    let mut path_buf = PathBuf::from(path);
    if let Some(filename) = path_buf.file_name() {
        let filename = filename.to_os_string().into_string().unwrap();
        path_buf.pop();
        path_buf.push(format!(".booklet_{filename}"));
        if !path_buf.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path_buf)?;
        let config = toml::from_str::<Config>(&content)?;
        return Ok(Some(config));
    }
    Ok(None)
}

fn write_config(path: &str, config: &Config) -> Result<()> {
    let mut path_buf = PathBuf::from(path);
    if let Some(filename) = path_buf.file_name() {
        let filename = filename.to_os_string().into_string().unwrap();
        path_buf.pop();
        path_buf.push(format!(".booklet_{filename}"));
        let content = toml::to_string(config)?;
        fs::write(path_buf, content)?;
    }
    Ok(())
}
