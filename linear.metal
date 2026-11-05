#include <metal_stdlib>
using namespace metal;

#pragma METAL internals : enable
template <typename T> struct tile {
  vec<T, 64> t;
  thread vec<T, 2> &v() { return *reinterpret_cast<thread vec<T, 2> *>(&t); }
  template <typename U, typename V> void mma(tile<U> a, tile<V> b) {
    t = __metal_simdgroup_matrix_8x8_multiply_accumulate(a.t, b.t, t, vec<T, 64>());
  }
};
#pragma METAL internals : disable

// Lane layout of an 8x8 tile: each lane holds row y, columns x and x + 1.
METAL_FUNC ushort2 lane_position(ushort lane) {
  ushort quad = lane / 4;
  return ushort2((quad & 2) * 2 + lane % 2 * 2, quad / 4 * 4 + lane / 2 % 4);
}

enum epilogue { BIAS, RESIDUAL, PLAIN, SPLIT, GELU, GELU_GRAD };

// erf with < 1 ulp error, from c10/metal/special_math.h.
METAL_FUNC float erf(float a) {
  float t = abs(a), s = a * a, r;
  if (t > 0.927734375f) {
    r = fma(fma(-1.72853470e-5f, t, 3.83197126e-4f), s, fma(-3.88396438e-3f, t, 2.42546219e-2f));
    r = fma(fma(fma(fma(r, t, -1.06777877e-1f), t, -6.34846687e-1f), t, -1.28717512e-1f), t, -t);
    return copysign(1.0f - exp(r), a);
  }
  r = fma(fma(fma(fma(fma(-5.96761703e-4f, s, 4.99119423e-3f), s, -2.67681349e-2f), s, 1.12819925e-1f),
              s, -3.76125336e-1f), s, 1.28379166e-1f);
  return fma(r, a, a);
}

// C[M, N] = op(A) op(B): op(A) is A [M, K] or, if A_T, A stored [K, M]; op(B) is B stored [N, K]
// transposed or, if !B_T, B stored [K, N]. A is rounded to bf16 on load. Each simdgroup owns a 32x32
// tile of C over the K rows [z * chunk, (z + 1) * chunk); SPLIT adds these partial sums atomically
// into a zeroed C, and the row sums of op(A) into a zeroed bias. GELU stores the biased output h in
// residual and gelu(h) in C; GELU_GRAD scales C by gelu'(h) with h read from residual.
template <typename TA, bool A_T, bool B_T, bool SUMS>
METAL_FUNC void accumulate(thread tile<float> (&acc)[4][4], thread float (&row_sums)[4],
                           device const TA *A, device const bfloat *B, uint M, uint N, uint K,
                           uint m0, uint n0, uint k0, uint k1, ushort2 p) {
  for (ushort i = 0; i < 4; i++) {
    row_sums[i] = 0;
    for (ushort j = 0; j < 4; j++) acc[i][j].v() = 0;
  }
  for (uint k = k0; k < k1; k += 8) {
    tile<bfloat> a[4], b[4];
    uint ka = k + p.x, kb = k + p.y;
    for (ushort i = 0; i < 4; i++) {
      uint m = min(m0 + i * 8 + p.y, M - 1);
      a[i].v() = bfloat2(A_T ? float2(ka < k1 ? A[ka * M + m] : 0, ka + 1 < k1 ? A[(ka + 1) * M + m] : 0)
                             : float2(ka < k1 ? A[m * K + ka] : 0, ka + 1 < k1 ? A[m * K + ka + 1] : 0));
      if (SUMS) row_sums[i] += float(a[i].v()[0]) + float(a[i].v()[1]);
    }
    for (ushort j = 0; j < 4; j++) {
      uint n = min(n0 + j * 8 + p.x, N - 1), n1 = min(n + 1, N - 1);
      b[j].v() = kb >= k1 ? bfloat2(0) : B_T ? bfloat2(B[n * K + kb], B[n1 * K + kb])
                                             : bfloat2(B[kb * N + n], B[kb * N + n1]);
    }
    for (ushort i = 0; i < 4; i++)
      for (ushort j = 0; j < 4; j++) acc[i][j].mma(a[i], b[j]);
  }
}

template <typename TA, bool A_T, bool B_T, typename OUT, epilogue E, typename TR>
METAL_FUNC void matmul(device const TA *A, device const bfloat *B, device OUT *C, device float *bias,
                       device const TR *residual, uint M, uint N, uint K, uint chunk,
                       uint3 group, ushort lane) {
  ushort2 p = lane_position(lane);
  uint m0 = group.y * 32, n0 = group.x * 32, k0 = group.z * chunk;
  tile<float> acc[4][4];
  float row_sums[4];
  accumulate<TA, A_T, B_T, E == SPLIT>(acc, row_sums, A, B, M, N, K, m0, n0, k0, min(K, k0 + chunk), p);
  for (ushort i = 0; i < 4; i++) {
    uint m = m0 + i * 8 + p.y;
    if (E == SPLIT && group.x == 0) {
      float sum = row_sums[i] + simd_shuffle_xor(row_sums[i], 1);
      sum += simd_shuffle_xor(sum, 8);
      if (p.x == 0 && m < M)
        atomic_fetch_add_explicit((device atomic_float *)(bias + m), sum, memory_order_relaxed);
    }
    for (ushort j = 0; j < 4; j++)
      for (ushort e = 0; e < 2; e++) {
        uint n = n0 + j * 8 + p.x + e, index = m * N + n;
        float value = acc[i][j].v()[e];
        if (m >= M || n >= N) continue;
        if (E == SPLIT)
          atomic_fetch_add_explicit((device atomic_float *)(C + index), value, memory_order_relaxed);
        else if (E == GELU || E == GELU_GRAD) {
          float h = E == GELU ? float(TR(value + bias[n])) : float(residual[index]);
          float cdf = 0.5f * (1 + erf(h * M_SQRT1_2_F));
          if (E == GELU) ((device TR *)residual)[index] = TR(h);
          C[index] = OUT(E == GELU ? h * cdf : value * (cdf + h * exp(-0.5f * h * h) * 0.398942280f));
        } else
          C[index] = OUT(value + (E == PLAIN ? 0 : bias[n]) + (E == RESIDUAL ? residual[index] : 0));
      }
  }
}

#define MATMUL(name, TA, A_T, B_T, OUT, E, TR)                                                    \
  kernel void name(device const TA *A [[buffer(0)]], device const bfloat *B [[buffer(1)]],        \
                   device OUT *C [[buffer(2)]], device float *bias [[buffer(3)]],                 \
                   device const TR *residual [[buffer(4)]], constant uint &M [[buffer(5)]],       \
                   constant uint &N [[buffer(6)]], constant uint &K [[buffer(7)]],                \
                   constant uint &chunk [[buffer(8)]], uint3 group [[threadgroup_position_in_grid]], \
                   ushort lane [[thread_index_in_simdgroup]]) {                                   \
    matmul<TA, A_T, B_T, OUT, E, TR>(A, B, C, bias, residual, M, N, K, chunk, group, lane);       \
  }
MATMUL(linear, bfloat, false, true, bfloat, BIAS, float)
MATMUL(linear_gelu, bfloat, false, true, bfloat, GELU, bfloat)
MATMUL(linear_residual, bfloat, false, true, float, RESIDUAL, float)
MATMUL(linear_input_grad, bfloat, false, false, bfloat, PLAIN, float)
MATMUL(linear_input_grad_float, float, false, false, bfloat, PLAIN, float)
MATMUL(linear_input_grad_float_gelu, float, false, false, bfloat, GELU_GRAD, bfloat)
MATMUL(linear_weight_grad, bfloat, true, false, float, SPLIT, float)
MATMUL(linear_weight_grad_float, float, true, false, float, SPLIT, float)

// Layer-norm backward fused into the input gradient of the following linear: G = grad_output B is the
// gradient of the normalized rows (width 128). A threadgroup of 4 simdgroups computes G for 32 full
// rows into threadgroup memory, then each simdgroup runs the layer-norm backward of 8 rows, writing
// their input gradient (plus residual) and the block's gamma and beta partial sums.
template <typename TA>
METAL_FUNC void input_grad_norm(device const TA *A, device const bfloat *B, device const float *input,
                                device const float *gamma, device const float *stats,
                                device const float *residual, device float *grad_input,
                                device float *partial, threadgroup float *G, uint M, uint K,
                                bool add_residual, uint block, ushort simd, ushort lane) {
  ushort2 p = lane_position(lane);
  uint m0 = block * 32;
  tile<float> acc[4][4];
  float row_sums[4];
  accumulate<TA, false, false, false>(acc, row_sums, A, B, M, 128, K, m0, simd * 32, 0, K, p);
  for (ushort i = 0; i < 4; i++)
    for (ushort j = 0; j < 4; j++)
      *(threadgroup float2 *)(G + (i * 8 + p.y) * 128 + simd * 32 + j * 8 + p.x) = acc[i][j].v();
  threadgroup_barrier(mem_flags::mem_threadgroup);
  float4 grad_gamma = 0, grad_beta = 0;
  for (uint r = simd * 8; r < simd * 8 + 8; r++) {
    uint m = m0 + r;
    if (m >= M) break;
    float mean = stats[m * 2], inverse_std = stats[m * 2 + 1];
    float4 normalized, grad;
    for (ushort part = 0; part < 4; part++) {
      uint column = lane + part * 32;
      float gradient = G[r * 128 + column];
      normalized[part] = (input[m * 128 + column] - mean) * inverse_std;
      grad_gamma[part] += gradient * normalized[part];
      grad_beta[part] += gradient;
      grad[part] = gradient * gamma[column];
    }
    float sum = simd_sum(grad[0] + grad[1] + grad[2] + grad[3]);
    float product = simd_sum(dot(grad, normalized));
    for (ushort part = 0; part < 4; part++) {
      uint index = m * 128 + lane + part * 32;
      grad_input[index] = (grad[part] - sum / 128 - normalized[part] * product / 128) * inverse_std
                          + (add_residual ? residual[index] : 0.0f);
    }
  }
  threadgroup_barrier(mem_flags::mem_threadgroup);
  for (ushort part = 0; part < 4; part++) {
    G[simd * 256 + lane + part * 32] = grad_gamma[part];
    G[simd * 256 + 128 + lane + part * 32] = grad_beta[part];
  }
  threadgroup_barrier(mem_flags::mem_threadgroup);
  for (ushort column = simd * 64 + lane; column < simd * 64 + 64; column += 32)
    partial[block * 256 + column] = G[column] + G[256 + column] + G[512 + column] + G[768 + column];
}

#define INPUT_GRAD_NORM(name, TA)                                                                  \
  kernel void name(device const TA *A [[buffer(0)]], device const bfloat *B [[buffer(1)]],         \
                   device const float *input [[buffer(2)]], device const float *gamma [[buffer(3)]], \
                   device const float *stats [[buffer(4)]], device const float *residual [[buffer(5)]], \
                   device float *grad_input [[buffer(6)]], device float *partial [[buffer(7)]],    \
                   constant uint &M [[buffer(8)]], constant uint &K [[buffer(9)]],                 \
                   constant uint &add_residual [[buffer(10)]],                                     \
                   uint block [[threadgroup_position_in_grid]],                                    \
                   ushort simd [[simdgroup_index_in_threadgroup]],                                 \
                   ushort lane [[thread_index_in_simdgroup]]) {                                    \
    threadgroup float G[32 * 128];                                                                 \
    input_grad_norm<TA>(A, B, input, gamma, stats, residual, grad_input, partial, G, M, K,        \
                        add_residual, block, simd, lane);                                          \
  }
INPUT_GRAD_NORM(linear_input_grad_norm, bfloat)
INPUT_GRAD_NORM(linear_input_grad_norm_float, float)
