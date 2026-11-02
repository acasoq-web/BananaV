//! Compressão deflate: LZ77 com cadeias de hash (busca preguiçosa nos níveis
//! altos) e blocos com Huffman dinâmico, fixo ou armazenado — o que for menor.

const WINDOW: usize = 32 * 1024;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const HASH_BITS: u32 = 15;
/// Símbolos por bloco antes de emitir.
const BLOCK_SYMBOLS: usize = 1 << 16;

#[derive(Clone, Copy)]
enum Token {
    Lit(u8),
    Match { len: u16, dist: u16 },
}

/// Escritor de bits LSB primeiro.
struct Writer {
    out: Vec<u8>,
    buf: u64,
    count: u32,
}

impl Writer {
    fn new() -> Writer {
        Writer {
            out: Vec::new(),
            buf: 0,
            count: 0,
        }
    }

    #[inline]
    fn bits(&mut self, v: u32, n: u32) {
        debug_assert!(n <= 32);
        self.buf |= (v as u64) << self.count;
        self.count += n;
        while self.count >= 8 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.count -= 8;
        }
    }

    /// Código de Huffman: os bits vão do mais significativo ao menos.
    #[inline]
    fn code(&mut self, code: u32, len: u8) {
        let mut r = 0;
        let mut c = code;
        for _ in 0..len {
            r = (r << 1) | (c & 1);
            c >>= 1;
        }
        self.bits(r, len as u32);
    }

    fn align(&mut self) {
        if self.count > 0 {
            self.out.push(self.buf as u8);
            self.buf = 0;
            self.count = 0;
        }
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn len_code(len: usize) -> usize {
    LEN_BASE
        .iter()
        .rposition(|&b| b as usize <= len)
        .expect("len ≥ 3")
}

fn dist_code(dist: usize) -> usize {
    DIST_BASE
        .iter()
        .rposition(|&b| b as usize <= dist)
        .expect("dist ≥ 1")
}

/// Comprime `data` em deflate bruto. `level` 0 só armazena; 1–9 trocam
/// velocidade por taxa de compressão.
pub fn deflate(data: &[u8], level: u8) -> Vec<u8> {
    let mut w = Writer::new();
    if level == 0 || data.is_empty() {
        stored_blocks(&mut w, data, true);
        return w.out;
    }
    let max_chain = match level {
        1..=3 => 8,
        4..=6 => 48,
        _ => 256,
    };
    let lazy = level >= 4;
    let tokens = lz77(data, max_chain, lazy);
    let mut start_byte = 0usize;
    let chunks: Vec<&[Token]> = tokens.chunks(BLOCK_SYMBOLS).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let bytes: usize = chunk
            .iter()
            .map(|t| match t {
                Token::Lit(_) => 1,
                Token::Match { len, .. } => *len as usize,
            })
            .sum();
        let last = i + 1 == chunks.len();
        write_block(&mut w, chunk, &data[start_byte..start_byte + bytes], last);
        start_byte += bytes;
    }
    w.align();
    w.out
}

fn stored_blocks(w: &mut Writer, data: &[u8], last: bool) {
    let mut chunks: Vec<&[u8]> = data.chunks(65_535).collect();
    if chunks.is_empty() {
        chunks.push(&[]);
    }
    let n = chunks.len();
    for (i, c) in chunks.into_iter().enumerate() {
        w.bits((last && i + 1 == n) as u32, 1);
        w.bits(0, 2);
        w.align();
        let len = c.len() as u16;
        w.out.extend_from_slice(&len.to_le_bytes());
        w.out.extend_from_slice(&(!len).to_le_bytes());
        w.out.extend_from_slice(c);
    }
}

fn hash3(d: &[u8], i: usize) -> usize {
    let v = (d[i] as u32) | (d[i + 1] as u32) << 8 | (d[i + 2] as u32) << 16;
    (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

fn lz77(data: &[u8], max_chain: usize, lazy: bool) -> Vec<Token> {
    let n = data.len();
    let mut head = vec![-1i64; 1 << HASH_BITS];
    let mut prev = vec![-1i64; WINDOW];
    let mut tokens = Vec::with_capacity(n / 2);
    let insert = |head: &mut [i64], prev: &mut [i64], i: usize| {
        if i + MIN_MATCH <= n {
            let h = hash3(data, i);
            prev[i % WINDOW] = head[h];
            head[h] = i as i64;
        }
    };
    let longest = |head: &[i64], prev: &[i64], i: usize| -> (usize, usize) {
        if i + MIN_MATCH > n {
            return (0, 0);
        }
        let max = (n - i).min(MAX_MATCH);
        let mut cand = head[hash3(data, i)];
        let (mut best_len, mut best_dist) = (0, 0);
        let mut chain = max_chain;
        while cand >= 0 && chain > 0 {
            let c = cand as usize;
            let dist = i - c;
            if dist > WINDOW || dist == 0 {
                break;
            }
            if data[c + best_len.min(max - 1)] == data[i + best_len.min(max - 1)] {
                let mut l = 0;
                while l < max && data[c + l] == data[i + l] {
                    l += 1;
                }
                if l > best_len {
                    best_len = l;
                    best_dist = dist;
                    if l == max {
                        break;
                    }
                }
            }
            cand = prev[c % WINDOW];
            chain -= 1;
        }
        if best_len >= MIN_MATCH {
            (best_len, best_dist)
        } else {
            (0, 0)
        }
    };

    let mut i = 0;
    while i < n {
        let (len, dist) = longest(&head, &prev, i);
        if len == 0 {
            tokens.push(Token::Lit(data[i]));
            insert(&mut head, &mut prev, i);
            i += 1;
            continue;
        }
        if lazy && len < 32 && i + 1 < n {
            insert(&mut head, &mut prev, i);
            let (len2, _) = longest(&head, &prev, i + 1);
            if len2 > len {
                tokens.push(Token::Lit(data[i]));
                i += 1;
                continue;
            }
            tokens.push(Token::Match {
                len: len as u16,
                dist: dist as u16,
            });
            for k in i + 1..i + len {
                insert(&mut head, &mut prev, k);
            }
            i += len;
            continue;
        }
        tokens.push(Token::Match {
            len: len as u16,
            dist: dist as u16,
        });
        for k in i..i + len {
            insert(&mut head, &mut prev, k);
        }
        i += len;
    }
    tokens
}

/// Comprimentos de código de Huffman com limite de `limit` bits.
fn huffman_lengths(freqs: &[u32], limit: u8) -> Vec<u8> {
    let mut f: Vec<u64> = freqs.iter().map(|&x| x as u64).collect();
    let used: Vec<usize> = (0..f.len()).filter(|&i| f[i] > 0).collect();
    let mut lengths = vec![0u8; f.len()];
    match used.len() {
        0 => return lengths,
        1 => {
            lengths[used[0]] = 1;
            return lengths;
        }
        _ => {}
    }
    loop {
        // Huffman clássico: une sempre os dois nós de menor peso.
        let mut nodes: Vec<(u64, Vec<usize>)> = used.iter().map(|&s| (f[s], vec![s])).collect();
        let mut depth = vec![0u8; f.len()];
        while nodes.len() > 1 {
            nodes.sort_by_key(|n| std::cmp::Reverse(n.0));
            let (wa, a) = nodes.pop().expect("dois nós");
            let (wb, b) = nodes.pop().expect("dois nós");
            for &s in a.iter().chain(&b) {
                depth[s] += 1;
            }
            let mut merged = a;
            merged.extend(b);
            nodes.push((wa + wb, merged));
        }
        if depth.iter().all(|&d| d <= limit) {
            for &s in &used {
                lengths[s] = depth[s];
            }
            return lengths;
        }
        // Achata a distribuição e tenta de novo.
        for &s in &used {
            f[s] = f[s].div_ceil(2);
        }
    }
}

/// Códigos canônicos a partir dos comprimentos.
fn canonical(lengths: &[u8]) -> Vec<u32> {
    let mut count = [0u32; 16];
    for &l in lengths {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; 16];
    let mut code = 0;
    for bits in 1..16 {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    lengths
        .iter()
        .map(|&l| {
            if l == 0 {
                0
            } else {
                let c = next[l as usize];
                next[l as usize] += 1;
                c
            }
        })
        .collect()
}

fn fixed_lengths() -> (Vec<u8>, Vec<u8>) {
    let mut l = vec![0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    (l, vec![5u8; 30])
}

/// Sequência RLE dos comprimentos (símbolos 0–18 com bits extras).
fn rle_lengths(all: &[u8]) -> Vec<(u8, u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < all.len() {
        let v = all[i];
        let mut run = 1;
        while i + run < all.len() && all[i + run] == v {
            run += 1;
        }
        let mut left = run;
        if v == 0 {
            while left >= 11 {
                let r = left.min(138);
                out.push((18, (r - 11) as u8, 7));
                left -= r;
            }
            if left >= 3 {
                out.push((17, (left - 3) as u8, 3));
                left = 0;
            }
        } else {
            out.push((v, 0, 0));
            left -= 1;
            while left >= 3 {
                let r = left.min(6);
                out.push((16, (r - 3) as u8, 2));
                left -= r;
            }
        }
        for _ in 0..left {
            out.push((v, 0, 0));
        }
        i += run;
    }
    out
}

fn token_cost(tokens: &[Token], lit_len: &[u8], dist_len: &[u8]) -> u64 {
    let mut bits = lit_len[256] as u64;
    for t in tokens {
        bits += match *t {
            Token::Lit(b) => lit_len[b as usize] as u64,
            Token::Match { len, dist } => {
                let lc = len_code(len as usize);
                let dc = dist_code(dist as usize);
                lit_len[257 + lc] as u64
                    + LEN_EXTRA[lc] as u64
                    + dist_len[dc] as u64
                    + DIST_EXTRA[dc] as u64
            }
        };
    }
    bits
}

fn write_block(w: &mut Writer, tokens: &[Token], raw: &[u8], last: bool) {
    let mut lit_freq = vec![0u32; 286];
    let mut dist_freq = vec![0u32; 30];
    lit_freq[256] = 1;
    for t in tokens {
        match *t {
            Token::Lit(b) => lit_freq[b as usize] += 1,
            Token::Match { len, dist } => {
                lit_freq[257 + len_code(len as usize)] += 1;
                dist_freq[dist_code(dist as usize)] += 1;
            }
        }
    }
    let lit_len = huffman_lengths(&lit_freq, 15);
    let mut dist_len = huffman_lengths(&dist_freq, 15);
    // Pelo menos dois códigos de distância, para a tabela ficar completa.
    let used = dist_len.iter().filter(|&&l| l > 0).count();
    if used < 2 {
        for l in dist_len.iter_mut().take(2) {
            if *l == 0 {
                *l = 1;
            }
        }
        if used == 1 {
            for l in dist_len.iter_mut() {
                if *l > 0 {
                    *l = 1;
                }
            }
            // Deixa exatamente dois códigos de 1 bit.
            let mut seen = 0;
            for l in dist_len.iter_mut() {
                if *l > 0 {
                    seen += 1;
                    if seen > 2 {
                        *l = 0;
                    }
                }
            }
        }
    }
    let hlit = lit_len
        .iter()
        .rposition(|&l| l > 0)
        .map_or(257, |p| (p + 1).max(257));
    let hdist = dist_len.iter().rposition(|&l| l > 0).map_or(1, |p| p + 1);
    let mut all = lit_len[..hlit].to_vec();
    all.extend_from_slice(&dist_len[..hdist]);
    let rle = rle_lengths(&all);
    let mut cl_freq = vec![0u32; 19];
    for &(s, _, _) in &rle {
        cl_freq[s as usize] += 1;
    }
    let cl_len = huffman_lengths(&cl_freq, 7);
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hclen = ORDER
        .iter()
        .rposition(|&i| cl_len[i] > 0)
        .map_or(4, |p| (p + 1).max(4));

    let header_bits = 5
        + 5
        + 4
        + 3 * hclen as u64
        + rle
            .iter()
            .map(|&(s, _, eb)| cl_len[s as usize] as u64 + eb as u64)
            .sum::<u64>();
    let dynamic = header_bits + token_cost(tokens, &lit_len, &dist_len);
    let (fl, fd) = fixed_lengths();
    let fixed = token_cost(tokens, &fl, &fd);
    let stored = (raw.len() as u64 + 5 * raw.len().div_ceil(65_535).max(1) as u64) * 8;

    if stored < dynamic.min(fixed) {
        stored_blocks(w, raw, last);
        return;
    }
    w.bits(last as u32, 1);
    let (lit_len, dist_len) = if dynamic < fixed {
        w.bits(2, 2);
        w.bits((hlit - 257) as u32, 5);
        w.bits((hdist - 1) as u32, 5);
        w.bits((hclen - 4) as u32, 4);
        for &i in ORDER.iter().take(hclen) {
            w.bits(cl_len[i] as u32, 3);
        }
        let cl_code = canonical(&cl_len);
        for &(s, extra, eb) in &rle {
            w.code(cl_code[s as usize], cl_len[s as usize]);
            if eb > 0 {
                w.bits(extra as u32, eb as u32);
            }
        }
        (lit_len, dist_len)
    } else {
        w.bits(1, 2);
        (fl, fd)
    };
    let lc = canonical(&lit_len);
    let dc = canonical(&dist_len);
    for t in tokens {
        match *t {
            Token::Lit(b) => w.code(lc[b as usize], lit_len[b as usize]),
            Token::Match { len, dist } => {
                let l = len_code(len as usize);
                w.code(lc[257 + l], lit_len[257 + l]);
                w.bits((len - LEN_BASE[l]) as u32, LEN_EXTRA[l] as u32);
                let d = dist_code(dist as usize);
                w.code(dc[d], dist_len[d]);
                w.bits((dist - DIST_BASE[d]) as u32, DIST_EXTRA[d] as u32);
            }
        }
    }
    w.code(lc[256], lit_len[256]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limite_de_comprimento_e_respeitado() {
        // Frequências de Fibonacci geram a árvore mais profunda possível.
        let mut f = vec![1u32, 1];
        while f.len() < 30 {
            let n = f[f.len() - 1] + f[f.len() - 2];
            f.push(n);
        }
        let l = huffman_lengths(&f, 15);
        assert!(l.iter().all(|&x| (1..=15).contains(&x)));
        // Kraft: a soma de 2^-l não passa de 1.
        let kraft: f64 = l.iter().map(|&x| 0.5f64.powi(x as i32)).sum();
        assert!(kraft <= 1.0 + 1e-12);
    }

    #[test]
    fn rle_cobre_todos_os_casos() {
        let mut v = vec![0u8; 200];
        v.extend([5u8; 9]);
        v.extend([3, 3, 7]);
        let rle = rle_lengths(&v);
        let mut expanded = Vec::new();
        let mut last = 0u8;
        for (s, extra, _) in rle {
            match s {
                0..=15 => {
                    expanded.push(s);
                    last = s;
                }
                16 => expanded.extend(std::iter::repeat_n(last, 3 + extra as usize)),
                17 => expanded.extend(std::iter::repeat_n(0, 3 + extra as usize)),
                _ => expanded.extend(std::iter::repeat_n(0, 11 + extra as usize)),
            }
        }
        assert_eq!(expanded, v);
    }
}
