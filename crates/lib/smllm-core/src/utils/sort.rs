//! A stable insertion sort: std's general sort pulls ~23 KB into the wasm, and
//! the engine only ever sorts a handful of items.

/// Sort `items` by `key`, stably, in place.
pub fn insertion_sort_by_key<T, K: Ord>(items: &mut [T], mut key: impl FnMut(&T) -> K) {
    for i in 1..items.len() {
        let mut j = i;
        while j > 0 && key(&items[j - 1]) > key(&items[j]) {
            items.swap(j - 1, j);
            j -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn sorts_stably() {
        let mut v = vec![(2, 'a'), (1, 'b'), (2, 'c'), (0, 'd')];
        insertion_sort_by_key(&mut v, |p| p.0);
        assert_eq!(v, vec![(0, 'd'), (1, 'b'), (2, 'a'), (2, 'c')]);
        let mut empty: Vec<u8> = Vec::new();
        insertion_sort_by_key(&mut empty, |x| *x);
        assert!(empty.is_empty());
    }
}
