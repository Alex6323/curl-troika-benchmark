/*
 * Copyright (c) 2019 c-mnd
 *
 * MIT License
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

use std::fmt;

use super::constants::*;
use super::luts::*;
use super::types::{
    Trit,
    T27,
};

/// Troika with maximum number of rounds.
pub fn ftroika(out_buf: &mut [Trit], in_buf: &[Trit]) {
    ftroika_var_rounds(out_buf, in_buf, NUM_MAX_ROUNDS);
}

/// Troika with variable number of rounds.
pub fn ftroika_var_rounds(out_buf: &mut [Trit], in_buf: &[Trit], num_rounds: usize) {
    assert!(num_rounds <= NUM_MAX_ROUNDS);

    let mut ftroika = Ftroika::new(num_rounds);
    ftroika.absorb(in_buf);
    ftroika.finalize();
    ftroika.squeeze(out_buf);
}

/// The Ftroika struct is a Sponge that uses the Troika
/// hashing algorithm.
///
/// ```rust
/// use curl_troika_benchmark::troika_f::ftroika::Ftroika;
///
/// // Create an array of 243 1s
/// let input = [1; 243];
///
/// // Create an array of 243 0s
/// let mut out = [0; 243];
/// let mut ftroika = Ftroika::default();
///
/// ftroika.absorb(&input);
/// ftroika.finalize();
/// ftroika.squeeze(&mut out);
/// ```
#[derive(Clone, Copy)]
pub struct Ftroika {
    num_rounds: usize,
    idx: usize,
    rowcol: usize,
    slice: usize,
    state: [T27; SLICESIZE],
}

impl Default for Ftroika {
    fn default() -> Ftroika {
        Ftroika {
            num_rounds: NUM_MAX_ROUNDS,
            idx: 0,
            rowcol: 0,
            slice: 0,
            state: [T27::zero(); SLICESIZE],
        }
    }
}

impl fmt::Debug for Ftroika {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Ftroika: [rounds: [{}], state: {:?}",
            self.num_rounds,
            self.state.to_vec(),
        )
    }
}

impl Ftroika {
    /// Creates a new Ftroika.
    pub fn new(num_rounds: usize) -> Self {
        assert!(num_rounds <= NUM_MAX_ROUNDS);

        Ftroika { num_rounds, ..Default::default() }
    }

    /// Absorb trits.
    pub fn absorb(&mut self, trits: &[Trit]) {
        let mut length = trits.len();
        let mut space = 0;
        let mut trit_idx = 0;
        while length > 0 {
            if self.idx == 0 {
                self.nullify_rate();
            }
            space = TROIKA_RATE - self.idx;
            if length < space {
                space = length;
            }
            for _ in 0..space {
                self.set(trits[trit_idx]);
                self.idx += 1;
                self.rowcol += 1;
                trit_idx += 1;
                if self.rowcol == SLICESIZE {
                    self.rowcol = 0;
                    self.slice += 1;
                }
            }
            length -= space;
            if self.idx == TROIKA_RATE {
                self.permutation();
                self.idx = 0;
                self.rowcol = 0;
                self.slice = 0;
            }
        }
    }

    pub fn finalize(&mut self) {
        let pad: [Trit; 1] = [1];
        self.absorb(&pad);
        if self.idx != 0 {
            self.permutation();
            self.reset_counters();
        }
    }

    /// Resets FTroika state.
    pub fn reset(&mut self) {
        self.state = [T27::zero(); SLICESIZE];
        self.reset_counters();
    }

    pub fn squeeze(&mut self, trits: &mut [Trit]) {
        let mut length = trits.len();
        let mut space = 0;
        let mut trit_idx = 0;
        while length > 0 {
            space = TROIKA_RATE - self.idx;
            if length < space {
                space = length;
            }
            for _ in 0..space {
                trits[trit_idx] = self.get();
                self.idx += 1;
                self.rowcol += 1;
                trit_idx += 1;
                if self.rowcol == SLICESIZE {
                    self.rowcol = 0;
                    self.slice += 1;
                }
            }
            //trit_idx += space;
            length -= space;
            if self.idx == TROIKA_RATE {
                self.permutation();
                self.reset_counters();
            }
        }
    }

    fn state(&self) -> &[T27] {
        &self.state
    }

    fn reset_counters(&mut self) {
        self.idx = 0;
        self.rowcol = 0;
        self.slice = 0;
    }

    fn set(&mut self, trit: Trit) {
        self.state[self.rowcol].set(self.slice, trit);
    }

    fn get(&mut self) -> Trit {
        self.state[self.rowcol].get(self.slice)
    }

    fn nullify_rate(&mut self) {
        let mask = 0x07fffe00u32;
        for i in 0..SLICESIZE {
            self.state[i].p &= mask;
            self.state[i].n &= mask;
        }
    }

    fn permutation(&mut self) {
        for round in 0..self.num_rounds {
            self.sub_trytes();
            self.shift_rows();
            self.shift_lanes();
            self.add_column_parity();
            self.add_round_constant(round);
        }
    }

    fn sub_tryte(a: &mut [T27]) {
        let d = a[0].dec();
        let e = d.mul(&a[1]).add(&a[2]);
        let f = e.mul(&a[1]).add(&d);
        let g = e.mul(&f).add(&a[1]);
        a[2] = e.clean();
        a[1] = f.clean();
        a[0] = g.clean();
    }

    fn sub_trytes(&mut self) {
        for rowcol in (0..SLICESIZE).step_by(3) {
            Ftroika::sub_tryte(&mut self.state[rowcol..(rowcol + 3)]);
        }
    }

    fn shift_rows(&mut self) {
        const shifts: [u8; 27] = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 13, 14, 15, 16, 17, 9, 10, 11, 24, 25, 26, 18,
            19, 20, 21, 22, 23,
        ];
        let mut new_state = [T27::zero(); SLICESIZE];
        for i in 0..SLICESIZE {
            new_state[shifts[i] as usize] = self.state[i];
        }
        self.state = new_state;
    }

    fn shift_lanes(&mut self) {
        const shifts: [u8; 27] = [
            19, 13, 21, 10, 24, 15, 2, 9, 3, 14, 0, 6, 5, 1, 25, 22, 23, 20, 7, 17, 26,
            12, 8, 18, 16, 11, 4,
        ];
        let mut new_state = [T27::zero(); SLICESIZE];
        for i in 0..SLICESIZE {
            new_state[i as usize] = self.state[i].roll(shifts[i] as usize);
        }
        self.state = new_state;
    }

    fn add_column_parity(&mut self) {
        let mut parity = [T27::zero(); COLUMNS];
        for col in 0..COLUMNS {
            let mut col_sum = T27::zero();
            for row in 0..ROWS {
                col_sum = col_sum.add(&self.state[COLUMNS * row + col]);
            }
            parity[col] = col_sum;
        }
        for row in 0..ROWS {
            for col in 0..COLUMNS {
                let idx = COLUMNS * row + col;
                let t1 = parity[if col == 0 { COLUMNS - 1 } else { col - 1 }];
                let t2 =
                    parity[if col == COLUMNS - 1 { 0 } else { col + 1 }].roll(SLICES - 1);
                let sum_to_add = t1.add(&t2);
                self.state[idx] = self.state[idx].add(&sum_to_add);
            }
        }
    }

    fn add_round_constant(&mut self, round: usize) {
        for col in 0..COLUMNS {
            let round_const = T27::new(
                FROUND_CONSTANTS[round][col][0],
                FROUND_CONSTANTS[round][col][1],
            );

            self.state[col] = self.state[col].add(&round_const);
        }
    }
}

#[cfg(test)]
mod test_ftroika {
    use super::*;
    use bytes::BytesMut;

    const HASH: [u8; 243] = [
        0, 2, 2, 1, 2, 1, 0, 1, 2, 1, 1, 1, 1, 2, 2, 1, 1, 1, 0, 1, 2, 1, 2, 1, 2, 1, 2,
        1, 2, 2, 1, 1, 1, 0, 1, 0, 2, 1, 0, 0, 0, 1, 2, 0, 2, 1, 0, 0, 2, 1, 1, 1, 1, 1,
        2, 0, 1, 0, 2, 1, 1, 2, 0, 1, 1, 1, 1, 1, 2, 2, 0, 0, 2, 2, 2, 2, 0, 0, 2, 2, 2,
        1, 2, 2, 0, 2, 1, 1, 2, 1, 1, 1, 2, 2, 1, 1, 0, 0, 0, 2, 2, 2, 0, 2, 1, 1, 1, 1,
        0, 0, 1, 0, 2, 0, 2, 0, 2, 0, 0, 0, 0, 1, 1, 1, 0, 2, 1, 1, 1, 0, 2, 0, 0, 1, 0,
        1, 0, 2, 0, 2, 2, 0, 0, 2, 2, 0, 1, 2, 1, 0, 0, 1, 2, 1, 1, 0, 0, 1, 1, 0, 2, 1,
        1, 0, 1, 2, 0, 0, 0, 1, 2, 2, 1, 1, 1, 0, 0, 2, 0, 1, 1, 2, 1, 1, 2, 1, 0, 1, 2,
        2, 2, 2, 1, 2, 0, 2, 2, 1, 2, 1, 2, 1, 2, 2, 1, 1, 2, 0, 2, 1, 0, 1, 1, 1, 0, 2,
        2, 0, 0, 2, 0, 2, 0, 1, 2, 0, 0, 2, 2, 1, 1, 2, 0, 1, 0, 0, 0, 0, 2, 0, 2, 2, 2,
    ];

    #[test]
    fn test_hash() {
        let mut ftroika = Ftroika::default();
        let mut output = [0u8; 243];
        let input = [0u8; 243];
        ftroika.absorb(&input);
        ftroika.finalize();
        ftroika.squeeze(&mut output);

        assert!(
            output.iter().zip(HASH.iter()).all(|(a, b)| a == b),
            "Arrays are not equal"
        );
    }

    const VECTOR1_HASH: [u8; 243] = [
        0, 0, 2, 0, 0, 0, 2, 0, 2, 1, 0, 2, 2, 2, 0, 2, 0, 1, 0, 0, 1, 2, 2, 0, 1, 1, 1,
        0, 0, 1, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 2, 2, 2, 1, 1, 2, 2, 1, 1, 0, 2, 1,
        1, 0, 0, 2, 1, 1, 0, 1, 2, 0, 2, 1, 0, 1, 1, 0, 1, 1, 0, 1, 2, 0, 1, 0, 1, 2, 0,
        1, 2, 1, 0, 2, 0, 2, 0, 1, 0, 1, 1, 1, 0, 0, 2, 2, 1, 1, 1, 0, 2, 0, 2, 2, 1, 2,
        0, 0, 1, 2, 2, 2, 1, 0, 2, 0, 2, 0, 2, 1, 0, 0, 2, 0, 0, 0, 2, 0, 1, 2, 2, 0, 0,
        2, 1, 1, 2, 2, 0, 0, 2, 1, 2, 0, 2, 0, 0, 1, 2, 0, 0, 1, 0, 1, 0, 2, 0, 1, 2, 2,
        1, 2, 0, 0, 0, 1, 0, 1, 1, 2, 0, 1, 0, 1, 0, 2, 1, 1, 2, 0, 0, 2, 1, 0, 0, 2, 1,
        0, 2, 0, 0, 0, 0, 0, 2, 1, 0, 0, 1, 2, 0, 2, 0, 0, 1, 1, 2, 2, 0, 0, 2, 2, 1, 0,
        2, 2, 1, 1, 1, 0, 0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 1, 2, 1, 2, 2, 2, 2, 0, 0, 0, 2,
    ];

    const VECTOR2_HASH: [u8; 243] = [
        2, 0, 2, 0, 0, 2, 1, 1, 1, 1, 1, 0, 1, 2, 0, 0, 1, 1, 1, 0, 1, 2, 2, 1, 2, 2, 2,
        1, 2, 0, 0, 2, 2, 1, 1, 1, 0, 1, 2, 2, 0, 1, 2, 0, 2, 1, 2, 1, 2, 1, 2, 0, 1, 0,
        0, 0, 0, 0, 1, 0, 2, 0, 2, 0, 2, 1, 2, 2, 2, 0, 1, 0, 2, 1, 2, 1, 2, 1, 2, 1, 0,
        2, 1, 0, 2, 0, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 0, 1, 0, 0, 0, 2, 1, 0, 0, 1, 2, 1,
        1, 1, 0, 0, 0, 1, 1, 2, 1, 2, 1, 2, 0, 0, 0, 2, 2, 2, 1, 2, 1, 2, 0, 2, 0, 0, 2,
        2, 1, 0, 0, 0, 2, 2, 2, 0, 2, 2, 0, 2, 2, 2, 2, 1, 0, 0, 2, 2, 1, 0, 1, 2, 1, 1,
        2, 0, 0, 1, 1, 1, 2, 1, 2, 1, 0, 2, 2, 0, 1, 1, 2, 0, 2, 2, 1, 1, 0, 2, 1, 1, 2,
        0, 2, 0, 0, 1, 1, 1, 0, 2, 0, 0, 0, 0, 2, 1, 0, 1, 2, 2, 1, 1, 0, 2, 2, 2, 1, 1,
        0, 0, 2, 1, 1, 2, 2, 0, 0, 2, 1, 2, 0, 1, 2, 2, 1, 1, 2, 0, 2, 2, 1, 2, 1, 1, 1,
    ];

    const VECTOR3_HASH: [u8; 243] = [
        1, 2, 0, 2, 2, 0, 1, 2, 1, 2, 1, 2, 0, 2, 0, 2, 1, 1, 0, 1, 2, 2, 0, 2, 2, 2, 1,
        1, 2, 1, 2, 1, 2, 2, 2, 1, 2, 1, 1, 0, 2, 2, 1, 1, 2, 2, 2, 2, 2, 0, 1, 2, 1, 2,
        0, 0, 1, 2, 2, 1, 0, 1, 1, 2, 0, 2, 2, 1, 1, 0, 2, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0,
        1, 0, 0, 0, 1, 2, 0, 2, 1, 2, 2, 2, 0, 1, 1, 2, 1, 1, 1, 1, 1, 2, 0, 2, 2, 1, 0,
        1, 0, 2, 2, 0, 2, 2, 1, 1, 1, 2, 0, 1, 0, 2, 2, 1, 1, 2, 2, 2, 0, 0, 0, 0, 0, 2,
        2, 1, 0, 2, 0, 2, 1, 2, 1, 0, 0, 1, 2, 2, 1, 0, 1, 0, 0, 2, 2, 0, 0, 1, 1, 0, 1,
        0, 2, 1, 0, 1, 0, 0, 0, 0, 0, 2, 1, 2, 2, 1, 0, 1, 1, 2, 2, 0, 0, 0, 2, 1, 0, 0,
        0, 1, 2, 2, 2, 1, 0, 2, 0, 0, 1, 0, 1, 1, 2, 0, 0, 1, 2, 2, 2, 0, 2, 0, 1, 1, 2,
        1, 0, 0, 2, 1, 1, 0, 2, 0, 2, 2, 1, 1, 2, 1, 1, 0, 1, 1, 0, 1, 1, 0, 2, 2, 1, 2,
    ];

    #[test]
    fn vector1_test() {
        let trits = [0u8; 243];
        let mut trits = BytesMut::from(&trits[..]);
        ftroika(&mut trits, &[0]);

        assert_eq!(VECTOR1_HASH.len(), trits.len());
        (0..trits.len()).for_each(|i| assert_eq!(trits[i], VECTOR1_HASH[i]));
    }

    #[test]
    fn vector2_test() {
        let trits = [0u8; 243];
        let mut trits = BytesMut::from(&trits[..]);
        let input = &[0, 0];
        ftroika(&mut trits, input);

        assert_eq!(VECTOR2_HASH.len(), trits.len());
        (0..trits.len()).for_each(|i| assert_eq!(trits[i], VECTOR2_HASH[i]));
    }

    #[test]
    fn vector3_test() {
        let trits = [0u8; 243];
        let mut trits = BytesMut::from(&trits[..]);
        let input = &[
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 2,
        ];
        ftroika(&mut trits, input);

        assert_eq!(VECTOR3_HASH.len(), trits.len());
        (0..trits.len()).for_each(|i| assert_eq!(trits[i], VECTOR3_HASH[i]));
    }
}
