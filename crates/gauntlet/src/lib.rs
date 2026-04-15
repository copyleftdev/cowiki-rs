//! # gauntlet
//!
//! VOPR-inspired adversarial test suite for cowiki primitives.
//!
//! ## Philosophy
//!
//! Property tests (proptest) generate random inputs and check properties.
//! The gauntlet goes further:
//!
//! - **Deterministic simulation**: seeded PRNG, fully reproducible. If it
//!   fails on seed 0xDEAD, you can replay that exact sequence.
//!
//! - **Pathological topologies**: handcrafted graphs designed to break things —
//!   stars, long chains, complete graphs, disconnected components, single-node.
//!
//! - **Numerical torture**: machine-epsilon weights, near-overflow activations,
//!   denormalized floats, near-threshold values.
//!
//! - **Chaos injection**: mid-simulation weight corruption, node deletion,
//!   sudden topology changes.
//!
//! - **Long-horizon stability**: run 1000+ REM cycles checking that no
//!   invariant ever breaks.
//!
//! - **Adversarial knapsack**: worst-case constructions from the knapsack
//!   approximation literature.
//!
//! Every test that takes a seed prints it on failure so you can reproduce.

pub mod chaos;
pub mod numerical;
pub mod pathological;
pub mod stability;
pub mod adversarial_knapsack;
