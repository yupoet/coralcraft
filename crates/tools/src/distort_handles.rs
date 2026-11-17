//! CoralCraft (P3 Task E5): the Push/Pull and Zipper dedicated on-canvas handles, CorelDRAW's
//! Distort tool draggables over the shared effects. With the Selection tool and one object
//! carrying a visible `distort.zigZag` (Zipper) or `distort.puckerBloat` (Push/Pull) selected:
//! Zipper gets an amplitude handle above the top edge (drag up to grow `size`) and a frequency
//! handle right of the right edge (drag right to grow `ridges`, ten points each); Push/Pull gets
//! one amount handle above the top edge (drag right to bloat, left to pucker, two percent a
//! point). Drawing and hit testing share the geometry of [`DistortHandles::of`]; a drag previews
//! the existing `effect.setParams` command and commits as one undo step.

use serde_json::{Value, json};
use vectorcraft_doc::{Document, NodeId, OrientedBox, Selection};

use crate::input::Action;
use crate::{Mods, ToolContext};

/// The radius of the handles (screen points).
pub const HANDLE_RADIUS: f32 = 5.0;

/// How far (screen points) a handle sits outside the bounding box, clear of its own handles.
pub const HANDLE_GAP: f64 = 22.0;

/// How near (screen points) a press must come to a handle to take it.
const REACH: f64 = 8.0;

/// The Zipper effect.
pub const ZIPPER: &str = "distort.zigZag";
/// The Push/Pull effect.
pub const PUSH_PULL: &str = "distort.puckerBloat";

#[derive(Clone, Debug)]
/// The dedicated handles of a selected distorted object: the amplitude (Zipper `size`) and
/// amount (Push/Pull `amount`) handle above the top edge, and the Zipper's frequency handle
/// right of the right edge — whichever the object's effect gives, in document space.
pub struct DistortHandles {
    /// The object and its effect's index in the stack.
    id: NodeId,
    index: usize,
    zipper: bool,
    /// The parameters the press found (with the effect's defaults for missing keys).
    from: Value,
    /// Above the top edge's middle.
    pub amplitude: vectorcraft_geom::Point,
    /// Right of the right edge's middle (Zipper alone).
    pub frequency: Option<vectorcraft_geom::Point>,
    /// Push/Pull's own name for its one handle (the same place as the amplitude's).
    pub amount: Option<vectorcraft_geom::Point>,
}

impl DistortHandles {
    /// The handles of the one selected object carrying a visible Zipper or Push/Pull effect,
    /// else `None`.
    pub fn of(doc: &Document, selection: &Selection, _frame: Option<OrientedBox>, zoom: f64) -> Option<Self> {
        if selection.objects.len() != 1 {
            return None;
        }
        let id = selection.objects[0];
        let node = doc.node(id)?;
        let bounds = node.geometric_bounds()?;
        let (index, fx) = node.appearance.effects.iter().enumerate().find(|(_, e)| e.visible && (e.id == ZIPPER || e.id == PUSH_PULL))?;
        let defaults = if fx.id == ZIPPER { 10.0 } else { 0.0 };
        let mut from = fx.params.clone();
        if from.get(if fx.id == ZIPPER { "size" } else { "amount" }).and_then(Value::as_f64).is_none() {
            let key = if fx.id == ZIPPER { "size" } else { "amount" };
            from[key] = json!(defaults);
        }
        if fx.id == ZIPPER && from.get("ridges").and_then(Value::as_f64).is_none() {
            from["ridges"] = json!(4);
        }
        let gap = HANDLE_GAP / zoom.max(1e-6);
        let (top_mid, right_mid) =
            (vectorcraft_geom::Point::new(bounds.center().x, bounds.y0), vectorcraft_geom::Point::new(bounds.x1, bounds.center().y));
        Some(Self {
            id,
            index,
            zipper: fx.id == ZIPPER,
            from,
            amplitude: vectorcraft_geom::Point::new(top_mid.x, top_mid.y - gap),
            frequency: (fx.id == ZIPPER).then_some(vectorcraft_geom::Point::new(right_mid.x + gap, right_mid.y)),
            amount: (fx.id == PUSH_PULL).then_some(vectorcraft_geom::Point::new(top_mid.x, top_mid.y - gap)),
        })
    }

    fn near(&self, p: vectorcraft_geom::Point, zoom: f64) -> Option<Part> {
        let reach = REACH / zoom.max(1e-6);
        let near = |a: vectorcraft_geom::Point, b: vectorcraft_geom::Point| a.distance(b) <= reach;
        if near(p, self.amplitude) && self.zipper {
            return Some(Part::Amplitude);
        }
        if let (Some(at), true) = (self.frequency, self.zipper)
            && near(p, at)
        {
            return Some(Part::Frequency);
        }
        if let Some(at) = self.amount
            && near(p, at)
        {
            return Some(Part::Amount);
        }
        None
    }

    /// The parameters the part at `delta` from the press gives.
    fn params(&self, part: Part, delta: vectorcraft_geom::Vec2) -> Value {
        match part {
            Part::Amplitude => json!({"size": (self.from["size"].as_f64().unwrap_or(10.0) - delta.y).max(0.0)}),
            Part::Frequency => json!({"ridges": (self.from["ridges"].as_f64().unwrap_or(4.0) + delta.x / 10.0).round().clamp(1.0, 100.0) as u64}),
            Part::Amount => json!({"amount": (self.from["amount"].as_f64().unwrap_or(0.0) + delta.x * 2.0).clamp(-200.0, 200.0)}),
        }
    }
}

/// A handle of a [`DistortHandles`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Amplitude,
    Frequency,
    Amount,
}

impl Part {
    fn label(self) -> &'static str {
        match self {
            Part::Amplitude => "Zipper Amplitude",
            Part::Frequency => "Zipper Frequency",
            Part::Amount => "Push/Pull Amount",
        }
    }
}

/// A drag of one of the dedicated handles.
#[derive(Clone, Debug)]
pub struct DistortDrag {
    handles: DistortHandles,
    part: Part,
    start: vectorcraft_geom::Point,
    /// The parameters the press found, and the last ones previewed.
    from: Value,
    last: Option<Value>,
}

impl DistortDrag {
    pub fn hit(cx: &ToolContext, p: vectorcraft_geom::Point, mods: Mods) -> Option<Self> {
        if mods.cmd || mods.ctrl || mods.alt {
            return None;
        }
        let frame = None;
        let handles = DistortHandles::of(cx.doc, cx.selection, frame, cx.zoom)?;
        let part = handles.near(p, cx.zoom)?;
        Some(Self { from: handles.params(part, vectorcraft_geom::Vec2::ZERO), handles, part, start: p, last: None })
    }

    /// Whether the drag has changed the effect (and so previews it).
    pub fn adjusting(&self) -> bool {
        self.last.is_some()
    }

    /// Preview the parameters the pointer at `p` gives, when they change; the first change opens
    /// the undo step.
    pub fn drag(&mut self, p: vectorcraft_geom::Point) -> Vec<Action> {
        if !p.x.is_finite() || !p.y.is_finite() {
            return vec![];
        }
        let params = self.handles.params(self.part, p - self.start);
        if self.last.as_ref().unwrap_or(&self.from).get("size") == params.get("size")
            && self.last.as_ref().unwrap_or(&self.from).get("ridges") == params.get("ridges")
            && self.last.as_ref().unwrap_or(&self.from).get("amount") == params.get("amount")
        {
            return vec![];
        }
        let mut out = vec![];
        if self.last.is_none() {
            out.push(Action::Begin(self.part.label().into()));
        }
        let v = json!({"id": self.handles.id.0, "index": self.handles.index, "params": params.clone()});
        out.push(Action::Preview("effect.setParams".into(), v));
        self.last = Some(params);
        out
    }

    /// Commit the drag; one that ends where it began leaves no undo step.
    pub fn finish(self) -> Vec<Action> {
        match self.last {
            Some(v) if v == self.from => vec![Action::Cancel],
            Some(_) => vec![Action::Commit],
            None => vec![],
        }
    }

    pub fn cancel(self) -> Vec<Action> {
        if self.last.is_some() { vec![Action::Cancel] } else { vec![] }
    }
}
