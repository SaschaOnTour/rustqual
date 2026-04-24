use crate::application::stats::get_stats;

pub fn post_stats() -> String {
    get_stats()
}

// Intentionally inlined: does not call `application::list::list_items`.
// Triggers the two findings the golden-example is designed to produce:
//   - `no_delegation` on `post_list` (this fn — no call into application).
//   - `missing_adapter` on `application::list::list_items` (cli + mcp
//     call it, rest doesn't — coverage gap).
pub fn post_list() -> Vec<String> {
    vec!["inline".to_string(), "rest-list".to_string()]
}
