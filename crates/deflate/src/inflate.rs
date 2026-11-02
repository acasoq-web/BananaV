//! Descompressão deflate (RFC 1951).

use crate::Error;

type Result<T> = std::result::Result<T, Error>;

/// Leitor de bits LSB primeiro, como o deflate usa.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    count: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Bits<'a> {
        Bits {
            data,
            pos: 0,
            buf: 0,
            count: 0,
        }
    }

    #[inline]
    fn refill(&mut self) {
        while self.count <= 56 {
            let Some(&b) = self.data.get(self.pos) else {
                return;
            };
            self.buf |= (b as u64) << self.count;
            self.pos += 1;
            self.count += 8;
        }
    }

    #[inline]
    fn bits(&mut self, n: u32) -> Result<u32> {
        if n == 0 {
            return Ok(0);
        }
        if self.count < n {
            self.refill();
            if self.count < n {
                return Err(Error::Truncated);
            }
        }
        let v = (self.buf & ((1u64 << n) - 1)) as u32;
        self.buf >>= n;
        self.count -= n;
        Ok(v)
    }

    #[inline]
    fn peek(&mut self, n: u32) -> u32 {
        if self.count < n {
            self.refill();
        }
        (self.buf & ((1u64 << n) - 1)) as u32
    }

    #[inline]
    fn consume(&mut self, n: u32) -> Result<()> {
        if self.count < n {
            return Err(Error::Truncated);
        }
        self.buf >>= n;
        self.count -= n;
        Ok(())
    }

    fn align(&mut self) {
        let drop = self.count % 8;
        self.buf >>= drop;
        self.count -= drop;
    }

    /// Bytes efetivamente consumidos (os bits em buffer voltam).
    fn consumed(&self) -> usize {
        self.pos - (self.count / 8) as usize
    }
}

const FAST_BITS: u32 = 9;

/// Tabela de Huffman canônica com consulta rápida para códigos curtos.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
    /// `(símbolo << 4) | comprimento`; 0 = código mais longo que FAST_BITS.
    fast: Vec<u32>,
}

fn reverse(mut code: u32, len: u32) -> u32 {
    let mut r = 0;
    for _ in 0..len {
        r = (r << 1) | (code & 1);
        code >>= 1;
    }
    r
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Huffman> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        // Excesso de códigos é erro; códigos incompletos são permitidos
        // (ex.: um único código de distância).
        let mut left: i32 = 1;
        for &c in &counts[1..] {
            left = left * 2 - c as i32;
            if left < 0 {
                return Err(Error::Corrupt("tabela de Huffman com códigos demais"));
            }
        }
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + counts[len];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        let mut fast = vec![0u32; 1 << FAST_BITS];
        let mut code = 0u32;
        let mut idx = 0usize;
        for len in 1..=15u32 {
            for _ in 0..counts[len as usize] {
                if len <= FAST_BITS {
                    let sym = symbols[idx] as u32;
                    let rev = reverse(code, len);
                    let mut fill = rev;
                    while fill < (1 << FAST_BITS) {
                        fast[fill as usize] = (sym << 4) | len;
                        fill += 1 << len;
                    }
                }
                code += 1;
                idx += 1;
            }
            code <<= 1;
        }
        Ok(Huffman {
            counts,
            symbols,
            fast,
        })
    }

    #[inline]
    fn decode(&self, br: &mut Bits<'_>) -> Result<u16> {
        let entry = self.fast[br.peek(FAST_BITS) as usize];
        if entry != 0 {
            br.consume(entry & 15)?;
            return Ok((entry >> 4) as u16);
        }
        // Caminho lento, bit a bit (códigos longos).
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= br.bits(1)? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + code - first) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(Error::Corrupt("código de Huffman inválido"))
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

fn fixed_tables() -> (Huffman, Huffman) {
    let mut l = [0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    (
        Huffman::new(&l).expect("tabela fixa"),
        Huffman::new(&[5u8; 30]).expect("tabela fixa"),
    )
}

/// Descomprime um fluxo deflate bruto.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>> {
    inflate_with_len(data).map(|(out, _)| out)
}

/// Descomprime e informa quantos bytes de entrada foram usados.
pub(crate) fn inflate_with_len(data: &[u8]) -> Result<(Vec<u8>, usize)> {
    let mut br = Bits::new(data);
    let mut out = Vec::with_capacity(data.len() * 4);
    loop {
        let last = br.bits(1)? == 1;
        match br.bits(2)? {
            0 => stored(&mut br, &mut out)?,
            1 => {
                let (lit, dist) = fixed_tables();
                codes(&mut br, &mut out, &lit, &dist)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut br)?;
                codes(&mut br, &mut out, &lit, &dist)?;
            }
            _ => return Err(Error::Corrupt("tipo de bloco reservado")),
        }
        if last {
            break;
        }
    }
    Ok((out, br.consumed()))
}

fn stored(br: &mut Bits<'_>, out: &mut Vec<u8>) -> Result<()> {
    br.align();
    let len = br.bits(16)?;
    let nlen = br.bits(16)?;
    if len != !nlen & 0xFFFF {
        return Err(Error::Corrupt("tamanho de bloco armazenado"));
    }
    // Bytes ainda no acumulador vêm primeiro.
    let mut len = len as usize;
    while len > 0 && br.count >= 8 {
        out.push(br.bits(8)? as u8);
        len -= 1;
    }
    let start = br.pos;
    let end = start
        .checked_add(len)
        .filter(|&e| e <= br.data.len())
        .ok_or(Error::Truncated)?;
    out.extend_from_slice(&br.data[start..end]);
    br.pos = end;
    Ok(())
}

fn dynamic_tables(br: &mut Bits<'_>) -> Result<(Huffman, Huffman)> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let nlen = br.bits(5)? as usize + 257;
    let ndist = br.bits(5)? as usize + 1;
    let ncode = br.bits(4)? as usize + 4;
    if nlen > 286 || ndist > 30 {
        return Err(Error::Corrupt("contagem de códigos"));
    }
    let mut cl = [0u8; 19];
    for &i in ORDER.iter().take(ncode) {
        cl[i] = br.bits(3)? as u8;
    }
    let clh = Huffman::new(&cl)?;
    let mut lengths = vec![0u8; nlen + ndist];
    let mut i = 0;
    while i < nlen + ndist {
        let sym = clh.decode(br)?;
        let (value, repeat) = match sym {
            0..=15 => (sym as u8, 1),
            16 => {
                let prev = *lengths
                    .get(i.wrapping_sub(1))
                    .filter(|_| i > 0)
                    .ok_or(Error::Corrupt("repetição sem comprimento anterior"))?;
                (prev, 3 + br.bits(2)? as usize)
            }
            17 => (0, 3 + br.bits(3)? as usize),
            _ => (0, 11 + br.bits(7)? as usize),
        };
        if i + repeat > lengths.len() {
            return Err(Error::Corrupt("comprimentos demais"));
        }
        lengths[i..i + repeat].fill(value);
        i += repeat;
    }
    if lengths[256] == 0 {
        return Err(Error::Corrupt("sem código de fim de bloco"));
    }
    Ok((
        Huffman::new(&lengths[..nlen])?,
        Huffman::new(&lengths[nlen..])?,
    ))
}

fn codes(br: &mut Bits<'_>, out: &mut Vec<u8>, lit: &Huffman, dist: &Huffman) -> Result<()> {
    loop {
        let sym = lit.decode(br)?;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => return Ok(()),
            257..=285 => {
                let i = (sym - 257) as usize;
                let len = LEN_BASE[i] as usize + br.bits(LEN_EXTRA[i] as u32)? as usize;
                let d = dist.decode(br)? as usize;
                if d >= 30 {
                    return Err(Error::Corrupt("código de distância"));
                }
                let distance = DIST_BASE[d] as usize + br.bits(DIST_EXTRA[d] as u32)? as usize;
                if distance > out.len() {
                    return Err(Error::Corrupt("distância antes do início"));
                }
                let from = out.len() - distance;
                if distance >= len {
                    out.extend_from_within(from..from + len);
                } else {
                    // Cópia que se sobrepõe a si mesma (repetição curta).
                    for k in 0..len {
                        out.push(out[from + k]);
                    }
                }
            }
            _ => return Err(Error::Corrupt("símbolo de comprimento")),
        }
    }
}
