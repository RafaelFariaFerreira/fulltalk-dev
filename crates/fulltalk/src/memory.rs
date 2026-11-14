//! Allocator tuning.
//!
//! glibc raises its `mmap` threshold dynamically after large blocks are freed.
//! ONNX Runtime allocates and frees hundreds of MB of temporaries while loading
//! the model, so later "large" buffers land in per-thread heap arenas and are
//! never returned to the OS — measured: ~1.15 GB resident instead of ~0.8 GB.
//! Pinning the threshold keeps big buffers in `mmap`, which is released on free.

/// Call once at startup, before any thread is spawned.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[allow(unsafe_code)]
pub fn tune_allocator() {
    const MMAP_THRESHOLD: libc::c_int = 128 * 1024;
    // SAFETY: `mallopt` only adjusts allocator parameters; it is called before
    // other threads exist, so no allocation can race with it.
    let ok = unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, MMAP_THRESHOLD) };
    if ok != 1 {
        tracing::debug!("mallopt(M_MMAP_THRESHOLD) was rejected");
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn tune_allocator() {}
