//! The terrain world — the mutable bundle a live session shapes (ADR 0022 §2).
//!
//! [`World`] pairs the integer [`HeightField`] (ADR 0017 §1) with its
//! terrain-owned immovable [`FeatureMap`] (ADR 0017 §5), and exposes a single
//! [`apply`](World::apply) that dispatches a discrete [`TerrainCommand`] to the
//! existing bounded raise/lower cascade ([`super::shape`]) — immovable-refusal
//! and all. It is the core side of the interactive seam ADR 0022 fixes: the
//! application holds a `World` behind the [`SimDriver`](providence_ports::SimDriver)
//! port and feeds it recorded commands, while the renderer only ever sees a
//! derived snapshot, never the core.
//!
//! [`apply`](World::apply) is *structural command dispatch* — it introduces no
//! behavioural literal (I1); every shaping number already lives in
//! [`TerrainParams`].

use providence_config::TerrainParams;
use providence_ports::TerrainCommand;

use super::feature::FeatureMap;
use super::field::{Height, HeightField};
use super::shape::{ShapeOutcome, lower, raise};

/// A shapeable terrain world: the height field plus the immovables shaping must
/// not disturb (ADR 0022 §2).
///
/// `features` is `None` for a world with no immovables (a bare test field);
/// worldgen builds one with [`super::place_features`] and the composition root
/// bundles the two. Cloneable and `Eq` so a replay can compare two independently
/// stepped worlds bit-for-bit (I3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    field: HeightField,
    features: Option<FeatureMap>,
}

impl World {
    /// Bundle a generated `field` with its (optional) immovables.
    #[must_use]
    pub fn new(field: HeightField, features: Option<FeatureMap>) -> Self {
        Self { field, features }
    }

    /// The height field (read-only).
    #[must_use]
    pub fn field(&self) -> &HeightField {
        &self.field
    }

    /// Grid width in vertices.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.field.width()
    }

    /// Grid height (depth) in vertices.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.field.height()
    }

    /// The current row-major height buffer — the snapshot a renderer draws
    /// (backs [`SimDriver::heights`](providence_ports::SimDriver::heights),
    /// ADR 0022 §3).
    #[must_use]
    pub fn heights(&self) -> &[Height] {
        self.field.heights()
    }

    /// Apply one discrete shaping command, dispatching to the bounded cascade
    /// (ADR 0022 §2).
    ///
    /// `Raise` / `Lower` map to [`raise`] / [`lower`] at the command's integer
    /// coordinates, threading the world's immovables so a cascade that would
    /// disturb one is refused whole ([`ShapeOutcome::UNCHANGED`], ADR 0017 §5) —
    /// never silently destroying it. Returns exactly what the underlying op
    /// reports (the vertices moved and the mana it would cost).
    pub fn apply(&mut self, params: &TerrainParams, command: TerrainCommand) -> ShapeOutcome {
        let features = self.features.as_ref();
        match command {
            TerrainCommand::Raise { x, y } => raise(&mut self.field, x, y, params, features),
            TerrainCommand::Lower { x, y } => lower(&mut self.field, x, y, params, features),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::World;
    use crate::terrain::{Feature, FeatureMap, HeightField};
    use providence_config::{RaiseParams, TerrainParams};
    use providence_ports::TerrainCommand;

    const MAX_STEP: u32 = 1;

    /// Params mirroring the shipped unit step, an ample ceiling, and unit cost.
    fn params() -> TerrainParams {
        TerrainParams {
            max_step: MAX_STEP,
            max_height: 64,
            raise: RaiseParams { mana_cost: 1 },
        }
    }

    #[test]
    fn new_world_exposes_its_field_dimensions_and_heights() {
        let world = World::new(HeightField::flat(4, 3, 0), None);
        assert_eq!((world.width(), world.height()), (4, 3));
        assert_eq!(world.heights().len(), 4 * 3, "row-major buffer is w × h");
        assert_eq!(world.field().get(0, 0), Some(0));
    }

    #[test]
    fn apply_raise_dispatches_to_the_cascade() {
        let mut world = World::new(HeightField::flat(5, 5, 0), None);
        let outcome = world.apply(&params(), TerrainCommand::Raise { x: 2, y: 2 });
        assert_eq!(
            outcome.moved, 1,
            "a single raise on flat ground moves the target"
        );
        assert_eq!(outcome.cost, 1);
        assert_eq!(world.field().get(2, 2), Some(1));
        assert_eq!(
            world.heights()[2 * 5 + 2],
            1,
            "the heights snapshot reflects the applied command"
        );
    }

    #[test]
    fn apply_lower_is_the_mirror() {
        let mut world = World::new(HeightField::flat(3, 3, 0), None);
        let outcome = world.apply(&params(), TerrainCommand::Lower { x: 1, y: 1 });
        assert_eq!(outcome.moved, 1);
        assert_eq!(world.field().get(1, 1), Some(-1), "lower drops the target");
    }

    #[test]
    fn apply_threads_immovables_and_refuses_a_disturbing_cascade() {
        // A tree sits on an orthogonal neighbour of the target. The first raise
        // only lifts the target; the second would cascade into the tree, so the
        // whole op is refused and rolled back — proving `apply` passes the
        // world's immovables through (ADR 0017 §5).
        let mut cells = alloc::vec![None; 7 * 7];
        cells[3 * 7 + 2] = Some(Feature::Tree);
        let features = FeatureMap::from_cells(7, 7, cells).expect("a well-sized feature map");
        let mut world = World::new(HeightField::flat(7, 7, 0), Some(features));
        let params = params();

        world.apply(&params, TerrainCommand::Raise { x: 3, y: 3 });
        let before = world.field().clone();
        let outcome = world.apply(&params, TerrainCommand::Raise { x: 3, y: 3 });

        assert_eq!(outcome.moved, 0, "the cascade into the tree is refused");
        assert_eq!(
            world.field(),
            &before,
            "the field is left exactly as it was"
        );
    }
}
