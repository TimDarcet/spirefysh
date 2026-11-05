#include <metal_stdlib>
using namespace metal;

// Ragged multi-head self-attention over packed sequences, 4 heads of width 32, bf16 in/out.
// qkv rows are [Q | K | V] (3 x 128 columns); a threadgroup of 4 simdgroups handles one
// (32-row chunk, head) and each simdgroup owns 8 of its rows. The other side of the attention
// is streamed through threadgroup memory 32 rows at a time.

#pragma METAL internals : enable
template <typename T> struct tile {
  vec<T, 64> t;
  thread vec<T, 2> &v() { return *reinterpret_cast<thread vec<T, 2> *>(&t); }
  template <typename U, typename V> void mma(tile<U> a, tile<V> b) {
    t = __metal_simdgroup_matrix_8x8_multiply_accumulate(a.t, b.t, t, vec<T, 64>());
  }
};
#pragma METAL internals : disable

constant float SCALE = 0.17677669f;
constant float SCALE_LOG2 = 0.25503486f;
constant float MASK = -1e30f;

// Lane layout of an 8x8 tile: each lane holds row y, columns x and x + 1.
METAL_FUNC ushort2 lane_position(ushort lane) {
  ushort quad = lane / 4;
  return ushort2((quad & 2) * 2 + lane % 2 * 2, quad / 4 * 4 + lane / 2 % 4);
}

template <typename T> METAL_FUNC tile<T> load_pair(const device bfloat *source) {
  tile<T> value;
  value.v() = vec<T, 2>(bfloat2(*(const device packed_bfloat2 *)source));
  return value;
}

// 8x8 block (row block r, column block c) of a 32x32 threadgroup array, or of its transpose.
METAL_FUNC tile<bfloat> block(const threadgroup bfloat *source, ushort r, ushort c, ushort2 p) {
  tile<bfloat> value;
  value.v() = bfloat2(*(const threadgroup packed_bfloat2 *)(source + (r * 8 + p.y) * 32 + c * 8 + p.x));
  return value;
}

METAL_FUNC tile<bfloat> transposed(const threadgroup bfloat *source, ushort r, ushort c, ushort2 p) {
  tile<bfloat> value;
  value.v() = bfloat2(source[(c * 8 + p.x) * 32 + r * 8 + p.y],
                      source[(c * 8 + p.x + 1) * 32 + r * 8 + p.y]);
  return value;
}

METAL_FUNC float row_sum(float value) {
  value += simd_shuffle_xor(value, 1);
  return value + simd_shuffle_xor(value, 8);
}

METAL_FUNC float row_max(float value) {
  value = max(value, simd_shuffle_xor(value, 1));
  return max(value, simd_shuffle_xor(value, 8));
}

// Copies rows first..first+31 (clamped to the sequence) of two 32-column slices into threadgroup memory.
METAL_FUNC void stage(threadgroup bfloat *left, threadgroup bfloat *right, const device bfloat *a,
                      uint a_stride, const device bfloat *b, uint b_stride, uint first, uint length,
                      ushort index) {
  ushort r = index / 4, c = index % 4 * 8;
  uint source = min(first + r, length - 1);
  a += source * a_stride + c;
  b += source * b_stride + c;
  *(threadgroup packed_bfloat4 *)(left + r * 32 + c) = *(const device packed_bfloat4 *)a;
  *(threadgroup packed_bfloat4 *)(left + r * 32 + c + 4) = *(const device packed_bfloat4 *)(a + 4);
  *(threadgroup packed_bfloat4 *)(right + r * 32 + c) = *(const device packed_bfloat4 *)b;
  *(threadgroup packed_bfloat4 *)(right + r * 32 + c + 4) = *(const device packed_bfloat4 *)(b + 4);
}

#define ATTENTION_ARGUMENTS                                                                       \
  device const int *offsets [[buffer(0)]], device const int *lengths [[buffer(1)]],             \
  device const int *chunk_rows [[buffer(2)]], device const int *chunk_starts [[buffer(3)]],     \
  uint group [[threadgroup_position_in_grid]], ushort simd [[simdgroup_index_in_threadgroup]],  \
  ushort lane [[thread_index_in_simdgroup]], ushort index [[thread_index_in_threadgroup]]

#define ATTENTION_PROLOGUE                                                                        \
  uint chunk = group / 4, head = group % 4, sequence = chunk_rows[chunk];                       \
  uint length = lengths[sequence], base = offsets[sequence];                                    \
  ushort2 p = lane_position(lane);                                                              \
  uint row = (chunk - chunk_starts[sequence]) * 32 + simd * 8 + p.y;                           \
  uint clamped = base + min(row, length - 1);

kernel void flash_forward(ATTENTION_ARGUMENTS, device const bfloat *qkv [[buffer(4)]],
                          device bfloat *output [[buffer(5)]], device float *lse [[buffer(6)]]) {
  threadgroup bfloat keys[1024], values[1024];
  ATTENTION_PROLOGUE
  tile<bfloat> query[4];
  tile<float> out[4];
  for (ushort d = 0; d < 4; d++) {
    query[d] = load_pair<bfloat>(qkv + clamped * 384 + head * 32 + d * 8 + p.x);
    out[d].v() = 0;
  }
  float maximum = MASK, total = 0;
  for (uint first = 0; first < length; first += 32) {
    threadgroup_barrier(mem_flags::mem_threadgroup);
    stage(keys, values, qkv + base * 384 + 128 + head * 32, 384,
          qkv + base * 384 + 256 + head * 32, 384, first, length, index);
    threadgroup_barrier(mem_flags::mem_threadgroup);
    tile<float> score[4];
    float current = MASK;
    for (ushort k = 0; k < 4; k++) {
      score[k].v() = 0;
      for (ushort d = 0; d < 4; d++) score[k].mma(query[d], transposed(keys, d, k, p));
      for (ushort i = 0; i < 2; i++)
        if (first + k * 8 + p.x + i >= length) score[k].v()[i] = MASK;
      current = max(current, max(score[k].v()[0], score[k].v()[1]));
    }
    float updated = max(maximum, row_max(current) * SCALE_LOG2);
    float correction = exp2(maximum - updated), sum = 0;
    for (ushort k = 0; k < 4; k++) {
      score[k].v() = exp2(score[k].v() * SCALE_LOG2 - updated);
      sum += score[k].v()[0] + score[k].v()[1];
    }
    total = total * correction + row_sum(sum);
    maximum = updated;
    for (ushort d = 0; d < 4; d++) {
      out[d].v() *= correction;
      for (ushort k = 0; k < 4; k++) out[d].mma(score[k], block(values, k, d, p));
    }
  }
  if (row < length) {
    for (ushort d = 0; d < 4; d++)
      *(device packed_bfloat2 *)(output + (base + row) * 128 + head * 32 + d * 8 + p.x) =
          packed_bfloat2(bfloat2(out[d].v() / total));
    if (p.x == 0) lse[(base + row) * 4 + head] = maximum + log2(total);
  }
}

kernel void flash_backward_query(ATTENTION_ARGUMENTS, device const bfloat *qkv [[buffer(4)]],
                                 device const bfloat *output [[buffer(5)]],
                                 device const float *lse [[buffer(6)]],
                                 device const bfloat *grad_output [[buffer(7)]],
                                 device bfloat *grad_qkv [[buffer(8)]], device float *delta [[buffer(9)]]) {
  threadgroup bfloat keys[1024], values[1024];
  ATTENTION_PROLOGUE
  tile<bfloat> query[4], grad[4];
  tile<float> grad_query[4];
  float rowdot = 0;
  for (ushort d = 0; d < 4; d++) {
    uint column = head * 32 + d * 8 + p.x;
    query[d] = load_pair<bfloat>(qkv + clamped * 384 + column);
    grad[d] = load_pair<bfloat>(grad_output + clamped * 128 + column);
    float2 out = float2(load_pair<float>(output + clamped * 128 + column).v());
    rowdot += dot(float2(grad[d].v()), out);
    grad_query[d].v() = 0;
  }
  rowdot = row_sum(rowdot);
  float log_sum = lse[clamped * 4 + head];
  for (uint first = 0; first < length; first += 32) {
    threadgroup_barrier(mem_flags::mem_threadgroup);
    stage(keys, values, qkv + base * 384 + 128 + head * 32, 384,
          qkv + base * 384 + 256 + head * 32, 384, first, length, index);
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (ushort k = 0; k < 4; k++) {
      tile<float> probability, grad_probability;
      probability.v() = 0;
      grad_probability.v() = 0;
      for (ushort d = 0; d < 4; d++) {
        probability.mma(query[d], transposed(keys, d, k, p));
        grad_probability.mma(grad[d], transposed(values, d, k, p));
      }
      for (ushort i = 0; i < 2; i++)
        if (first + k * 8 + p.x + i >= length) probability.v()[i] = MASK;
      probability.v() = exp2(probability.v() * SCALE_LOG2 - log_sum);
      probability.v() *= grad_probability.v() - rowdot;
      for (ushort d = 0; d < 4; d++) grad_query[d].mma(probability, block(keys, k, d, p));
    }
  }
  if (row < length) {
    for (ushort d = 0; d < 4; d++)
      *(device packed_bfloat2 *)(grad_qkv + (base + row) * 384 + head * 32 + d * 8 + p.x) =
          packed_bfloat2(bfloat2(grad_query[d].v() * SCALE));
    if (p.x == 0) delta[(base + row) * 4 + head] = rowdot;
  }
}

kernel void flash_backward_key_value(ATTENTION_ARGUMENTS, device const bfloat *qkv [[buffer(4)]],
                                     device const float *lse [[buffer(5)]],
                                     device const float *delta [[buffer(6)]],
                                     device const bfloat *grad_output [[buffer(7)]],
                                     device bfloat *grad_qkv [[buffer(8)]]) {
  threadgroup bfloat queries[1024], grads[1024];
  threadgroup float log_sums[32], dots[32];
  ATTENTION_PROLOGUE
  tile<bfloat> key[4], value[4];
  tile<float> grad_key[4], grad_value[4];
  for (ushort d = 0; d < 4; d++) {
    uint column = head * 32 + d * 8 + p.x;
    key[d] = load_pair<bfloat>(qkv + clamped * 384 + 128 + column);
    value[d] = load_pair<bfloat>(qkv + clamped * 384 + 256 + column);
    grad_key[d].v() = 0;
    grad_value[d].v() = 0;
  }
  for (uint first = 0; first < length; first += 32) {
    threadgroup_barrier(mem_flags::mem_threadgroup);
    stage(queries, grads, qkv + base * 384 + head * 32, 384, grad_output + base * 128 + head * 32,
          128, first, length, index);
    if (index < 32) {
      bool valid = first + index < length;
      uint source = (base + min(first + index, length - 1)) * 4 + head;
      log_sums[index] = valid ? lse[source] : -MASK;
      dots[index] = valid ? delta[source] : 0;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (ushort j = 0; j < 4; j++) {
      tile<float> probability, grad_probability;
      probability.v() = 0;
      grad_probability.v() = 0;
      for (ushort d = 0; d < 4; d++) {
        probability.mma(key[d], transposed(queries, d, j, p));
        grad_probability.mma(value[d], transposed(grads, d, j, p));
      }
      float2 log_sum = float2(log_sums[j * 8 + p.x], log_sums[j * 8 + p.x + 1]);
      float2 rowdot = float2(dots[j * 8 + p.x], dots[j * 8 + p.x + 1]);
      probability.v() = exp2(probability.v() * SCALE_LOG2 - log_sum);
      for (ushort d = 0; d < 4; d++) grad_value[d].mma(probability, block(grads, j, d, p));
      probability.v() *= grad_probability.v() - rowdot;
      for (ushort d = 0; d < 4; d++) grad_key[d].mma(probability, block(queries, j, d, p));
    }
  }
  if (row < length) {
    for (ushort d = 0; d < 4; d++) {
      device bfloat *destination = grad_qkv + (base + row) * 384 + head * 32 + d * 8 + p.x;
      *(device packed_bfloat2 *)(destination + 128) = packed_bfloat2(bfloat2(grad_key[d].v() * SCALE));
      *(device packed_bfloat2 *)(destination + 256) = packed_bfloat2(bfloat2(grad_value[d].v()));
    }
  }
}

// Final-layer attention of sparse queries over whole packed sequences: the queries of sequence s are
// rows positions[query_offsets[s]...query_offsets[s + 1]), its keys rows offsets[s]...offsets[s + 1].
// A threadgroup handles one sequence and simdgroup h its head h, over 8x8 tiles of queries and keys.
// Products with probabilities run in fp32, as the scalar kernels they replace.
#define SPARSE_ARGUMENTS                                                                          \
  device const int *offsets [[buffer(0)]], device const int *query_offsets [[buffer(1)]],       \
  device const int *positions [[buffer(2)]], device const bfloat *qkv [[buffer(3)]],            \
  uint sequence [[threadgroup_position_in_grid]], ushort head [[simdgroup_index_in_threadgroup]], \
  ushort lane [[thread_index_in_simdgroup]]

#define SPARSE_PROLOGUE                                                                           \
  ushort2 p = lane_position(lane);                                                              \
  uint base = offsets[sequence], length = offsets[sequence + 1] - base;                         \
  uint first = query_offsets[sequence], queries = query_offsets[sequence + 1] - first;          \
  const device bfloat *Q = qkv + head * 32, *K = qkv + 128 + head * 32, *V = qkv + 256 + head * 32;

// 8x8 tile whose lane holds rows[x], rows[x + 1] of one column: the transpose of 8 rows of source.
METAL_FUNC tile<bfloat> columns(const device bfloat *source, uint stride, uint a, uint b, ushort y) {
  tile<bfloat> value;
  value.v() = bfloat2(source[a * stride + y], source[b * stride + y]);
  return value;
}

kernel void sparse_forward(SPARSE_ARGUMENTS, device bfloat *output [[buffer(4)]],
                           device float *lse [[buffer(5)]]) {
  SPARSE_PROLOGUE
  for (uint q0 = 0; q0 < queries; q0 += 8) {
    uint query = first + min(q0 + p.y, queries - 1), token = positions[query];
    tile<bfloat> q[4];
    tile<float> out[4];
    for (ushort d = 0; d < 4; d++) {
      q[d] = load_pair<bfloat>(Q + token * 384 + d * 8 + p.x);
      out[d].v() = 0;
    }
    float maximum = MASK, total = 0;
    for (uint k0 = 0; k0 < length; k0 += 8) {
      uint a = base + min(k0 + p.x, length - 1), b = base + min(k0 + p.x + 1, length - 1);
      tile<float> score;
      score.v() = 0;
      for (ushort d = 0; d < 4; d++) score.mma(q[d], columns(K + d * 8, 384, a, b, p.y));
      for (ushort i = 0; i < 2; i++)
        if (k0 + p.x + i >= length) score.v()[i] = MASK;
      float updated = max(maximum, row_max(max(score.v()[0], score.v()[1])) * SCALE_LOG2);
      float correction = exp2(maximum - updated);
      score.v() = exp2(score.v() * SCALE_LOG2 - updated);
      total = total * correction + row_sum(score.v()[0] + score.v()[1]);
      maximum = updated;
      uint key = base + min(k0 + p.y, length - 1);
      for (ushort d = 0; d < 4; d++) {
        out[d].v() *= correction;
        out[d].mma(score, load_pair<float>(V + key * 384 + d * 8 + p.x));
      }
    }
    if (q0 + p.y < queries) {
      for (ushort d = 0; d < 4; d++)
        *(device packed_bfloat2 *)(output + query * 128 + head * 32 + d * 8 + p.x) =
            packed_bfloat2(bfloat2(out[d].v() / total));
      if (p.x == 0) lse[query * 4 + head] = maximum + log2(total);
    }
  }
}

kernel void sparse_backward_query(SPARSE_ARGUMENTS, device const bfloat *output [[buffer(4)]],
                                  device const float *lse [[buffer(5)]],
                                  device const bfloat *grad_output [[buffer(6)]],
                                  device bfloat *grad_qkv [[buffer(7)]]) {
  SPARSE_PROLOGUE
  for (uint q0 = 0; q0 < queries; q0 += 8) {
    uint query = first + min(q0 + p.y, queries - 1), token = positions[query];
    tile<bfloat> q[4], grad[4];
    tile<float> grad_query[4];
    float rowdot = 0;
    for (ushort d = 0; d < 4; d++) {
      uint column = head * 32 + d * 8 + p.x;
      q[d] = load_pair<bfloat>(Q + token * 384 + d * 8 + p.x);
      grad[d] = load_pair<bfloat>(grad_output + query * 128 + column);
      rowdot += dot(float2(grad[d].v()), float2(load_pair<float>(output + query * 128 + column).v()));
      grad_query[d].v() = 0;
    }
    rowdot = row_sum(rowdot);
    float log_sum = lse[query * 4 + head];
    for (uint k0 = 0; k0 < length; k0 += 8) {
      uint a = base + min(k0 + p.x, length - 1), b = base + min(k0 + p.x + 1, length - 1);
      tile<float> probability, grad_probability;
      probability.v() = 0;
      grad_probability.v() = 0;
      for (ushort d = 0; d < 4; d++) {
        probability.mma(q[d], columns(K + d * 8, 384, a, b, p.y));
        grad_probability.mma(grad[d], columns(V + d * 8, 384, a, b, p.y));
      }
      for (ushort i = 0; i < 2; i++)
        if (k0 + p.x + i >= length) probability.v()[i] = MASK;
      probability.v() = exp2(probability.v() * SCALE_LOG2 - log_sum) * (grad_probability.v() - rowdot);
      uint key = base + min(k0 + p.y, length - 1);
      for (ushort d = 0; d < 4; d++)
        grad_query[d].mma(probability, load_pair<float>(K + key * 384 + d * 8 + p.x));
    }
    if (q0 + p.y < queries) {
      for (ushort d = 0; d < 4; d++)
        *(device packed_bfloat2 *)(grad_qkv + token * 384 + head * 32 + d * 8 + p.x) =
            packed_bfloat2(bfloat2(grad_query[d].v() * SCALE));
    }
  }
}

// Writes dK and dV of every key and zeroes its query columns; sparse_backward_query runs after it.
kernel void sparse_backward_key_value(SPARSE_ARGUMENTS, device const float *lse [[buffer(4)]],
                                      device const float *delta [[buffer(5)]],
                                      device const bfloat *grad_output [[buffer(6)]],
                                      device bfloat *grad_qkv [[buffer(7)]]) {
  SPARSE_PROLOGUE
  const device bfloat *G = grad_output + head * 32;
  for (uint k0 = 0; k0 < length; k0 += 8) {
    uint key = base + min(k0 + p.y, length - 1);
    tile<bfloat> k[4], v[4];
    tile<float> grad_key[4], grad_value[4];
    for (ushort d = 0; d < 4; d++) {
      k[d] = load_pair<bfloat>(K + key * 384 + d * 8 + p.x);
      v[d] = load_pair<bfloat>(V + key * 384 + d * 8 + p.x);
      grad_key[d].v() = 0;
      grad_value[d].v() = 0;
    }
    for (uint q0 = 0; q0 < queries; q0 += 8) {
      uint a = first + min(q0 + p.x, queries - 1), b = first + min(q0 + p.x + 1, queries - 1);
      uint row = first + min(q0 + p.y, queries - 1);
      tile<float> probability, grad_probability;
      probability.v() = 0;
      grad_probability.v() = 0;
      for (ushort d = 0; d < 4; d++) {
        probability.mma(k[d], columns(Q + d * 8, 384, positions[a], positions[b], p.y));
        grad_probability.mma(v[d], columns(G + d * 8, 128, a, b, p.y));
      }
      float2 log_sum = float2(q0 + p.x < queries ? lse[a * 4 + head] : -MASK,
                              q0 + p.x + 1 < queries ? lse[b * 4 + head] : -MASK);
      probability.v() = exp2(probability.v() * SCALE_LOG2 - log_sum);
      for (ushort d = 0; d < 4; d++)
        grad_value[d].mma(probability, load_pair<float>(G + row * 128 + d * 8 + p.x));
      probability.v() *= grad_probability.v() - float2(delta[a * 4 + head], delta[b * 4 + head]);
      for (ushort d = 0; d < 4; d++)
        grad_key[d].mma(probability, load_pair<float>(Q + positions[row] * 384 + d * 8 + p.x));
    }
    if (k0 + p.y < length) {
      for (ushort d = 0; d < 4; d++) {
        device bfloat *destination = grad_qkv + key * 384 + head * 32 + d * 8 + p.x;
        *(device packed_bfloat2 *)destination = packed_bfloat2(bfloat2(0));
        *(device packed_bfloat2 *)(destination + 128) = packed_bfloat2(bfloat2(grad_key[d].v() * SCALE));
        *(device packed_bfloat2 *)(destination + 256) = packed_bfloat2(bfloat2(grad_value[d].v()));
      }
    }
  }
}

kernel void sparse_delta(device const bfloat *output [[buffer(0)]], device const bfloat *grad_output [[buffer(1)]],
                         device float *delta [[buffer(2)]], uint index [[thread_position_in_grid]]) {
  float sum = 0;
  for (ushort d = 0; d < 32; d++) sum += float(output[index * 32 + d]) * float(grad_output[index * 32 + d]);
  delta[index] = sum;
}
