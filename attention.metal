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
