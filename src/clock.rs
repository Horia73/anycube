//! Processor time of this process, all threads together.
//!
//! The searches run on every core, so on a machine that is busy with other
//! work the time on the wall says little about what a step costs; the
//! processor time it used says more.

#[repr(C)]
struct TimeVal {
    sec: i64,
    usec: i32,
    _pad: i32,
}

#[repr(C)]
struct RUsage {
    user: TimeVal,
    system: TimeVal,
    rest: [i64; 14],
}

extern "C" {
    fn getrusage(who: i32, usage: *mut RUsage) -> i32;
}

/// Milliseconds of processor time used so far, user and system.
pub fn cpu_ms() -> f64 {
    let mut u = RUsage { user: TimeVal { sec: 0, usec: 0, _pad: 0 }, system: TimeVal { sec: 0, usec: 0, _pad: 0 }, rest: [0; 14] };
    // RUSAGE_SELF is 0 on macOS and Linux; the structure starts with the two times on both.
    if unsafe { getrusage(0, &mut u) } != 0 {
        return 0.0;
    }
    (u.user.sec + u.system.sec) as f64 * 1e3 + (u.user.usec + u.system.usec) as f64 / 1e3
}
