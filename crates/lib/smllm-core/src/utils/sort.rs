//! A small stable merge sort: std's general sort pulls ~23 KB into the wasm,
//! and the paused list it sorts can hold hundreds (PLAN-008 D8-5), where an
//! insertion sort was quadratic.

use core::cmp::Ordering;

use crate::prelude::*;

/// Sort `items` by `compare`, stably, in O(n log n) comparisons.
pub fn merge_sort_by<T>(items: &mut Vec<T>, mut compare: impl FnMut(&T, &T) -> Ordering) {
    let all = core::mem::take(items);
    *items = sort(all, &mut compare);
}

fn sort<T>(mut left: Vec<T>, compare: &mut impl FnMut(&T, &T) -> Ordering) -> Vec<T> {
    if left.len() < 2 {
        return left;
    }
    let right = left.split_off(left.len() / 2);
    let (left, right) = (sort(left, compare), sort(right, compare));
    let mut out = Vec::with_capacity(left.len() + right.len());
    let mut right = right.into_iter().peekable();
    for l in left {
        // Equal items keep their order: the left one goes first.
        while let Some(r) = right.next_if(|r| compare(r, &l) == Ordering::Less) {
            out.push(r);
        }
        out.push(l);
    }
    out.extend(right);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_stably() {
        let mut v = vec![(2, 'a'), (1, 'b'), (2, 'c'), (0, 'd'), (1, 'e')];
        merge_sort_by(&mut v, |a, b| a.0.cmp(&b.0));
        assert_eq!(v, vec![(0, 'd'), (1, 'b'), (1, 'e'), (2, 'a'), (2, 'c')]);
        let mut empty: Vec<u8> = Vec::new();
        merge_sort_by(&mut empty, u8::cmp);
        assert!(empty.is_empty());
        let mut one = vec![7];
        merge_sort_by(&mut one, u8::cmp);
        assert_eq!(one, vec![7]);
    }

    #[test]
    fn agrees_with_a_stable_reference_sort() {
        // Keys with many repeats, so stability shows.
        let mut v: Vec<(u32, usize)> = (0..500).map(|i| ((i * 7919) % 13, i as usize)).collect();
        let mut expected = v.clone();
        expected.sort_by_key(|p| p.0);
        merge_sort_by(&mut v, |a, b| a.0.cmp(&b.0));
        assert_eq!(v, expected);
    }
}
