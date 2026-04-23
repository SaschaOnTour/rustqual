use crate::application::stats::get_stats;

pub fn post_stats() -> String {
    get_stats()
}

// Intentionally inlined: does not call `application::list::list_items`.
// Triggers both findings the golden-example is designed to produce:
//   - `no_delegation` on `post_stats`? No, post_stats delegates fine.
//   - `no_delegation` on `post_list` — see below.
//   - `missing_adapter` on `application::list::list_items` — REST
//     doesn't reach it.
pub fn post_list() -> Vec<String> {
    vec!["inline".to_string(), "rest-list".to_string()]
}
