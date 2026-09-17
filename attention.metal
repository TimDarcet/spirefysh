#include <metal_stdlib>
using namespace metal;
namespace metal {
enum class simdgroup_async_copy_clamp_mode { clamp_to_zero, clamp_to_edge };
struct simdgroup_event {
  ushort lane;
  METAL_FUNC simdgroup_event(ushort value) thread : lane(value) {}
  template <typename T>
  METAL_FUNC void async_copy(threadgroup T *dst, const device T *src, ulong count) thread {
    for (ulong index = lane; index < count; index += 32) dst[index] = src[index];
  }
  template <typename T>
  METAL_FUNC void async_copy(device T *dst, const threadgroup T *src, ulong count) thread {
    for (ulong index = lane; index < count; index += 32) dst[index] = src[index];
  }
  template <typename T>
  METAL_FUNC void async_copy(
      threadgroup T *dst, ushort dst_stride, ushort2 dst_size,
      const device T *src, uint src_stride, ushort2 src_size,
      bool transpose = false,
      simdgroup_async_copy_clamp_mode clamp = simdgroup_async_copy_clamp_mode::clamp_to_zero) thread {
    for (uint index = lane; index < uint(dst_size.x) * dst_size.y; index += 32) {
      uint x = index % dst_size.x, y = index / dst_size.x;
      dst[y * dst_stride + x] = x < src_size.x && y < src_size.y
          ? (transpose ? src[x * src_stride + y] : src[y * src_stride + x]) : T(0);
    }
  }
  template <typename T>
  METAL_FUNC void async_copy(
      device T *dst, uint dst_stride, ushort2 dst_size,
      const threadgroup T *src, ushort src_stride, ushort2 src_size,
      bool transpose = false) thread {
    for (uint index = lane; index < uint(dst_size.x) * dst_size.y; index += 32) {
      uint x = index % dst_size.x, y = index / dst_size.x;
      if (x < src_size.x && y < src_size.y)
        dst[y * dst_stride + x] = src[y * src_stride + x];
    }
  }
  METAL_FUNC static void wait(int, thread simdgroup_event *) {}
};
}
// -*- Metal -*-
//===-- metal_simdgroup_matrix_storage ------------------------------------===//
// Copyright (c) 2024 Philip Turner. See MIT LICENSE
//===----------------------------------------------------------------------===//

#ifndef __METAL_SIMDGROUP_MATRIX_STORAGE
#define __METAL_SIMDGROUP_MATRIX_STORAGE

// The layout of threads within a SIMD matrix.
//
//  0  0  1  1  8  8  9  9
//  2  2  3  3 10 10 11 11
//  4  4  5  5 12 12 13 13
//  6  6  7  7 14 14 15 15
// 16 16 17 17 24 24 25 25
// 18 18 19 19 26 26 27 27
// 20 20 21 21 28 28 29 29
// 22 22 23 23 30 30 31 31
//
// This is Morton order, a method for coalescing data accesses. It is used
// in a variety of contexts, from ray tracing acceleration structures, to
// nodal-point Laplacians, to sorting large lattices of atoms.
//
// Source: https://patents.google.com/patent/US11256518B2
METAL_FUNC static ushort2 morton_order(ushort thread_index_in_simdgroup) {
  ushort lane_id = thread_index_in_simdgroup;
  ushort quad_id = lane_id / 4;

  constexpr ushort QUADRANT_SPAN_M = 4;
  constexpr ushort THREADS_PER_QUADRANT = 8;
  ushort M_floor_of_quadrant = (quad_id / 4) * QUADRANT_SPAN_M;
  ushort M_in_quadrant = (lane_id / 2) % (THREADS_PER_QUADRANT / 2);
  ushort M_in_simd = M_floor_of_quadrant + M_in_quadrant;

  ushort N_floor_of_quadrant = (quad_id & 2) * 2; // 0 or 4
  ushort N_in_quadrant = (lane_id % 2) * 2; // 0 or 2
  ushort N_in_simd = N_floor_of_quadrant + N_in_quadrant;

  return ushort2(N_in_simd, M_in_simd);
}

#pragma METAL internals : enable
namespace metal
{
  template <typename T>
  struct simdgroup_matrix_storage {
    typedef vec<T, 64> storage_type;

    storage_type t;

    METAL_FUNC thread vec<T, 2>* thread_elements() thread {
      return reinterpret_cast<thread vec<T, 2>*>(&t);
    }

    METAL_FUNC simdgroup_matrix_storage() thread = default;

    METAL_FUNC simdgroup_matrix_storage(vec<T, 2> thread_elements) thread {
      *(this->thread_elements()) = thread_elements;
    }

    METAL_FUNC static device T* apply_offset(device T *src, uint elements_per_row, uint2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        return src + ulong(matrix_origin.x * elements_per_row) + matrix_origin.y;
      } else {
        return src + ulong(matrix_origin.y * elements_per_row) + matrix_origin.x;
      }
    }

    METAL_FUNC static threadgroup T* apply_offset(threadgroup T *src, ushort elements_per_row, ushort2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        return src + matrix_origin.x * elements_per_row + matrix_origin.y;
      } else {
        return src + matrix_origin.y * elements_per_row + matrix_origin.x;
      }
    }
    template <typename U>
    METAL_FUNC void load(const device U *src, uint elements_per_row, ushort2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        uint address0 = uint(matrix_origin.x + 0) * elements_per_row + uint(matrix_origin.y);
        uint address1 = uint(matrix_origin.x + 1) * elements_per_row + uint(matrix_origin.y);
        U memoryForm0 = src[address0];
        U memoryForm1 = src[address1];
        ((thread T*)thread_elements())[0] = T(memoryForm0);
        ((thread T*)thread_elements())[1] = T(memoryForm1);
      } else if (elements_per_row % 2 != 0) {
        uint address0 = uint(matrix_origin.y) * elements_per_row + uint(matrix_origin.x + 0);
        uint address1 = uint(matrix_origin.y) * elements_per_row + uint(matrix_origin.x + 1);
        U memoryForm0 = src[address0];
        U memoryForm1 = src[address1];
        ((thread T*)thread_elements())[0] = T(memoryForm0);
        ((thread T*)thread_elements())[1] = T(memoryForm1);
      } else {
        auto combinedAddress = uint(matrix_origin.y) * elements_per_row + uint(matrix_origin.x + 0);
        vec<U, 2> memoryForm = *(const device vec<U, 2>*)(src + combinedAddress);
        *(thread_elements()) = vec<T, 2>(memoryForm);
      }
    }

    // WARNING: 'T' must be 'float'.
    METAL_FUNC void load_bfloat(const device bfloat *src, uint elements_per_row, ushort2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        uint address0 = uint(matrix_origin.x + 0) * elements_per_row + uint(matrix_origin.y);
        uint address1 = uint(matrix_origin.x + 1) * elements_per_row + uint(matrix_origin.y);
        bfloat memoryForm0 = src[address0];
        bfloat memoryForm1 = src[address1];

        bfloat4 registerForm = *(thread bfloat4*)(thread_elements());
        registerForm[1] = memoryForm0;
        registerForm[3] = memoryForm1;
        ((thread bfloat4*)thread_elements())[0] = registerForm;
      } else {
        auto combinedAddress = uint(matrix_origin.y) * elements_per_row + uint(matrix_origin.x + 0);
        bfloat2 memoryForm = *(const device packed_bfloat2*)(src + combinedAddress);

        bfloat4 registerForm = *(thread bfloat4*)(thread_elements());
        ((thread float*)&registerForm)[1] = *(thread float*)(&memoryForm);
        ((thread bfloat*)&registerForm)[1] = memoryForm[0];
        ((thread bfloat4*)thread_elements())[0] = registerForm;
      }
    }

    template <typename U>
    METAL_FUNC void load(const threadgroup U *src, ushort elements_per_row, ushort2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        ushort address0 = ushort(matrix_origin.x + 0) * elements_per_row + ushort(matrix_origin.y);
        ushort address1 = ushort(matrix_origin.x + 1) * elements_per_row + ushort(matrix_origin.y);
        U memoryForm0 = src[address0];
        U memoryForm1 = src[address1];
        ((thread T*)thread_elements())[0] = T(memoryForm0);
        ((thread T*)thread_elements())[1] = T(memoryForm1);
      } else if (elements_per_row % 2 != 0) {
        ushort address0 = ushort(matrix_origin.y) * elements_per_row + ushort(matrix_origin.x + 0);
        ushort address1 = ushort(matrix_origin.y) * elements_per_row + ushort(matrix_origin.x + 1);
        U memoryForm0 = src[address0];
        U memoryForm1 = src[address1];
        ((thread T*)thread_elements())[0] = T(memoryForm0);
        ((thread T*)thread_elements())[1] = T(memoryForm1);
      } else {
        auto combinedAddress = ushort(matrix_origin.y) * elements_per_row + ushort(matrix_origin.x + 0);
        vec<U, 2> memoryForm = *(const threadgroup vec<U, 2>*)(src + combinedAddress);
        *(thread_elements()) = vec<T, 2>(memoryForm);
      }
    }

    // WARNING: 'T' must be 'float'.
    METAL_FUNC void load_bfloat(const threadgroup bfloat *src, ushort elements_per_row, ushort2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        ushort address0 = ushort(matrix_origin.x + 0) * elements_per_row + ushort(matrix_origin.y);
        ushort address1 = ushort(matrix_origin.x + 1) * elements_per_row + ushort(matrix_origin.y);
        bfloat memoryForm0 = src[address0];
        bfloat memoryForm1 = src[address1];

        bfloat4 registerForm = *(thread bfloat4*)(thread_elements());
        registerForm[1] = memoryForm0;
        registerForm[3] = memoryForm1;
        ((thread bfloat4*)thread_elements())[0] = registerForm;
      } else {
        auto combinedAddress = ushort(matrix_origin.y) * elements_per_row + ushort(matrix_origin.x + 0);
        bfloat2 memoryForm = *(const threadgroup packed_bfloat2*)(src + combinedAddress);

        bfloat4 registerForm = *(thread bfloat4*)(thread_elements());
        ((thread float*)&registerForm)[1] = *(thread float*)(&memoryForm);
        ((thread bfloat*)&registerForm)[1] = memoryForm[0];
        ((thread bfloat4*)thread_elements())[0] = registerForm;
      }
    }

    template <typename U>
    METAL_FUNC void store(device U *dst, uint elements_per_row, ushort2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        uint address0 = uint(matrix_origin.x + 0) * elements_per_row + uint(matrix_origin.y);
        uint address1 = uint(matrix_origin.x + 1) * elements_per_row + uint(matrix_origin.y);
        T registerForm0 = ((thread T*)thread_elements())[0];
        T registerForm1 = ((thread T*)thread_elements())[1];
        dst[address0] = U(registerForm0);
        dst[address1] = U(registerForm1);
      } else if (elements_per_row % 2 != 0) {
        uint address0 = uint(matrix_origin.y) * elements_per_row + uint(matrix_origin.x + 0);
        uint address1 = uint(matrix_origin.y) * elements_per_row + uint(matrix_origin.x + 1);
        T registerForm0 = ((thread T*)thread_elements())[0];
        T registerForm1 = ((thread T*)thread_elements())[1];
        dst[address0] = U(registerForm0);
        dst[address1] = U(registerForm1);
      } else {
        auto combinedAddress = uint(matrix_origin.y) * elements_per_row + uint(matrix_origin.x + 0);
        vec<T, 2> registerForm = *(thread_elements());
        *(device vec<U, 2>*)(dst + combinedAddress) = vec<U, 2>(registerForm);
      }
    }

    // WARNING: 'T' must be 'float'.
    METAL_FUNC void store_bfloat(device bfloat *dst, uint elements_per_row, ushort2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        uint address0 = uint(matrix_origin.x + 0) * elements_per_row + uint(matrix_origin.y);
        uint address1 = uint(matrix_origin.x + 1) * elements_per_row + uint(matrix_origin.y);
        bfloat4 registerForm = *(thread bfloat4*)(thread_elements());
        registerForm[2] = registerForm[1];
        dst[address0] = registerForm[2];
        dst[address1] = registerForm[3];
      } else {
        uint address0 = uint(matrix_origin.y) * elements_per_row + uint(matrix_origin.x + 0);
        uint address1 = uint(matrix_origin.y) * elements_per_row + uint(matrix_origin.x + 1);
        bfloat4 registerForm = *(thread bfloat4*)(thread_elements());
        registerForm[2] = registerForm[1];
        dst[address0] = registerForm[2];
        dst[address1] = registerForm[3];
      }
    }

    template <typename U>
    METAL_FUNC void store(threadgroup U *dst, ushort elements_per_row, ushort2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        ushort address0 = ushort(matrix_origin.x + 0) * elements_per_row + ushort(matrix_origin.y);
        ushort address1 = ushort(matrix_origin.x + 1) * elements_per_row + ushort(matrix_origin.y);
        T registerForm0 = ((thread T*)thread_elements())[0];
        T registerForm1 = ((thread T*)thread_elements())[1];
        dst[address0] = U(registerForm0);
        dst[address1] = U(registerForm1);
      } else if (elements_per_row % 2 != 0) {
        ushort address0 = ushort(matrix_origin.y) * elements_per_row + ushort(matrix_origin.x + 0);
        ushort address1 = ushort(matrix_origin.y) * elements_per_row + ushort(matrix_origin.x + 1);
        T registerForm0 = ((thread T*)thread_elements())[0];
        T registerForm1 = ((thread T*)thread_elements())[1];
        dst[address0] = U(registerForm0);
        dst[address1] = U(registerForm1);
      } else {
        auto combinedAddress = ushort(matrix_origin.y) * elements_per_row + ushort(matrix_origin.x + 0);
        vec<T, 2> registerForm = *(thread_elements());
        *(threadgroup vec<U, 2>*)(dst + combinedAddress) = vec<U, 2>(registerForm);
      }
    }

    // WARNING: 'T' must be 'float'.
    METAL_FUNC void store_bfloat(threadgroup bfloat *dst, ushort elements_per_row, ushort2 matrix_origin, bool transpose_matrix = false) {
      if (transpose_matrix) {
        ushort address0 = ushort(matrix_origin.x + 0) * elements_per_row + ushort(matrix_origin.y);
        ushort address1 = ushort(matrix_origin.x + 1) * elements_per_row + ushort(matrix_origin.y);
        bfloat4 registerForm = *(thread bfloat4*)(thread_elements());
        registerForm[2] = registerForm[1];
        dst[address0] = registerForm[2];
        dst[address1] = registerForm[3];
      } else {
        ushort address0 = ushort(matrix_origin.y) * elements_per_row + ushort(matrix_origin.x + 0);
        ushort address1 = ushort(matrix_origin.y) * elements_per_row + ushort(matrix_origin.x + 1);
        bfloat4 registerForm = *(thread bfloat4*)(thread_elements());
        registerForm[2] = registerForm[1];
        dst[address0] = registerForm[2];
        dst[address1] = registerForm[3];
      }
    }

    template <typename U, typename V>
    METAL_FUNC void multiply(simdgroup_matrix_storage<U> a, simdgroup_matrix_storage<V> b, bool accumulate = true) {
      if (!accumulate) {
        *(thread_elements()) = vec<T, 2>(0);
      }
      t = __metal_simdgroup_matrix_8x8_multiply_accumulate(a.t, b.t, t, typename simdgroup_matrix_storage<T>::storage_type());
    }
  };
} // namespace metal
#pragma METAL internals : disable

#endif // __METAL_SIMDGROUP_MATRIX_STORAGE

using namespace metal;


// R = row dimension (output sequence)
// C = column dimension (input sequence)



// Declare the function.
kernel void flash_forward(
    device bfloat* Q [[buffer(0)]],
  device bfloat* K [[buffer(1)]],
  device bfloat* V [[buffer(2)]],
  device bfloat* O [[buffer(3)]],
  device float* L [[buffer(4)]],


  device const int* offsets [[buffer(5)]],
  device const int* lengths [[buffer(6)]],
  device const int* block_rows [[buffer(7)]],
  device const int* block_starts [[buffer(8)]],
  uint global_gid [[threadgroup_position_in_grid]],
  ushort sidx [[simdgroup_index_in_threadgroup]],
  ushort lane_id [[thread_index_in_simdgroup]]
) {
  threadgroup uchar threadgroup_block[512];
  uint block = global_gid / 4;
  uint head = global_gid % 4;
  uint row = block_rows[block];
  uint gid = block - uint(block_starts[row]);
  uint R = uint(lengths[row]);
  uint C = R;
  uint valid_C = uint(lengths[row]);
  uint base = uint(offsets[row]);
  Q += base * 384 + head * 32;
  K += base * 384 + 128 + head * 32;
  V += base * 384 + 256 + head * 32;
  O += base * 128 + head * 32;
  L += base * 4 + head;
  ushort2 morton_offset = morton_order(lane_id);
  uint parallelization_group_offset = gid;
  parallelization_group_offset *= 16;

  // Return early if the entire SIMD is out of bounds.
  if (parallelization_group_offset >= R) {
    return;
  }



simdgroup_matrix_storage<bfloat> Q_sram[4];


#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 8
) {

if (true && (
  (32 % 8 == 0) ||
  (d_outer + 8 <= 32)
)) {


uint2 Q_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1));
auto Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q, 384,
  Q_src_offset, false);

if (
  true ||
  (parallelization_group_offset + sidx * 8 + morton_offset.y < R)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  ushort2 Q_origin(d, 0);
  Q_sram[(d_outer + d) / 8].load(
    Q_src, 384,
    Q_origin, false);
}

}

} else {


threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 Q_offset(d_outer, parallelization_group_offset);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    Q, 384,
    Q_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(8),
    ushort(32 - d_outer));
  ushort D_dst_dimension = min(
    ushort(8),
    ushort(32 - d_outer));
  ushort R_dimension = min(
    uint(16),
    uint(R - parallelization_group_offset));
  ushort2 tile_src(D_src_dimension, R_dimension);
  ushort2 tile_dst(D_dst_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 8, tile_dst,
    src, 384, tile_src,
    false);
  simdgroup_event::wait(1, &event);
}


ushort2 Q_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto Q_src =
(threadgroup bfloat*)(threadgroup_block);

Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q_src, 8,
  Q_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);

if (d_outer + 8 <= 32) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  ushort2 Q_origin(d, 0);
  Q_sram[(d_outer + d) / 8].load(
    Q_src, 8,
    Q_origin, false);
}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 0; d += 8) {
  ushort2 Q_origin(d, 0);
  Q_sram[(d_outer + d) / 8].load(
    Q_src, 8,
    Q_origin, false);
}

}


}

}

simdgroup_matrix_storage<float> O_sram[4];

float m = -numeric_limits<float>::max();
float l = numeric_limits<float>::denorm_min();


// Outer loop over the traversal dimension.
for (uint c = 0; c < C; c += 32) {
  // S = Q * K^T


simdgroup_matrix_storage<float> S_sram[32 / 8];




#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 8
) {

if ((
  (C % 32 == 0) ||
  (c + 32 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 8 <= 32)
)) {




uint2 K_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + c);
auto K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K, 384,
  K_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 384,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

}


} else {




threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 K_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    K, 384,
    K_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(8),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 8;
  ushort C_src_dimension = min(
    uint(32),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 8, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 K_block_offset(
  morton_offset.x,
  morton_offset.y);
auto K_src = (threadgroup bfloat*)(threadgroup_block);
K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K_src, 8,
  K_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 8,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

  if (c + 32
      < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 8,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

  }
}


}

}


if (false) {
  ushort d_outer = 32;

if ((
  (C % 32 == 0) ||
  (c + 32 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 8 <= 32)
)) {




uint2 K_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + c);
auto K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K, 384,
  K_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 384,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

}


} else {




threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 K_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    K, 384,
    K_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(8),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 8;
  ushort C_src_dimension = min(
    uint(32),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 8, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 K_block_offset(
  morton_offset.x,
  morton_offset.y);
auto K_src = (threadgroup bfloat*)(threadgroup_block);
K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K_src, 8,
  K_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 8,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

  if (c + 32
      < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 8,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

  }
}


}

}



if (((C % 32) != 0) &&
    (c + 32 > C)) {
  // Prevent the value from becoming -INF during the FMA before the
  // exponentiation. If the multiplication during FMA returns -INF,
  // subtracting a positive 'm' value will turn it into zero. We don't want
  // that. exp(0) evaluates to 1.00 and corrupts the value of 'l'.
  const float mask_value =
  (0.875 / 1.442695) * -numeric_limits<float>::max();

  #pragma clang loop unroll(full)
  for (ushort index = 0; index < 2; ++index) {
    if (morton_offset.x + index >= (C % 32) - ((C % 32) - ((C % 32) % 8))) {
      auto S_elements = S_sram[((C % 32) - ((C % 32) % 8)) / 8].thread_elements();
      (*S_elements)[index] = mask_value;
    }
  }
  #pragma clang loop unroll(full)
  for (ushort c = ((C % 32) - ((C % 32) % 8)) + 8; c < 32; c += 8) {
    auto S_elements = S_sram[c / 8].thread_elements();
    *S_elements = mask_value;
  }
}


  // m = reduce(m)

// update 'm'
vec<float, 2> m_new_accumulator;
#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  auto S_elements = S_sram[c / 8].thread_elements();
  if (c == 0) {
    m_new_accumulator = *S_elements;
  } else {
    m_new_accumulator = max(m_new_accumulator, *S_elements);
  }
}
float m_new = max(m_new_accumulator[0], m_new_accumulator[1]);
m_new = max(m_new, simd_shuffle_xor(m_new, 1));
m_new = max(m_new, simd_shuffle_xor(m_new, 8));
m_new *= 0.25503486;


  // correction = exp(m_old) / exp(m_new)

// update 'O'
float correction = 1;
if (m_new > m) {
  correction = fast::exp2(m - m_new);
  m = m_new;
}


  // P = softmax(S * scaleFactor)


simdgroup_matrix_storage<float> P_sram[32 / 8];

{

#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  auto L_elements = m;

auto S = *(S_sram[c / 8].thread_elements());
auto P = vec<float, 2>(
  fast::exp2(float2(S) * 0.25503486 - float2(L_elements)));
*(P_sram[c / 8].thread_elements()) = P;

}

}


  // l = reduce(l)

// update 'l'
float2 l_new_accumulator;
#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  auto P_elements = P_sram[c / 8].thread_elements();
  if (c == 0) {
    l_new_accumulator = float2(*P_elements);
  } else {
    l_new_accumulator += float2(*P_elements);
  }
}
float l_new = l_new_accumulator[0] + l_new_accumulator[1];
l_new += simd_shuffle_xor(l_new, 1);
l_new += simd_shuffle_xor(l_new, 8);
l = l * correction + l_new;


  // O *= correction
  // O += P * V
  // O /= l


#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 8
) {

if ((
  (C % 32 == 0) ||
  (c + 32 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 8 <= 32)
)) {


if (c == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *O = simdgroup_matrix_storage<float>(0);
}

} else {


#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *(O->thread_elements()) *= correction;
}

}

uint2 V_src_offset(
  morton_offset.x + d_outer,
  morton_offset.y + c);
auto V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V, 384,
  V_src_offset, false);



#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(d, c);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 384,
    V_origin, false);

  // Issue one SIMD matmul instruction.
  O_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], V, /*accumulate=*/true);
}

}

if (
  (C % 32 == 0) &&
  (c + 32 == C)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *(O->thread_elements()) *= fast::divide(1, l);
}

}



} else {


if (c == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *O = simdgroup_matrix_storage<float>(0);
}

} else {


#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *(O->thread_elements()) *= correction;
}

}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 V_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    V, 384,
    V_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_dimension = min(
    ushort(8),
    ushort(32 - d_outer));
  ushort C_src_dimension = min(
    uint(32),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_dimension, C_src_dimension);
  ushort2 tile_dst(D_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 8, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 V_block_offset(
  morton_offset.x,
  morton_offset.y);
auto V_src = (threadgroup bfloat*)(threadgroup_block);
V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V_src, 8,
  V_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);




#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(d, c);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 8,
    V_origin, false);

  // Issue one SIMD matmul instruction.
  O_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], V, /*accumulate=*/true);
}

}

if (c + 32
    < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(d, c);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 8,
    V_origin, false);

  // Issue one SIMD matmul instruction.
  O_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], V, /*accumulate=*/true);
}

}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *(O->thread_elements()) *= fast::divide(1, l);
}

}



}

}


if (false) {
  ushort d_outer = 32;

if ((
  (C % 32 == 0) ||
  (c + 32 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 8 <= 32)
)) {


if (c == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *O = simdgroup_matrix_storage<float>(0);
}

} else {


#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *(O->thread_elements()) *= correction;
}

}

uint2 V_src_offset(
  morton_offset.x + d_outer,
  morton_offset.y + c);
auto V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V, 384,
  V_src_offset, false);



#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(d, c);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 384,
    V_origin, false);

  // Issue one SIMD matmul instruction.
  O_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], V, /*accumulate=*/true);
}

}

if (
  (C % 32 == 0) &&
  (c + 32 == C)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *(O->thread_elements()) *= fast::divide(1, l);
}

}



} else {


if (c == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *O = simdgroup_matrix_storage<float>(0);
}

} else {


#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *(O->thread_elements()) *= correction;
}

}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 V_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    V, 384,
    V_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_dimension = min(
    ushort(8),
    ushort(32 - d_outer));
  ushort C_src_dimension = min(
    uint(32),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_dimension, C_src_dimension);
  ushort2 tile_dst(D_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 8, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 V_block_offset(
  morton_offset.x,
  morton_offset.y);
auto V_src = (threadgroup bfloat*)(threadgroup_block);
V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V_src, 8,
  V_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);




#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(d, c);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 8,
    V_origin, false);

  // Issue one SIMD matmul instruction.
  O_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], V, /*accumulate=*/true);
}

}

if (c + 32
    < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 32 == 0) ? 32 : C % 32) + 7) / 8 * 8; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(d, c);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 8,
    V_origin, false);

  // Issue one SIMD matmul instruction.
  O_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], V, /*accumulate=*/true);
}

}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  auto O = O_sram + (d_outer + d) / 8;
  *(O->thread_elements()) *= fast::divide(1, l);
}

}



}

}


}




#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 8
) {

if (true && (
  (32 % 8 == 0) ||
  (d_outer + 8 <= 32)
)) {


uint2 O_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1));
auto O_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  O, 128,
  O_src_offset, false);

if (
  false ||
  (parallelization_group_offset + sidx * 8 + morton_offset.y < R)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  ushort2 O_origin(d, 0);
  O_sram[(d_outer + d) / 8].store_bfloat(
    O_src, 128,
    O_origin, false);
}

}

} else {



ushort2 O_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto O_src =
(threadgroup bfloat*)(threadgroup_block);

O_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  O_src, 8,
  O_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);

if (d_outer + 8 <= 32) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 8; d += 8) {
  ushort2 O_origin(d, 0);
  O_sram[(d_outer + d) / 8].store_bfloat(
    O_src, 8,
    O_origin, false);
}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 0; d += 8) {
  ushort2 O_origin(d, 0);
  O_sram[(d_outer + d) / 8].store_bfloat(
    O_src, 8,
    O_origin, false);
}

}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 O_offset(d_outer, parallelization_group_offset);
  auto src = (threadgroup bfloat*)(threadgroup_block);
  auto dst = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    O, 128,
    O_offset, false);

  ushort D_dimension = min(
    ushort(8),
    ushort(32 - d_outer));
  ushort R_dimension = min(
    uint(16),
    uint(R - parallelization_group_offset));
  ushort2 tile(D_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 128, tile,
    src, 8, tile,
    false);
  simdgroup_event::wait(1, &event);
}


}

}

if (parallelization_group_offset + sidx * 8 + morton_offset.y < R) {
  // Premultiplied by log_base_2(e).
  float L_sram = m + fast::log2(l);
  L[min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1) * 4] = L_sram;
}

}



using namespace metal;


// R = row dimension (output sequence)
// C = column dimension (input sequence)



// Declare the function.
kernel void flash_backward_query(
    device bfloat* Q [[buffer(0)]],
  device bfloat* K [[buffer(1)]],
  device bfloat* V [[buffer(2)]],
  device bfloat* O [[buffer(3)]],
  device float* L [[buffer(4)]],
  device float* D [[buffer(5)]],
  device bfloat* dO [[buffer(6)]],
  device bfloat* dQ [[buffer(7)]],


  device const int* offsets [[buffer(8)]],
  device const int* lengths [[buffer(9)]],
  device const int* block_rows [[buffer(10)]],
  device const int* block_starts [[buffer(11)]],
  uint global_gid [[threadgroup_position_in_grid]],
  ushort sidx [[simdgroup_index_in_threadgroup]],
  ushort lane_id [[thread_index_in_simdgroup]]
) {
  threadgroup uchar threadgroup_block[2048];
  uint block = global_gid / 4;
  uint head = global_gid % 4;
  uint row = block_rows[block];
  uint gid = block - uint(block_starts[row]);
  uint R = uint(lengths[row]);
  uint C = R;
  uint valid_C = uint(lengths[row]);
  uint base = uint(offsets[row]);
  Q += base * 384 + head * 32;
  K += base * 384 + 128 + head * 32;
  V += base * 384 + 256 + head * 32;
  O += base * 128 + head * 32;
  L += base * 4 + head;
  D += base * 4 + head;
  dO += base * 128 + head * 32;
  dQ += base * 384 + head * 32;
  ushort2 morton_offset = morton_order(lane_id);
  uint parallelization_group_offset = gid;
  parallelization_group_offset *= 16;

  // Return early if the entire SIMD is out of bounds.
  if (parallelization_group_offset >= R) {
    return;
  }



simdgroup_matrix_storage<bfloat> Q_sram[4];


#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if (true && (
  (32 % 16 == 0) ||
  (d_outer + 16 <= 32)
)) {


uint2 Q_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1));
auto Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q, 384,
  Q_src_offset, false);

if (
  true ||
  (parallelization_group_offset + sidx * 8 + morton_offset.y < R)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 Q_origin(d, 0);
  Q_sram[(d_outer + d) / 8].load(
    Q_src, 384,
    Q_origin, false);
}

}

} else {


threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 Q_offset(d_outer, parallelization_group_offset);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    Q, 384,
    Q_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort R_dimension = min(
    uint(16),
    uint(R - parallelization_group_offset));
  ushort2 tile_src(D_src_dimension, R_dimension);
  ushort2 tile_dst(D_dst_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src,
    false);
  simdgroup_event::wait(1, &event);
}


ushort2 Q_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto Q_src =
(threadgroup bfloat*)(threadgroup_block);

Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q_src, 16,
  Q_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);

if (d_outer + 16 <= 32) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 Q_origin(d, 0);
  Q_sram[(d_outer + d) / 8].load(
    Q_src, 16,
    Q_origin, false);
}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 0; d += 8) {
  ushort2 Q_origin(d, 0);
  Q_sram[(d_outer + d) / 8].load(
    Q_src, 16,
    Q_origin, false);
}

}


}

}

simdgroup_matrix_storage<float> dQ_sram[4];

float L_sram = L[min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1) * 4];

float2 D_accumulator(0);
{

// Threads outside of the matrix along the row dimension,
// have their origin shifted in-bounds.
uint D_offset = morton_offset.x;
uint R_offset = min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1);
uint2 offset_src(D_offset, R_offset);


// Where the dO data will be read from.
auto dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO, 128,
  offset_src, false);


// Where the O data will be read from.
auto O_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  O, 128,
  offset_src, false);

// Going to use async copy to handle the matrix edge.
#pragma clang loop unroll(disable)
for (ushort d = 0; d < 32; d += 8) {

simdgroup_matrix_storage<bfloat> dO;
dO.load(
  dO_src, 128,
  ushort2(d, 0), false);


  simdgroup_matrix_storage<float> O;
  O.load_bfloat(
    O_src, 128,
    ushort2(d, 0), false);

  // Perform the pointwise multiplication.
  auto dO_value = *(dO.thread_elements());
  auto O_value = *(O.thread_elements());
  D_accumulator += float2(dO_value) * float2(O_value);
}

}
{

}

float D_sram = D_accumulator[0] + D_accumulator[1];
D_sram += simd_shuffle_xor(D_sram, 1);
D_sram += simd_shuffle_xor(D_sram, 8);
D_sram *= 0.17677669;



// Outer loop over the traversal dimension.
for (uint c = 0; c < C; c += 64) {
  // S = Q * K^T


simdgroup_matrix_storage<float> S_sram[64 / 8];




#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if ((
  (C % 64 == 0) ||
  (c + 64 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {




uint2 K_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + c);
auto K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K, 384,
  K_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 64; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 384,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

}


} else {




threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 K_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    K, 384,
    K_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort C_src_dimension = min(
    uint(64),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 K_block_offset(
  morton_offset.x,
  morton_offset.y);
auto K_src = (threadgroup bfloat*)(threadgroup_block);
K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K_src, 16,
  K_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 16,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

  if (c + 64
      < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c < 64; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 16,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

  }
}


}

}


if (false) {
  ushort d_outer = 32;

if ((
  (C % 64 == 0) ||
  (c + 64 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {




uint2 K_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + c);
auto K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K, 384,
  K_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 64; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 384,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

}


} else {




threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 K_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    K, 384,
    K_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort C_src_dimension = min(
    uint(64),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 K_block_offset(
  morton_offset.x,
  morton_offset.y);
auto K_src = (threadgroup bfloat*)(threadgroup_block);
K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K_src, 16,
  K_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 16,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

  if (c + 64
      < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c < 64; c += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(c, d);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 16,
    K_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    Q_sram[(d_outer + d) / 8],
    K, ((d_outer > 0) || (d > 0)));
}

  }
}


}

}




  const float edge_mask = (0.875 / 1.442695) * -numeric_limits<float>::max();
  #pragma clang loop unroll(full)
  for (ushort edge = 0; edge < 64; edge += 8) {
    auto elements = S_sram[edge / 8].thread_elements();
    #pragma clang loop unroll(full)
    for (ushort element = 0; element < 2; ++element)
      if (c + edge + morton_offset.x + element >= valid_C)
        (*elements)[element] = edge_mask;
  }

  // P = softmax(S * scaleFactor)


simdgroup_matrix_storage<float> P_sram[64 / 8];

{

#pragma clang loop unroll(full)
for (ushort c = 0; c < 64; c += 8) {
  auto L_elements = L_sram;

auto S = *(S_sram[c / 8].thread_elements());
auto P = vec<float, 2>(
  fast::exp2(float2(S) * 0.25503486 - float2(L_elements)));
*(P_sram[c / 8].thread_elements()) = P;

}

}


  // dP = dO * V^T


simdgroup_matrix_storage<float> dP_sram[64 / 8];


#pragma clang loop unroll(full)
for (ushort c = 0; c < 64; c += 8) {
  auto dP = dP_sram + c / 8;
  *dP = simdgroup_matrix_storage<float>(0);
}



#pragma clang loop unroll(disable)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if ((
  (C % 64 == 0) ||
  (c + 64 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {


simdgroup_matrix_storage<bfloat> dO_sram[16 / 8];



uint2 dO_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1));
auto dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO, 128,
  dO_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dO_origin(d, 0);
  dO_sram[d / 8].load(
    dO_src, 128,
    dO_origin, false);
}


uint2 V_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + c);
auto V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V, 384,
  V_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 64; c += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(c, d);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 384,
    V_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    dO_sram[(0 + d) / 8],
    V, true);
}

}


} else {


simdgroup_matrix_storage<bfloat> dO_sram[16 / 8];



threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 dO_offset(d_outer, parallelization_group_offset);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    dO, 128,
    dO_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort R_dimension = min(
    uint(16),
    uint(R - parallelization_group_offset));
  ushort2 tile_src(D_src_dimension, R_dimension);
  ushort2 tile_dst(D_dst_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 128, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 dO_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto dO_src = (threadgroup bfloat*)(threadgroup_block);
dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO_src, 16,
  dO_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dO_origin(d, 0);
  dO_sram[d / 8].load(
    dO_src, 16,
    dO_origin, false);
}


threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 V_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    V, 384,
    V_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort C_src_dimension = min(
    uint(64),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 V_block_offset(
  morton_offset.x,
  morton_offset.y);
auto V_src = (threadgroup bfloat*)(threadgroup_block);
V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V_src, 16,
  V_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(c, d);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 16,
    V_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    dO_sram[(0 + d) / 8],
    V, true);
}

  if (c + 64
      < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c < 64; c += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(c, d);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 16,
    V_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    dO_sram[(0 + d) / 8],
    V, true);
}

  }
}


}

}


if (false) {
  ushort d_outer = 32;

if ((
  (C % 64 == 0) ||
  (c + 64 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {


simdgroup_matrix_storage<bfloat> dO_sram[16 / 8];



uint2 dO_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1));
auto dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO, 128,
  dO_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dO_origin(d, 0);
  dO_sram[d / 8].load(
    dO_src, 128,
    dO_origin, false);
}


uint2 V_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + c);
auto V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V, 384,
  V_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 64; c += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(c, d);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 384,
    V_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    dO_sram[(0 + d) / 8],
    V, true);
}

}


} else {


simdgroup_matrix_storage<bfloat> dO_sram[16 / 8];



threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 dO_offset(d_outer, parallelization_group_offset);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    dO, 128,
    dO_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort R_dimension = min(
    uint(16),
    uint(R - parallelization_group_offset));
  ushort2 tile_src(D_src_dimension, R_dimension);
  ushort2 tile_dst(D_dst_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 128, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 dO_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto dO_src = (threadgroup bfloat*)(threadgroup_block);
dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO_src, 16,
  dO_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dO_origin(d, 0);
  dO_sram[d / 8].load(
    dO_src, 16,
    dO_origin, false);
}


threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 V_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    V, 384,
    V_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort C_src_dimension = min(
    uint(64),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 V_block_offset(
  morton_offset.x,
  morton_offset.y);
auto V_src = (threadgroup bfloat*)(threadgroup_block);
V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V_src, 16,
  V_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(c, d);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 16,
    V_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    dO_sram[(0 + d) / 8],
    V, true);
}

  if (c + 64
      < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c < 64; c += 8) {
  // Load the RHS from memory.
  ushort2 V_origin(c, d);
  simdgroup_matrix_storage<bfloat> V;
  V.load(
    V_src, 16,
    V_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    dO_sram[(0 + d) / 8],
    V, true);
}

  }
}


}

}



  // dS = P * (dP - D) * scaleFactor


simdgroup_matrix_storage<float> dS_sram[64 / 8];

{

#pragma clang loop unroll(full)
for (ushort c = 0; c < 64; c += 8) {
  auto D_elements = D_sram;

auto P = *(P_sram[c / 8].thread_elements());
auto dP = *(dP_sram[c / 8].thread_elements());
auto dS = vec<float, 2>(
  float2(P) * (float2(dP) * 0.17677669 - float2(D_elements)));
*(dS_sram[c / 8].thread_elements()) = dS;

}

}


  // dQ += dS * K


#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if ((
  (C % 64 == 0) ||
  (c + 64 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {


if (c == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dQ = dQ_sram + (d_outer + d) / 8;
  *dQ = simdgroup_matrix_storage<float>(0);
}

} else {


}

uint2 K_src_offset(
  morton_offset.x + d_outer,
  morton_offset.y + c);
auto K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K, 384,
  K_src_offset, false);



#pragma clang loop unroll(full)
for (ushort c = 0; c < 64; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(d, c);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 384,
    K_origin, false);

  // Issue one SIMD matmul instruction.
  dQ_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], K, /*accumulate=*/true);
}

}

if (
  (C % 64 == 0) &&
  (c + 64 == C)
) {

}



} else {


if (c == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dQ = dQ_sram + (d_outer + d) / 8;
  *dQ = simdgroup_matrix_storage<float>(0);
}

} else {


}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 K_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    K, 384,
    K_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort C_src_dimension = min(
    uint(64),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_dimension, C_src_dimension);
  ushort2 tile_dst(D_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 K_block_offset(
  morton_offset.x,
  morton_offset.y);
auto K_src = (threadgroup bfloat*)(threadgroup_block);
K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K_src, 16,
  K_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);




#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(d, c);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 16,
    K_origin, false);

  // Issue one SIMD matmul instruction.
  dQ_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], K, /*accumulate=*/true);
}

}

if (c + 64
    < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c < 64; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(d, c);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 16,
    K_origin, false);

  // Issue one SIMD matmul instruction.
  dQ_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], K, /*accumulate=*/true);
}

}

} else {

}



}

}


if (false) {
  ushort d_outer = 32;

if ((
  (C % 64 == 0) ||
  (c + 64 <= C)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {


if (c == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dQ = dQ_sram + (d_outer + d) / 8;
  *dQ = simdgroup_matrix_storage<float>(0);
}

} else {


}

uint2 K_src_offset(
  morton_offset.x + d_outer,
  morton_offset.y + c);
auto K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K, 384,
  K_src_offset, false);



#pragma clang loop unroll(full)
for (ushort c = 0; c < 64; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(d, c);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 384,
    K_origin, false);

  // Issue one SIMD matmul instruction.
  dQ_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], K, /*accumulate=*/true);
}

}

if (
  (C % 64 == 0) &&
  (c + 64 == C)
) {

}



} else {


if (c == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dQ = dQ_sram + (d_outer + d) / 8;
  *dQ = simdgroup_matrix_storage<float>(0);
}

} else {


}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 K_offset(d_outer, c);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    K, 384,
    K_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort C_src_dimension = min(
    uint(64),
    uint(C - c));
  ushort C_dst_dimension = max(
    ushort((((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_dimension, C_src_dimension);
  ushort2 tile_dst(D_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 K_block_offset(
  morton_offset.x,
  morton_offset.y);
auto K_src = (threadgroup bfloat*)(threadgroup_block);
K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K_src, 16,
  K_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);




#pragma clang loop unroll(full)
for (ushort c = 0; c < (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(d, c);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 16,
    K_origin, false);

  // Issue one SIMD matmul instruction.
  dQ_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], K, /*accumulate=*/true);
}

}

if (c + 64
    < C) {

#pragma clang loop unroll(full)
for (ushort c = (((C % 64 == 0) ? 64 : C % 64) + 7) / 8 * 8; c < 64; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 K_origin(d, c);
  simdgroup_matrix_storage<bfloat> K;
  K.load(
    K_src, 16,
    K_origin, false);

  // Issue one SIMD matmul instruction.
  dQ_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], K, /*accumulate=*/true);
}

}

} else {

}



}

}


}




#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if (true && (
  (32 % 16 == 0) ||
  (d_outer + 16 <= 32)
)) {


uint2 dQ_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1));
auto dQ_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dQ, 384,
  dQ_src_offset, false);

if (
  false ||
  (parallelization_group_offset + sidx * 8 + morton_offset.y < R)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dQ_origin(d, 0);
  dQ_sram[(d_outer + d) / 8].store_bfloat(
    dQ_src, 384,
    dQ_origin, false);
}

}

} else {



ushort2 dQ_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto dQ_src =
(threadgroup bfloat*)(threadgroup_block);

dQ_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dQ_src, 16,
  dQ_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);

if (d_outer + 16 <= 32) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dQ_origin(d, 0);
  dQ_sram[(d_outer + d) / 8].store_bfloat(
    dQ_src, 16,
    dQ_origin, false);
}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 0; d += 8) {
  ushort2 dQ_origin(d, 0);
  dQ_sram[(d_outer + d) / 8].store_bfloat(
    dQ_src, 16,
    dQ_origin, false);
}

}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 dQ_offset(d_outer, parallelization_group_offset);
  auto src = (threadgroup bfloat*)(threadgroup_block);
  auto dst = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    dQ, 384,
    dQ_offset, false);

  ushort D_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort R_dimension = min(
    uint(16),
    uint(R - parallelization_group_offset));
  ushort2 tile(D_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 384, tile,
    src, 16, tile,
    false);
  simdgroup_event::wait(1, &event);
}


}

}

if (parallelization_group_offset + sidx * 8 + morton_offset.y < R) {

D[min(parallelization_group_offset + sidx * 8 + morton_offset.y, R - 1) * 4] = D_sram;

}

}



using namespace metal;


// R = row dimension (output sequence)
// C = column dimension (input sequence)



// Declare the function.
kernel void flash_backward_key_value(
    device bfloat* Q [[buffer(0)]],
  device bfloat* K [[buffer(1)]],
  device bfloat* V [[buffer(2)]],
  device float* L [[buffer(3)]],
  device float* D [[buffer(4)]],
  device bfloat* dO [[buffer(5)]],
  device bfloat* dV [[buffer(6)]],
  device bfloat* dK [[buffer(7)]],


  device const int* offsets [[buffer(8)]],
  device const int* lengths [[buffer(9)]],
  device const int* block_rows [[buffer(10)]],
  device const int* block_starts [[buffer(11)]],
  uint global_gid [[threadgroup_position_in_grid]],
  ushort sidx [[simdgroup_index_in_threadgroup]],
  ushort lane_id [[thread_index_in_simdgroup]]
) {
  threadgroup uchar threadgroup_block[1024];
  uint block = global_gid / 4;
  uint head = global_gid % 4;
  uint row = block_rows[block];
  uint gid = block - uint(block_starts[row]);
  uint R = uint(lengths[row]);
  uint C = R;
  uint valid_C = uint(lengths[row]);
  uint base = uint(offsets[row]);
  Q += base * 384 + head * 32;
  K += base * 384 + 128 + head * 32;
  V += base * 384 + 256 + head * 32;
  L += base * 4 + head;
  D += base * 4 + head;
  dO += base * 128 + head * 32;
  dV += base * 384 + 256 + head * 32;
  dK += base * 384 + 128 + head * 32;
  ushort2 morton_offset = morton_order(lane_id);
  uint parallelization_group_offset = gid;
  parallelization_group_offset *= 16;

  // Return early if the entire SIMD is out of bounds.
  if (parallelization_group_offset >= C) {
    return;
  }



simdgroup_matrix_storage<bfloat> K_sram[4];


#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if (true && (
  (32 % 16 == 0) ||
  (d_outer + 16 <= 32)
)) {


uint2 K_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, C - 1));
auto K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K, 384,
  K_src_offset, false);

if (
  true ||
  (parallelization_group_offset + sidx * 8 + morton_offset.y < C)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 K_origin(d, 0);
  K_sram[(d_outer + d) / 8].load(
    K_src, 384,
    K_origin, false);
}

}

} else {


threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 K_offset(d_outer, parallelization_group_offset);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    K, 384,
    K_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort R_dimension = min(
    uint(16),
    uint(C - parallelization_group_offset));
  ushort2 tile_src(D_src_dimension, R_dimension);
  ushort2 tile_dst(D_dst_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src,
    false);
  simdgroup_event::wait(1, &event);
}


ushort2 K_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto K_src =
(threadgroup bfloat*)(threadgroup_block);

K_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  K_src, 16,
  K_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);

if (d_outer + 16 <= 32) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 K_origin(d, 0);
  K_sram[(d_outer + d) / 8].load(
    K_src, 16,
    K_origin, false);
}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 0; d += 8) {
  ushort2 K_origin(d, 0);
  K_sram[(d_outer + d) / 8].load(
    K_src, 16,
    K_origin, false);
}

}


}

}


simdgroup_matrix_storage<bfloat> V_sram[4];


#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if (true && (
  (32 % 16 == 0) ||
  (d_outer + 16 <= 32)
)) {


uint2 V_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, C - 1));
auto V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V, 384,
  V_src_offset, false);

if (
  true ||
  (parallelization_group_offset + sidx * 8 + morton_offset.y < C)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 V_origin(d, 0);
  V_sram[(d_outer + d) / 8].load(
    V_src, 384,
    V_origin, false);
}

}

} else {


threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 V_offset(d_outer, parallelization_group_offset);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    V, 384,
    V_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort R_dimension = min(
    uint(16),
    uint(C - parallelization_group_offset));
  ushort2 tile_src(D_src_dimension, R_dimension);
  ushort2 tile_dst(D_dst_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src,
    false);
  simdgroup_event::wait(1, &event);
}


ushort2 V_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto V_src =
(threadgroup bfloat*)(threadgroup_block);

V_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  V_src, 16,
  V_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);

if (d_outer + 16 <= 32) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 V_origin(d, 0);
  V_sram[(d_outer + d) / 8].load(
    V_src, 16,
    V_origin, false);
}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 0; d += 8) {
  ushort2 V_origin(d, 0);
  V_sram[(d_outer + d) / 8].load(
    V_src, 16,
    V_origin, false);
}

}


}

}

simdgroup_matrix_storage<float> dK_sram[4];

simdgroup_matrix_storage<float> dV_sram[4];


// Outer loop over the traversal dimension.
for (uint r = 0; r < R; r += 32) {
  // S^T = K * Q^T


simdgroup_matrix_storage<float> S_sram[32 / 8];




#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if ((
  (R % 32 == 0) ||
  (r + 32 <= R)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {




uint2 Q_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + r);
auto Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q, 384,
  Q_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(c, d);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 384,
    Q_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    K_sram[(d_outer + d) / 8],
    Q, ((d_outer > 0) || (d > 0)));
}

}


} else {




threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 Q_offset(d_outer, r);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    Q, 384,
    Q_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort C_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort C_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 Q_block_offset(
  morton_offset.x,
  morton_offset.y);
auto Q_src = (threadgroup bfloat*)(threadgroup_block);
Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q_src, 16,
  Q_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(c, d);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 16,
    Q_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    K_sram[(d_outer + d) / 8],
    Q, ((d_outer > 0) || (d > 0)));
}

  if (r + 32
      < R) {

#pragma clang loop unroll(full)
for (ushort c = (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(c, d);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 16,
    Q_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    K_sram[(d_outer + d) / 8],
    Q, ((d_outer > 0) || (d > 0)));
}

  }
}


}

}


if (false) {
  ushort d_outer = 32;

if ((
  (R % 32 == 0) ||
  (r + 32 <= R)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {




uint2 Q_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + r);
auto Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q, 384,
  Q_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(c, d);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 384,
    Q_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    K_sram[(d_outer + d) / 8],
    Q, ((d_outer > 0) || (d > 0)));
}

}


} else {




threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 Q_offset(d_outer, r);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    Q, 384,
    Q_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort C_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort C_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 Q_block_offset(
  morton_offset.x,
  morton_offset.y);
auto Q_src = (threadgroup bfloat*)(threadgroup_block);
Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q_src, 16,
  Q_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(c, d);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 16,
    Q_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    K_sram[(d_outer + d) / 8],
    Q, ((d_outer > 0) || (d > 0)));
}

  if (r + 32
      < R) {

#pragma clang loop unroll(full)
for (ushort c = (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(c, d);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 16,
    Q_origin, true);

  // Issue one SIMD matmul instruction.
  S_sram[c / 8].multiply(
    K_sram[(d_outer + d) / 8],
    Q, ((d_outer > 0) || (d > 0)));
}

  }
}


}

}



  // P^T = exp(S^T - L)


simdgroup_matrix_storage<float> P_sram[32 / 8];

if (true && (
  (R % 32 == 0) ||
  (r + 32 <= R)
)) {

auto L_src = L + r * 4;


#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  ushort2 L_origin(c + morton_offset.x, 0);
  simdgroup_matrix_storage<float> L;
  L.load(
    L_src, 4,
    L_origin, true);
  auto L_elements = *(L.thread_elements());


auto S = *(S_sram[c / 8].thread_elements());
auto P = vec<float, 2>(
  fast::exp2(float2(S) * 0.25503486 - float2(L_elements)));
*(P_sram[c / 8].thread_elements()) = P;

}

} else {

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  auto L_src = L + r * 4;
  auto L_dst =
  (threadgroup float*)(threadgroup_block);

  ushort R_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort R_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(R_src_dimension));

  // Issue an async copy.
  simdgroup_event event(lane_id);
  event.async_copy(
    L_dst, 1, ushort2(R_dst_dimension, 1),
    L_src, 4, ushort2(R_src_dimension, 1), true);
  simdgroup_event::wait(1, &event);
}


auto L_src =
(threadgroup float*)(threadgroup_block);
L_src += morton_offset.x;
threadgroup_barrier(mem_flags::mem_threadgroup);


#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  ushort2 L_origin(c, 0);
  simdgroup_matrix_storage<float> L;
  L.load(
    L_src, 1,
    L_origin, false);
  auto L_elements = *(L.thread_elements());


auto S = *(S_sram[c / 8].thread_elements());
auto P = vec<float, 2>(
  fast::exp2(float2(S) * 0.25503486 - float2(L_elements)));
*(P_sram[c / 8].thread_elements()) = P;

}

}



  #pragma clang loop unroll(full)
  for (ushort edge = 0; edge < 32; edge += 8) {
    auto elements = P_sram[edge / 8].thread_elements();
    #pragma clang loop unroll(full)
    for (ushort element = 0; element < 2; ++element)
      if (r + edge + morton_offset.x + element >= R)
        (*elements)[element] = 0;
  }

  // dV += P^T * dO


#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if ((
  (R % 32 == 0) ||
  (r + 32 <= R)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {


if (r == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dV = dV_sram + (d_outer + d) / 8;
  *dV = simdgroup_matrix_storage<float>(0);
}

} else {


}

uint2 dO_src_offset(
  morton_offset.x + d_outer,
  morton_offset.y + r);
auto dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO, 128,
  dO_src_offset, false);



#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(d, c);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 128,
    dO_origin, false);

  // Issue one SIMD matmul instruction.
  dV_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], dO, /*accumulate=*/true);
}

}

if (
  (R % 32 == 0) &&
  (r + 32 == R)
) {

}



} else {


if (r == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dV = dV_sram + (d_outer + d) / 8;
  *dV = simdgroup_matrix_storage<float>(0);
}

} else {


}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 dO_offset(d_outer, r);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    dO, 128,
    dO_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort C_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort C_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_dimension, C_src_dimension);
  ushort2 tile_dst(D_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 128, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 dO_block_offset(
  morton_offset.x,
  morton_offset.y);
auto dO_src = (threadgroup bfloat*)(threadgroup_block);
dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO_src, 16,
  dO_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);




#pragma clang loop unroll(full)
for (ushort c = 0; c < (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(d, c);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 16,
    dO_origin, false);

  // Issue one SIMD matmul instruction.
  dV_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], dO, /*accumulate=*/true);
}

}

if (r + 32
    < R) {

#pragma clang loop unroll(full)
for (ushort c = (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(d, c);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 16,
    dO_origin, false);

  // Issue one SIMD matmul instruction.
  dV_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], dO, /*accumulate=*/true);
}

}

} else {

}



}

}


if (false) {
  ushort d_outer = 32;

if ((
  (R % 32 == 0) ||
  (r + 32 <= R)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {


if (r == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dV = dV_sram + (d_outer + d) / 8;
  *dV = simdgroup_matrix_storage<float>(0);
}

} else {


}

uint2 dO_src_offset(
  morton_offset.x + d_outer,
  morton_offset.y + r);
auto dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO, 128,
  dO_src_offset, false);



#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(d, c);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 128,
    dO_origin, false);

  // Issue one SIMD matmul instruction.
  dV_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], dO, /*accumulate=*/true);
}

}

if (
  (R % 32 == 0) &&
  (r + 32 == R)
) {

}



} else {


if (r == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dV = dV_sram + (d_outer + d) / 8;
  *dV = simdgroup_matrix_storage<float>(0);
}

} else {


}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 dO_offset(d_outer, r);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    dO, 128,
    dO_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort C_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort C_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_dimension, C_src_dimension);
  ushort2 tile_dst(D_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 128, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 dO_block_offset(
  morton_offset.x,
  morton_offset.y);
auto dO_src = (threadgroup bfloat*)(threadgroup_block);
dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO_src, 16,
  dO_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);




#pragma clang loop unroll(full)
for (ushort c = 0; c < (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(d, c);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 16,
    dO_origin, false);

  // Issue one SIMD matmul instruction.
  dV_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], dO, /*accumulate=*/true);
}

}

if (r + 32
    < R) {

#pragma clang loop unroll(full)
for (ushort c = (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(d, c);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 16,
    dO_origin, false);

  // Issue one SIMD matmul instruction.
  dV_sram[(d_outer + d) / 8].multiply(
    P_sram[c / 8], dO, /*accumulate=*/true);
}

}

} else {

}



}

}



  // dP^T = V * dO^T


simdgroup_matrix_storage<float> dP_sram[32 / 8];




#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if ((
  (R % 32 == 0) ||
  (r + 32 <= R)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {




uint2 dO_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + r);
auto dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO, 128,
  dO_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(c, d);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 128,
    dO_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    V_sram[(d_outer + d) / 8],
    dO, ((d_outer > 0) || (d > 0)));
}

}


} else {




threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 dO_offset(d_outer, r);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    dO, 128,
    dO_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort C_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort C_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 128, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 dO_block_offset(
  morton_offset.x,
  morton_offset.y);
auto dO_src = (threadgroup bfloat*)(threadgroup_block);
dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO_src, 16,
  dO_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(c, d);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 16,
    dO_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    V_sram[(d_outer + d) / 8],
    dO, ((d_outer > 0) || (d > 0)));
}

  if (r + 32
      < R) {

#pragma clang loop unroll(full)
for (ushort c = (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(c, d);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 16,
    dO_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    V_sram[(d_outer + d) / 8],
    dO, ((d_outer > 0) || (d > 0)));
}

  }
}


}

}


if (false) {
  ushort d_outer = 32;

if ((
  (R % 32 == 0) ||
  (r + 32 <= R)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {




uint2 dO_src_offset(
  morton_offset.y + d_outer,
  morton_offset.x + r);
auto dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO, 128,
  dO_src_offset, false);


#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(c, d);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 128,
    dO_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    V_sram[(d_outer + d) / 8],
    dO, ((d_outer > 0) || (d > 0)));
}

}


} else {




threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 dO_offset(d_outer, r);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    dO, 128,
    dO_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_src_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort D_dst_dimension = 16;
  ushort C_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort C_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_src_dimension, C_src_dimension);
  ushort2 tile_dst(D_dst_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 128, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 dO_block_offset(
  morton_offset.x,
  morton_offset.y);
auto dO_src = (threadgroup bfloat*)(threadgroup_block);
dO_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dO_src, 16,
  dO_block_offset, true);
threadgroup_barrier(mem_flags::mem_threadgroup);



#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {

#pragma clang loop unroll(full)
for (ushort c = 0; c < (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(c, d);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 16,
    dO_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    V_sram[(d_outer + d) / 8],
    dO, ((d_outer > 0) || (d > 0)));
}

  if (r + 32
      < R) {

#pragma clang loop unroll(full)
for (ushort c = (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c < 32; c += 8) {
  // Load the RHS from memory.
  ushort2 dO_origin(c, d);
  simdgroup_matrix_storage<bfloat> dO;
  dO.load(
    dO_src, 16,
    dO_origin, true);

  // Issue one SIMD matmul instruction.
  dP_sram[c / 8].multiply(
    V_sram[(d_outer + d) / 8],
    dO, ((d_outer > 0) || (d > 0)));
}

  }
}


}

}



  // dS^T = P^T * (dP^T - D) * scaleFactor


simdgroup_matrix_storage<float> dS_sram[32 / 8];

if (true && (
  (R % 32 == 0) ||
  (r + 32 <= R)
)) {

auto D_src = D + r * 4;


#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  ushort2 D_origin(c + morton_offset.x, 0);
  simdgroup_matrix_storage<float> D;
  D.load(
    D_src, 4,
    D_origin, true);
  auto D_elements = *(D.thread_elements());


auto P = *(P_sram[c / 8].thread_elements());
auto dP = *(dP_sram[c / 8].thread_elements());
auto dS = vec<float, 2>(
  float2(P) * (float2(dP) * 0.17677669 - float2(D_elements)));
*(dS_sram[c / 8].thread_elements()) = dS;

}

} else {

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  auto D_src = D + r * 4;
  auto D_dst =
  (threadgroup float*)(threadgroup_block);

  ushort R_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort R_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(R_src_dimension));

  // Issue an async copy.
  simdgroup_event event(lane_id);
  event.async_copy(
    D_dst, 1, ushort2(R_dst_dimension, 1),
    D_src, 4, ushort2(R_src_dimension, 1), true);
  simdgroup_event::wait(1, &event);
}


auto D_src =
(threadgroup float*)(threadgroup_block);
D_src += morton_offset.x;
threadgroup_barrier(mem_flags::mem_threadgroup);


#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {
  ushort2 D_origin(c, 0);
  simdgroup_matrix_storage<float> D;
  D.load(
    D_src, 1,
    D_origin, false);
  auto D_elements = *(D.thread_elements());


auto P = *(P_sram[c / 8].thread_elements());
auto dP = *(dP_sram[c / 8].thread_elements());
auto dS = vec<float, 2>(
  float2(P) * (float2(dP) * 0.17677669 - float2(D_elements)));
*(dS_sram[c / 8].thread_elements()) = dS;

}

}


  // dK += dS^T * Q


#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if ((
  (R % 32 == 0) ||
  (r + 32 <= R)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {


if (r == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dK = dK_sram + (d_outer + d) / 8;
  *dK = simdgroup_matrix_storage<float>(0);
}

} else {


}

uint2 Q_src_offset(
  morton_offset.x + d_outer,
  morton_offset.y + r);
auto Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q, 384,
  Q_src_offset, false);



#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(d, c);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 384,
    Q_origin, false);

  // Issue one SIMD matmul instruction.
  dK_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], Q, /*accumulate=*/true);
}

}

if (
  (R % 32 == 0) &&
  (r + 32 == R)
) {

}



} else {


if (r == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dK = dK_sram + (d_outer + d) / 8;
  *dK = simdgroup_matrix_storage<float>(0);
}

} else {


}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 Q_offset(d_outer, r);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    Q, 384,
    Q_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort C_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort C_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_dimension, C_src_dimension);
  ushort2 tile_dst(D_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 Q_block_offset(
  morton_offset.x,
  morton_offset.y);
auto Q_src = (threadgroup bfloat*)(threadgroup_block);
Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q_src, 16,
  Q_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);




#pragma clang loop unroll(full)
for (ushort c = 0; c < (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(d, c);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 16,
    Q_origin, false);

  // Issue one SIMD matmul instruction.
  dK_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], Q, /*accumulate=*/true);
}

}

if (r + 32
    < R) {

#pragma clang loop unroll(full)
for (ushort c = (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(d, c);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 16,
    Q_origin, false);

  // Issue one SIMD matmul instruction.
  dK_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], Q, /*accumulate=*/true);
}

}

} else {

}



}

}


if (false) {
  ushort d_outer = 32;

if ((
  (R % 32 == 0) ||
  (r + 32 <= R)
) && (
  (32 % 8 == 0) ||
  (d_outer + 16 <= 32)
)) {


if (r == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dK = dK_sram + (d_outer + d) / 8;
  *dK = simdgroup_matrix_storage<float>(0);
}

} else {


}

uint2 Q_src_offset(
  morton_offset.x + d_outer,
  morton_offset.y + r);
auto Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q, 384,
  Q_src_offset, false);



#pragma clang loop unroll(full)
for (ushort c = 0; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(d, c);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 384,
    Q_origin, false);

  // Issue one SIMD matmul instruction.
  dK_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], Q, /*accumulate=*/true);
}

}

if (
  (R % 32 == 0) &&
  (r + 32 == R)
) {

}



} else {


if (r == 0) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  auto dK = dK_sram + (d_outer + d) / 8;
  *dK = simdgroup_matrix_storage<float>(0);
}

} else {


}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 Q_offset(d_outer, r);
  auto src = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    Q, 384,
    Q_offset, false);
  auto dst = (threadgroup bfloat*)(threadgroup_block);

  ushort D_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort C_src_dimension = min(
    uint(32),
    uint(R - r));
  ushort C_dst_dimension = max(
    ushort((((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8),
    ushort(C_src_dimension));
  ushort2 tile_src(D_dimension, C_src_dimension);
  ushort2 tile_dst(D_dimension, C_dst_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 16, tile_dst,
    src, 384, tile_src, false);
  simdgroup_event::wait(1, &event);
}


ushort2 Q_block_offset(
  morton_offset.x,
  morton_offset.y);
auto Q_src = (threadgroup bfloat*)(threadgroup_block);
Q_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  Q_src, 16,
  Q_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);




#pragma clang loop unroll(full)
for (ushort c = 0; c < (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(d, c);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 16,
    Q_origin, false);

  // Issue one SIMD matmul instruction.
  dK_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], Q, /*accumulate=*/true);
}

}

if (r + 32
    < R) {

#pragma clang loop unroll(full)
for (ushort c = (((R % 32 == 0) ? 32 : R % 32) + 7) / 8 * 8; c < 32; c += 8) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  // Load the RHS from memory.
  ushort2 Q_origin(d, c);
  simdgroup_matrix_storage<bfloat> Q;
  Q.load(
    Q_src, 16,
    Q_origin, false);

  // Issue one SIMD matmul instruction.
  dK_sram[(d_outer + d) / 8].multiply(
    dS_sram[c / 8], Q, /*accumulate=*/true);
}

}

} else {

}



}

}


}




#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if (true && (
  (32 % 16 == 0) ||
  (d_outer + 16 <= 32)
)) {


uint2 dK_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, C - 1));
auto dK_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dK, 384,
  dK_src_offset, false);

if (
  false ||
  (parallelization_group_offset + sidx * 8 + morton_offset.y < C)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dK_origin(d, 0);
  dK_sram[(d_outer + d) / 8].store_bfloat(
    dK_src, 384,
    dK_origin, false);
}

}

} else {



ushort2 dK_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto dK_src =
(threadgroup bfloat*)(threadgroup_block);

dK_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dK_src, 16,
  dK_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);

if (d_outer + 16 <= 32) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dK_origin(d, 0);
  dK_sram[(d_outer + d) / 8].store_bfloat(
    dK_src, 16,
    dK_origin, false);
}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 0; d += 8) {
  ushort2 dK_origin(d, 0);
  dK_sram[(d_outer + d) / 8].store_bfloat(
    dK_src, 16,
    dK_origin, false);
}

}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 dK_offset(d_outer, parallelization_group_offset);
  auto src = (threadgroup bfloat*)(threadgroup_block);
  auto dst = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    dK, 384,
    dK_offset, false);

  ushort D_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort R_dimension = min(
    uint(16),
    uint(C - parallelization_group_offset));
  ushort2 tile(D_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 384, tile,
    src, 16, tile,
    false);
  simdgroup_event::wait(1, &event);
}


}

}



#pragma clang loop unroll(full)
for (
  ushort d_outer = 0;
  d_outer < 32;
  d_outer += 16
) {

if (true && (
  (32 % 16 == 0) ||
  (d_outer + 16 <= 32)
)) {


uint2 dV_src_offset(
  morton_offset.x + d_outer,
  min(parallelization_group_offset + sidx * 8 + morton_offset.y, C - 1));
auto dV_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dV, 384,
  dV_src_offset, false);

if (
  false ||
  (parallelization_group_offset + sidx * 8 + morton_offset.y < C)
) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dV_origin(d, 0);
  dV_sram[(d_outer + d) / 8].store_bfloat(
    dV_src, 384,
    dV_origin, false);
}

}

} else {



ushort2 dV_block_offset(
  morton_offset.x,
  morton_offset.y + sidx * 8);
auto dV_src =
(threadgroup bfloat*)(threadgroup_block);

dV_src = simdgroup_matrix_storage<bfloat>
::apply_offset(
  dV_src, 16,
  dV_block_offset, false);
threadgroup_barrier(mem_flags::mem_threadgroup);

if (d_outer + 16 <= 32) {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 16; d += 8) {
  ushort2 dV_origin(d, 0);
  dV_sram[(d_outer + d) / 8].store_bfloat(
    dV_src, 16,
    dV_origin, false);
}

} else {

#pragma clang loop unroll(full)
for (ushort d = 0; d < 0; d += 8) {
  ushort2 dV_origin(d, 0);
  dV_sram[(d_outer + d) / 8].store_bfloat(
    dV_src, 16,
    dV_origin, false);
}

}

threadgroup_barrier(mem_flags::mem_threadgroup);
if (sidx == 0) {
  uint2 dV_offset(d_outer, parallelization_group_offset);
  auto src = (threadgroup bfloat*)(threadgroup_block);
  auto dst = simdgroup_matrix_storage<bfloat>
  ::apply_offset(
    dV, 384,
    dV_offset, false);

  ushort D_dimension = min(
    ushort(16),
    ushort(32 - d_outer));
  ushort R_dimension = min(
    uint(16),
    uint(C - parallelization_group_offset));
  ushort2 tile(D_dimension, R_dimension);

  simdgroup_event event(lane_id);
  event.async_copy(
    dst, 384, tile,
    src, 16, tile,
    false);
  simdgroup_event::wait(1, &event);
}


}

}

}
