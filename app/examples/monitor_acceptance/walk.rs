//! The planned-step engine, the queued updates and the walk's own bookkeeping.

use crate::prelude::*;

#[derive(Clone, Copy)]
pub(crate) enum Queued {
    Key(KeyCode),
    Button(MouseButton),
}

/// One action: a function of the world and the harness's own bookkeeping.
pub(crate) type Act = fn(&mut World, &mut Harness);

/// One planned step: acts at chosen frames, an optional per-frame tick, a
/// settle window, an optional screenshot and an optional check at the end.
/// A predicate that holds a step's check until the world has settled.
pub(crate) type Gate = fn(&mut World, &mut Harness) -> bool;

#[derive(Clone)]
pub(crate) struct Step {
    pub(crate) name: &'static str,
    pub(crate) acts: Vec<(u32, Act)>,
    pub(crate) tick: Option<Act>,
    pub(crate) check: Option<Act>,
    /// While this returns false the step keeps ticking and does not run its
    /// check or advance — the drawer's projection is a bounded progressive pass,
    /// so a real frame's settle time is not a fixed constant.
    pub(crate) until: Option<Gate>,
    pub(crate) settle: u32,
    pub(crate) shot: Option<(u32, &'static str)>,
}

impl Step {
    pub(crate) fn new(name: &'static str) -> Step {
        Step {
            name,
            acts: Vec::new(),
            tick: None,
            check: None,
            until: None,
            settle: 4,
            shot: None,
        }
    }

    pub(crate) fn at(mut self, frame: u32, act: Act) -> Step {
        self.acts.push((frame, act));
        self
    }

    pub(crate) fn tick(mut self, tick: Act) -> Step {
        self.tick = Some(tick);
        self
    }

    pub(crate) fn check(mut self, check: Act) -> Step {
        self.check = Some(check);
        self
    }

    pub(crate) fn until(mut self, until: Gate) -> Step {
        self.until = Some(until);
        self
    }

    pub(crate) fn settle(mut self, settle: u32) -> Step {
        self.settle = settle;
        self
    }

    pub(crate) fn shot(mut self, frame: u32, shot: &'static str) -> Step {
        self.shot = Some((frame, shot));
        self
    }
}

#[derive(Resource)]
pub(crate) struct Harness {
    /// The explicit construction-time override the walk applied, if any — so the
    /// gate mode can distinguish the build default from a forced value.
    pub(crate) explicit_dev: Option<bool>,
    pub(crate) plan: Vec<Step>,
    pub(crate) step: usize,
    pub(crate) frame: u32,
    pub(crate) report: Report,
    pub(crate) queued: Vec<Queued>,
    pub(crate) before: Option<Vec<String>>,
    pub(crate) hud_before: Option<Vec<String>>,
    pub(crate) gate: GatedResolver,
    pub(crate) emit_from: u32,
    pub(crate) emit_to: u32,
    pub(crate) saw_updating: bool,
    pub(crate) wait_before: Option<String>,
    pub(crate) draft_before: Option<String>,
    /// The rendered rows, in order, at the moment the small-history anchor was
    /// set, and the anchor row's index into them. The filter-fallback check
    /// derives the documented expectation from this real state instead of
    /// assuming which row the oldest retained entry happens to be.
    pub(crate) rows_before_flip: Vec<RowMark>,
    pub(crate) anchor_index: Option<usize>,
    /// A settings value captured before a drag, to prove the drag was inert.
    pub(crate) before_value: Option<f32>,
    /// The key field's `(text, caret, selection)` captured before Monitor took
    /// the frame, and re-read after the close on the pre-paste frame, so the
    /// rendered walk pins the nontrivial caret/selection across the whole
    /// suspension.
    pub(crate) draft_edit_before: Option<DraftEdit>,
    pub(crate) draft_edit_restored: Option<DraftEdit>,
    /// Whether the same-frame opening step's real pointer press set the
    /// underlying New Game widget's `Interaction` to `Pressed` on the press
    /// frame — the production hit-test result.
    pub(crate) pointer_hit: Option<bool>,
    /// The runner's wait after a fresh advance, for the F9 load comparison.
    pub(crate) after_advance: Option<String>,
    /// Every row body read off the rendered window while the broken walk
    /// paged through the startup history, so the final step can prove the
    /// whole capped list was readable in-window.
    pub(crate) read_bodies: Vec<String>,
    pub(crate) finished: bool,
}

/// The Settings key field's live text, caret byte index and selected byte
/// range, read from its own editor.
pub(crate) type DraftEdit = (String, usize, std::ops::Range<usize>);

impl Harness {
    pub(crate) fn new(mode: Mode, gate: GatedResolver, explicit_dev: Option<bool>) -> Harness {
        Harness {
            explicit_dev,
            plan: plan(mode),
            step: 0,
            frame: 0,
            report: Report::default(),
            queued: Vec::new(),
            before: None,
            hud_before: None,
            gate,
            emit_from: 0,
            emit_to: 0,
            saw_updating: false,
            wait_before: None,
            draft_before: None,
            rows_before_flip: Vec::new(),
            anchor_index: None,
            before_value: None,
            draft_edit_before: None,
            draft_edit_restored: None,
            pointer_hit: None,
            after_advance: None,
            read_bodies: Vec::new(),
            finished: false,
        }
    }
}
