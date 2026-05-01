// Per-kernel CUDA timing wrapper (ADR-070 D1.3)
// Header-only.  Include from any .cu file that needs per-kernel timing.
//
// Usage — struct API:
//
//     KernelTimer timer;
//     timer.start(stream);
//     my_kernel<<<grid, block, smem, stream>>>(args...);
//     timer.stop(stream);
//     float ms = timer.elapsed_ms();  // synchronises
//
// Usage — macro API (one-liner):
//
//     KernelTimer timer;
//     TIMED_KERNEL_LAUNCH(timer, my_kernel, grid, block, smem, stream, arg1, arg2);
//     float ms = timer.elapsed_ms();
//

#ifndef VISIONCLAW_KERNEL_TIMING_CUH
#define VISIONCLAW_KERNEL_TIMING_CUH

#include <cuda_runtime.h>
#include <cstdio>

// ---------------------------------------------------------------------------
// KernelTimer — lightweight RAII wrapper around cudaEvent pairs
// ---------------------------------------------------------------------------

struct KernelTimer {
    cudaEvent_t ev_start;
    cudaEvent_t ev_stop;
    bool        events_valid;

    /// Construct and create the underlying CUDA events.
    KernelTimer() : ev_start(nullptr), ev_stop(nullptr), events_valid(false) {
        cudaError_t e1 = cudaEventCreate(&ev_start);
        cudaError_t e2 = cudaEventCreate(&ev_stop);
        events_valid = (e1 == cudaSuccess && e2 == cudaSuccess);
        if (!events_valid) {
            fprintf(stderr, "[KernelTimer] cudaEventCreate failed: %s / %s\n",
                    cudaGetErrorString(e1), cudaGetErrorString(e2));
        }
    }

    /// Destroy CUDA events.
    ~KernelTimer() {
        if (ev_start) cudaEventDestroy(ev_start);
        if (ev_stop)  cudaEventDestroy(ev_stop);
    }

    // Non-copyable (events are owned resources).
    KernelTimer(const KernelTimer&)            = delete;
    KernelTimer& operator=(const KernelTimer&) = delete;

    // Movable.
    KernelTimer(KernelTimer&& other) noexcept
        : ev_start(other.ev_start), ev_stop(other.ev_stop),
          events_valid(other.events_valid) {
        other.ev_start     = nullptr;
        other.ev_stop      = nullptr;
        other.events_valid = false;
    }

    KernelTimer& operator=(KernelTimer&& other) noexcept {
        if (this != &other) {
            if (ev_start) cudaEventDestroy(ev_start);
            if (ev_stop)  cudaEventDestroy(ev_stop);
            ev_start     = other.ev_start;
            ev_stop      = other.ev_stop;
            events_valid = other.events_valid;
            other.ev_start     = nullptr;
            other.ev_stop      = nullptr;
            other.events_valid = false;
        }
        return *this;
    }

    /// Record the start event on the given stream.
    void start(cudaStream_t stream = 0) {
        if (events_valid) {
            cudaEventRecord(ev_start, stream);
        }
    }

    /// Record the stop event on the given stream.
    void stop(cudaStream_t stream = 0) {
        if (events_valid) {
            cudaEventRecord(ev_stop, stream);
        }
    }

    /// Return elapsed time in milliseconds between start() and stop().
    /// This call synchronises on the stop event (blocks until the kernel
    /// and event recording complete).  Returns -1.0f on error.
    float elapsed_ms() const {
        if (!events_valid) return -1.0f;

        cudaError_t err = cudaEventSynchronize(ev_stop);
        if (err != cudaSuccess) {
            fprintf(stderr, "[KernelTimer] cudaEventSynchronize failed: %s\n",
                    cudaGetErrorString(err));
            return -1.0f;
        }

        float ms = 0.0f;
        err = cudaEventElapsedTime(&ms, ev_start, ev_stop);
        if (err != cudaSuccess) {
            fprintf(stderr, "[KernelTimer] cudaEventElapsedTime failed: %s\n",
                    cudaGetErrorString(err));
            return -1.0f;
        }
        return ms;
    }
};

// ---------------------------------------------------------------------------
// TIMED_KERNEL_LAUNCH macro
// ---------------------------------------------------------------------------
// Wraps a kernel launch with start/stop timing.
//
//   TIMED_KERNEL_LAUNCH(timer, kernel, grid, block, shared_mem, stream, ...)
//
// Expands to:
//   timer.start(stream);
//   kernel<<<grid, block, shared_mem, stream>>>(__VA_ARGS__);
//   timer.stop(stream);
//
// The caller retrieves elapsed time later via timer.elapsed_ms().
// ---------------------------------------------------------------------------

#define TIMED_KERNEL_LAUNCH(timer, kernel, grid, block, shared, stream, ...) \
    do {                                                                     \
        (timer).start((stream));                                             \
        (kernel)<<<(grid), (block), (shared), (stream)>>>(__VA_ARGS__);      \
        (timer).stop((stream));                                              \
    } while (0)

#endif  // VISIONCLAW_KERNEL_TIMING_CUH
