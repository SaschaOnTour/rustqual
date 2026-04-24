use crate::application::list::list_items;
use crate::application::stats::get_stats;

pub fn handle_stats() -> String {
    get_stats()
}

pub fn handle_list() -> Vec<String> {
    list_items()
}
