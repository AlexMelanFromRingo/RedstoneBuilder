//! Packed-long encoder for the Litematica `BlockStates` array.
//!
//! Spanning layout (per `contracts/litematic-nbt.md`): indices are
//! concatenated as a contiguous LSB-first bit-stream. Each index occupies
//! `bits_per_index` consecutive bits and **may span** across two
//! adjacent `i64`s. The total long count is
//! `ceil(total_indices * bits_per_index / 64)`.

/// Pack `indices` (each in `0..2^bits_per_index`) into a `Vec<i64>` using
/// the spanning, LSB-first layout.
///
/// # Panics
///
/// Never panics (uses checked arithmetic throughout). Callers should
/// have already validated `bits_per_index <= 32` via
/// [`crate::palette::Palette::bits_per_index`].
pub fn pack_block_states(indices: &[u32], bits_per_index: u32) -> Vec<i64> {
    if indices.is_empty() {
        return Vec::new();
    }
    let bits_per_index = bits_per_index.max(2);

    let total_bits: u64 = (indices.len() as u64).saturating_mul(u64::from(bits_per_index));
    let total_longs = total_bits.div_ceil(64) as usize;
    let mut out = vec![0u64; total_longs];

    let mut bit_cursor: u64 = 0;
    for &idx in indices {
        let value = u64::from(idx) & ((1u64 << bits_per_index) - 1);

        let long_idx = (bit_cursor / 64) as usize;
        let within = (bit_cursor % 64) as u32;
        let space_in_long = 64u32 - within;

        if space_in_long >= bits_per_index {
            out[long_idx] |= value << within;
        } else {
            let low_bits = space_in_long;
            let low_mask = (1u64 << low_bits) - 1;
            out[long_idx] |= (value & low_mask) << within;

            let high_bits = bits_per_index - low_bits;
            let high_value = value >> low_bits;
            let high_mask = (1u64 << high_bits) - 1;
            if let Some(slot) = out.get_mut(long_idx + 1) {
                *slot |= high_value & high_mask;
            }
        }

        bit_cursor = bit_cursor.saturating_add(u64::from(bits_per_index));
    }

    out.into_iter().map(|u| u as i64).collect()
}

/// Inverse of [`pack_block_states`], used by the round-trip test.
pub fn unpack_block_states(longs: &[i64], bits_per_index: u32, count: usize) -> Vec<u32> {
    let bits_per_index = bits_per_index.max(2);
    let mut out = Vec::with_capacity(count);
    let mask: u64 = (1u64 << bits_per_index) - 1;
    let mut bit_cursor: u64 = 0;
    for _ in 0..count {
        let long_idx = (bit_cursor / 64) as usize;
        let within = (bit_cursor % 64) as u32;
        let space_in_long = 64u32 - within;

        let value: u64 = if space_in_long >= bits_per_index {
            ((longs.get(long_idx).copied().unwrap_or(0) as u64) >> within) & mask
        } else {
            let low_bits = space_in_long;
            let low_mask = (1u64 << low_bits) - 1;
            let low_part =
                ((longs.get(long_idx).copied().unwrap_or(0) as u64) >> within) & low_mask;
            let high_bits = bits_per_index - low_bits;
            let high_mask = (1u64 << high_bits) - 1;
            let high_part = (longs.get(long_idx + 1).copied().unwrap_or(0) as u64) & high_mask;
            low_part | (high_part << low_bits)
        };

        out.push(value as u32);
        bit_cursor = bit_cursor.saturating_add(u64::from(bits_per_index));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_round_trip_2bit() {
        let indices: Vec<u32> = vec![0, 1, 2, 3, 0, 1, 2, 3];
        let packed = pack_block_states(&indices, 2);
        let unpacked = unpack_block_states(&packed, 2, indices.len());
        assert_eq!(indices, unpacked);
    }

    #[test]
    fn pack_unpack_round_trip_5bit_spanning() {
        // 5 bits × 14 indices = 70 bits → spans 2 longs.
        let indices: Vec<u32> = (0..14).collect();
        let packed = pack_block_states(&indices, 5);
        assert_eq!(packed.len(), 2);
        let unpacked = unpack_block_states(&packed, 5, indices.len());
        assert_eq!(indices, unpacked);
    }

    #[test]
    fn pack_unpack_round_trip_larger_palette() {
        // 200 indices × 7 bits = 1400 bits → 22 longs.
        let indices: Vec<u32> = (0..200u32).map(|i| i % 100).collect();
        let packed = pack_block_states(&indices, 7);
        assert_eq!(packed.len(), 22);
        let unpacked = unpack_block_states(&packed, 7, indices.len());
        assert_eq!(indices, unpacked);
    }
}
