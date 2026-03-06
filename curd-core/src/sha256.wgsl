// SHA-256 constants
var<private> K: array<u32, 64> = array<u32, 64>(
    0x428a2f98u, 0x71374491u, 0xb5c0fbcfu, 0xe9b5dba5u,
    0x3956c25bu, 0x59f111f1u, 0x923f82a4u, 0xab1c5ed5u,
    0xd807aa98u, 0x12835b01u, 0x243185beu, 0x550c7dc3u,
    0x72be5d74u, 0x80deb1feu, 0x9bdc06a7u, 0xc19bf174u,
    0xe49b69c1u, 0xefbe4786u, 0x0fc19dc6u, 0x240ca1ccu,
    0x2de92c6fu, 0x4a7484aau, 0x5cb0a9dcu, 0x76f988dau,
    0x983e5152u, 0xa831c66du, 0xb00327c8u, 0xbf597fc7u,
    0xc6e00bf3u, 0xd5a79147u, 0x06ca6351u, 0x14292967u,
    0x27b70a85u, 0x2e1b2138u, 0x4d2c6dfcu, 0x53380d13u,
    0x650a7354u, 0x766a0abbu, 0x81c2c92eu, 0x92722c85u,
    0xa2bfe8a1u, 0xa81a664bu, 0xc24b8b70u, 0xc76c51a3u,
    0xd192e819u, 0xd6990624u, 0xf40e3585u, 0x106aa070u,
    0x19a4c116u, 0x1e376c08u, 0x2748774cu, 0x34b0bcb5u,
    0x391c0cb3u, 0x4ed8aa4au, 0x5b9cca4fu, 0x682e6ff3u,
    0x748f82eeu, 0x78a5636fu, 0x84c87814u, 0x8cc70208u,
    0x90befffau, 0xa4506cebu, 0xbef9a3f7u, 0xc67178f2u
);

struct Block {
    data: array<u32, 16>,
};

struct BatchInput {
    // Each string will be padded to a multiple of 64 bytes (16 u32s)
    // We pass the length in bytes of the original string, followed by the padded blocks
    lengths: array<u32>,
};

@group(0) @binding(0) var<storage, read> inputs: array<u32>;
@group(0) @binding(1) var<storage, read_write> outputs: array<u32>;
@group(0) @binding(2) var<uniform> num_items: u32;

fn right_rotate(val: u32, amount: u32) -> u32 {
    return (val >> amount) | (val << (32u - amount));
}

fn process_chunk(state: ptr<function, array<u32, 8>>, chunk: ptr<function, array<u32, 16>>) {
    var a = (*state)[0];
    var b = (*state)[1];
    var c = (*state)[2];
    var d = (*state)[3];
    var e = (*state)[4];
    var f = (*state)[5];
    var g = (*state)[6];
    var h = (*state)[7];

    var w: array<u32, 64>;
    for (var i = 0u; i < 16u; i = i + 1u) {
        // Swap endianness (WGSL uses little-endian, SHA-256 wants big-endian)
        let v = (*chunk)[i];
        w[i] = ((v & 0xFFu) << 24u) | ((v & 0xFF00u) << 8u) | ((v >> 8u) & 0xFF00u) | (v >> 24u);
    }
    for (var i = 16u; i < 64u; i = i + 1u) {
        let s0 = right_rotate(w[i - 15u], 7u) ^ right_rotate(w[i - 15u], 18u) ^ (w[i - 15u] >> 3u);
        let s1 = right_rotate(w[i - 2u], 17u) ^ right_rotate(w[i - 2u], 19u) ^ (w[i - 2u] >> 10u);
        w[i] = w[i - 16u] + s0 + w[i - 7u] + s1;
    }

    for (var i = 0u; i < 64u; i = i + 1u) {
        let s1 = right_rotate(e, 6u) ^ right_rotate(e, 11u) ^ right_rotate(e, 25u);
        let ch = (e & f) ^ (~e & g);
        let temp1 = h + s1 + ch + K[i] + w[i];
        let s0 = right_rotate(a, 2u) ^ right_rotate(a, 13u) ^ right_rotate(a, 22u);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0 + maj;

        h = g;
        g = f;
        f = e;
        e = d + temp1;
        d = c;
        c = b;
        b = a;
        a = temp1 + temp2;
    }

    (*state)[0] = (*state)[0] + a;
    (*state)[1] = (*state)[1] + b;
    (*state)[2] = (*state)[2] + c;
    (*state)[3] = (*state)[3] + d;
    (*state)[4] = (*state)[4] + e;
    (*state)[5] = (*state)[5] + f;
    (*state)[6] = (*state)[6] + g;
    (*state)[7] = (*state)[7] + h;
}

// Memory Layout for `inputs`:
// [0..N-1]: length of each string (N items)
// [N..]: padded message blocks, concatenated. Each padded message has a multiple of 16 u32s.
// Since WGSL cannot have a dynamically sized array of arrays easily, we must compute offsets.
// A helper block array is passed implicitly by packing. The CPU side computes offsets so the GPU
// can just jump to its segment.
// To make it fully GPU-parallel without sequential scans for offsets, CPU should pass:
// [0]: string length, [1]: offset_in_u32s, [2]: num_blocks_in_u32s
// But to save bandwidth, we'll arrange `inputs` as:
// [0..N-1]: offset to the start of the padded bytes for string `i`
// [N..2N-1]: length of the padded bytes in u32s for string `i`
// [2N..]: actual data

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    if (index >= num_items) {
        return;
    }

    // Read offset and length
    let offset = inputs[index];
    let blocks_count = inputs[num_items + index]; // number of u32s
    let num_chunks = blocks_count / 16u;

    // Initial hash values
    var state = array<u32, 8>(
        0x6a09e667u, 0xbb67ae85u, 0x3c6ef372u, 0xa54ff53au,
        0x510e527fu, 0x9b05688cu, 0x1f83d9abu, 0x5be0cd19u
    );

    let data_start = 2u * num_items + offset;

    for (var i = 0u; i < num_chunks; i = i + 1u) {
        var chunk: array<u32, 16>;
        for (var j = 0u; j < 16u; j = j + 1u) {
            chunk[j] = inputs[data_start + i * 16u + j];
        }
        process_chunk(&state, &chunk);
    }

    // Write output: 8 u32s per hash
    for (var i = 0u; i < 8u; i = i + 1u) {
        // Swap bytes back to little-endian for the output buffer since CPU reads it
        // Or CPU can do it. Let's let GPU output big-endian integers and let CPU convert 
        // them to hex string. Actually CPU usually wants bytes.
        let v = state[i];
        let swapped = ((v & 0xFFu) << 24u) | ((v & 0xFF00u) << 8u) | ((v >> 8u) & 0xFF00u) | (v >> 24u);
        outputs[index * 8u + i] = swapped;
    }
}
