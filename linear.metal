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

enum epilogue { BIAS, RESIDUAL, PLAIN, SPLIT };

// C[M, N] = op(A) op(B): op(A) is A [M, K] or, if A_T, A stored [K, M]; op(B) is B stored [N, K]
// transposed or, if !B_T, B stored [K, N]. Each simdgroup owns a 32x32 tile of C over the K rows
// [z * chunk, (z + 1) * chunk); SPLIT adds these partial sums atomically into a zeroed C.
template <bool A_T, bool B_T, typename OUT, epilogue E>
METAL_FUNC void matmul(device const bfloat *A, device const bfloat *B, device OUT *C,
                       device const float *bias, device const float *residual, uint M, uint N,
                       uint K, uint chunk, uint3 group, ushort lane) {
  ushort2 p = lane_position(lane);
  uint m0 = group.y * 32, n0 = group.x * 32, k0 = group.z * chunk, k1 = min(K, k0 + chunk);
  tile<float> acc[4][4];
  for (ushort i = 0; i < 4; i++)
    for (ushort j = 0; j < 4; j++) acc[i][j].v() = 0;
  for (uint k = k0; k < k1; k += 8) {
    tile<bfloat> a[4], b[4];
    uint ka = k + p.x, kb = k + p.y;
    for (ushort i = 0; i < 4; i++) {
      uint m = min(m0 + i * 8 + p.y, M - 1);
      a[i].v() = A_T ? bfloat2(ka < k1 ? A[ka * M + m] : 0, ka + 1 < k1 ? A[(ka + 1) * M + m] : 0)
                     : bfloat2(ka < k1 ? A[m * K + ka] : 0, ka + 1 < k1 ? A[m * K + ka + 1] : 0);
    }
    for (ushort j = 0; j < 4; j++) {
      uint n = min(n0 + j * 8 + p.x, N - 1), n1 = min(n + 1, N - 1);
      b[j].v() = kb >= k1 ? bfloat2(0) : B_T ? bfloat2(B[n * K + kb], B[n1 * K + kb])
                                             : bfloat2(B[kb * N + n], B[kb * N + n1]);
    }
    for (ushort i = 0; i < 4; i++)
      for (ushort j = 0; j < 4; j++) acc[i][j].mma(a[i], b[j]);
  }
  for (ushort i = 0; i < 4; i++) {
    uint m = m0 + i * 8 + p.y;
    for (ushort j = 0; j < 4; j++)
      for (ushort e = 0; e < 2; e++) {
        uint n = n0 + j * 8 + p.x + e, index = m * N + n;
        float value = acc[i][j].v()[e];
        if (m >= M || n >= N) continue;
        if (E == SPLIT)
          atomic_fetch_add_explicit((device atomic_float *)(C + index), value, memory_order_relaxed);
        else
          C[index] = OUT(value + (E == PLAIN ? 0 : bias[n]) + (E == RESIDUAL ? residual[index] : 0));
      }
  }
}

#define MATMUL(name, A_T, B_T, OUT, E)                                                            \
  kernel void name(device const bfloat *A [[buffer(0)]], device const bfloat *B [[buffer(1)]],    \
                   device OUT *C [[buffer(2)]], device const float *bias [[buffer(3)]],           \
                   device const float *residual [[buffer(4)]], constant uint &M [[buffer(5)]],    \
                   constant uint &N [[buffer(6)]], constant uint &K [[buffer(7)]],                \
                   constant uint &chunk [[buffer(8)]], uint3 group [[threadgroup_position_in_grid]], \
                   ushort lane [[thread_index_in_simdgroup]]) {                                   \
    matmul<A_T, B_T, OUT, E>(A, B, C, bias, residual, M, N, K, chunk, group, lane);               \
  }
MATMUL(linear, false, true, bfloat, BIAS)
MATMUL(linear_residual, false, true, float, RESIDUAL)
MATMUL(linear_input_grad, false, false, bfloat, PLAIN)
MATMUL(linear_weight_grad, true, false, float, SPLIT)

// out[c] += sum over rows [y * chunk, (y + 1) * chunk) of values[:, c].
kernel void column_sum(device const bfloat *values [[buffer(0)]], device float *out [[buffer(1)]],
                       constant uint &rows [[buffer(2)]], constant uint &columns [[buffer(3)]],
                       constant uint &chunk [[buffer(4)]], uint2 index [[thread_position_in_grid]]) {
  if (index.x >= columns) return;
  float sum = 0;
  for (uint row = index.y * chunk; row < min(rows, (index.y + 1) * chunk); row++)
    sum += float(values[row * columns + index.x]);
  atomic_fetch_add_explicit((device atomic_float *)(out + index.x), sum, memory_order_relaxed);
}
