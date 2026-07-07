//! Port interfaces (docs/20-architecture.md §2.4).
//!
//! Ports are the trait boundary between the application and its adapters:
//! every side effect crosses one (I2/I4). This crate stays `no_std` and
//! depends on nothing — the DTOs a port hands across are defined *here*, so the
//! interface layer never imports `providence-core` and no adapter does either
//! (ADR 0020 §1). Adding or changing a port is an architectural change and
//! requires an ADR (docs/20-architecture.md §5 rule 5).
//!
//! Realised ports:
//! - [`RendererPort`] — present the terrain as a drawable [`TerrainFrame`]
//!   snapshot; the workbench renderer adapter implements it (ADR 0020).
//! - [`SimDriver`] — the interactive seam (ADR 0022): the renderer *holds* one
//!   to submit a discrete [`TerrainCommand`] and pull fresh snapshots to draw;
//!   the application implements it over a terrain world and its recorded log.
//!   The remaining ports (`ConfigPort`, `LLMOpponentPort`, …) land with the
//!   subsystems that need them, each behind its own ADR.

#![no_std]
#![forbid(unsafe_code)]

/// A vertex's integer height in a [`TerrainFrame`]. Mirrors the core's `Height`
/// (ADR 0017) as a plain `i32` so `providence-ports` need not — and must not —
/// import `providence-core`: the frame is a *derived snapshot*, not core state.
pub type Height = i32;

/// A read-only, derived snapshot of the terrain height field handed to a
/// [`RendererPort`] to draw (ADR 0020 §1).
///
/// It carries only what a renderer needs — the grid dimensions and a borrow of
/// the row-major heights — and **no** simulation or camera/view state. The
/// application builds one from the core's height field and passes it in; the
/// renderer only ever sees this snapshot, never the core. Row-major: the vertex
/// at `(x, y)` is `heights[y * width + x]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainFrame<'a> {
    width: u32,
    height: u32,
    heights: &'a [Height],
}

impl<'a> TerrainFrame<'a> {
    /// Wrap a row-major height buffer as a drawable snapshot.
    ///
    /// `heights` is expected to be `width * height` long in row-major order;
    /// [`TerrainFrame::get`] bounds-checks every access, so a mismatched buffer
    /// yields `None` rather than a panic.
    #[must_use]
    pub const fn new(width: u32, height: u32, heights: &'a [Height]) -> Self {
        Self {
            width,
            height,
            heights,
        }
    }

    /// Grid width in vertices.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Grid height (depth) in vertices.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The backing row-major height buffer.
    #[must_use]
    pub const fn heights(&self) -> &[Height] {
        self.heights
    }

    /// The height at `(x, y)`, or `None` if the coordinate is out of bounds or
    /// the backing buffer is too short for the stated dimensions.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<Height> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = y as usize * self.width as usize + x as usize;
        self.heights.get(index).copied()
    }
}

/// Presents terrain as a drawable surface (ADR 0020 §1).
///
/// The composition root drives this to draw the world. Implementors own their
/// view/camera state — moving the camera is adapter-local and never crosses
/// this boundary (ADR 0020 §3), so nothing a renderer does can mutate the
/// simulation. The on-screen `wgpu`/`winit` renderer, a headless
/// render-to-PNG capture, and a no-op test double all realise it (ADR 0020 §2).
pub trait RendererPort {
    /// Present the given terrain snapshot as the current frame. Called by the
    /// window/redraw loop whenever a fresh frame should be drawn.
    fn present(&mut self, frame: TerrainFrame<'_>);
}

/// A single, discrete shaping command — the *one* vocabulary for "shape a
/// vertex" (ADR 0022 §1, ADR 0019 item 4).
///
/// It is produced at the input edge, consumed by the core, and recorded in a
/// session's replay log — every layer speaks this one type, so there is no
/// duplicate command type and no translation site. It carries **integer grid
/// coordinates only**: no float, no frame rate, no wall-clock. That is the
/// constraint that keeps a live, mutating sim replayable bit-for-bit (I3): a
/// gesture wanting finer control emits *more* commands, never a continuous one.
///
/// It lives in `providence-ports` for the same reason [`TerrainFrame`] does —
/// it is a plain value a port hands across a boundary, so defining it in the
/// interface crate keeps every adapter (and the core) free of a translation
/// type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainCommand {
    /// Raise the vertex at `(x, y)` by one step, cascading to restore the step
    /// invariant (ADR 0017 §3).
    Raise {
        /// Grid column of the target vertex.
        x: u32,
        /// Grid row of the target vertex.
        y: u32,
    },
    /// Lower the vertex at `(x, y)` by one step — the mirror of `Raise`.
    Lower {
        /// Grid column of the target vertex.
        x: u32,
        /// Grid row of the target vertex.
        y: u32,
    },
}

/// The interactive simulation seam (ADR 0022 §3): the renderer **holds** a
/// `SimDriver`, submitting shaping commands and pulling fresh snapshots to draw,
/// without ever importing the core.
///
/// The application implements it over a terrain world and its recorded command
/// log; the composition root passes `&mut dyn SimDriver` into the renderer's
/// run loop. It sits *alongside* [`RendererPort::present`], not replacing it, so
/// the static headless/no-op renderer adapters are unaffected (ADR 0022 §4).
pub trait SimDriver {
    /// The single input entry point (ADR 0022 §3): apply `command` to the sim
    /// and **record** it. Input reaches the sim *only* through here, as a
    /// discrete [`TerrainCommand`] — so a session is exactly `seed + params +
    /// log` and replays bit-for-bit (I3).
    fn submit(&mut self, command: TerrainCommand);

    /// Grid width in vertices — a frame-production read for the renderer.
    fn width(&self) -> u32;

    /// Grid height (depth) in vertices — a frame-production read.
    fn height(&self) -> u32;

    /// The current row-major height snapshot to draw. Row-major, mirroring
    /// [`TerrainFrame`]: the vertex at `(x, y)` is `heights()[y * width() + x]`.
    fn heights(&self) -> &[Height];

    /// A revision that **bumps whenever the heights change**, so the renderer
    /// can tell a fresh frame from a repeat and animate the change (ADR 0022
    /// §3). A no-op command (out of bounds, at the ceiling, or refused by an
    /// immovable) leaves it unchanged.
    fn revision(&self) -> u64;
}

#[cfg(test)]
mod tests {
    use super::{Height, SimDriver, TerrainCommand};

    /// A minimal in-crate `SimDriver` proving the port is implementable — and
    /// object-safe — without importing the core: it serves a tiny fixed grid and
    /// records the last command, bumping its revision on each submit.
    struct MockDriver {
        cells: [Height; 4],
        revision: u64,
        last: Option<TerrainCommand>,
    }

    impl SimDriver for MockDriver {
        fn submit(&mut self, command: TerrainCommand) {
            self.last = Some(command);
            self.revision += 1;
        }
        fn width(&self) -> u32 {
            2
        }
        fn height(&self) -> u32 {
            2
        }
        fn heights(&self) -> &[Height] {
            &self.cells
        }
        fn revision(&self) -> u64 {
            self.revision
        }
    }

    #[test]
    fn a_mock_realises_the_sim_driver_port() {
        let mut driver = MockDriver {
            cells: [0, 1, 1, 2],
            revision: 0,
            last: None,
        };
        // The snapshot reads are consistent (width × height == buffer length).
        assert_eq!(
            driver.width() as usize * driver.height() as usize,
            driver.heights().len()
        );
        // Submitting drives the sim through the port and bumps the revision.
        driver.submit(TerrainCommand::Raise { x: 0, y: 0 });
        assert_eq!(driver.last, Some(TerrainCommand::Raise { x: 0, y: 0 }));
        assert_eq!(driver.revision(), 1);

        // The port is object-safe: usable behind a `&mut dyn` as the renderer
        // will hold it (ADR 0022 §4).
        let dynamic: &mut dyn SimDriver = &mut driver;
        dynamic.submit(TerrainCommand::Lower { x: 1, y: 1 });
        assert_eq!(dynamic.revision(), 2);
    }
}
