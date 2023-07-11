use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub bookmarks: Vec<usize>,
    pub markers: Vec<(usize, usize, usize)>,
}

impl Config {
    pub fn from_path(path: &str) -> Result<Self> {
        let mut path_buf = PathBuf::from(path);
        if let Some(filename) = path_buf.file_name() {
            let filename = filename.to_os_string().into_string().unwrap();
            path_buf.pop();
            path_buf.push(format!(".booklet_{filename}"));
            if !path_buf.exists() {
                return Ok(Config::default());
            }
            let content = fs::read_to_string(path_buf)?;
            let mut config = toml::from_str::<Self>(&content)?;
            config.bookmarks.sort();
            return Ok(config);
        }
        Ok(Config::default())
    }

    pub fn write(&self, path: &str) -> Result<()> {
        let mut path_buf = PathBuf::from(path);
        if let Some(filename) = path_buf.file_name() {
            let filename = filename.to_os_string().into_string().unwrap();
            path_buf.pop();
            path_buf.push(format!(".booklet_{filename}"));
            let content = toml::to_string(self)?;
            fs::write(path_buf, content)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Book {
    pub lines: Vec<String>,
    pub line_count: usize,
    pub line_width: usize,
}

impl Book {
    pub fn from_path(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let content = Book::apply_modifiers(&content);
        let lines = content
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<String>>();
        let line_count = lines.len();
        Ok(Self {
            lines,
            line_count,
            line_width: 80,
        })
    }

    fn apply_modifiers(content: &str) -> String {
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
        chars.iter().collect()
    }
}

#[derive(Debug)]
pub struct State {
    pub path: String,
    pub config: Config,
    pub book: Book,
    pub screen_width: usize,
    pub screen_height: usize,
    pub line_number: usize,
    pub selection: Option<(usize, usize, usize)>,
    pub pad_left: usize,
    pub update_screen: bool,
}

impl State {
    pub fn new(path: &str, config: Config, book: Book) -> Self {
        Self {
            path: path.to_string(),
            config,
            book,
            screen_width: 0,
            screen_height: 0,
            line_number: 0,
            selection: None,
            pad_left: 0,
            update_screen: false,
        }
    }

    pub fn update_screen(&mut self) {
        self.update_screen = true;
    }

    pub fn resize_screen(&mut self, screen_width: usize, screen_height: usize) {
        self.screen_width = screen_width;
        self.screen_height = screen_height;
        self.pad_left = (self.screen_width / 2).saturating_sub(self.book.line_width / 2);
        self.update_screen();
    }

    pub fn move_up(&mut self) {
        if self.line_number > 0 {
            self.line_number -= 1;
            self.update_screen();
        }
    }

    pub fn move_down(&mut self) {
        if self.line_number < self.book.line_count.saturating_sub(1) {
            self.line_number += 1;
            self.update_screen();
        }
    }

    pub fn goto_top(&mut self) {
        if self.line_number > 0 {
            self.line_number = 0;
            self.update_screen();
        }
    }

    pub fn goto_bottom(&mut self) {
        if self.line_number != self.book.line_count.saturating_sub(1) {
            self.line_number = self.book.line_count.saturating_sub(1);
            self.update_screen();
        }
    }

    pub fn goto_next_bookmark(&mut self) {
        for bookmark in &self.config.bookmarks {
            if bookmark > &self.line_number {
                self.line_number = *bookmark;
                self.update_screen();
                break;
            }
        }
    }

    pub fn goto_prev_bookmark(&mut self) {
        for bookmark in self.config.bookmarks.iter().rev() {
            if bookmark < &self.line_number {
                self.line_number = *bookmark;
                self.update_screen();
                break;
            }
        }
    }

    pub fn clear_selection(&mut self) {
        if self.selection.is_some() {
            self.selection = None;
            self.update_screen();
        }
    }

    pub fn has_bookmark(&mut self, line_number: usize) -> bool {
        self.config
            .bookmarks
            .iter()
            .any(|item| item == &line_number)
    }

    pub fn add_bookmark(&mut self, line_number: usize) -> Result<()> {
        if !self
            .config
            .bookmarks
            .iter()
            .any(|item| item == &line_number)
        {
            self.config.bookmarks.push(line_number);
            self.config.write(&self.path)?;
            self.update_screen();
        }
        Ok(())
    }

    pub fn remove_bookmark(&mut self, line_number: usize) -> Result<()> {
        if let Some(index) = self
            .config
            .bookmarks
            .iter()
            .position(|item| item == &line_number)
        {
            self.config.bookmarks.remove(index);
            self.config.write(&self.path)?;
            self.update_screen();
        }
        Ok(())
    }
}

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
