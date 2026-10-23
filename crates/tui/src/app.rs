use std::collections::HashSet;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use help::{Block, HelpIndex, Page, SearchHit, TokenKind};
use lib::JEngine;

const BLINK_INTERVAL: Duration = Duration::from_millis(500);
const SEARCH_LIMIT: usize = 100;
const SEARCH_VISIBLE: usize = 10;
const SCROLL_STEP: u16 = 12;

#[derive(Clone, Debug)]
pub(crate) enum HistoryEntry {
    Input(String),
    Output(String),
    Info(String),
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Focus {
    Input,
    Help,
}

#[derive(Clone, Debug)]
pub(crate) struct HelpView {
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) url: String,
    pub(crate) blocks: Vec<Block>,
    pub(crate) examples: Vec<String>,
}

impl HelpView {
    fn from_page(page: &Page) -> Self {
        let mut subtitle = page.kind.label().to_string();
        if !page.symbols.is_empty() {
            subtitle.push_str(" · ");
            subtitle.push_str(&page.symbols.join(" "));
        }
        if !page.monad.is_empty() {
            subtitle.push_str(" · ");
            subtitle.push_str(&page.monad);
        }
        if !page.dyad.is_empty() {
            subtitle.push_str(" · ");
            subtitle.push_str(&page.dyad);
        }
        Self {
            title: page.title.clone(),
            subtitle,
            url: page.url.clone(),
            blocks: page.blocks.clone(),
            examples: page.examples.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SearchState {
    pub(crate) query: String,
    pub(crate) hits: Vec<SearchHit>,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
}

pub(crate) struct App {
    pub(crate) input_buffer: String,
    pub(crate) cursor: usize,
    pub(crate) history: Vec<HistoryEntry>,
    pub(crate) history_offset_from_bottom: u16,
    pub(crate) focus: Focus,
    pub(crate) help: Option<HelpView>,
    pub(crate) help_scroll: u16,
    pub(crate) search: Option<SearchState>,
    pub(crate) should_quit: bool,
    pub(crate) cursor_visible: bool,

    command_history: Vec<String>,
    history_index: Option<usize>,
    help_back: Vec<HelpView>,
    help_example_index: usize,
    help_index: Option<&'static HelpIndex>,
    help_error: Option<String>,
    notice: Option<String>,
    j_engine: JEngine,
    last_blink: Instant,
}

impl App {
    pub(crate) fn new() -> Self {
        let (help_index, help_error) = match HelpIndex::bundled() {
            Ok(index) => (Some(index), None),
            Err(err) => (None, Some(format!("Help data unavailable: {err}"))),
        };

        Self {
            input_buffer: String::new(),
            cursor: 0,
            history: vec![
                HistoryEntry::Info("Welcome to the J Language TUI Interface!".to_string()),
                HistoryEntry::Info("Type a J expression and press Enter.".to_string()),
                HistoryEntry::Info(
                    "F1 opens context help · Ctrl+P searches the dictionary · /help <topic>"
                        .to_string(),
                ),
            ],
            history_offset_from_bottom: 0,
            focus: Focus::Input,
            help: None,
            help_scroll: 0,
            search: None,
            should_quit: false,
            cursor_visible: true,
            command_history: Vec::new(),
            history_index: None,
            help_back: Vec::new(),
            help_example_index: 0,
            help_index,
            help_error,
            notice: None,
            j_engine: JEngine::new(),
            last_blink: Instant::now(),
        }
    }

    pub(crate) fn help_view(&self) -> Option<&HelpView> {
        self.help.as_ref()
    }

    pub(crate) fn help_example_index(&self) -> usize {
        self.help_example_index
    }

    pub(crate) fn tick_blink(&mut self) {
        if self.focus != Focus::Input || self.search.is_some() {
            self.cursor_visible = false;
            self.last_blink = Instant::now();
            return;
        }
        if self.last_blink.elapsed() >= BLINK_INTERVAL {
            self.cursor_visible = !self.cursor_visible;
            self.last_blink = Instant::now();
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        self.notice = None;
        self.cursor_visible = true;
        self.last_blink = Instant::now();

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            self.should_quit = true;
            return;
        }

        if self.search.is_some() {
            self.handle_search_key(key);
            return;
        }

        if key.code == KeyCode::F(1) {
            self.toggle_help();
            return;
        }

        if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.open_search(None);
            return;
        }

        if key.code == KeyCode::Tab && self.help.is_some() {
            self.focus = match self.focus {
                Focus::Input => Focus::Help,
                Focus::Help => Focus::Input,
            };
            return;
        }

        match self.focus {
            Focus::Input => self.handle_input_key(key),
            Focus::Help => self.handle_help_key(key),
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if self.help.is_some() {
                    self.close_help();
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Up => self.history_previous(),
            KeyCode::Down => self.history_next(),
            KeyCode::PageUp => self.history_page_up(),
            KeyCode::PageDown => self.history_page_down(),
            KeyCode::Left => self.move_cursor_left(),
            KeyCode::Right => self.move_cursor_right(),
            KeyCode::Home => self.move_cursor_home(),
            KeyCode::End => self.move_cursor_end(),
            KeyCode::Delete => self.delete_forward(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Enter => self.submit(),
            KeyCode::F(2) => {
                let input = self.input_buffer.clone();
                self.open_explain(&input);
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'a' | 'A' => self.move_cursor_home(),
                        'e' | 'E' => self.move_cursor_end(),
                        'k' | 'K' => self.kill_to_end(),
                        'u' | 'U' => self.kill_to_beginning(),
                        'r' | 'R' => {
                            let pattern = self.input_buffer.clone();
                            self.reverse_search(&pattern);
                        }
                        _ => {}
                    }
                } else {
                    self.insert_char(c);
                }
            }
            _ => {}
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.close_help(),
            KeyCode::Tab => self.focus = Focus::Input,
            KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                self.help_scroll = self.help_scroll.saturating_add(1);
            }
            KeyCode::PageUp => {
                self.help_scroll = self.help_scroll.saturating_sub(SCROLL_STEP);
            }
            KeyCode::PageDown => {
                self.help_scroll = self.help_scroll.saturating_add(SCROLL_STEP);
            }
            KeyCode::Home => self.help_scroll = 0,
            KeyCode::End => self.help_scroll = u16::MAX,
            KeyCode::Backspace => self.help_back_page(),
            KeyCode::Char('b') | KeyCode::Char('B') => self.help_back_page(),
            KeyCode::Char('e') | KeyCode::Char('E') => self.help_cycle_example(),
            KeyCode::Char('y') | KeyCode::Char('Y') => self.help_insert_example(),
            KeyCode::Char('o') | KeyCode::Char('O') => self.help_open_source(),
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search = None;
            }
            KeyCode::Enter => {
                let page_id = self
                    .search
                    .as_ref()
                    .and_then(|search| search.hits.get(search.selected))
                    .map(|hit| hit.page_id.clone());
                if let Some(page_id) = page_id {
                    self.search = None;
                    self.open_page(&page_id, true);
                }
            }
            KeyCode::Up => self.move_search_selection(-1),
            KeyCode::Down => self.move_search_selection(1),
            KeyCode::PageUp => self.move_search_selection(-(SEARCH_VISIBLE as isize)),
            KeyCode::PageDown => self.move_search_selection(SEARCH_VISIBLE as isize),
            KeyCode::Home => self.set_search_selection(0),
            KeyCode::End => self.set_search_selection(usize::MAX),
            KeyCode::Backspace => {
                if let Some(search) = self.search.as_mut() {
                    search.query.pop();
                }
                self.refresh_search();
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if matches!(c, 'u' | 'U') {
                        if let Some(search) = self.search.as_mut() {
                            search.query.clear();
                        }
                        self.refresh_search();
                    }
                } else if let Some(search) = self.search.as_mut() {
                    search.query.push(c);
                    self.refresh_search();
                }
            }
            _ => {}
        }
    }

    fn submit(&mut self) {
        if self.input_buffer.trim().is_empty() {
            self.input_buffer.clear();
            self.cursor = 0;
            self.history_index = None;
            return;
        }

        let command = self.input_buffer.clone();
        self.push_history(HistoryEntry::Input(command.clone()));
        if self.command_history.last().map(|item| item.as_str()) != Some(command.as_str()) {
            self.command_history.push(command.clone());
        }

        self.input_buffer.clear();
        self.cursor = 0;
        self.history_index = None;
        self.history_offset_from_bottom = 0;

        if command_word(&command).is_some() {
            self.handle_command(&command);
        } else {
            self.run_j(&command);
        }
    }

    fn handle_command(&mut self, line: &str) {
        let trimmed = line.trim_start();
        let Some(word) = command_word(trimmed) else {
            return;
        };
        let rest = trimmed.get(word.len()..).unwrap_or("").trim();
        match word.to_ascii_lowercase().as_str() {
            "/help" => {
                if rest.is_empty() {
                    self.open_search(None);
                } else {
                    self.open_topic(rest);
                }
            }
            "/explain" => {
                if rest.is_empty() {
                    self.notice = Some("Usage: /explain <sentence> or press F2 on a line".to_string());
                } else {
                    self.open_explain(rest);
                }
            }
            "/clear" => {
                self.history.clear();
                self.history_offset_from_bottom = 0;
                self.push_history(HistoryEntry::Info("History cleared.".to_string()));
            }
            "/quit" | "/exit" => self.should_quit = true,
            _ => {
                self.push_history(HistoryEntry::Info(format!(
                    "Unknown command {word}. Try /help <topic>."
                )));
            }
        }
    }

    fn run_j(&mut self, command: &str) {
        match self.j_engine.eval(command) {
            Ok(output) => {
                if !output.trim().is_empty() {
                    for line in output.lines() {
                        self.push_history(HistoryEntry::Output(line.to_string()));
                    }
                }
            }
            Err(err) => {
                self.push_history(HistoryEntry::Error(format!("J engine error: {err}")));
            }
        }
    }

    fn push_history(&mut self, entry: HistoryEntry) {
        self.history.push(entry);
        self.history_offset_from_bottom = 0;
    }

    fn toggle_help(&mut self) {
        if self.help.is_some() {
            self.close_help();
        } else {
            self.open_context_help();
        }
    }

    fn open_context_help(&mut self) {
        let Some(index) = self.help_index else {
            if let Some(err) = &self.help_error {
                self.notice = Some(err.clone());
            }
            return;
        };

        if let Some(token) = index.token_at(&self.input_buffer, self.cursor) {
            let page = match token.kind {
                TokenKind::Symbol => index.lookup_symbol(&token.text),
                TokenKind::Name => index.lookup_name(&token.text),
                TokenKind::Other => None,
            };
            if let Some(page) = page {
                let page_id = page.id.clone();
                self.open_page(&page_id, true);
                return;
            }

            let hits = index.search(&token.text, 12);
            if hits.len() == 1 {
                let page_id = hits[0].page_id.clone();
                self.open_page(&page_id, true);
                return;
            }
            if !hits.is_empty() {
                self.open_search(Some(token.text));
                return;
            }
            if token.kind != TokenKind::Other {
                self.notice = Some(format!(
                    "No bundled help found for '{}'. Try Ctrl+P for full-text search.",
                    token.text
                ));
                return;
            }
        }

        self.open_search(None);
    }

    fn open_topic(&mut self, query: &str) {
        let Some(index) = self.help_index else {
            if let Some(err) = &self.help_error {
                self.notice = Some(err.clone());
            }
            return;
        };

        if let Some(page) = index.page(query) {
            let page_id = page.id.clone();
            self.open_page(&page_id, true);
            return;
        }
        if let Some(page) = index.lookup_symbol(query) {
            let page_id = page.id.clone();
            self.open_page(&page_id, true);
            return;
        }
        if let Some(page) = index.lookup_name(query) {
            let page_id = page.id.clone();
            self.open_page(&page_id, true);
            return;
        }

        let hits = index.search(query, 20);
        match hits.len() {
            0 => {
                self.notice = Some(format!(
                    "No help found for '{query}'. Ctrl+P searches all bundled pages."
                ));
            }
            1 => {
                let page_id = hits[0].page_id.clone();
                self.open_page(&page_id, true);
            }
            _ => self.open_search(Some(query.to_string())),
        }
    }

    fn open_page(&mut self, page_id: &str, push_back: bool) {
        let Some(index) = self.help_index else {
            if let Some(err) = &self.help_error {
                self.notice = Some(err.clone());
            }
            return;
        };
        let Some(page) = index.page(page_id) else {
            self.notice = Some(format!("Help page '{page_id}' was not found."));
            return;
        };
        let view = HelpView::from_page(page);
        self.show_view(view, push_back);
    }

    fn show_view(&mut self, view: HelpView, push_back: bool) {
        if push_back
            && let Some(current) = self.help.take()
        {
            self.help_back.push(current);
        }
        self.help = Some(view);
        self.help_scroll = 0;
        self.help_example_index = 0;
        self.focus = Focus::Help;
        self.search = None;
    }

    fn close_help(&mut self) {
        self.help = None;
        self.help_back.clear();
        self.help_scroll = 0;
        self.focus = Focus::Input;
    }

    fn help_back_page(&mut self) {
        if let Some(previous) = self.help_back.pop() {
            self.help = Some(previous);
            self.help_scroll = 0;
            self.help_example_index = 0;
        } else {
            self.close_help();
        }
    }

    fn help_cycle_example(&mut self) {
        let Some(view) = self.help.as_ref() else {
            return;
        };
        if view.examples.is_empty() {
            self.notice = Some("This page has no runnable examples.".to_string());
            return;
        }
        let count = view.examples.len();
        self.help_example_index = (self.help_example_index + 1) % count;
    }

    fn help_insert_example(&mut self) {
        let example = self.help.as_ref().and_then(|view| {
            view.examples
                .get(self.help_example_index)
                .cloned()
        });
        let Some(example) = example else {
            self.notice = Some("This page has no runnable examples.".to_string());
            return;
        };
        self.input_buffer = example;
        self.cursor = self.input_buffer.len();
        self.focus = Focus::Input;
        self.history_index = None;
        self.notice = Some("Example inserted. Press Enter to run it.".to_string());
    }

    fn help_open_source(&mut self) {
        let Some(view) = self.help.as_ref() else {
            return;
        };
        if view.url.is_empty() {
            self.notice = Some("This page has no source URL.".to_string());
            return;
        }
        let url = view.url.clone();
        match open_url(&url) {
            Ok(()) => self.notice = Some(format!("Opened {url}")),
            Err(err) => self.notice = Some(format!("Could not open browser: {err}")),
        }
    }

    fn open_explain(&mut self, sentence: &str) {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            self.notice = Some("Type a sentence to explain, or press F2 on a line.".to_string());
            return;
        }
        let Some(index) = self.help_index else {
            if let Some(err) = &self.help_error {
                self.notice = Some(err.clone());
            }
            return;
        };

        let mut blocks = vec![
            Block::Heading {
                level: 2,
                spans: vec![help::Inline {
                    text: "Sentence".to_string(),
                    code: false,
                    bold: true,
                    italic: false,
                }],
            },
            Block::Code {
                text: format!("   {sentence}"),
            },
        ];

        let mut seen = HashSet::new();
        let mut found = 0usize;
        for token in index.scan_tokens(sentence) {
            if token.kind == TokenKind::Other || !seen.insert(token.text.clone()) {
                continue;
            }

            let page = match token.kind {
                TokenKind::Symbol => index.lookup_symbol(&token.text),
                TokenKind::Name => index.lookup_name(&token.text),
                TokenKind::Other => None,
            };

            if let Some(page) = page {
                found += 1;
                blocks.push(Block::Heading {
                    level: 3,
                    spans: vec![help::Inline {
                        text: format!("{}  ·  {}", token.text, page.title),
                        code: token.kind == TokenKind::Symbol,
                        bold: true,
                        italic: false,
                    }],
                });
                blocks.push(Block::Para {
                    spans: vec![help::Inline {
                        text: page.summary(),
                        code: false,
                        bold: false,
                        italic: false,
                    }],
                });
            } else if token.kind == TokenKind::Name {
                blocks.push(Block::Heading {
                    level: 3,
                    spans: vec![help::Inline {
                        text: token.text,
                        code: false,
                        bold: true,
                        italic: false,
                    }],
                });
                blocks.push(Block::Para {
                    spans: vec![help::Inline {
                        text: "Not found in the bundled vocabulary; this may be a user-defined name."
                            .to_string(),
                        code: false,
                        bold: false,
                        italic: false,
                    }],
                });
            }
        }

        if found == 0 {
            blocks.push(Block::Para {
                spans: vec![help::Inline {
                    text: "No symbols or documented names were recognized in that sentence."
                        .to_string(),
                    code: false,
                    bold: false,
                    italic: false,
                }],
            });
        }

        let view = HelpView {
            title: "Explain Sentence".to_string(),
            subtitle: sentence.to_string(),
            url: String::new(),
            blocks,
            examples: Vec::new(),
        };
        self.show_view(view, true);
    }

    fn open_search(&mut self, prefill: Option<String>) {
        if self.help_index.is_none() {
            if let Some(err) = &self.help_error {
                self.notice = Some(err.clone());
            }
            return;
        }
        self.search = Some(SearchState {
            query: prefill.unwrap_or_default(),
            hits: Vec::new(),
            selected: 0,
            scroll: 0,
        });
        self.refresh_search();
    }

    fn refresh_search(&mut self) {
        let Some(index) = self.help_index else {
            return;
        };
        let Some(search) = self.search.as_mut() else {
            return;
        };
        search.hits = index.search(&search.query, SEARCH_LIMIT);
        search.selected = 0;
        search.scroll = 0;
    }

    fn move_search_selection(&mut self, delta: isize) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        if search.hits.is_empty() {
            search.selected = 0;
            search.scroll = 0;
            return;
        }
        let len = search.hits.len();
        let selected = if delta < 0 {
            search.selected.saturating_sub(delta.unsigned_abs())
        } else {
            search
                .selected
                .saturating_add(delta as usize)
                .min(len.saturating_sub(1))
        };
        search.selected = selected;
        if selected < search.scroll {
            search.scroll = selected;
        } else if selected >= search.scroll.saturating_add(SEARCH_VISIBLE) {
            search.scroll = selected.saturating_add(1).saturating_sub(SEARCH_VISIBLE);
        }
    }

    fn set_search_selection(&mut self, selected: usize) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        let selected = selected.min(search.hits.len().saturating_sub(1));
        search.selected = selected;
        if selected < search.scroll {
            search.scroll = selected;
        } else if selected >= search.scroll.saturating_add(SEARCH_VISIBLE) {
            search.scroll = selected.saturating_add(1).saturating_sub(SEARCH_VISIBLE);
        }
    }

    pub(crate) fn status_line(&self) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }

        if self.search.is_some() {
            return "Search: type to filter · ↑↓ select · PgUp/PgDn · Enter open · Esc close"
                .to_string();
        }

        if let Some(view) = &self.help {
            if self.focus == Focus::Help {
                let examples = if view.examples.is_empty() {
                    "no examples".to_string()
                } else {
                    let index = self
                        .help_example_index
                        .min(view.examples.len().saturating_sub(1));
                    format!("example {}/{}", index + 1, view.examples.len())
                };
                return format!(
                    "↑↓ scroll · PgUp/PgDn page · b back · e cycle ({examples}) · y insert · o source · Tab input · Esc close"
                );
            }
            return "F1 close help · Ctrl+P search · Tab focus help · /help <topic>".to_string();
        }

        let mut parts = Vec::new();
        if let Some((label, detail)) = self.token_hint() {
            parts.push(format!("{label}: {detail}"));
        }
        parts.push("F1 help".to_string());
        parts.push("Ctrl+P search".to_string());
        parts.push("PgUp/PgDn history".to_string());
        parts.push("Esc quit".to_string());
        parts.join(" · ")
    }

    fn token_hint(&self) -> Option<(String, String)> {
        let index = self.help_index?;
        let token = index.token_at(&self.input_buffer, self.cursor)?;
        match token.kind {
            TokenKind::Symbol => {
                let page = index.lookup_symbol(&token.text)?;
                let mut detail = page.title.clone();
                if !page.monad.is_empty() || !page.dyad.is_empty() {
                    let mut names = Vec::new();
                    if !page.monad.is_empty() {
                        names.push(page.monad.as_str());
                    }
                    if !page.dyad.is_empty() {
                        names.push(page.dyad.as_str());
                    }
                    detail = format!("{} · {}", page.title, names.join(" / "));
                }
                Some((token.text, detail))
            }
            TokenKind::Name => {
                let page = index.lookup_name(&token.text)?;
                Some((token.text, page.title.clone()))
            }
            TokenKind::Other => None,
        }
    }

    fn move_cursor_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let width = self.input_buffer[..self.cursor]
            .chars()
            .next_back()
            .map(|ch| ch.len_utf8())
            .unwrap_or(1);
        self.cursor = self.cursor.saturating_sub(width);
    }

    fn move_cursor_right(&mut self) {
        if self.cursor >= self.input_buffer.len() {
            return;
        }
        let width = self.input_buffer[self.cursor..]
            .chars()
            .next()
            .map(|ch| ch.len_utf8())
            .unwrap_or(1);
        self.cursor = self.cursor.saturating_add(width);
    }

    fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    fn move_cursor_end(&mut self) {
        self.cursor = self.input_buffer.len();
    }

    fn insert_char(&mut self, ch: char) {
        self.input_buffer.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.history_index = None;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.input_buffer[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.input_buffer.replace_range(start..self.cursor, "");
        self.cursor = start;
        self.history_index = None;
    }

    fn delete_forward(&mut self) {
        if self.cursor >= self.input_buffer.len() {
            return;
        }
        let end = self.input_buffer[self.cursor..]
            .chars()
            .next()
            .map(|ch| self.cursor + ch.len_utf8())
            .unwrap_or(self.cursor);
        self.input_buffer.replace_range(self.cursor..end, "");
        self.history_index = None;
    }

    fn kill_to_beginning(&mut self) {
        self.input_buffer.replace_range(..self.cursor, "");
        self.cursor = 0;
        self.history_index = None;
    }

    fn kill_to_end(&mut self) {
        self.input_buffer.truncate(self.cursor);
        self.history_index = None;
    }

    fn history_previous(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        let index = match self.history_index {
            None => self.command_history.len().saturating_sub(1),
            Some(0) => 0,
            Some(index) => index.saturating_sub(1),
        };
        self.history_index = Some(index);
        if let Some(command) = self.command_history.get(index) {
            self.input_buffer = command.clone();
            self.cursor = self.input_buffer.len();
        }
    }

    fn history_next(&mut self) {
        match self.history_index {
            Some(index) if index.saturating_add(1) < self.command_history.len() => {
                let next = index.saturating_add(1);
                self.history_index = Some(next);
                if let Some(command) = self.command_history.get(next) {
                    self.input_buffer = command.clone();
                    self.cursor = self.input_buffer.len();
                }
            }
            Some(_) => {
                self.history_index = None;
                self.input_buffer.clear();
                self.cursor = 0;
            }
            None => {}
        }
    }

    fn history_page_up(&mut self) {
        self.history_offset_from_bottom = self.history_offset_from_bottom.saturating_add(8);
    }

    fn history_page_down(&mut self) {
        self.history_offset_from_bottom = self.history_offset_from_bottom.saturating_sub(8);
    }

    fn reverse_search(&mut self, pattern: &str) {
        if pattern.is_empty() {
            return;
        }
        for (index, command) in self.command_history.iter().enumerate().rev() {
            if command.contains(pattern) {
                self.history_index = Some(index);
                self.input_buffer = command.clone();
                self.cursor = self.input_buffer.len();
                return;
            }
        }
    }
}

fn command_word(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    if chars.next()? != '/' {
        return None;
    }
    if !chars.next()?.is_ascii_alphabetic() {
        return None;
    }
    let end = trimmed
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(trimmed.len());
    trimmed.get(..end)
}

fn open_url(url: &str) -> Result<(), String> {
    let mut command = if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_word_recognizes_help_namespace() {
        assert_eq!(command_word("/help +"), Some("/help"));
        assert_eq!(command_word("  /explain +/ 1 2"), Some("/explain"));
        assert_eq!(command_word("/help"), Some("/help"));
        assert_eq!(command_word("/"), None);
        assert_eq!(command_word("/1"), None);
        assert_eq!(command_word("+/ 1 2"), None);
        assert_eq!(command_word(")help"), None);
    }
}
