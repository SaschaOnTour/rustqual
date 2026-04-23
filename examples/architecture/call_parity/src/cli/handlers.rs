use crate::application::list::list_items;
use crate::application::stats::get_stats;

pub fn cmd_stats() {
    let s = get_stats();
    println!("{s}");
}

pub fn cmd_list() {
    for item in list_items() {
        println!("{item}");
    }
}

// qual:allow(architecture) — CLI-only diagnostic with no MCP / REST peer.
// Documents the legitimate asymmetric feature instead of faking a peer.
pub fn cmd_debug() {
    println!("debug");
}
