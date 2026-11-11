// On `wasm32` the library holds the 2x2x2 and 3x3x3 solvers only: the
// modules marked below index tables too large for a 32-bit address space,
// or depend on one that does, and are left out of that build.
pub mod api;
#[cfg(all(feature = "app", not(target_arch = "wasm32")))]
pub mod app;
pub mod axes;
pub mod big1;
pub mod big2;
pub mod c2;
pub mod c3;
pub mod c5;
pub mod centres5;
pub mod clock;
pub mod cache;
#[cfg(not(target_arch = "wasm32"))]
pub mod ctab;
pub mod cube;
pub mod cuben;
pub mod edges5;
pub mod first3;
#[cfg(not(target_arch = "wasm32"))]
pub mod free;
#[cfg(not(target_arch = "wasm32"))]
pub mod joint;
#[cfg(not(target_arch = "wasm32"))]
pub mod nnn;
pub mod opt3;
pub mod orbits;
pub mod rings;
pub mod search;
#[cfg(not(target_arch = "wasm32"))]
pub mod solver;
pub mod solver5;
#[cfg(not(target_arch = "wasm32"))]
pub mod solver6;
pub mod solver7;
pub mod stage5;
pub mod stage6;
pub mod stage7;
#[cfg(not(target_arch = "wasm32"))]
pub mod staging;
pub mod sym;
pub mod tables;
pub mod two3;
