use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SimdLevel {
    Scalar,
    Avx2,
    Avx512,
}
pub fn runtime() -> SimdLevel {
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx512bw") {
            return SimdLevel::Avx512;
        }
        if std::is_x86_feature_detected!("avx2") {
            return SimdLevel::Avx2;
        }
    }
    SimdLevel::Scalar
}
pub fn encode(seq: &[u8], level: SimdLevel) -> Vec<Option<u8>> {
    match level {
        SimdLevel::Scalar => scalar(seq),
        SimdLevel::Avx2 => avx2(seq),
        SimdLevel::Avx512 => avx512(seq),
    }
}
pub fn scalar(seq: &[u8]) -> Vec<Option<u8>> {
    seq.iter()
        .map(|b| match b | 32 {
            b'a' => Some(0),
            b'c' => Some(1),
            b'g' => Some(2),
            b't' => Some(3),
            _ => None,
        })
        .collect()
}
pub fn avx2(seq: &[u8]) -> Vec<Option<u8>> {
    scalar(seq)
}
pub fn avx512(seq: &[u8]) -> Vec<Option<u8>> {
    scalar(seq)
}
