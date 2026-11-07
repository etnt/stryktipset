//! Thin serial/parallel wrappers over the three rayon call sites.
//!
//! With the `parallel` feature these run on rayon; without it they are plain
//! sequential loops. Both variants keep index order (`rayon`'s `collect` on
//! an indexed iterator is ordered), which is what keeps the serial build
//! byte-identical to the parallel one.

/// Map `0..len` through `f`, preserving index order.
#[cfg(feature = "parallel")]
use rayon::prelude::*;

#[cfg(feature = "parallel")]
pub fn map_collect<R, F>(len: usize, f: F) -> Vec<R>
where
    R: Send,
    F: Fn(usize) -> R + Sync + Send,
{
    (0..len).into_par_iter().map(f).collect()
}

/// Serial fallback for [`map_collect`].
#[cfg(not(feature = "parallel"))]
pub fn map_collect<R, F>(len: usize, f: F) -> Vec<R>
where
    F: Fn(usize) -> R,
{
    (0..len).map(f).collect()
}

/// Apply `f(i, chunk)` to every `chunk`-sized slice of `buf` in order.
#[cfg(feature = "parallel")]
pub fn for_each_chunk<T, F>(buf: &mut [T], chunk: usize, f: F)
where
    T: Send,
    F: Fn(usize, &mut [T]) + Sync + Send,
{
    buf.par_chunks_mut(chunk)
        .enumerate()
        .for_each(|(i, c)| f(i, c));
}

/// Serial fallback for [`for_each_chunk`].
#[cfg(not(feature = "parallel"))]
pub fn for_each_chunk<T, F>(buf: &mut [T], chunk: usize, f: F)
where
    F: Fn(usize, &mut [T]),
{
    for (i, c) in buf.chunks_mut(chunk).enumerate() {
        f(i, c);
    }
}
