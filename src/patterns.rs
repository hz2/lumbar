//! Built-in access-pattern generators, for comparing e.g. naive vs. blocked
//! matmul without hand-writing a trace. Each generator returns a lazy
//! iterator so large logical traces don't need to be materialized up front.
//!
//! Arrays are laid out row-major and given disjoint address ranges so
//! multi-array patterns (matmul's A/B/C) don't alias; [`Access::stream`]
//! tags which logical array each access belongs to.

use crate::access::{Access, Address, StreamId};

/// Sequential row-major traversal of a `rows` x `cols` array.
pub fn row_major(rows: usize, cols: usize, elem_size: usize) -> impl Iterator<Item = Access> {
    let esz = elem_size as u32;
    (0..rows * cols).map(move |idx| Access::read(idx as Address * elem_size as Address, esz))
}

/// Column-major traversal of a row-major-laid-out `rows` x `cols` array --
/// the classic "wrong traversal order" locality demonstration.
pub fn col_major(rows: usize, cols: usize, elem_size: usize) -> impl Iterator<Item = Access> {
    let esz = elem_size as u32;
    (0..rows * cols).map(move |idx| {
        let (row, col) = (idx % rows, idx / rows);
        Access::read((row * cols + col) as Address * elem_size as Address, esz)
    })
}

/// Out-of-place transpose: reads `rows` x `cols` row-major, writes the result
/// column-major into a second, equally sized buffer placed right after the
/// source in the address space.
pub fn transpose(rows: usize, cols: usize, elem_size: usize) -> impl Iterator<Item = Access> {
    let esz = elem_size as u32;
    let elem = elem_size as Address;
    let dst_base = (rows * cols) as Address * elem;
    (0..rows * cols).flat_map(move |idx| {
        let (row, col) = (idx / cols, idx % cols);
        let src_addr = idx as Address * elem;
        let dst_addr = dst_base + (col * rows + row) as Address * elem;
        [
            Access::read(src_addr, esz).with_stream(StreamId(0)),
            Access::write(dst_addr, esz).with_stream(StreamId(1)),
        ]
        .into_iter()
    })
}

/// Naive triple-nested-loop (ijk) matrix multiply: `C = A * B`, `A` is `m` x
/// `k`, `B` is `k` x `n`, `C` is `m` x `n`. For each output element, reads the
/// full length-`k` row of `A` and column of `B` before writing `C`.
pub fn matmul_naive(
    m: usize,
    n: usize,
    k: usize,
    elem_size: usize,
) -> impl Iterator<Item = Access> {
    let esz = elem_size as u32;
    let elem = elem_size as Address;
    let a_base: Address = 0;
    let b_base = (m * k) as Address * elem;
    let c_base = b_base + (k * n) as Address * elem;

    (0..m * n).flat_map(move |ij| {
        let (i, j) = (ij / n, ij % n);
        let c_addr = c_base + ij as Address * elem;
        let ab_reads = (0..k).flat_map(move |p| {
            let a_addr = a_base + (i * k + p) as Address * elem;
            let b_addr = b_base + (p * n + j) as Address * elem;
            [
                Access::read(a_addr, esz).with_stream(StreamId(0)),
                Access::read(b_addr, esz).with_stream(StreamId(1)),
            ]
            .into_iter()
        });
        ab_reads.chain(std::iter::once(
            Access::write(c_addr, esz).with_stream(StreamId(2)),
        ))
    })
}

/// Blocked/tiled matrix multiply with square tiles of side `block`, using the
/// standard `(ib, jb, pb, i, j, p)` loop order so a tile of `A` and `B` is
/// reused across the tile's `i`/`j` sub-loop before moving to the next `k`
/// block. `C` is modeled as read-modify-write on every `k` block (a
/// simplification versus keeping the accumulator in a register).
pub fn matmul_blocked(
    m: usize,
    n: usize,
    k: usize,
    block: usize,
    elem_size: usize,
) -> impl Iterator<Item = Access> {
    let block = block.max(1);
    let esz = elem_size as u32;
    let elem = elem_size as Address;
    let a_base: Address = 0;
    let b_base = (m * k) as Address * elem;
    let c_base = b_base + (k * n) as Address * elem;

    (0..m).step_by(block).flat_map(move |ib| {
        let i_end = (ib + block).min(m);
        (0..n).step_by(block).flat_map(move |jb| {
            let j_end = (jb + block).min(n);
            (0..k).step_by(block).flat_map(move |pb| {
                let p_end = (pb + block).min(k);
                let is_first_block = pb == 0;
                (ib..i_end).flat_map(move |i| {
                    (jb..j_end).flat_map(move |j| {
                        let c_addr = c_base + (i * n + j) as Address * elem;
                        let ab_reads = (pb..p_end).flat_map(move |p| {
                            let a_addr = a_base + (i * k + p) as Address * elem;
                            let b_addr = b_base + (p * n + j) as Address * elem;
                            [
                                Access::read(a_addr, esz).with_stream(StreamId(0)),
                                Access::read(b_addr, esz).with_stream(StreamId(1)),
                            ]
                            .into_iter()
                        });
                        let c_read = (!is_first_block)
                            .then(|| Access::read(c_addr, esz).with_stream(StreamId(2)));
                        let c_write = Access::write(c_addr, esz).with_stream(StreamId(2));
                        ab_reads.chain(c_read).chain(std::iter::once(c_write))
                    })
                })
            })
        })
    })
}

/// A `(2 * radius + 1)`-point von Neumann stencil over a `rows` x `cols` grid,
/// ping-ponging between two equally sized buffers across `iterations` time
/// steps. Panics if `rows <= 2 * radius` or `cols <= 2 * radius` (there would
/// be no interior points).
pub fn stencil_2d(
    rows: usize,
    cols: usize,
    radius: usize,
    iterations: usize,
    elem_size: usize,
) -> impl Iterator<Item = Access> {
    let radius = radius.max(1);
    let esz = elem_size as u32;
    let elem = elem_size as Address;
    let buf_size = (rows * cols) as Address * elem;

    (0..iterations).flat_map(move |iter| {
        let (src_base, dst_base) = if iter % 2 == 0 {
            (0, buf_size)
        } else {
            (buf_size, 0)
        };
        (radius..rows - radius).flat_map(move |row| {
            (radius..cols - radius).flat_map(move |col| {
                let offsets = (1..=radius).flat_map(move |d| {
                    [
                        (row - d) * cols + col,
                        (row + d) * cols + col,
                        row * cols + (col - d),
                        row * cols + (col + d),
                    ]
                    .into_iter()
                });
                let reads = std::iter::once(row * cols + col)
                    .chain(offsets)
                    .map(move |idx| Access::read(src_base + idx as Address * elem, esz));
                let write = Access::write(dst_base + (row * cols + col) as Address * elem, esz);
                reads.chain(std::iter::once(write))
            })
        })
    })
}
