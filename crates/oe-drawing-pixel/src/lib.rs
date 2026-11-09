//! Pixel-level lower half of the drawing route.
//!
//! A vision model proposes members of drawing expressions (dimension lines, extension lines, arrow
//! tips, text boxes, centre lines, hatch regions) as approximate pixel coordinates. This crate refines
//! those proposals on the binary ink of the page and vectorises the remaining ink into line and arc
//! primitives. Everything is a pure deterministic algorithm: no network, no model calls, no files; the
//! only I/O is decoding a PNG from bytes ([`Ink::from_png`]).
//!
//! Conventions shared by every function:
//! - Coordinates are pixel-centre coordinates: pixel `(x, y)` is centred at `(x as f64, y as f64)`,
//!   x grows to the right and y grows downwards (image order). Angles are `atan2(dy, dx)` in that
//!   frame; callers flip the sign when converting to a y-up CAD frame.
//! - "Ink within 1.5 px" is the support test everywhere: a sample point is supported when some dark
//!   pixel centre lies within 1.5 px of it. Supports are fractions in `[0, 1]` of 1-px samples.
//! - Nothing is kept silently: every refined member reports its support before and after refinement
//!   and how far it moved, every vectorised stroke reports its support and the number of skeleton
//!   pixels that produced it. Callers decide what to accept.
//! - Iteration orders are fixed (raster order, fixed neighbour order, sorted pixel lists); no hashing
//!   with random state is used, so results are reproducible bit for bit.

//!
//! The synthesis half ([`synthesize_page`]) turns the observation files written outside the engine
//! into the pixel expression graph: [`observe`] reads them into proposals with provenance,
//! [`refine`] fits them on the ink, [`dedupe`] merges tile overlaps, [`calibrate`] finds the page
//! scale, [`structure`] vectorises the unexplained ink, and [`synthesize`] assembles the graph, its
//! provenance and an overlay rendering. Its only I/O is still bytes in, bytes out.

pub mod calibrate;
mod callouts;
mod chains;
pub mod dedupe;
mod geom;
mod ink;
mod lines;
pub mod linetypes;
mod members;
pub mod observe;
pub mod overlay;
mod pattern;
pub mod refine;
mod scale;
mod skeleton;
mod stroke;
pub mod structure;
pub mod synthesize;
pub mod textform;
mod vector;

pub use ink::Ink;
pub use members::{Fit, Segment, arrow_tip, erase, fit_segment, snap_point, text_box_fit};
pub use scale::calibrate_scale;
pub use stroke::{Pattern, Primitive, Stroke};
pub use synthesize::{PageInput, SynthesisOutput, synthesize_page};
pub use vector::{merge_collinear, vectorize};
