//! `BeautifulSoup(text, "html.parser").get_text(separator, strip=True)` on top of the
//! html5ever tokenizer.
//!
//! bs4 builds a tree from Python's `html.parser` events and `get_text` joins its text nodes.
//! Without tree construction the text nodes are the runs of character data between markup
//! (tags, comments, doctypes, processing instructions). Like bs4 4.15, the contents of
//! `<script>`, `<style>` and `<template>` (including everything nested in a template) are not
//! text; `<textarea>`/`<title>` contents are read as raw text, as Python 3.11.14's parser does.

use std::cell::RefCell;

use html5ever::tendril::StrTendril;
use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, CharacterTokens, CommentToken, DoctypeToken, EOFToken, EndTag, NullCharacterToken,
    ParseError, StartTag, TagToken, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};

#[derive(Default)]
struct State {
    strings: Vec<String>,
    current: String,
    template_depth: usize,
    raw_skip: bool,
}

impl State {
    fn flush(&mut self) {
        let text = std::mem::take(&mut self.current);
        if self.template_depth == 0 && !self.raw_skip {
            self.strings.push(text);
        }
    }
}

struct TextSink {
    state: RefCell<State>,
}

impl TokenSink for TextSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        let mut state = self.state.borrow_mut();
        match token {
            CharacterTokens(text) => {
                state.current.push_str(&text);
                TokenSinkResult::Continue
            }
            NullCharacterToken => {
                state.current.push('\0');
                TokenSinkResult::Continue
            }
            TagToken(tag) => {
                if !state.current.is_empty() {
                    state.flush();
                }
                let name: &str = &tag.name;
                match tag.kind {
                    StartTag => {
                        state.raw_skip = false;
                        match name {
                            "template" if !tag.self_closing => {
                                state.template_depth += 1;
                                TokenSinkResult::Continue
                            }
                            "script" if !tag.self_closing => {
                                state.raw_skip = true;
                                TokenSinkResult::RawData(RawKind::ScriptData)
                            }
                            "style" if !tag.self_closing => {
                                state.raw_skip = true;
                                TokenSinkResult::RawData(RawKind::Rawtext)
                            }
                            "textarea" | "title" if !tag.self_closing => {
                                TokenSinkResult::RawData(RawKind::Rcdata)
                            }
                            _ => TokenSinkResult::Continue,
                        }
                    }
                    EndTag => {
                        state.raw_skip = false;
                        if name == "template" && state.template_depth > 0 {
                            state.template_depth -= 1;
                        }
                        TokenSinkResult::Continue
                    }
                }
            }
            CommentToken(_) | DoctypeToken(_) => {
                if !state.current.is_empty() {
                    state.flush();
                }
                TokenSinkResult::Continue
            }
            EOFToken => {
                if !state.current.is_empty() {
                    state.flush();
                }
                TokenSinkResult::Continue
            }
            ParseError(_) => TokenSinkResult::Continue,
        }
    }
}

/// The text strings of an HTML document, in document order.
pub(crate) fn html_strings(html: &str) -> Vec<String> {
    let sink = TextSink {
        state: RefCell::new(State::default()),
    };
    let tokenizer = Tokenizer::new(
        sink,
        TokenizerOpts {
            discard_bom: false,
            ..TokenizerOpts::default()
        },
    );
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(html));
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let sink = tokenizer.sink;
    let mut state = sink.state.into_inner();
    if !state.current.is_empty() {
        state.flush();
    }
    state.strings
}

/// `soup.get_text(separator=separator, strip=True)`.
pub(crate) fn get_text_stripped(html: &str, separator: &str) -> String {
    html_strings(html)
        .iter()
        .map(|s| crate::pystr::strip(s))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(separator)
}
