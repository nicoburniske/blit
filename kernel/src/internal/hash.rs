// adapted from rustc-hash 2.1.3, algorithm by Orson Peters

use std::hash::Hasher;

#[derive(Default)]
pub struct FxHasher {
    hash: usize,
}

impl FxHasher {
    pub const fn new() -> Self {
        Self { hash: 0 }
    }

    #[inline]
    pub const fn write(&mut self, bytes: &[u8]) {
        let len = bytes.len();
        let mut s0 = 0x243f6a8885a308d3_u64;
        let mut s1 = 0x13198a2e03707344_u64;

        if len <= 16 {
            if len >= 8 {
                s0 ^= u64::from_le_bytes(*bytes.first_chunk::<8>().unwrap());
                s1 ^= u64::from_le_bytes(*bytes.last_chunk::<8>().unwrap());
            } else if len >= 4 {
                s0 ^= u32::from_le_bytes(*bytes.first_chunk::<4>().unwrap()) as u64;
                s1 ^= u32::from_le_bytes(*bytes.last_chunk::<4>().unwrap()) as u64;
            } else if len > 0 {
                s0 ^= bytes[0] as u64;
                s1 ^= ((bytes[len - 1] as u64) << 8) | bytes[len / 2] as u64;
            }
        } else {
            let mut bulk = bytes.split_at(len - 1).0;
            while let Some((chunk, rest)) = bulk.split_first_chunk::<16>() {
                let x = u64::from_le_bytes(*chunk.first_chunk::<8>().unwrap());
                let y = u64::from_le_bytes(*chunk.last_chunk::<8>().unwrap());
                let mixed = multiply_mix(s0 ^ x, 0xa4093822299f31d0 ^ y);
                s0 = s1;
                s1 = mixed;
                bulk = rest;
            }
            let suffix = bytes.last_chunk::<16>().unwrap();
            s0 ^= u64::from_le_bytes(*suffix.first_chunk::<8>().unwrap());
            s1 ^= u64::from_le_bytes(*suffix.last_chunk::<8>().unwrap());
        }

        self.write_u64(multiply_mix(s0, s1) ^ len as u64);
    }

    #[inline]
    pub const fn write_usize(&mut self, value: usize) {
        self.add(value);
    }

    #[inline]
    pub const fn write_u64(&mut self, value: u64) {
        self.add(value as usize);
        #[cfg(target_pointer_width = "32")]
        self.add((value >> 32) as usize);
    }

    #[inline]
    pub const fn finish(&self) -> u64 {
        #[cfg(target_pointer_width = "64")]
        const ROTATE: u32 = 26;
        #[cfg(target_pointer_width = "32")]
        const ROTATE: u32 = 15;
        self.hash.rotate_left(ROTATE) as u64
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        Self::write(self, bytes);
    }

    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.add(value as usize);
    }

    #[inline]
    fn write_u16(&mut self, value: u16) {
        self.add(value as usize);
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.add(value as usize);
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        Self::write_u64(self, value);
    }

    #[inline]
    fn write_u128(&mut self, value: u128) {
        self.add(value as usize);
        #[cfg(target_pointer_width = "32")]
        self.add((value >> 32) as usize);
        self.add((value >> 64) as usize);
        #[cfg(target_pointer_width = "32")]
        self.add((value >> 96) as usize);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        Self::write_usize(self, value);
    }

    #[inline]
    fn finish(&self) -> u64 {
        Self::finish(self)
    }
}

impl FxHasher {
    #[inline]
    const fn add(&mut self, value: usize) {
        #[cfg(target_pointer_width = "64")]
        const K: usize = 0xf1357aea2e62a9c5;
        #[cfg(target_pointer_width = "32")]
        const K: usize = 0x93d765dd;
        self.hash = self.hash.wrapping_add(value).wrapping_mul(K);
    }
}

#[inline]
const fn multiply_mix(x: u64, y: u64) -> u64 {
    if cfg!(any(
        all(
            target_pointer_width = "64",
            not(any(target_arch = "sparc64", target_arch = "wasm64")),
        ),
        target_arch = "aarch64",
        target_arch = "x86_64",
        all(target_family = "wasm", target_feature = "wide-arithmetic"),
    )) {
        let product = (x as u128).wrapping_mul(y as u128);
        product as u64 ^ (product >> 64) as u64
    } else {
        let a = (x as u32 as u64).wrapping_mul(y >> 32);
        let b = (x >> 32).wrapping_mul(y as u32 as u64);
        a ^ b.rotate_right(32)
    }
}
