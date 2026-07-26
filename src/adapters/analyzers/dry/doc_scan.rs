//! Reading a doc comment line by line.
//!
//! A doc comment reaches `syn` as one `#[doc = "…"]` attribute **per line**, so
//! whether a line sits inside a ``` fence is only knowable by remembering what
//! came before it — a small state machine, and the reason this is a type rather
//! than a function.
//!
//! The distinction it draws is what makes doc comments usable as evidence at
//! all: inside a fence the text is code `cargo test` compiles and runs, so every
//! name in it is a real (test) reference; outside one it is prose, where only a
//! bracketed intra-doc link names something.

use crate::adapters::shared::text_names::{code_names, doc_link_names, is_doc_fence};

/// What one doc line contributes.
pub(crate) enum DocLine {
    /// The fence marker itself — it names nothing.
    Fence,
    /// Code inside a fence: a test reference, whatever the documented item is.
    Example(Vec<String>),
    /// Prose: only its intra-doc link targets, which document the API.
    Prose(Vec<String>),
}

/// Tracks whether the doc lines currently arriving are inside a fence.
#[derive(Default)]
pub(crate) struct DocScanner {
    in_fence: bool,
}

impl DocScanner {
    /// Read one line, updating the fence state. An unclosed fence leaks into
    /// the next item — over-collection, the safe direction, and broken docs to
    /// begin with.
    /// Operation: fence toggle + mode dispatch, own calls in the arms.
    pub(crate) fn line(&mut self, text: &str) -> DocLine {
        if is_doc_fence(text) {
            self.in_fence = !self.in_fence;
            return DocLine::Fence;
        }
        match self.in_fence {
            true => DocLine::Example(code_names(text)),
            false => DocLine::Prose(doc_link_names(text)),
        }
    }
}
