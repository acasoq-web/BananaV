//! Paralelismo simples por faixas de linhas, sem dependências.

/// Items (pixels) a thread gets at least.
const MIN_PER_THREAD: usize = 32 * 1024;

pub(crate) fn threads() -> usize {
    bv_color::convert::threads()
}

/// Chama `f(y, linha)` para cada linha de `data` (linhas de `width` pixels),
/// dividindo as linhas entre as threads disponíveis.
pub(crate) fn for_each_row<T: Send>(
    data: &mut [T],
    width: usize,
    f: impl Fn(usize, &mut [T]) + Sync,
) {
    if width == 0 || data.is_empty() {
        return;
    }
    let height = data.len() / width;
    // A thread costs tens of microseconds to start: small images get few.
    let n = threads().min(data.len() / MIN_PER_THREAD + 1);
    if n == 1 || height < 16 {
        for (y, row) in data.chunks_mut(width).enumerate() {
            f(y, row);
        }
        return;
    }
    let rows_per_chunk = height.div_ceil(n);
    std::thread::scope(|s| {
        for (ci, chunk) in data.chunks_mut(width * rows_per_chunk).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (i, row) in chunk.chunks_mut(width).enumerate() {
                    f(ci * rows_per_chunk + i, row);
                }
            });
        }
    });
}

/// Aplica `f` a cada pixel, em paralelo.
pub(crate) fn for_each_pixel<T: Send>(
    data: &mut [T],
    width: usize,
    f: impl Fn(usize, usize, &mut T) + Sync,
) {
    for_each_row(data, width, |y, row| {
        for (x, px) in row.iter_mut().enumerate() {
            f(x, y, px);
        }
    });
}
