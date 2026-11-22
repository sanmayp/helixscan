struct Config {
    slots: u32,
    reads: u32,
    windows: u32,
    hit_capacity: u32,
    hash_bits: u32,
    no_bitmap: u32, // Used only by the exact-table development test.
}
@group(0) @binding(0) var<uniform> config: Config;
// Two-bit bases occupy bits 0..31 of each packed word (16 bases).
@group(0) @binding(1) var<storage, read> reads: array<u32>;
@group(0) @binding(2) var<storage, read> read_info: array<vec4<u32>>;
// One invalid-base/tail mask in each low-u16 word, parallel to packed reads.
@group(0) @binding(3) var<storage, read> invalid_masks: array<u32>;
@group(0) @binding(4) var<storage, read> bitmap: array<u32>;
@group(0) @binding(5) var<storage, read> keys: array<vec2<u32>>;
@group(0) @binding(6) var<storage, read> hits: array<array<u32,4>>;
@group(0) @binding(7) var<storage, read_write> output: array<u32>;
@group(0) @binding(8) var<storage, read_write> output_count: atomic<u32>;

fn canonical(forward: vec2<u32>, reverse: vec2<u32>) -> vec2<u32> {
    if forward.x < reverse.x || (forward.x == reverse.x && forward.y <= reverse.y) {
        return forward;
    }
    return reverse;
}

// Low 64 bits of a multiplication, explicitly assembled from 16-bit limbs.
fn mul64_golden(key: vec2<u32>) -> vec2<u32> {
    let a0 = key.y & 65535u;
    let a1 = key.y >> 16u;
    let a2 = key.x & 65535u;
    let a3 = key.x >> 16u;
    let r0 = a0 * 0x7c15u;
    let r1 = a1 * 0x7c15u + a0 * 0x7f4au + (r0 >> 16u);
    let r2 = a2 * 0x7c15u + a1 * 0x7f4au + a0 * 0x79b9u + (r1 >> 16u);
    let r3 = a3 * 0x7c15u + a2 * 0x7f4au + a1 * 0x79b9u + a0 * 0x9e37u + (r2 >> 16u);
    return vec2<u32>(((r3 & 65535u) << 16u) | (r2 & 65535u),
        ((r1 & 65535u) << 16u) | (r0 & 65535u));
}

fn append(slot: u32, read_id: u32, start: u32, forward_first: bool) {
    let packed_hits = hits[slot];
    for (var i = 0u; i < 4u; i++) {
        // An all-ones sentinel denotes padding. Real packing keeps bit 31
        // clear (strand is bit 30), including a legitimate all-zero hit.
        if packed_hits[i] == 0xffffffffu { continue; }
        let hit = packed_hits[i] & 0x3fffffffu;
        if select(hit, hit | 0x40000000u, !forward_first) != packed_hits[i] { continue; }
        let count = atomicAdd(&output_count, 1u);
        if count >= config.hit_capacity { return; }
        let offset = count * 8u;
        output[offset] = read_id;
        output[offset + 1u] = start;
        output[offset + 2u] = packed_hits[i];
        output[offset + 3u] = 0u;
        output[offset + 4u] = u32(forward_first);
    }
}

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) global: vec3<u32>) {
    let read_id = global.x / config.windows;
    let window = global.x % config.windows;
    if read_id >= config.reads { return; }
    let info = read_info[read_id];
    let first = window * 64u;
    if first + 30u >= info.y { return; }
    let last = min(info.y - 31u, first + 63u);
    var forward = vec2<u32>(0u,0u);
    var reverse = vec2<u32>(0u,0u);
    var run = 0u;
    for (var i=0u; i<94u; i++) {
        let pos = first + i;
        if pos >= info.y { break; }
        let word_index = info.x + pos / 16u;
        let word = reads[word_index];
        let shift = pos % 16u;
        if ((invalid_masks[word_index] & (1u << shift)) != 0u) {
            forward = vec2<u32>(0u,0u);
            reverse = vec2<u32>(0u,0u);
            run = 0u;
            continue;
        }
        let b = (word >> (2u * shift)) & 3u;
        forward = vec2<u32>(((forward.x << 2u) | (forward.y >> 30u)) & 0x3fffffffu,
            (forward.y << 2u) | b);
        reverse = vec2<u32>((reverse.x >> 2u) | ((3u-b) << 28u),
            (reverse.y >> 2u) | ((reverse.x & 3u) << 30u));
        run++;
        if run >= 31u && pos >= first + 30u && pos <= last + 30u {
            let key = canonical(forward, reverse);
            let h = mul64_golden(key).x >> 11u;
            let present = bitmap[h >> 5u] & (1u << (h & 31u));
            if config.no_bitmap == 0u && (h >= config.hash_bits || present == 0u) { continue; }
            var slot = h & (config.slots - 1u);
            for (var probe=0u; probe<config.slots; probe++) {
                let stored = keys[slot];
                if stored.x == 0u && stored.y == 0u { break; }
                if stored.x == key.x && stored.y == key.y {
                    var is_forward = true;
                    if forward.x > reverse.x || (forward.x == reverse.x && forward.y > reverse.y) {
                        is_forward = false;
                    }
                    append(slot, read_id, pos - 30u, is_forward);
                    break;
                }
                slot = (slot + 1u) & (config.slots - 1u);
            }
        }
        if pos >= last + 30u { break; }
    }
}
