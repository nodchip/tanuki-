const N: usize = 624;
const M: usize = 397;

/// Minimal CPython `random.Random` compatibility needed by opening-book tie breaks.
/// Integer seeding follows `_randommodule.c`'s little-endian 32-bit key path.
#[derive(Clone, Debug)]
pub struct PythonRandom {
    state: [u32; N],
    index: usize,
}

impl PythonRandom {
    pub fn seeded(seed: u64) -> Self {
        let key = [seed as u32, (seed >> 32) as u32];
        let key_len = if key[1] == 0 { 1 } else { 2 };
        let mut random = Self {
            state: [0; N],
            index: N,
        };
        random.init_by_array(&key[..key_len]);
        random
    }

    pub fn choice_index(&mut self, length: usize) -> Option<usize> {
        if length == 0 {
            return None;
        }
        let bits = usize::BITS - length.leading_zeros();
        loop {
            let value = self.getrandbits(bits) as usize;
            if value < length {
                return Some(value);
            }
        }
    }

    fn init_genrand(&mut self, seed: u32) {
        self.state[0] = seed;
        for index in 1..N {
            self.state[index] = 1_812_433_253u32
                .wrapping_mul(self.state[index - 1] ^ (self.state[index - 1] >> 30))
                .wrapping_add(index as u32);
        }
        self.index = N;
    }

    fn init_by_array(&mut self, key: &[u32]) {
        self.init_genrand(19_650_218);
        let mut i = 1usize;
        let mut j = 0usize;
        for _ in 0..N.max(key.len()) {
            self.state[i] = (self.state[i]
                ^ (self.state[i - 1] ^ (self.state[i - 1] >> 30)).wrapping_mul(1_664_525))
            .wrapping_add(key[j])
            .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                self.state[0] = self.state[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            self.state[i] = (self.state[i]
                ^ (self.state[i - 1] ^ (self.state[i - 1] >> 30)).wrapping_mul(1_566_083_941))
            .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                self.state[0] = self.state[N - 1];
                i = 1;
            }
        }
        self.state[0] = 0x8000_0000;
    }

    fn getrandbits(&mut self, bits: u32) -> u32 {
        debug_assert!((1..=32).contains(&bits));
        self.next_u32() >> (32 - bits)
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            for index in 0..N {
                let y =
                    (self.state[index] & 0x8000_0000) | (self.state[(index + 1) % N] & 0x7fff_ffff);
                self.state[index] = self.state[(index + M) % N]
                    ^ (y >> 1)
                    ^ if y & 1 == 0 { 0 } else { 0x9908_b0df };
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::PythonRandom;

    #[test]
    fn choice_sequence_matches_cpython_random_42() {
        let mut random = PythonRandom::seeded(42);
        let actual: Vec<_> = (0..10).map(|_| random.choice_index(3).unwrap()).collect();
        assert_eq!(actual, [2, 0, 0, 2, 1, 0, 0, 0, 2, 0]);
    }
}
