use std::env;
use std::io::Stdout;
use std::io::Write;

use booklet::Book;
use booklet::Config;
use booklet::Definition;
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

use booklet::Codes;
use booklet::Result;
use booklet::State;

const OFFSET: usize = 15;

#[async_std::main]
async fn main() {
    if let Err(err) = run().await {
        println!("{err}");
    }
}

async fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let path = match args.next() {
        Some(path) => path,
        None => return Ok(()),
    };
    let mut term = terminal::stdout();
    term.batch(Action::EnterAlternateScreen)?;
    term.batch(Action::EnableRawMode)?;
    term.batch(Action::HideCursor)?;
    term.batch(Action::EnableMouseCapture)?;
    term.flush_batch()?;
    let config = Config::from_path(&path)?;
    let book = Book::from_path(&path)?;
    let mut state = State::new(&path, config, book);
    if let Some((cols, rows)) = read_size(&mut term)? {
        state.resize_screen(cols as usize, rows as usize);
    }
    state.goto_next_bookmark();
    loop {
        if state.update_screen {
            render(&mut term, &state)?;
            state.update_screen = false;
        }
        if let Retrieved::Event(Some(event)) = term.get(Value::Event(None))? {
            match event {
                Event::Resize => {
                    if let Some((cols, rows)) = read_size(&mut term)? {
                        state.resize_screen(cols as usize, rows as usize);
                    }
                }
                Event::Key(key) => {
                    match key.code {
                        KeyCode::Char(char) => {
                            match char {
                                'q' => break,
                                'j' => state.move_down(),
                                'k' => state.move_up(),
                                'g' => {
                                    if let Some(key) = read_key(&mut term)? {
                                        match key.code {
                                            KeyCode::Esc => break,
                                            KeyCode::Char(char) => match char {
                                                'g' => state.goto_top(),
                                                'e' => state.goto_bottom(),
                                                'n' => state.goto_next_bookmark(),
                                                'p' => state.goto_prev_bookmark(),
                                                _ => (),
                                            },
                                            _ => (),
                                        }
                                    }
                                }
                                'x' => {
                                    let line_number = state.line_number;
                                    if state.has_bookmark(line_number) {
                                        state.remove_bookmark(line_number)?;
                                    } else {
                                        state.add_bookmark(line_number)?;
                                    }
                                }
                                'd' => {
                                    if let Some(selection) = state.selection {
                                        let (pos, start, end) = selection;
                                        let line = state.book.lines.get(pos).unwrap();
                                        let word = &line[start..end];
                                        let url = format!("https://api.dictionaryapi.dev/api/v2/entries/en/{word}");
                                        let res = reqwest::get(url).await?;
                                        let result: serde_json::Value = res.json().await?;
                                        if let Some(definition) = Definition::from_json(&result) {
                                            state.definition = Some((selection, definition));
                                            render(&mut term, &state)?;
                                        }
                                    }
                                }
                                'f' => {
                                    state.focus_mode = !state.focus_mode;
                                    render(&mut term, &state)?;
                                }
                                // 'm' => {
                                //     if let Some(selection) = state.selection {
                                //         match state
                                //             .config
                                //             .markers
                                //             .iter()
                                //             .position(|item| item == &selection)
                                //         {
                                //             Some(index) => {
                                //                 state.config.markers.remove(index);
                                //             }
                                //             None => {
                                //                 state.config.markers.push(selection);
                                //             }
                                //         }
                                //         state.config.write(&path)?;
                                //         render(&mut term, &state)?;
                                //     }
                                // }
                                _ => (),
                            }
                        }
                        KeyCode::Esc => {
                            state.clear_selection();
                            state.definition = None;
                            render(&mut term, &state)?;
                        }
                        _ => (),
                    }
                }
                Event::Mouse(MouseEvent::Up(MouseButton::Left, col, row, _)) => {
                    let col = col as usize;
                    let row = row as usize;
                    let pos = (state.line_number + row).saturating_sub(OFFSET);
                    if col >= state.pad_left + 8 {
                        let col = col.saturating_sub(state.pad_left).saturating_sub(10);
                        if let Some(line) = state.book.lines.get(pos) {
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
                                        render(&mut term, &state)?;
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
                                        render(&mut term, &state)?;
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

fn render(term: &mut Terminal<Stdout>, state: &State) -> Result<()> {
    for i in 0..state.screen_height {
        term.act(Action::MoveCursorTo(0, i as u16))?;
        term.batch(Action::ClearTerminal(Clear::CurrentLine))?;
        if state.line_number + i >= OFFSET {
            let pos = (state.line_number + i).saturating_sub(OFFSET);
            if let Some(line) = state.book.lines.get(pos) {
                let mut line = line.to_string();
                let line_number = pos;
                let is_bookmarked = state.config.bookmarks.contains(&line_number);
                let mut line_color = if state.focus_mode {
                    match i {
                        i if i + 1 == OFFSET => "\x1b[38;2;160;160;160m",
                        i if i == OFFSET => "\x1b[38;2;240;240;240m",
                        i if i == OFFSET + 1 => "\x1b[38;2;160;160;160m",
                        _ => "\x1b[38;2;100;100;100m",
                    }
                } else {
                    "\x1b[38;2;240;240;240m"
                };
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
                            slices.push("\x1b[48;2;100;100;100m".to_string());
                            slices.push("\x1b[38;2;240;240;240m".to_string())
                        }
                        Codes::RESET_BACKGROUND => {
                            slices.push("\x1b[49m".to_string());
                            slices.push(line_color.to_string())
                        }
                        _ => slices.push(char.to_string()),
                    }
                }
                line = slices.join("");
                if let Some(definition) = &state.definition {
                    let ((row, _, _), definition) = definition;
                    if row + 1 == pos {
                        line = "".to_string();
                        line_color = "\x1b[38;2;240;240;240m";
                    }
                    if row + 1 < pos && row + 1 + definition.list.len() >= pos {
                        let index = pos.saturating_sub(row + 2);
                        if let Some(item) = definition.list.get(index) {
                            line = format!("\x1b[38;2;160;160;160m  {}. {item}\x1b[0m", index + 1);
                            line_color = "\x1b[38;2;240;240;240m";
                        }
                    }
                    if row + 1 + definition.list.len() + 1 == pos {
                        line = "".to_string();
                        line_color = "\x1b[38;2;240;240;240m";
                    }
                }
                term.flush_batch()?;
                term.write_all(
                    format!(
                        "{: >pad_left$}{}{: >5} {}\x1b[0m {}{line}\x1b[0m",
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
                            "\x1b[38;2;240;240;240m>>>\x1b[0m"
                        } else {
                            "   "
                        },
                        line_color,
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
