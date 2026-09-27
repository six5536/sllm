mod small_map;
mod sort;

pub use small_map::SmallMap;
pub use sort::insertion_sort_by;

use crate::prelude::*;

/// `items` separated by `, `.
pub(crate) fn join<'a>(items: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for (i, s) in items.into_iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(s);
    }
    out
}
