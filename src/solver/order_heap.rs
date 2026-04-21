//! Variable-activity max-heap for VSIDS branching.
//!
//! A binary heap of variable indices keyed by an externally-owned activity
//! slice. An inverse-position vector lets every variable jump to its heap
//! slot in O(1), so `update` after an activity bump and `insert` after a
//! backtrack both cost O(log n). The heap stores zero-based variable
//! indices; callers translate through [`Var::new`] / [`Var::index`].
//!
//! The activity slice is owned outside the heap so that conflict analysis
//! can bump activities in place without routing through the heap, and so
//! rescaling (dividing all activities by `1e100` when any entry crosses the
//! ceiling) can be done on the flat slice.

use alloc::vec::Vec;

use crate::types::Var;

/// Binary max-heap of variables keyed by an external activity array.
#[derive(Debug, Default)]
pub(crate) struct OrderHeap {
    heap: Vec<u32>,
    positions: Vec<Option<u32>>,
}

impl OrderHeap {
    /// Creates an empty heap.
    pub(crate) const fn new() -> Self {
        Self { heap: Vec::new(), positions: Vec::new() }
    }

    /// Grows the internal position table so every variable up to
    /// `num_vars` is addressable.
    pub(crate) fn grow_to(&mut self, num_vars: usize) {
        if self.positions.len() < num_vars {
            self.positions.resize(num_vars, None);
        }
    }

    /// Returns `true` if `var` is currently present in the heap.
    pub(crate) fn contains(&self, var: Var) -> bool {
        self.positions
            .get(var.index())
            .copied()
            .flatten()
            .is_some()
    }

    /// Returns the number of variables currently in the heap.
    pub(crate) fn len(&self) -> usize {
        self.heap.len()
    }

    /// Returns `true` if the heap is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Inserts `var` if it is not already in the heap.
    pub(crate) fn insert(&mut self, var: Var, activities: &[f64]) {
        if self.contains(var) {
            return;
        }
        #[allow(clippy::cast_possible_truncation)]
        let slot = self.heap.len() as u32;
        self.heap.push(u32_from_usize(var.index()));
        self.positions[var.index()] = Some(slot);
        self.sift_up(slot as usize, activities);
    }

    /// Notifies the heap that `var`'s activity increased. If the variable
    /// is not currently in the heap this is a no-op.
    pub(crate) fn update_bumped(&mut self, var: Var, activities: &[f64]) {
        if let Some(Some(pos)) = self.positions.get(var.index()).copied() {
            self.sift_up(pos as usize, activities);
        }
    }

    /// Removes and returns the highest-activity variable.
    pub(crate) fn pop_max(&mut self, activities: &[f64]) -> Option<Var> {
        let top = *self.heap.first()?;
        let top_idx = top as usize;
        self.positions[top_idx] = None;

        if self.heap.len() == 1 {
            let _ = self.heap.pop();
        } else if let Some(last) = self.heap.pop() {
            self.heap[0] = last;
            self.positions[last as usize] = Some(0);
            self.sift_down(0, activities);
        }

        #[allow(clippy::cast_possible_truncation)]
        let raw = (top_idx as u32).saturating_add(1);
        Var::new(raw)
    }

    /// Drops every variable from the heap, preserving allocated storage.
    pub(crate) fn clear(&mut self) {
        for &v in &self.heap {
            self.positions[v as usize] = None;
        }
        self.heap.clear();
    }

    fn sift_up(&mut self, mut i: usize, activities: &[f64]) {
        while i > 0 {
            let parent = (i - 1) / 2;
            let child_v = self.heap[i] as usize;
            let parent_v = self.heap[parent] as usize;
            if activities[child_v] > activities[parent_v] {
                self.heap.swap(i, parent);
                #[allow(clippy::cast_possible_truncation)]
                {
                    self.positions[child_v] = Some(parent as u32);
                    self.positions[parent_v] = Some(i as u32);
                }
                i = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(&mut self, mut i: usize, activities: &[f64]) {
        let len = self.heap.len();
        loop {
            let left = 2 * i + 1;
            let right = 2 * i + 2;
            if left >= len {
                break;
            }
            let mut best = left;
            if right < len {
                let l_v = self.heap[left] as usize;
                let r_v = self.heap[right] as usize;
                if activities[r_v] > activities[l_v] {
                    best = right;
                }
            }
            let cur_v = self.heap[i] as usize;
            let best_v = self.heap[best] as usize;
            if activities[best_v] > activities[cur_v] {
                self.heap.swap(i, best);
                #[allow(clippy::cast_possible_truncation)]
                {
                    self.positions[cur_v] = Some(best as u32);
                    self.positions[best_v] = Some(i as u32);
                }
                i = best;
            } else {
                break;
            }
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
const fn u32_from_usize(u: usize) -> u32 {
    u as u32
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    #[test]
    fn empty_heap_is_empty() {
        let h = OrderHeap::new();
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
    }

    #[test]
    fn insert_and_pop_return_highest_activity() {
        let mut h = OrderHeap::new();
        h.grow_to(3);
        let activities = [0.5_f64, 1.5, 1.0];
        h.insert(v(1), &activities);
        h.insert(v(2), &activities);
        h.insert(v(3), &activities);
        assert_eq!(h.pop_max(&activities), Some(v(2)));
        assert_eq!(h.pop_max(&activities), Some(v(3)));
        assert_eq!(h.pop_max(&activities), Some(v(1)));
        assert!(h.is_empty());
    }

    #[test]
    fn insert_is_idempotent() {
        let mut h = OrderHeap::new();
        h.grow_to(2);
        let activities = [1.0_f64, 2.0];
        h.insert(v(1), &activities);
        h.insert(v(1), &activities);
        assert_eq!(h.len(), 1);
        assert_eq!(h.pop_max(&activities), Some(v(1)));
    }

    #[test]
    fn contains_reflects_membership() {
        let mut h = OrderHeap::new();
        h.grow_to(2);
        let activities = [1.0_f64, 2.0];
        assert!(!h.contains(v(1)));
        h.insert(v(1), &activities);
        assert!(h.contains(v(1)));
        let _ = h.pop_max(&activities);
        assert!(!h.contains(v(1)));
    }

    #[test]
    fn update_bumped_after_activity_raise() {
        let mut h = OrderHeap::new();
        h.grow_to(3);
        let mut activities = [1.0_f64, 2.0, 3.0];
        h.insert(v(1), &activities);
        h.insert(v(2), &activities);
        h.insert(v(3), &activities);
        // v(1) starts lowest; bump it past v(3) and re-heapify.
        activities[0] = 10.0;
        h.update_bumped(v(1), &activities);
        assert_eq!(h.pop_max(&activities), Some(v(1)));
    }

    #[test]
    fn pop_max_from_empty_returns_none() {
        let mut h = OrderHeap::new();
        let activities: [f64; 0] = [];
        assert_eq!(h.pop_max(&activities), None);
    }

    #[test]
    fn update_bumped_ignores_absent_var() {
        let mut h = OrderHeap::new();
        h.grow_to(2);
        let activities = [1.0_f64, 2.0];
        h.insert(v(1), &activities);
        h.update_bumped(v(2), &activities);
        assert_eq!(h.len(), 1);
        assert_eq!(h.pop_max(&activities), Some(v(1)));
    }

    #[test]
    fn reinsert_after_pop_goes_back() {
        let mut h = OrderHeap::new();
        h.grow_to(2);
        let activities = [1.0_f64, 2.0];
        h.insert(v(1), &activities);
        h.insert(v(2), &activities);
        assert_eq!(h.pop_max(&activities), Some(v(2)));
        h.insert(v(2), &activities);
        assert_eq!(h.pop_max(&activities), Some(v(2)));
    }

    #[test]
    fn clear_empties_heap() {
        let mut h = OrderHeap::new();
        h.grow_to(2);
        let activities = [1.0_f64, 2.0];
        h.insert(v(1), &activities);
        h.insert(v(2), &activities);
        h.clear();
        assert!(h.is_empty());
        assert!(!h.contains(v(1)));
    }
}
