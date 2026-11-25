//! §9.2's fixture states: the steps that reach each of today's surfaces from a
//! fresh take, and the states a pack cannot reach.
//!
//! The play states are derived from the pack: the script is walked with the
//! engine's own runner along the request's picks and the author's decision
//! fallbacks, so the `advance` and `pick` steps a state carries are the ones
//! the pack really meets. An overlay or a confirmation is a fixed command list
//! from play, raised through the app's own controls. Every state proves itself
//! with an assertion before its frame, so a frame is named for the surface the
//! take really reached.
//!
//! A park the pack does not have plans no case at all: the slate reports the
//! state as `preview.state.unreachable` and writes no file (spec §9.2).

use unmei_engine::keiko::{
    Case, ControlCondition, DecisionState, Expect, Fault, Header, HeaderLine, LoadTarget, Step,
    StepLine,
};
use unmei_engine::Pack;

use trail::{add_moves, trail, Park, Trail};

use super::PreviewRequest;

mod trail;

/// The speaker id a narration line carries; every other speaker is a character.
const NARRATOR: &str = "Narrator";

/// The key names the case grammar uses for the two taps the rail needs.
const TAB: &str = "Tab";
const ENTER: &str = "Enter";

/// How many Tab presses from the page's first ledger row reach each section: the
/// section column reads Save, Load, Timeline, Gallery, Settings
/// (`docs/title-and-menu.md` § Default look §4).
const LOAD_TABS: usize = 2;
const TIMELINE_TABS: usize = 3;
const GALLERY_TABS: usize = 4;
const SETTINGS_TABS: usize = 5;

/// One fixture state and what the slate plans for it.
pub struct Fixture {
    /// The state's file stem: `say`, `menu-save`, `confirm-load`.
    pub name: &'static str,
    pub outcome: Outcome,
}

/// A state's plan.
pub enum Outcome {
    /// The case that reaches the state and frames it as `DIR/<name>.png`.
    Case(Case),
    /// Why this pack cannot reach the state.
    Unreachable(String),
}

/// §9.2's table, in its order, for one pack and request.
pub fn slate(pack: &Pack, request: &PreviewRequest) -> Vec<Fixture> {
    let slate = Slate {
        pack,
        request,
        trail: trail(pack, request),
    };
    let still = if pack.gallery_entries().is_empty() {
        Outcome::Unreachable(
            "the pack declares no gallery art, so no card opens a still".to_string(),
        )
    } else {
        let mut steps = rail(GALLERY_TABS);
        steps.push(activate("gallery.card.1"));
        slate.reached(&steps, &[surface("still")], "still")
    };
    // The overlay's settings door, the rail's one session-scoped stop, the
    // title it returns to, and the title's two confirming rows.
    let settings = rail(SETTINGS_TABS);
    let mut exit = settings.clone();
    exit.push(activate("overlay.exit-to-title"));
    let mut title = exit.clone();
    title.push(activate("confirm.yes"));
    let mut new_game = title.clone();
    new_game.push(activate("title.new-game"));
    let mut quit = title.clone();
    quit.push(activate("title.quit"));

    vec![
        fixture("title-fresh", slate.title()),
        fixture(
            "title-continue",
            slate.reached(
                &title,
                &[
                    surface("title"),
                    enabled("title.continue", ControlCondition::Enabled),
                ],
                "title-continue",
            ),
        ),
        fixture("say", slate.say()),
        fixture("narration", slate.narration()),
        fixture("choice", slate.choice()),
        fixture("decision", slate.decision()),
        fixture("scene-end", slate.scene_end()),
        fixture(
            "menu-save",
            slate.reached(&[Step::Menu], &[surface("menu")], "menu-save"),
        ),
        fixture(
            "menu-load",
            slate.reached(
                &rail(LOAD_TABS),
                &[
                    surface("menu"),
                    enabled("overlay.load-tab", ControlCondition::Focused),
                ],
                "menu-load",
            ),
        ),
        fixture(
            "menu-settings",
            slate.reached(&settings, &[surface("settings")], "menu-settings"),
        ),
        fixture(
            "menu-gallery",
            slate.reached(&rail(GALLERY_TABS), &[surface("gallery")], "menu-gallery"),
        ),
        fixture(
            "menu-timeline",
            slate.reached(
                &rail(TIMELINE_TABS),
                &[surface("timeline")],
                "menu-timeline",
            ),
        ),
        fixture("still", still),
        fixture(
            "confirm-overwrite",
            slate.reached(
                &[Step::Save(1), Step::Save(1)],
                &[surface("confirm")],
                "confirm-overwrite",
            ),
        ),
        fixture(
            "confirm-load",
            slate.reached(
                &[Step::Save(1), Step::Load(LoadTarget::Slot(1))],
                &[surface("confirm")],
                "confirm-load",
            ),
        ),
        fixture(
            "confirm-reset-audio",
            slate.reached(
                &with(&settings, &["settings.reset-audio"]),
                &[surface("confirm")],
                "confirm-reset-audio",
            ),
        ),
        fixture(
            "confirm-reset-play",
            slate.reached(
                &with(&settings, &["settings.reset-play"]),
                &[surface("confirm")],
                "confirm-reset-play",
            ),
        ),
        fixture(
            "confirm-new-game",
            slate.reached(&new_game, &[surface("confirm")], "confirm-new-game"),
        ),
        fixture(
            "confirm-quit",
            slate.reached(&quit, &[surface("confirm")], "confirm-quit"),
        ),
        fixture(
            "confirm-exit-to-title",
            slate.reached(
                &with(&settings, &["overlay.exit-to-title"]),
                &[surface("confirm")],
                "confirm-exit-to-title",
            ),
        ),
        fixture(
            "confirm-clear-key",
            slate.reached(
                &with(&settings, &["settings.clear-key"]),
                &[surface("confirm")],
                "confirm-clear-key",
            ),
        ),
        fixture("confirm-timeline-jump", slate.confirm_timeline_jump()),
    ]
}

/// The pack, the request and the path that connects them, shared by every
/// state's planner.
struct Slate<'a> {
    pack: &'a Pack,
    request: &'a PreviewRequest,
    trail: Trail,
}

impl Slate<'_> {
    /// `title-fresh`: a boot on the title with no save to continue.
    fn title(&self) -> Outcome {
        let mut builder = CaseBuilder::default();
        builder.header(text_speed());
        builder.step(Step::Expect(surface("title")));
        builder.step(Step::Expect(enabled(
            "title.continue",
            ControlCondition::Disabled,
        )));
        builder.step(frame("title-fresh"));
        Outcome::Case(builder.finish())
    }

    /// `say`: the first say-line a character other than `Narrator` speaks.
    fn say(&self) -> Outcome {
        let Some((index, speaker)) = self.first_say(|speaker| speaker != NARRATOR) else {
            return Outcome::Unreachable(
                "the pack has no say-line spoken by a character other than `Narrator`".to_string(),
            );
        };
        self.say_state(index, speaker, "say")
    }

    /// `narration`: the first say-line `Narrator` speaks.
    fn narration(&self) -> Outcome {
        let Some((index, speaker)) = self.first_say(|speaker| speaker == NARRATOR) else {
            return Outcome::Unreachable(
                "the pack has no say-line spoken by `Narrator`".to_string(),
            );
        };
        self.say_state(index, speaker, "narration")
    }

    /// A say-line state: the walk to the park, the fully revealed line, and the
    /// frame.
    fn say_state(&self, index: usize, speaker: &str, name: &str) -> Outcome {
        let mut builder = self.play();
        add_moves(&mut builder, &self.trail.moves_to(index));
        builder.step(Step::Expect(surface("play")));
        builder.step(Step::Expect(Expect::Speaker(Some(speaker.to_string()))));
        // The take's frames have no duration, so `setting play.text_speed 0`
        // is what draws the line whole in one frame (spec §9.2).
        builder.step(Step::Expect(Expect::Revealed(true)));
        builder.step(frame(name));
        Outcome::Case(builder.finish())
    }

    /// `choice`: the first `choice` park, its options on screen.
    fn choice(&self) -> Outcome {
        let Some((index, options)) =
            self.trail
                .stops
                .iter()
                .enumerate()
                .find_map(|(index, stop)| match &stop.park {
                    Park::Choice { options } => Some((index, options.clone())),
                    _ => None,
                })
        else {
            return Outcome::Unreachable("the pack has no `choice`".to_string());
        };
        let mut builder = self.play();
        add_moves(&mut builder, &self.trail.moves_to(index));
        builder.step(Step::Expect(surface("play")));
        builder.step(Step::Expect(Expect::Choices(options)));
        builder.step(frame("choice"));
        Outcome::Case(builder.finish())
    }

    /// `decision`: the first `decide` park, held pending by a fault.
    fn decision(&self) -> Outcome {
        let Some((index, point)) = self
            .trail
            .stops
            .iter()
            .enumerate()
            .find_map(|(index, stop)| match &stop.park {
                Park::Decision { point } => Some((index, point.clone())),
                _ => None,
            })
        else {
            return Outcome::Unreachable("the pack has no `decide` point".to_string());
        };
        let mut builder = self.play();
        // A held answer is what keeps the point parked; without it the take
        // lands the author's fallback and the frame shows the line it reached.
        builder.header(Header::Fault {
            fault: Fault::DecisionHold {
                point: point.clone(),
            },
        });
        add_moves(&mut builder, &self.trail.moves_to(index));
        builder.step(Step::Expect(surface("play")));
        builder.step(Step::Expect(Expect::Decision(DecisionState::Pending)));
        builder.step(frame("decision"));
        Outcome::Case(builder.finish())
    }

    /// `scene-end`: the play surface after the script's last line.
    fn scene_end(&self) -> Outcome {
        if let Some(reason) = &self.trail.stopped {
            return Outcome::Unreachable(format!("the script never reaches its end: {reason}"));
        }
        let mut builder = self.play();
        add_moves(&mut builder, &self.trail.to_end());
        builder.step(Step::Expect(surface("play")));
        // The band's end-of-scene placeholder: no parked line at all.
        builder.step(Step::Expect(Expect::Speaker(None)));
        builder.step(frame("scene-end"));
        Outcome::Case(builder.finish())
    }

    /// `confirm-timeline-jump`: the Timeline's one confirm, raised by
    /// activating a visited scene's card (`docs/title-and-menu.md` § Timeline
    /// §2/§3). A fresh take has read only the entry scene, so the card is the
    /// entry's place in the declared order; with no order, or an order that
    /// omits the entry, the take has no card it can activate.
    fn confirm_timeline_jump(&self) -> Outcome {
        let entries = self.pack.timeline_entries();
        let Some(index) = entries
            .iter()
            .position(|id| *id == self.pack.manifest.entry)
        else {
            let reason = if entries.is_empty() {
                "the pack declares no scene order, so the Timeline has no card".to_string()
            } else {
                format!(
                    "the pack's scene order does not list the entry scene `{}`, the only scene a fresh take has read",
                    self.pack.manifest.entry
                )
            };
            return Outcome::Unreachable(reason);
        };
        let mut steps = rail(TIMELINE_TABS);
        steps.push(activate(&format!("timeline.card.{}", index + 1)));
        self.reached(&steps, &[surface("confirm")], "confirm-timeline-jump")
    }

    /// A state reached from play by the commands given, asserted, then framed.
    fn reached(&self, commands: &[Step], expects: &[Expect], name: &str) -> Outcome {
        let mut builder = self.play();
        add_moves(&mut builder, &self.trail.overlay_prefix());
        for step in commands {
            builder.step(step.clone());
        }
        for expect in expects {
            builder.step(Step::Expect(expect.clone()));
        }
        builder.step(frame(name));
        Outcome::Case(builder.finish())
    }

    /// The first say-line whose speaker a predicate accepts, with its park.
    fn first_say(&self, spoken_by: impl Fn(&str) -> bool) -> Option<(usize, &str)> {
        self.trail
            .stops
            .iter()
            .enumerate()
            .find_map(|(index, stop)| match &stop.park {
                Park::Say { speaker } if spoken_by(speaker) => Some((index, speaker.as_str())),
                _ => None,
            })
    }

    /// The case every play state starts from: the settings the take needs, the
    /// request's state, and the entry into play.
    fn play(&self) -> CaseBuilder {
        let mut builder = CaseBuilder::default();
        builder.header(text_speed());
        if self.request.state.is_empty() {
            builder.step(Step::NewGame);
        } else {
            // The take applies `state` headers through its `start` seam, so an
            // override rides the entry scene's restart rather than a plain New
            // Game (spec §9.1, §13 rule 23).
            builder.header(Header::Start {
                scene: self.pack.manifest.entry.clone(),
            });
            for (key, value) in &self.request.state {
                builder.header(Header::State {
                    key: key.clone(),
                    value: super::state_text(value),
                });
            }
        }
        builder
    }
}

fn fixture(name: &'static str, outcome: Outcome) -> Fixture {
    Fixture { name, outcome }
}

fn text_speed() -> Header {
    Header::Setting {
        key: "play.text_speed".to_string(),
        value: "0".to_string(),
    }
}

fn frame(name: &str) -> Step {
    Step::Frame(Some(name.to_string()))
}

fn surface(name: &str) -> Expect {
    Expect::Surface(name.to_string())
}

fn enabled(id: &str, condition: ControlCondition) -> Expect {
    Expect::Control {
        id: id.to_string(),
        condition,
    }
}

fn activate(id: &str) -> Step {
    Step::Activate(id.to_string())
}

/// An overlay open plus `tabs` presses to a rail control, then Enter.
fn rail(tabs: usize) -> Vec<Step> {
    let mut steps = vec![Step::Menu];
    steps.extend(std::iter::repeat_n(Step::Press(TAB.to_string()), tabs));
    steps.push(Step::Press(ENTER.to_string()));
    steps
}

/// A command list with the named controls activated after it.
fn with(commands: &[Step], ids: &[&str]) -> Vec<Step> {
    let mut steps = commands.to_vec();
    steps.extend(ids.iter().map(|id| activate(id)));
    steps
}

/// A generated case, numbering its lines as a case file numbers them, so a
/// finding names the step it came from.
#[derive(Default)]
struct CaseBuilder {
    case: Case,
    written: usize,
}

impl CaseBuilder {
    fn header(&mut self, header: Header) {
        self.written += 1;
        self.case.headers.push(HeaderLine {
            line: self.written + 1,
            header,
        });
    }

    fn step(&mut self, step: Step) {
        self.written += 1;
        self.case.steps.push(StepLine {
            line: self.written + 1,
            step,
        });
    }

    fn finish(self) -> Case {
        self.case
    }
}

#[cfg(test)]
mod tests;
