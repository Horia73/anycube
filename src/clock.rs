//! Processor time of this process, all threads together.
//!
//! The searches run on every core, so on a machine that is busy with other
//! work the time on the wall says little about what a step costs; the
//! processor time it used says more.

#[cfg(unix)]
#[repr(C)]
struct TimeVal {
    sec: i64,
    usec: i32,
    _pad: i32,
}

#[cfg(unix)]
#[repr(C)]
struct RUsage {
    user: TimeVal,
    system: TimeVal,
    rest: [i64; 14],
}

#[cfg(unix)]
extern "C" {
    fn getrusage(who: i32, usage: *mut RUsage) -> i32;
}

/// Milliseconds of processor time used so far, user and system.
#[cfg(unix)]
pub fn cpu_ms() -> f64 {
    let mut u = RUsage { user: TimeVal { sec: 0, usec: 0, _pad: 0 }, system: TimeVal { sec: 0, usec: 0, _pad: 0 }, rest: [0; 14] };
    // RUSAGE_SELF is 0 on macOS and Linux; the structure starts with the two times on both.
    if unsafe { getrusage(0, &mut u) } != 0 {
        return 0.0;
    }
    (u.user.sec + u.system.sec) as f64 * 1e3 + (u.user.usec + u.system.usec) as f64 / 1e3
}

/// Milliseconds of processor time used so far, user and system.
#[cfg(windows)]
pub fn cpu_ms() -> f64 {
    use std::ffi::c_void;

    #[derive(Clone, Copy, Default)]
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }

    let [mut creation, mut exit, mut kernel, mut user] = [FileTime::default(); 4];
    // The current process handle is a pseudo-handle. The four outputs are
    // separate writable FILETIME values, which the API fills on success.
    let success = unsafe {
        GetProcessTimes(GetCurrentProcess(), &mut creation, &mut exit, &mut kernel, &mut user)
    };
    if success == 0 {
        return 0.0;
    }
    let ticks = |time: FileTime| (u64::from(time.high) << 32) | u64::from(time.low);
    (ticks(kernel) + ticks(user)) as f64 / 10_000.0
}

#[cfg(not(any(unix, windows)))]
pub fn cpu_ms() -> f64 {
    0.0
}
