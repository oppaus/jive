//! Offline J language documentation and lookup helpers.
//!
//! The documentation is generated ahead of time by `tools/gen_help.py` and
//! embedded into the binary with `include_str!`, so the TUI never needs a
//! network connection or a separate help installation.

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use serde::Deserialize;

const BUNDLED_JSON: &str = include_str!("../data/help.json");

/// Metadata about the vendored documentation source.
#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    pub name: String,
    pub commit: String,
    pub url: String,
    pub license: String,
    pub generated: String,
}

/// The broad category a help page belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    Primitive,
    Concept,
    Phrase,
    Idiom,
}

impl PageKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Primitive => "primitive",
            Self::Concept => "concept",
            Self::Phrase => "phrase",
            Self::Idiom => "idiom",
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::Primitive => "P",
            Self::Concept => "C",
            Self::Phrase => "Ph",
            Self::Idiom => "I",
        }
    }
}

/// A styled piece of inline text.
#[derive(Debug, Clone, Deserialize)]
pub struct Inline {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub code: bool,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

/// A block in a help page's rendered body.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Block {
    Heading {
        #[serde(default)]
        level: u8,
        #[serde(default)]
        spans: Vec<Inline>,
    },
    Para {
        #[serde(default)]
        spans: Vec<Inline>,
    },
    Code {
        #[serde(default)]
        text: String,
    },
    Table {
        #[serde(default)]
        rows: Vec<Vec<String>>,
    },
    List {
        #[serde(default)]
        items: Vec<Vec<Inline>>,
    },
}

/// One documentation page.
#[derive(Debug, Clone, Deserialize)]
pub struct Page {
    pub id: String,
    pub kind: PageKind,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub monad: String,
    #[serde(default)]
    pub dyad: String,
    #[serde(default)]
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub examples: Vec<String>,
}

/// One entry in the vocabulary index.
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolEntry {
    #[serde(default)]
    pub symbol: String,
    pub page: String,
    #[serde(default)]
    pub monad: String,
    #[serde(default)]
    pub dyad: String,
    #[serde(default)]
    pub title: String,
}

/// The complete generated dataset.
#[derive(Debug, Clone, Deserialize)]
pub struct HelpData {
    pub source: Source,
    #[serde(default)]
    pub pages: Vec<Page>,
    #[serde(default)]
    pub symbols: Vec<SymbolEntry>,
}

impl Page {
    /// A plain-text representation used for searching and previews.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.title);
        if !self.symbols.is_empty() {
            out.push(' ');
            out.push_str(&self.symbols.join(" "));
        }
        if !self.monad.is_empty() {
            out.push(' ');
            out.push_str(&self.monad);
        }
        if !self.dyad.is_empty() {
            out.push(' ');
            out.push_str(&self.dyad);
        }
        for block in &self.blocks {
            out.push('\n');
            out.push_str(&block_plain_text(block));
        }
        for example in &self.examples {
            out.push('\n');
            out.push_str(example);
        }
        out
    }

    /// A short one-line preview suitable for a search result list.
    pub fn summary(&self) -> String {
        let mut text = String::new();
        for block in &self.blocks {
            match block {
                Block::Para { spans } => {
                    text = spans_text(spans);
                    if !text.trim().is_empty() {
                        break;
                    }
                }
                Block::Code { text: code } => {
                    if !text.trim().is_empty() {
                        break;
                    }
                    text = code.lines().next().unwrap_or("").to_string();
                }
                Block::Table { rows } => {
                    if let Some(row) = rows.first() {
                        text = row.join(" | ");
                        break;
                    }
                }
                Block::List { items } => {
                    if let Some(item) = items.first() {
                        text = spans_text(item);
                        break;
                    }
                }
                Block::Heading { .. } => {}
            }
        }
        let text = collapse_whitespace(&text);
        if text.is_empty() {
            collapse_whitespace(&self.title)
        } else {
            truncate_chars(&text, 160)
        }
    }
}

fn block_plain_text(block: &Block) -> String {
    match block {
        Block::Heading { spans, .. } | Block::Para { spans } => spans_text(spans),
        Block::Code { text } => text.clone(),
        Block::Table { rows } => rows
            .iter()
            .map(|row| row.join(" | "))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::List { items } => items
            .iter()
            .map(|item| spans_text(item))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Concatenate the text of inline spans.
pub fn spans_text(spans: &[Inline]) -> String {
    let mut out = String::new();
    for span in spans {
        out.push_str(&span.text);
    }
    out
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out = String::new();
    for ch in value.chars().take(max) {
        out.push(ch);
    }
    out.push('…');
    out
}

/// Error returned when the embedded help data cannot be loaded.
#[derive(Debug, Clone)]
pub struct HelpError(String);

impl HelpError {
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HelpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HelpError {}

/// A result returned by [`HelpIndex::search`].
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub page_id: String,
    pub title: String,
    pub kind: PageKind,
    pub score: i32,
    pub snippet: String,
}

/// The kind of token found in a J sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Symbol,
    Name,
    Other,
}

/// A token and its byte range in the source sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

/// The loaded documentation plus lookup indexes.
pub struct HelpIndex {
    data: HelpData,
    page_map: HashMap<String, usize>,
    symbol_map: HashMap<String, usize>,
    name_map: HashMap<String, usize>,
    search_text: Vec<String>,
    summaries: Vec<String>,
    page_names: Vec<Vec<String>>,
    symbols_by_length: Vec<String>,
}

impl HelpIndex {
    /// Parse the bundled JSON. This is public so tests and tooling can load a
    /// different generated dataset if needed.
    pub fn from_json(json: &str) -> Result<Self, HelpError> {
        let data: HelpData =
            serde_json::from_str(json).map_err(|err| HelpError(format!("invalid help data: {err}")))?;

        let mut page_map = HashMap::new();
        let mut search_text = Vec::with_capacity(data.pages.len());
        let mut summaries = Vec::with_capacity(data.pages.len());
        let mut page_names = Vec::with_capacity(data.pages.len());

        for (index, page) in data.pages.iter().enumerate() {
            page_map.entry(page.id.clone()).or_insert(index);
            let plain = page.plain_text();
            search_text.push(plain.to_lowercase());
            summaries.push(page.summary());
            page_names.push(page_name_candidates(page));
        }

        let mut symbol_map = HashMap::new();
        for entry in &data.symbols {
            if entry.symbol.is_empty() {
                continue;
            }
            if let Some(&index) = page_map.get(&entry.page) {
                symbol_map.entry(entry.symbol.clone()).or_insert(index);
            }
        }
        for (index, page) in data.pages.iter().enumerate() {
            for symbol in &page.symbols {
                if !symbol.is_empty() {
                    symbol_map.entry(symbol.clone()).or_insert(index);
                }
            }
        }

        let mut name_map = HashMap::new();
        for (index, names) in page_names.iter().enumerate() {
            for name in names {
                if name.chars().count() >= 2 {
                    name_map.entry(name.clone()).or_insert(index);
                }
            }
        }

        let mut symbols_by_length: Vec<String> = symbol_map
            .keys()
            .filter(|symbol| !symbol.is_empty() && !symbol.chars().any(char::is_whitespace))
            .cloned()
            .collect();
        symbols_by_length.sort_by(|a, b| {
            b.chars()
                .count()
                .cmp(&a.chars().count())
                .then_with(|| b.len().cmp(&a.len()))
                .then_with(|| a.cmp(b))
        });

        Ok(Self {
            data,
            page_map,
            symbol_map,
            name_map,
            search_text,
            summaries,
            page_names,
            symbols_by_length,
        })
    }

    /// Load the documentation embedded in the binary. The result is cached.
    pub fn bundled() -> Result<&'static HelpIndex, HelpError> {
        static INDEX: OnceLock<Result<HelpIndex, HelpError>> = OnceLock::new();
        INDEX
            .get_or_init(|| Self::from_json(BUNDLED_JSON))
            .as_ref()
            .map_err(Clone::clone)
    }

    pub fn source(&self) -> &Source {
        &self.data.source
    }

    pub fn pages(&self) -> &[Page] {
        &self.data.pages
    }

    pub fn symbols(&self) -> &[SymbolEntry] {
        &self.data.symbols
    }

    pub fn page(&self, id: &str) -> Option<&Page> {
        self.page_map
            .get(id)
            .and_then(|&index| self.data.pages.get(index))
    }

    pub fn page_count(&self) -> usize {
        self.data.pages.len()
    }

    /// Look up a primitive by its written symbol, e.g. `+`, `=:`, or `&.:`.
    pub fn lookup_symbol(&self, symbol: &str) -> Option<&Page> {
        let symbol = symbol.trim();
        if symbol.is_empty() {
            return None;
        }
        self.symbol_map
            .get(symbol)
            .and_then(|&index| self.data.pages.get(index))
    }

    /// Look up a page by an exact documented monad/dyad name or title.
    pub fn lookup_name(&self, name: &str) -> Option<&Page> {
        let key = collapse_whitespace(name).to_lowercase();
        if key.is_empty() {
            return None;
        }
        self.name_map
            .get(&key)
            .and_then(|&index| self.data.pages.get(index))
    }

    /// Fuzzy-ish full-text search. Every whitespace-separated term must occur
    /// somewhere in the page; exact title, symbol, and name matches rank best.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let query = collapse_whitespace(query).to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let terms: Vec<&str> = query.split_whitespace().collect();
        if terms.is_empty() {
            return Vec::new();
        }

        let limit = limit.max(1);
        let mut hits = Vec::new();
        for (index, page) in self.data.pages.iter().enumerate() {
            let haystack = &self.search_text[index];
            if !terms.iter().all(|term| haystack.contains(term)) {
                continue;
            }

            let title = page.title.to_lowercase();
            let id = page.id.to_lowercase();
            let mut score = 0i32;

            if id == query {
                score += 2200;
            }
            if title == query {
                score += 1400;
            } else if title.starts_with(&query) {
                score += 850;
            } else if title.contains(&query) {
                score += 520;
            }

            for symbol in &page.symbols {
                let symbol = symbol.to_lowercase();
                if symbol == query {
                    score += 1700;
                } else if symbol.starts_with(&query) {
                    score += 1200;
                } else if symbol.contains(&query) {
                    score += 700;
                }
            }

            for name in &self.page_names[index] {
                if *name == query {
                    score += 1050;
                } else if name.starts_with(&query) {
                    score += 700;
                } else if name.contains(&query) {
                    score += 360;
                }
            }

            if haystack.contains(&query) {
                score += 220;
            }
            for term in &terms {
                if title.contains(term) {
                    score += 70;
                }
                if haystack.contains(term) {
                    score += 25;
                }
            }

            if score > 0 {
                hits.push(SearchHit {
                    page_id: page.id.clone(),
                    title: page.title.clone(),
                    kind: page.kind,
                    score,
                    snippet: self.summaries[index].clone(),
                });
            }
        }

        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                .then_with(|| a.page_id.cmp(&b.page_id))
        });
        hits.truncate(limit);
        hits
    }

    /// Return the token under (or immediately before) a byte cursor.
    pub fn token_at(&self, text: &str, cursor: usize) -> Option<Token> {
        let mut cursor = cursor.min(text.len());
        while cursor > 0 && !text.is_char_boundary(cursor) {
            cursor -= 1;
        }
        self.scan_tokens(text)
            .into_iter()
            .find(|token| token.start <= cursor && cursor <= token.end)
    }

    /// All tokens in a J sentence. Strings and comments are skipped; numbers
    /// and other punctuation are returned as [`TokenKind::Other`] so callers
    /// can preserve positions.
    pub fn scan_tokens(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut index = 0usize;

        while index < text.len() {
            let Some(ch) = text[index..].chars().next() else {
                break;
            };
            let width = ch.len_utf8();

            if ch.is_whitespace() {
                index += width;
                continue;
            }

            if text[index..].starts_with("NB.") {
                match text[index..].find('\n') {
                    Some(offset) => index += offset + 1,
                    None => break,
                }
                continue;
            }

            if ch == '\'' {
                index = skip_string(text, index);
                continue;
            }

            if is_number_start(text, index, ch) {
                if let Some((symbol, end)) = self.match_symbol(text, index)
                    && symbol.chars().count() > 1
                    && symbol != "_"
                {
                    tokens.push(Token {
                        text: text[index..end].to_string(),
                        kind: TokenKind::Symbol,
                        start: index,
                        end,
                    });
                    index = end;
                    continue;
                }
                let end = scan_number(text, index);
                tokens.push(Token {
                    text: text[index..end].to_string(),
                    kind: TokenKind::Other,
                    start: index,
                    end,
                });
                index = end;
                continue;
            }

            if let Some((_symbol, end)) = self.match_symbol(text, index) {
                tokens.push(Token {
                    text: text[index..end].to_string(),
                    kind: TokenKind::Symbol,
                    start: index,
                    end,
                });
                index = end;
                continue;
            }

            if ch.is_ascii_alphabetic() {
                let end = scan_name(text, index);
                tokens.push(Token {
                    text: text[index..end].to_string(),
                    kind: TokenKind::Name,
                    start: index,
                    end,
                });
                index = end;
                continue;
            }

            tokens.push(Token {
                text: ch.to_string(),
                kind: TokenKind::Other,
                start: index,
                end: index + width,
            });
            index += width;
        }

        tokens
    }

    fn match_symbol(&self, text: &str, start: usize) -> Option<(&str, usize)> {
        let rest = text.get(start..)?;
        for symbol in &self.symbols_by_length {
            if rest.starts_with(symbol.as_str()) {
                return Some((symbol.as_str(), start + symbol.len()));
            }
        }
        None
    }
}

fn page_name_candidates(page: &Page) -> Vec<String> {
    let mut names = Vec::new();
    let mut push = |value: &str| {
        let value = collapse_whitespace(value).to_lowercase();
        if !value.is_empty() && !names.contains(&value) {
            names.push(value);
        }
    };

    push(&page.id);
    push(&page.title);
    for symbol in &page.symbols {
        push(symbol);
    }
    push(&page.monad);
    push(&page.dyad);

    let mut word = String::new();
    for ch in page.title.chars() {
        if ch.is_alphanumeric() {
            word.push(ch.to_ascii_lowercase());
        } else if !word.is_empty() {
            if word.chars().count() >= 2 {
                push(&word);
            }
            word.clear();
        }
    }
    if word.chars().count() >= 2 {
        push(&word);
    }

    names
}

fn is_number_start(text: &str, index: usize, ch: char) -> bool {
    if ch.is_ascii_digit() {
        return true;
    }
    if ch == '_' {
        return text[index + ch.len_utf8()..]
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_digit() || next == '.');
    }
    ch == '.' && text[index + 1..].chars().next().is_some_and(|next| next.is_ascii_digit())
}

fn scan_number(text: &str, start: usize) -> usize {
    let mut index = start;
    while let Some(ch) = text[index..].chars().next() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    index
}

fn scan_name(text: &str, start: usize) -> usize {
    let mut index = start;
    while let Some(ch) = text[index..].chars().next() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    index
}

fn skip_string(text: &str, start: usize) -> usize {
    let mut index = start + 1;
    while index < text.len() {
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        index += ch.len_utf8();
        if ch == '\'' {
            if text[index..].starts_with('\'') {
                index += 1;
            } else {
                break;
            }
        }
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> &'static HelpIndex {
        match HelpIndex::bundled() {
            Ok(index) => index,
            Err(err) => panic!("bundled help data failed to load: {err}"),
        }
    }

    #[test]
    fn bundled_data_loads() {
        let index = index();
        assert!(index.page_count() > 250);
        assert_eq!(index.source().name, "jsoftware/docs_help");
        assert_eq!(
            index.source().commit,
            "ae1314972b37f16ba0a67621aedef26db2c344ae"
        );
        assert!(index.symbols().len() > 100);
    }

    #[test]
    fn symbols_and_names_resolve() {
        let index = index();
        let plus = index.lookup_symbol("+").expect("plus page");
        assert_eq!(plus.id, "d100");
        assert!(plus.monad.to_lowercase().contains("conjugate"));
        assert!(plus.dyad.to_lowercase().contains("plus"));

        let box_page = index.lookup_symbol("<").expect("less-than page");
        assert_eq!(box_page.id, "d010");
        assert!(box_page.title.contains("Box"));

        let sorting = index
            .lookup_name("Sorting")
            .or_else(|| index.lookup_name("sorting"))
            .expect("sorting page");
        assert!(sorting.title.to_lowercase().contains("sort"));
    }

    #[test]
    fn search_ranks_exact_matches() {
        let index = index();
        let results = index.search("+", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].page_id, "d100");

        let results = index.search("sorting", 10);
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .any(|hit| hit.title.to_lowercase().contains("sorting"))
        );

        let results = index.search("sum", 20);
        assert!(
            results
                .iter()
                .any(|hit| hit.page_id == "idioms" || hit.title.to_lowercase().contains("sum"))
        );
    }

    #[test]
    fn tokenizer_finds_symbols_and_names() {
        let index = index();

        let token = index.token_at("2 + 3", 3).expect("plus token");
        assert_eq!(token.text, "+");
        assert_eq!(token.kind, TokenKind::Symbol);

        let token = index.token_at("+/ 1 2 3", 1).expect("plus token");
        assert_eq!(token.text, "+");

        let token = index.token_at("foo i. 5", 5).expect("iota token");
        assert_eq!(token.text, "i.");
        assert_eq!(token.kind, TokenKind::Symbol);

        assert!(index.token_at("NB. a comment", 5).is_none());
        assert!(index.token_at("'a + b'", 3).is_none());

        let tokens = index.scan_tokens("a=: +/ i. 5");
        assert!(tokens.iter().any(|token| token.text == "a"));
        assert!(tokens.iter().any(|token| token.text == "=:"));
        assert!(tokens.iter().any(|token| token.text == "+"));
        assert!(tokens.iter().any(|token| token.text == "i."));
    }

    #[test]
    fn tokenizer_handles_negative_numbers_and_comments() {
        let index = index();
        let tokens = index.scan_tokens("_1 + 2 NB. ignored");
        assert!(tokens.iter().any(|token| token.text == "_1"));
        assert!(tokens.iter().any(|token| token.text == "+"));
        assert!(!tokens.iter().any(|token| token.text.contains("ignored")));
    }
}
