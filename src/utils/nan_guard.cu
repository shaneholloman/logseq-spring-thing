// NaN/Inf guard kernel for GPU physics pipeline (ADR-070 D1.2)
// Scans position arrays for non-finite values after each physics step.
// Compiled by build.rs as PTX + object file, linked into libthrust_wrapper.a.
// FFI declarations live in the Rust side (src/utils/unified_gpu_compute/types.rs).

#include <cuda_runtime.h>
#include <math.h>

// ---------------------------------------------------------------------------
// Device helpers
// ---------------------------------------------------------------------------

// Warp-level OR reduction: returns nonzero in lane 0 if ANY lane has a
// nonzero value.
__device__ __forceinline__ int warpReduceOr(int val) {
    #pragma unroll
    for (int offset = 16; offset > 0; offset /= 2)
        val |= __shfl_down_sync(0xffffffff, val, offset);
    return val;
}

// ---------------------------------------------------------------------------
// Kernel: check_nan_positions_kernel
// ---------------------------------------------------------------------------
// Positions are stored as float triples (x, y, z) contiguously:
//   positions[i*3 + 0] = x
//   positions[i*3 + 1] = y
//   positions[i*3 + 2] = z
//
// The kernel scans all 3*num_nodes floats.  If ANY value is NaN or +/-Inf
// it atomically ORs 1 into *result.
//
// Strategy:
//   1. Each thread checks a grid-stride slice of the flat array.
//   2. Warp-level OR reduction collapses 32 thread flags into one.
//   3. Warp leader writes to shared memory.
//   4. First warp reduces shared memory across warps.
//   5. Block leader does a single atomicOr to global *result.
//
// This minimises global atomics to at most one per block.
// ---------------------------------------------------------------------------

__global__ void check_nan_positions_kernel(
    const float* __restrict__ positions,
    const int    num_elements,   // = num_nodes * 3
    int*         result          // output flag, caller must zero before launch
) {
    extern __shared__ int s_flag[];  // one int per warp

    const int tid = threadIdx.x;
    const int global_tid = blockIdx.x * blockDim.x + tid;
    const int stride = blockDim.x * gridDim.x;

    // --- Phase 1: thread-local scan via grid-stride loop ----------------
    int thread_bad = 0;

    #pragma unroll 4
    for (int i = global_tid; i < num_elements; i += stride) {
        if (!isfinite(positions[i])) {
            thread_bad = 1;
            break;  // early-out: one bad value is enough
        }
    }

    // --- Phase 2: warp-level OR reduction --------------------------------
    int warp_bad = warpReduceOr(thread_bad);

    const int lane   = tid & 31;
    const int warp_id = tid >> 5;

    if (lane == 0) {
        s_flag[warp_id] = warp_bad;
    }

    __syncthreads();

    // --- Phase 3: first warp reduces across all warps in the block -------
    if (tid < 32) {
        const int num_warps = (blockDim.x + 31) >> 5;
        int val = (tid < num_warps) ? s_flag[tid] : 0;
        val = warpReduceOr(val);

        // --- Phase 4: single atomic write per block ----------------------
        if (tid == 0 && val != 0) {
            atomicOr(result, 1);
        }
    }
}

// ---------------------------------------------------------------------------
// Host API
// ---------------------------------------------------------------------------

extern "C" {

/// Launch the NaN guard kernel on the given positions buffer.
///
/// @param positions   Device pointer to float array of length num_nodes*3
///                    (x0,y0,z0, x1,y1,z1, ...).
/// @param num_nodes   Number of graph nodes.
/// @param result      Device pointer to a single int.  Will be set to 1 if
///                    ANY non-finite value is found, 0 otherwise.
///                    Caller must allocate; this function zeroes it before
///                    the kernel launch.
/// @param stream      CUDA stream for async execution (0 for default).
/// @return            cudaSuccess on success, or the relevant CUDA error.
cudaError_t check_nan_positions(
    const float* positions,
    int          num_nodes,
    int*         result,
    cudaStream_t stream
) {
    if (num_nodes <= 0) {
        // Nothing to check -- zero the flag and return.
        cudaError_t err = cudaMemsetAsync(result, 0, sizeof(int), stream);
        return err;
    }

    const int num_elements = num_nodes * 3;

    // Zero the result flag before launch.
    cudaError_t err = cudaMemsetAsync(result, 0, sizeof(int), stream);
    if (err != cudaSuccess) return err;

    // Launch configuration: 256 threads/block, enough blocks to cover all
    // elements with at least one pass (capped to keep grid reasonable).
    const int block_size = 256;
    const int max_blocks = 1024;
    int grid_size = (num_elements + block_size - 1) / block_size;
    if (grid_size > max_blocks) grid_size = max_blocks;

    // Shared memory: one int per warp in a block.
    const int num_warps = (block_size + 31) / 32;
    const size_t smem_bytes = num_warps * sizeof(int);

    check_nan_positions_kernel<<<grid_size, block_size, smem_bytes, stream>>>(
        positions, num_elements, result
    );

    // Check for launch errors (does NOT synchronise).
    err = cudaGetLastError();
    return err;
}

/// Synchronous convenience wrapper: launches the kernel, synchronises, and
/// copies the result back to host memory.
///
/// @param positions        Device pointer to positions (num_nodes * 3 floats).
/// @param num_nodes        Number of graph nodes.
/// @param host_result_out  Host pointer where the 0/1 flag is written.
/// @return                 cudaSuccess on success.
cudaError_t check_nan_positions_sync(
    const float* positions,
    int          num_nodes,
    int*         host_result_out
) {
    // Allocate a single device int for the flag.
    int* d_result = nullptr;
    cudaError_t err = cudaMalloc(&d_result, sizeof(int));
    if (err != cudaSuccess) return err;

    err = check_nan_positions(positions, num_nodes, d_result, 0);
    if (err != cudaSuccess) { cudaFree(d_result); return err; }

    err = cudaDeviceSynchronize();
    if (err != cudaSuccess) { cudaFree(d_result); return err; }

    err = cudaMemcpy(host_result_out, d_result, sizeof(int), cudaMemcpyDeviceToHost);
    cudaFree(d_result);
    return err;
}

}  // extern "C"
