use crate::{KMER_MASK, KMER_SIZE};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SimdLevel {
    Scalar,
    Avx2,
    Avx512,
}
impl SimdLevel {
    pub fn available(self) -> bool {
        match self {
            Self::Scalar => true,
            #[cfg(target_arch = "x86_64")]
            Self::Avx2 => std::is_x86_feature_detected!("avx2"),
            #[cfg(not(target_arch = "x86_64"))]
            Self::Avx2 => false,
            #[cfg(target_arch = "x86_64")]
            Self::Avx512 => std::is_x86_feature_detected!("avx512bw"),
            #[cfg(not(target_arch = "x86_64"))]
            Self::Avx512 => false,
        }
    }
}
pub fn runtime() -> SimdLevel {
    if SimdLevel::Avx512.available() {
        SimdLevel::Avx512
    } else if SimdLevel::Avx2.available() {
        SimdLevel::Avx2
    } else {
        SimdLevel::Scalar
    }
}
#[cfg(target_arch = "x86_64")]
static LOWER: [u8; 32] = [32; 32];
pub fn encode_into(seq: &[u8], out: &mut Vec<u8>, level: SimdLevel) -> Result<(), &'static str> {
    out.resize(seq.len(), 4);
    let mut left = seq;
    let mut offset = 0;
    let mut encoded = &mut out[..];
    // The assembly helpers own their unaligned loads/stores, so slices need
    // no alignment. Do not use these intrinsics unless CPUID was checked.
    match level {
        SimdLevel::Avx512 => {
            #[cfg(target_arch = "x86_64")]
            if level.available() {
                while left.len() >= 64 {
                    unsafe { encode64_asm(left.as_ptr(), encoded.as_mut_ptr()) };
                    left = &left[64..];
                    encoded = &mut encoded[64..];
                    offset += 64;
                }
            } else {
                return Err("AVX-512BW is not available");
            }
            #[cfg(not(target_arch = "x86_64"))]
            return Err("AVX-512BW is unavailable on this architecture");
        }
        SimdLevel::Avx2 => {
            #[cfg(target_arch = "x86_64")]
            if level.available() {
                while left.len() >= 32 {
                    unsafe { encode32_asm(left.as_ptr(), encoded.as_mut_ptr()) };
                    left = &left[32..];
                    encoded = &mut encoded[32..];
                    offset += 32;
                }
            } else {
                return Err("AVX2 is not available");
            }
            #[cfg(not(target_arch = "x86_64"))]
            return Err("AVX2 is unavailable on this architecture");
        }
        SimdLevel::Scalar => {}
    }
    scalar_into(left, encoded);
    debug_assert_eq!(offset + left.len(), seq.len());
    Ok(())
}
pub fn encode(seq: &[u8], level: SimdLevel) -> Vec<Option<u8>> {
    let mut packed = Vec::new();
    encode_into(seq, &mut packed, level).expect("requested SIMD is unavailable");
    packed
        .into_iter()
        .map(|base| (base < 4).then_some(base))
        .collect()
}
pub fn scalar_into(seq: &[u8], out: &mut [u8]) {
    for (&base, out) in seq.iter().zip(out) {
        *out = encode_base(base);
    }
}
pub fn scalar(seq: &[u8]) -> Vec<Option<u8>> {
    seq.iter()
        .map(|&b| (encode_base(b) < 4).then_some(encode_base(b)))
        .collect()
}
#[inline]
pub const fn encode_base(base: u8) -> u8 {
    match base | 32 {
        b'a' => 0,
        b'c' => 1,
        b'g' => 2,
        b't' => 3,
        _ => 4,
    }
}
pub fn avx2(seq: &[u8]) -> Vec<Option<u8>> {
    encode(seq, SimdLevel::Avx2)
}
pub fn avx512(seq: &[u8]) -> Vec<Option<u8>> {
    encode(seq, SimdLevel::Avx512)
}

/// Assembly processes complete 32-byte blocks. Each comparison result is a
/// byte mask (0x00/0xff); OR masks to form 2-bit codes, and zeroing the
/// invalid-byte mask guarantees an exact sentinel of 4 for N/lowercase/other.
#[cfg(target_arch = "x86_64")]
unsafe fn encode32_asm(input: *const u8, output: *mut u8) {
    unsafe {
        std::arch::asm!(
            "vpxor xmm4, xmm4, xmm4",
            "vmovdqu ymm5, ymmword ptr [{ones}]",
            "vmovdqu ymm6, ymmword ptr [{twos}]",
            "vmovdqu ymm7, ymmword ptr [{fours}]",
            "vmovdqu ymm0, ymmword ptr [{input}]",
            "vpor ymm0, ymm0, ymmword ptr [{lower}]",
            "vpcmpeqb ymm1, ymm0, ymmword ptr [{c}]",
            "vpand ymm1, ymm1, ymm5",
            "vpcmpeqb ymm2, ymm0, ymmword ptr [{g}]",
            "vpand ymm2, ymm2, ymm6",
            "vpcmpeqb ymm3, ymm0, ymmword ptr [{t}]",
            "vpand ymm3, ymm3, ymmword ptr [{threes}]",
            "vpor ymm1, ymm1, ymm3",
            "vpor ymm1, ymm1, ymm2",
            "vpcmpeqb ymm2, ymm0, ymmword ptr [{a}]",
            "vpcmpeqb ymm3, ymm0, ymmword ptr [{c}]",
            "vpor ymm2, ymm2, ymm3",
            "vpcmpeqb ymm3, ymm0, ymmword ptr [{g}]",
            "vpor ymm2, ymm2, ymm3",
            "vpcmpeqb ymm3, ymm0, ymmword ptr [{t}]",
            "vpor ymm2, ymm2, ymm3",
            "vpandn ymm2, ymm2, ymm7",
            "vpor ymm0, ymm1, ymm2",
            "vmovdqu ymmword ptr [{output}], ymm0",
            "vzeroupper",
            input = in(reg) input,
            output = in(reg) output,
            lower = in(reg) LOWER.as_ptr(),
            a = in(reg) A.as_ptr(),
            c = in(reg) C.as_ptr(),
            g = in(reg) G.as_ptr(),
            t = in(reg) T.as_ptr(),
            ones = in(reg) ONES.as_ptr(),
            twos = in(reg) TWOS.as_ptr(),
            fours = in(reg) FOURS.as_ptr(),
            threes = in(reg) THREES.as_ptr(),
            out("ymm0") _, out("ymm1") _, out("ymm2") _, out("ymm3") _,
            out("ymm4") _, out("ymm5") _, out("ymm6") _, out("ymm7") _,
            options(nostack, preserves_flags),
        );
    }
}
#[cfg(target_arch = "x86_64")]
unsafe fn encode64_asm(input: *const u8, output: *mut u8) {
    unsafe {
        std::arch::asm!(
            "vpxord zmm3, zmm3, zmm3",
            "vmovdqu8 zmm4, zmmword ptr [{lower}]",
            "vpbroadcastb zmm5, byte ptr [{ones}]",
            "vpbroadcastb zmm6, byte ptr [{twos}]",
            "vpbroadcastb zmm7, byte ptr [{fours}]",
            "vpbroadcastb zmm3, byte ptr [{threes}]",
            "vpxord zmm8, zmm8, zmm8",
            "vmovdqu64 zmm0, zmmword ptr [{input}]",
            "vporq zmm0, zmm0, zmm4",
            "vpbroadcastb zmm1, byte ptr [{c}]",
            "vpcmpeqb k1, zmm0, zmm1",
            "vmovdqu8 zmm1 {{k1}} {{z}}, zmm5",
            "vpbroadcastb zmm2, byte ptr [{g}]",
            "vpcmpeqb k1, zmm0, zmm2",
            "vmovdqu8 zmm2 {{k1}} {{z}}, zmm6",
            "vporq zmm1, zmm1, zmm2",
            "vpbroadcastb zmm2, byte ptr [{t}]",
            "vpcmpeqb k1, zmm0, zmm2",
            "vmovdqu8 zmm2, zmm8",
            "vmovdqu8 zmm2 {{k1}}, zmm3",
            "vporq zmm1, zmm1, zmm2",
            "vpbroadcastb zmm2, byte ptr [{a}]",
            "vpcmpeqb k1, zmm0, zmm2",
            "vpbroadcastb zmm2, byte ptr [{c}]",
            "vpcmpeqb k2, zmm0, zmm2",
            "korq k1, k1, k2",
            "vpbroadcastb zmm2, byte ptr [{g}]",
            "vpcmpeqb k2, zmm0, zmm2",
            "korq k1, k1, k2",
            "vpbroadcastb zmm2, byte ptr [{t}]",
            "vpcmpeqb k2, zmm0, zmm2",
            "korq k1, k1, k2",
            "knotq k1, k1",
            "vmovdqu8 zmm2, zmm8",
            "vmovdqu8 zmm2 {{k1}}, zmm7",
            "vporq zmm0, zmm1, zmm2",
            "vmovdqu64 zmmword ptr [{output}], zmm0",
            "vzeroupper",
            input = in(reg) input,
            output = in(reg) output,
            lower = in(reg) LOWER64.as_ptr(),
            a = in(reg) A64.as_ptr(),
            c = in(reg) C64.as_ptr(),
            g = in(reg) G64.as_ptr(),
            t = in(reg) T64.as_ptr(),
            // Read-only byte broadcasts make explicit vector comparands; the
            // mask registers only select already initialized vector values.
            ones = in(reg) ONES64.as_ptr(),
            twos = in(reg) TWOS64.as_ptr(),
            fours = in(reg) FOURS64.as_ptr(),
            threes = in(reg) THREES64.as_ptr(),
            out("zmm0") _, out("zmm1") _, out("zmm2") _, out("zmm3") _, out("zmm4") _, out("zmm8") _,
            out("zmm5") _, out("zmm6") _, out("zmm7") _, out("k1") _, out("k2") _,
            options(nostack, preserves_flags),
        );
    }
}
/// Intrinsics twin of the AVX-512 assembly encoder. Keeping the entire
/// comparison in one helper lets tests compare output on identical byte slices.
#[cfg(all(test, target_arch = "x86_64"))]
#[target_feature(enable = "avx512f,avx512bw")]
unsafe fn encode64_intrinsics(input: *const u8, output: *mut u8) {
    use std::arch::x86_64::*;
    let bytes = unsafe { _mm512_loadu_si512(input.cast()) };
    let lower = _mm512_or_si512(bytes, _mm512_set1_epi8(32));
    let mut codes = _mm512_setzero_si512();
    for (base, code) in [(b'c', 1), (b'g', 2), (b't', 3)] {
        let matches = _mm512_cmpeq_epi8_mask(lower, _mm512_set1_epi8(base as i8));
        codes = _mm512_mask_mov_epi8(codes, matches, _mm512_set1_epi8(code));
    }
    let a = _mm512_cmpeq_epi8_mask(lower, _mm512_set1_epi8(b'a' as i8));
    let c = _mm512_cmpeq_epi8_mask(lower, _mm512_set1_epi8(b'c' as i8));
    let g = _mm512_cmpeq_epi8_mask(lower, _mm512_set1_epi8(b'g' as i8));
    let t = _mm512_cmpeq_epi8_mask(lower, _mm512_set1_epi8(b't' as i8));
    let invalid = !(a | c | g | t);
    codes = _mm512_mask_mov_epi8(codes, invalid, _mm512_set1_epi8(4));
    unsafe { _mm512_storeu_si512(output.cast(), codes) };
}
#[cfg(target_arch = "x86_64")]
static A: [u8; 32] = [b'a'; 32];
#[cfg(target_arch = "x86_64")]
static C: [u8; 32] = [b'c'; 32];
#[cfg(target_arch = "x86_64")]
static G: [u8; 32] = [b'g'; 32];
#[cfg(target_arch = "x86_64")]
static T: [u8; 32] = [b't'; 32];
#[cfg(target_arch = "x86_64")]
static ONES: [u8; 32] = [1; 32];
#[cfg(target_arch = "x86_64")]
static TWOS: [u8; 32] = [2; 32];
#[cfg(target_arch = "x86_64")]
static FOURS: [u8; 32] = [4; 32];
#[cfg(target_arch = "x86_64")]
static THREES: [u8; 32] = [3; 32];
#[cfg(target_arch = "x86_64")]
static LOWER64: [u8; 64] = [32; 64];
#[cfg(target_arch = "x86_64")]
static A64: [u8; 64] = [b'a'; 64];
#[cfg(target_arch = "x86_64")]
static C64: [u8; 64] = [b'c'; 64];
#[cfg(target_arch = "x86_64")]
static G64: [u8; 64] = [b'g'; 64];
#[cfg(target_arch = "x86_64")]
static T64: [u8; 64] = [b't'; 64];
#[cfg(target_arch = "x86_64")]
static ONES64: [u8; 64] = [1; 64];
#[cfg(target_arch = "x86_64")]
static TWOS64: [u8; 64] = [2; 64];
#[cfg(target_arch = "x86_64")]
static FOURS64: [u8; 64] = [4; 64];
#[cfg(target_arch = "x86_64")]
static THREES64: [u8; 64] = [3; 64];

/// Streaming canonical 31-mer encoding. Invalid windows are skipped without
/// clearing the rolling state at every length-31 boundary.
pub struct KmerEncoder {
    level: SimdLevel,
    encoded: Vec<u8>,
    forward: u64,
    reverse: u64,
    run: usize,
    emitted: usize,
}
impl KmerEncoder {
    pub fn new(level: SimdLevel) -> Self {
        Self {
            level,
            encoded: Vec::new(),
            forward: 0,
            reverse: 0,
            run: 0,
            emitted: 0,
        }
    }
    pub fn kmer_keys_into(&mut self, seq: &[u8], out: &mut Vec<KmerKey>) {
        out.clear();
        self.forward = 0;
        self.reverse = 0;
        self.run = 0;
        if !self.level.available() {
            // Caller selects the scalar implementation, not an unsupported
            // instruction path. No illegal instruction can reach here.
            scalar_keys(seq, out);
            return;
        }
        encode_into(seq, &mut self.encoded, self.level).expect("runtime SIMD check");
        for (index, &base) in self.encoded.iter().enumerate() {
            if base >= 4 {
                self.forward = 0;
                self.reverse = 0;
                self.run = 0;
                continue;
            }
            self.forward = ((self.forward << 2) | u64::from(base)) & KMER_MASK;
            self.reverse = (self.reverse >> 2) | (u64::from(3 - base) << 60);
            self.run += 1;
            if self.run < KMER_SIZE {
                continue;
            }
            let start = index + 1 - KMER_SIZE;
            out.push(KmerKey {
                canonical: self.forward.min(self.reverse),
                forward: self.forward <= self.reverse,
                start,
            });
            self.emitted += 1;
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KmerKey {
    pub canonical: u64,
    pub forward: bool,
    pub start: usize,
}
pub fn scalar_keys(seq: &[u8], out: &mut Vec<KmerKey>) {
    out.clear();
    let (mut forward, mut reverse, mut run) = (0, 0, 0);
    for (index, &byte) in seq.iter().enumerate() {
        let base = encode_base(byte);
        if base >= 4 {
            forward = 0;
            reverse = 0;
            run = 0;
            continue;
        }
        forward = ((forward << 2) | u64::from(base)) & KMER_MASK;
        reverse = (reverse >> 2) | (u64::from(3 - base) << 60);
        run += 1;
        if run >= KMER_SIZE {
            out.push(KmerKey {
                canonical: forward.min(reverse),
                forward: forward <= reverse,
                start: index + 1 - KMER_SIZE,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_boundaries_and_arbitrary_bytes_match_scalar() {
        let mut expected = Vec::new();
        for length in 0..257 {
            let seq: Vec<_> = (0..length)
                .map(|i| {
                    if i % 67 < 2 {
                        b"Nn"[i % 2]
                    } else {
                        b"ACGTacgt"[(i * 7 + i / 17) % 8]
                    }
                })
                .collect();
            encode_into(&seq, &mut expected, SimdLevel::Scalar).unwrap();
            let mut reference = Vec::new();
            scalar_keys(&seq, &mut reference);
            for level in [SimdLevel::Avx2, SimdLevel::Avx512] {
                if !level.available() {
                    continue;
                }
                let mut actual = Vec::new();
                encode_into(&seq, &mut actual, level).unwrap();
                assert_eq!(actual, expected, "length {length}, {level:?}");
                // Unaligned slices are explicitly supported, not accidental.
                if !seq.is_empty() {
                    let mut actual = Vec::new();
                    encode_into(&seq[1..], &mut actual, level).unwrap();
                    assert_eq!(actual, expected[1..]);
                }
                let mut keys = Vec::new();
                KmerEncoder::new(level).kmer_keys_into(&seq, &mut keys);
                assert_eq!(keys, reference, "keys length {length}, {level:?}");
                #[cfg(target_arch = "x86_64")]
                if length >= 64 && level == SimdLevel::Avx512 {
                    let mut twin = vec![0u8; 64];
                    unsafe { encode64_intrinsics(seq.as_ptr(), twin.as_mut_ptr()) };
                    assert_eq!(twin, actual[..64], "intrinsics length {length}");
                    unsafe { encode64_asm(seq.as_ptr(), twin.as_mut_ptr()) };
                    assert_eq!(twin, actual[..64], "assembly length {length}");
                }
            }
            if length >= 34 {
                assert!(!reference.is_empty(), "test must cover valid 31-mers");
            }
        }
    }
    #[test]
    fn pseudorandom_invalid_bytes_and_unaligned_storage_match() {
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut padded = vec![0xa5u8; 32]
            .into_iter()
            .chain((0..4097).map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (state >> 56) as u8
            }))
            .collect::<Vec<_>>();
        for (i, base) in padded[96..].iter_mut().enumerate() {
            *base = if i % 73 == 0 {
                b'N'
            } else {
                b"ACGT"[(i * 7 + i / 13) % 4]
            };
        }
        let mut expected = Vec::new();
        encode_into(&padded, &mut expected, SimdLevel::Scalar).unwrap();
        let mut reference = Vec::new();
        scalar_keys(&padded, &mut reference);
        for level in [SimdLevel::Avx2, SimdLevel::Avx512] {
            if !level.available() {
                continue;
            }
            for offset in 0..3 {
                let seq = &padded[offset..];
                encode_into(seq, &mut Vec::new(), level).unwrap();
                let mut output = Vec::new();
                encode_into(seq, &mut output, level).unwrap();
                assert_eq!(output, expected[offset..]);
                let mut encoder = KmerEncoder::new(level);
                let mut keys = Vec::new();
                encoder.kmer_keys_into(seq, &mut keys);
                let mut expected_keys = Vec::new();
                scalar_keys(seq, &mut expected_keys);
                assert_eq!(keys, expected_keys);
            }
        }
        assert!(reference.len() > 100);
    }
    #[test]
    fn multiple_reads_and_n_boundaries_match_scalar() {
        let mut scalar_encoder = KmerEncoder::new(SimdLevel::Scalar);
        let mut avx_encoder = KmerEncoder::new(SimdLevel::Avx512);
        let reads = [
            &b"ACGTACGTACGTACGTACGTACGTACGTACG"[..],
            b"NNNNNNN",
            b"ACGTNNNNACGTACGTACGTACGTACGTACGTACGTACGTACG",
            b"CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
        ];
        for seq in reads {
            let mut reference = Vec::new();
            let mut actual = Vec::new();
            scalar_encoder.kmer_keys_into(seq, &mut reference);
            avx_encoder.kmer_keys_into(seq, &mut actual);
            assert_eq!(actual, reference);
        }
    }
    #[test]
    fn known_encoding_and_canonical_key() {
        let mut encoded = Vec::new();
        encode_into(b"ACGTNacgtn!", &mut encoded, SimdLevel::Scalar).unwrap();
        assert_eq!(encoded, [0, 1, 2, 3, 4, 0, 1, 2, 3, 4, 4]);
        let text = b"ACGTACGTACGTACGTACGTACGTACGTACG";
        let mut keys = Vec::new();
        scalar_keys(text, &mut keys);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].canonical, crate::kmer(text).unwrap());
        assert_eq!(keys[0].start, 0);
    }
}
