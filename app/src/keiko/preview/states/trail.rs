//! The script's default path: every park the walk meets from the entry scene's
//! top, and the moves that carry the replay from one park to the next.
//!
//! The walk uses the engine's own runner, so the steps a fixture state carries
//! are the ones the pack really meets, and a script that stops short of its end
//! says why (spec §9.2).

use std::mem;

use unmei_engine::keiko::{Pick, Step};
use unmei_engine::runner::Wait;
use unmei_engine::{Pack, ScriptRunner};

use super::CaseBuilder;
use crate::keiko::preview::PreviewRequest;

/// One move that carries the replay from one park to the next.
#[derive(Clone, Copy)]
pub(super) enum Move {
    /// Advance past a say-line.
    Advance,
    /// Take a `choice`'s option by its 1-based number.
    Pick(u32),
}

/// Add a run of moves, counting consecutive advances as one `advance n`, so a
/// generated case reads like a hand-written one.
pub(super) fn add_moves(builder: &mut CaseBuilder, moves: &[Move]) {
    let mut run = 0;
    for step in moves {
        match step {
            Move::Advance => run += 1,
            Move::Pick(index) => {
                if run > 0 {
                    builder.step(Step::Advance(mem::take(&mut run)));
                }
                builder.step(Step::Pick(Pick::Index(*index)));
            }
        }
    }
    if run > 0 {
        builder.step(Step::Advance(run));
    }
}

/// One park the default path meets, and the moves that reach it from the
/// previous park's answer.
pub(super) struct Stop {
    moves: Vec<Move>,
    pub(super) park: Park,
}

/// What the runner parks on.
pub(super) enum Park {
    Say { speaker: String },
    Choice { options: Vec<String> },
    Decision { point: String },
}

/// The script's default path: every park from the entry scene's top, crossing
/// scene files the way the player does, until the script ends.
#[derive(Default)]
pub(super) struct Trail {
    pub(super) stops: Vec<Stop>,
    /// The moves after the last park, up to the script's end.
    tail: Vec<Move>,
    /// Why the walk stopped before the script ended.
    pub(super) stopped: Option<String>,
}

impl Trail {
    /// Every move from the entry scene's top to the park at `index`.
    pub(super) fn moves_to(&self, index: usize) -> Vec<Move> {
        self.stops[..=index]
            .iter()
            .flat_map(|stop| stop.moves.iter().copied())
            .collect()
    }

    /// Every move from the entry scene's top to the script's end.
    pub(super) fn to_end(&self) -> Vec<Move> {
        let mut moves: Vec<Move> = self
            .stops
            .iter()
            .flat_map(|stop| stop.moves.iter().copied())
            .collect();
        moves.extend(self.tail.iter().copied());
        moves
    }

    /// Where the overlay states open from: the first say-line the path meets,
    /// or its first park, or the entry into play when the pack parks nowhere.
    pub(super) fn overlay_prefix(&self) -> Vec<Move> {
        let index = self
            .stops
            .iter()
            .position(|stop| matches!(stop.park, Park::Say { .. }))
            .or_else(|| (!self.stops.is_empty()).then_some(0));
        index.map(|index| self.moves_to(index)).unwrap_or_default()
    }
}

/// Walk the script from the entry scene's top with the request's state and
/// picks, taking the author's fallback at every decision point.
pub(super) fn trail(pack: &Pack, request: &PreviewRequest) -> Trail {
    let mut trail = Trail::default();
    let entry = pack.manifest.entry.clone();
    let mut state = pack.state.clone();
    for (key, value) in &request.state {
        state.insert(key.clone(), value.clone());
    }
    let mut runner = ScriptRunner::new(pack.clone());
    if let Err(error) = runner.restore_position(entry.clone(), 0, state, None) {
        trail.stopped = Some(format!("scene `{entry}` could not be started: {error}"));
        return trail;
    }
    let mut picks = request.picks.iter().copied();
    let mut moves: Vec<Move> = Vec::new();
    while trail.stops.len() < crate::keiko::preview::MAX_PARKS {
        let outcome = match runner.run() {
            Ok(outcome) => outcome,
            Err(error) => {
                trail.stopped = Some(format!("the script stops: {error}"));
                return trail;
            }
        };
        let Some(wait) = outcome.wait else {
            break;
        };
        let park = match &wait {
            Wait::Say { speaker, .. } => Park::Say {
                speaker: speaker.clone(),
            },
            Wait::Choice { options } => Park::Choice {
                options: options.iter().map(|(text, _)| text.clone()).collect(),
            },
            Wait::Decision {
                npc_id, question, ..
            } => Park::Decision {
                point: format!("{npc_id}.{question}"),
            },
        };
        trail.stops.push(Stop {
            moves: mem::take(&mut moves),
            park,
        });
        let moved = match wait {
            Wait::Say { .. } => {
                moves.push(Move::Advance);
                runner.advance_say().map(|_| ())
            }
            Wait::Choice { options } => {
                let index = picks.next().unwrap_or(1).clamp(1, options.len().max(1)) as u32;
                moves.push(Move::Pick(index));
                runner.choose(index as usize - 1).map(|_| ())
            }
            Wait::Decision {
                npc_id, question, ..
            } => {
                // The app answers a decision point with the author's fallback
                // when the take names no landing, so the walk lands there too.
                let landing = pack
                    .npcs
                    .get(&npc_id)
                    .and_then(|npc| npc.decisions.get(&question))
                    .map(|config| config.fallback.landing_key());
                match landing {
                    Some(landing) => runner.resolve_decision(&landing).map(|_| ()),
                    None => {
                        trail.stopped = Some(format!(
                            "decision `{npc_id}.{question}` has no fallback to land"
                        ));
                        return trail;
                    }
                }
            }
        };
        if let Err(error) = moved {
            trail.stopped = Some(format!("the script stops: {error}"));
            return trail;
        }
    }
    trail.tail = moves;
    if trail.stops.len() >= crate::keiko::preview::MAX_PARKS {
        trail.stopped = Some(format!(
            "the script did not end within {} parks",
            crate::keiko::preview::MAX_PARKS
        ));
    }
    trail
}
