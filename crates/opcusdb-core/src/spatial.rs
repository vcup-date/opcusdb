//! A uniform **spatial grid** for area-of-interest (AOI) queries
//! (`CORE_SPEC.md` §3, §5.5).
//!
//! Interest management is the central MMO scaling lever: only send/process what
//! is near each observer. This grid buckets entities by cell so a "what's near
//! `(x, y)`" query touches a handful of cells instead of every entity. Like the
//! spec says, it is cheap to rebuild each tick (`clear` + re-`insert`).
//!
//! Queries return ids in **ascending order** (deterministic, per the §2
//! contract). The grid stores positions alongside ids so radius/box queries are
//! exact, not just cell-granular.

use crate::entity::EntityId;

/// A uniform grid over a `width × height` area with square cells of side `cell`.
#[derive(Clone, Debug)]
pub struct SpatialGrid {
    cell: i32,
    cols: i32,
    rows: i32,
    width: i32,
    height: i32,
    // cells[row * cols + col] -> entities in that cell, with their positions.
    cells: Vec<Vec<(EntityId, i32, i32)>>,
}

impl SpatialGrid {
    /// Create a grid covering `0..width × 0..height` with the given `cell` size
    /// (clamped to at least 1). Dimensions are clamped to at least 1.
    pub fn new(width: i32, height: i32, cell: i32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let cell = cell.max(1);
        let cols = (width + cell - 1) / cell;
        let rows = (height + cell - 1) / cell;
        Self {
            cell,
            cols,
            rows,
            width,
            height,
            cells: vec![Vec::new(); (cols * rows) as usize],
        }
    }

    /// Remove all entities (positions are kept allocated for cheap reuse).
    pub fn clear(&mut self) {
        for c in &mut self.cells {
            c.clear();
        }
    }

    #[inline]
    fn col_of(&self, x: i32) -> i32 {
        (x / self.cell).clamp(0, self.cols - 1)
    }
    #[inline]
    fn row_of(&self, y: i32) -> i32 {
        (y / self.cell).clamp(0, self.rows - 1)
    }

    /// Insert `id` at pixel `(x, y)`. Coordinates are clamped into bounds.
    pub fn insert(&mut self, id: EntityId, x: i32, y: i32) {
        let cx = x.clamp(0, self.width - 1);
        let cy = y.clamp(0, self.height - 1);
        let idx = (self.row_of(cy) * self.cols + self.col_of(cx)) as usize;
        self.cells[idx].push((id, cx, cy));
    }

    /// Entities whose position lies in the half-open box `[x0,x1) × [y0,y1)`,
    /// ascending by id. Only cells overlapping the box are scanned.
    pub fn query_aabb(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<EntityId> {
        let mut out = Vec::new();
        if x1 <= x0 || y1 <= y0 {
            return out;
        }
        let (c0, c1) = (self.col_of(x0), self.col_of(x1 - 1));
        let (r0, r1) = (self.row_of(y0), self.row_of(y1 - 1));
        for r in r0..=r1 {
            for c in c0..=c1 {
                for &(id, x, y) in &self.cells[(r * self.cols + c) as usize] {
                    if x >= x0 && x < x1 && y >= y0 && y < y1 {
                        out.push(id);
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// Entities within `radius` (inclusive) of `(cx, cy)`, ascending by id.
    /// Exact (Euclidean) — the cells overlapping the bounding box are scanned and
    /// then distance-filtered.
    pub fn query_radius(&self, cx: i32, cy: i32, radius: i32) -> Vec<EntityId> {
        let mut out = Vec::new();
        if radius < 0 {
            return out;
        }
        let r = radius as i64;
        let r2 = r * r;
        let (lo_c, hi_c) = (self.col_of(cx - radius), self.col_of(cx + radius));
        let (lo_r, hi_r) = (self.row_of(cy - radius), self.row_of(cy + radius));
        for row in lo_r..=hi_r {
            for col in lo_c..=hi_c {
                for &(id, x, y) in &self.cells[(row * self.cols + col) as usize] {
                    let dx = (x - cx) as i64;
                    let dy = (y - cy) as i64;
                    if dx * dx + dy * dy <= r2 {
                        out.push(id);
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// Total number of inserted entities.
    pub fn len(&self) -> usize {
        self.cells.iter().map(|c| c.len()).sum()
    }

    /// Whether the grid holds no entities.
    pub fn is_empty(&self) -> bool {
        self.cells.iter().all(|c| c.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entities;
    use crate::rng::Rng;

    #[test]
    fn aabb_matches_brute_force() {
        let mut ent = Entities::new();
        let mut grid = SpatialGrid::new(1000, 1000, 64);
        let mut rng = Rng::seed(7);
        let mut points = Vec::new();
        for _ in 0..2000 {
            let id = ent.spawn();
            let (x, y) = (rng.range(0, 1000) as i32, rng.range(0, 1000) as i32);
            points.push((id, x, y));
            grid.insert(id, x, y);
        }
        assert_eq!(grid.len(), 2000);

        // Compare several boxes against the brute-force answer.
        for &(bx0, by0, bx1, by1) in &[(100, 100, 400, 400), (0, 0, 50, 1000), (700, 700, 1000, 1000)] {
            let mut brute: Vec<EntityId> = points
                .iter()
                .filter(|(_, x, y)| *x >= bx0 && *x < bx1 && *y >= by0 && *y < by1)
                .map(|(id, _, _)| *id)
                .collect();
            brute.sort_unstable();
            assert_eq!(grid.query_aabb(bx0, by0, bx1, by1), brute);
        }
    }

    #[test]
    fn radius_matches_brute_force() {
        let mut ent = Entities::new();
        let mut grid = SpatialGrid::new(500, 500, 40);
        let mut rng = Rng::seed(99);
        let mut points = Vec::new();
        for _ in 0..1500 {
            let id = ent.spawn();
            let (x, y) = (rng.range(0, 500) as i32, rng.range(0, 500) as i32);
            points.push((id, x, y));
            grid.insert(id, x, y);
        }
        let (cx, cy, r) = (250, 250, 80);
        let r2 = (r as i64) * (r as i64);
        let mut brute: Vec<EntityId> = points
            .iter()
            .filter(|(_, x, y)| {
                let (dx, dy) = ((*x - cx) as i64, (*y - cy) as i64);
                dx * dx + dy * dy <= r2
            })
            .map(|(id, _, _)| *id)
            .collect();
        brute.sort_unstable();
        assert_eq!(grid.query_radius(cx, cy, r), brute);
        assert!(!brute.is_empty(), "the test region should contain some entities");
    }

    #[test]
    fn results_are_ascending() {
        let mut ent = Entities::new();
        let mut grid = SpatialGrid::new(100, 100, 10);
        // Insert in descending position order; ids ascend with spawn order.
        let ids: Vec<_> = (0..20).map(|_| ent.spawn()).collect();
        for (i, &id) in ids.iter().enumerate() {
            grid.insert(id, 99 - i as i32, 50);
        }
        let got = grid.query_aabb(0, 0, 100, 100);
        let mut sorted = got.clone();
        sorted.sort_unstable();
        assert_eq!(got, sorted, "ids come back ascending regardless of insert order");
        assert_eq!(got.len(), 20);
    }

    #[test]
    fn clear_and_edge_cases() {
        let mut ent = Entities::new();
        let mut grid = SpatialGrid::new(100, 100, 16);
        grid.insert(ent.spawn(), 10, 10);
        assert_eq!(grid.len(), 1);
        // Out-of-bounds insert is clamped in, still found by a full-area query.
        grid.insert(ent.spawn(), 9999, -50);
        assert_eq!(grid.query_aabb(0, 0, 100, 100).len(), 2);
        // Empty / inverted boxes and negative radius return nothing.
        assert!(grid.query_aabb(50, 50, 50, 50).is_empty());
        assert!(grid.query_aabb(80, 80, 10, 10).is_empty());
        assert!(grid.query_radius(50, 50, -1).is_empty());
        grid.clear();
        assert!(grid.is_empty());
    }
}
