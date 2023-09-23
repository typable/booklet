use std::fmt;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

const LICENSE_START: &str = "START OF THE PROJECT GUTENBERG";
const LICENSE_END: &str = "END OF THE PROJECT GUTENBERG";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub bookmarks: Vec<usize>,
    pub markers: Vec<(usize, usize, usize)>,
    pub focus_mode: Option<bool>,
}

#[derive(Debug)]
pub struct Definition {
    pub word: String,
    pub list: Vec<String>,
}

impl Definition {
    pub fn from_json(value: &serde_json::Value) -> Option<Definition> {
        let entry = value.as_array()?.get(0)?;
        let word = entry.get("word")?.as_str()?;
        let mut list = Vec::new();
        let meanings = entry.get("meanings")?.as_array()?;
        for meaning in meanings {
            let definitions = meaning.get("definitions")?.as_array()?;
            for definition in definitions {
                let sentence = definition.get("definition")?.as_str()?;
                list.push(sentence.to_string());
            }
        }
        Some(Definition {
            word: word.to_string(),
            list,
        })
    }
}

impl fmt::Display for Definition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.word)?;
        writeln!(f)?;
        for (i, item) in self.list.iter().enumerate() {
            write!(f, "{i}. {item}")?;
        }
        Ok(())
    }
}

impl Config {
    pub fn from_path(path: &str) -> anyhow::Result<Self> {
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

    pub fn write(&self, path: &str) -> anyhow::Result<()> {
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
    pub fn from_path(path: &str) -> anyhow::Result<Self> {
        let mut content = fs::read_to_string(path)?;
        content = Book::remove_license(&content);
        content = Book::highlight_italic(&content);
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

    fn remove_license(content: &str) -> String {
        if !content.contains(LICENSE_START) || !content.contains(LICENSE_END) {
            return content.to_string();
        }
        let mut is_content = false;
        let mut lines = Vec::new();
        for line in content.lines() {
            if line.contains(LICENSE_END) {
                break;
            }
            if is_content {
                lines.push(line);
            }
            if line.contains(LICENSE_START) {
                is_content = true;
            }
        }
        lines.join("\n")
    }

    fn highlight_italic(content: &str) -> String {
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
    pub pad_left: usize,
    pub update_screen: bool,
    pub selection: Option<(usize, usize, usize)>,
    pub definition: Option<((usize, usize, usize), Definition)>,
    pub message: Option<String>,
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
            definition: None,
            message: None,
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

    pub fn get_text(&self, (pos, start, end): (usize, usize, usize)) -> Option<String> {
        let line = self.book.lines.get(pos)?;
        let text = line.get(start..end)?.to_string();
        Some(text)
    }

    pub fn set_selection(&mut self, selection: (usize, usize, usize)) {
        if !self
            .selection
            .is_some_and(|sel| sel.0 == selection.0 && sel.1 == selection.1 && sel.2 == selection.2)
        {
            self.selection = Some(selection);
            self.update_screen();
        }
    }

    pub fn get_selection(&mut self) -> Option<(usize, usize, usize)> {
        self.selection
    }

    pub fn clear_selection(&mut self) {
        if self.selection.is_some() {
            self.selection = None;
            self.update_screen();
        }
    }

    pub fn clear_definition(&mut self) {
        if self.definition.is_some() {
            self.definition = None;
            self.update_screen();
        }
    }

    pub fn clear_message(&mut self) {
        if self.message.is_some() {
            self.message = None;
            self.update_screen();
        }
    }

    pub fn toggle_focus_mode(&mut self) -> anyhow::Result<()> {
        self.config.focus_mode = Some(!self.config.focus_mode.unwrap_or_default());
        self.config.write(&self.path)?;
        self.show_message("(i) Toggled focus mode");
        self.update_screen();
        Ok(())
    }

    pub fn toggle_bookmark(&mut self, line_number: usize) -> anyhow::Result<()> {
        if self.has_bookmark(line_number) {
            self.remove_bookmark(line_number)?;
        } else {
            self.add_bookmark(line_number)?;
        }
        Ok(())
    }

    pub fn has_bookmark(&mut self, line_number: usize) -> bool {
        self.config
            .bookmarks
            .iter()
            .any(|item| item == &line_number)
    }

    pub fn add_bookmark(&mut self, line_number: usize) -> anyhow::Result<()> {
        if !self
            .config
            .bookmarks
            .iter()
            .any(|item| item == &line_number)
        {
            self.config.bookmarks.push(line_number);
            self.config.bookmarks.sort();
            self.config.write(&self.path)?;
            self.show_message("(i) Added bookmark");
            self.update_screen();
        }
        Ok(())
    }

    pub fn remove_bookmark(&mut self, line_number: usize) -> anyhow::Result<()> {
        if let Some(index) = self
            .config
            .bookmarks
            .iter()
            .position(|item| item == &line_number)
        {
            self.config.bookmarks.remove(index);
            self.config.write(&self.path)?;
            self.show_message("(i) Removed bookmark");
            self.update_screen();
        }
        Ok(())
    }

    pub async fn define_selection(&mut self) -> anyhow::Result<()> {
        let selection = match self.selection {
            Some(selection) => selection,
            None => {
                self.show_message("(i) No selection found");
                return Ok(());
            }
        };
        let text = match self.get_text(selection) {
            Some(text) => text,
            None => {
                self.show_message("(i) No text at specified selection");
                return Ok(());
            }
        };
        let url = format!("https://api.dictionaryapi.dev/api/v2/entries/en/{text}");
        let res = reqwest::get(url).await?;
        let result: serde_json::Value = res.json().await?;
        let definition = match Definition::from_json(&result) {
            Some(definition) => definition,
            None => {
                self.show_message("(i) No definition found");
                return Ok(());
            }
        };
        self.definition = Some((selection, definition));
        self.update_screen();
        Ok(())
    }

    pub fn show_message(&mut self, message: &str) {
        self.message = Some(message.to_string());
        self.update_screen();
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
