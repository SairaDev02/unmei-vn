//! The single report funnel and the defensive hold it engages.

use bevy::prelude::*;

use super::rows::{FailureRow, Posture};
use super::{Notice, LOG_TARGET};

/// One failure: its inventory row, the posture derived from it, its one short
/// string (the log-line prefix, when the row has one), and the machine detail
/// suffix.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub row: FailureRow,
    pub posture: Option<Posture>,
    pub short: Option<String>,
    pub detail: String,
}

impl Failure {
    /// Construct from the row table: posture and the short string are derived,
    /// never chosen by the call site.
    pub fn new(row: FailureRow, detail: impl Into<String>) -> Self {
        Failure {
            row,
            posture: row.posture(),
            short: row.short_text().map(str::to_string),
            detail: detail.into(),
        }
    }

    /// Row 16 only: its short string is the slot card's own reason, so the call
    /// site supplies it. `report` still logs it Notice-less.
    pub fn with_short(
        row: FailureRow,
        short: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Failure {
            row,
            posture: row.posture(),
            short: Some(short.into()),
            detail: detail.into(),
        }
    }
}

/// The single funnel: log exactly once under [`LOG_TARGET`] at the row's tabled
/// level, and install the Notice exactly when the inventory marks the row with
/// one. No call site chooses its own surface. A [`Posture::Defensive`] row also
/// engages the [`DefensiveHold`], so the table — not the call site — decides
/// which failures freeze the runner.
pub fn report(failure: Failure, notice: &mut Notice, hold: &mut DefensiveHold) {
    if failure.posture == Some(Posture::Defensive) {
        hold.engage();
    }

    if let Some(level) = failure.row.level() {
        let line = match (&failure.short, failure.detail.is_empty()) {
            (Some(text), true) => text.clone(),
            (Some(text), false) => format!("{text}: {}", failure.detail),
            (None, _) => failure.detail.clone(),
        };
        log::log!(target: LOG_TARGET, level, "{line}");
    }

    if failure.row.notices() {
        if let Some(text) = failure.short.clone() {
            notice.show(text);
        }
    }
}

/// Funnel a [`RunnerError`](unmei_engine::runner::RunnerError) from an
/// `advance_say` / `choose` / `resolve_decision` call: row 13 (defensive). The
/// wait and index checks at the call sites make these unreachable, so the
/// inventory's hold is the only recovery.
pub fn report_runner_call(
    error: impl std::fmt::Display,
    notice: &mut Notice,
    hold: &mut DefensiveHold,
) {
    report(
        Failure::new(FailureRow::RunnerCall, error.to_string()),
        notice,
        hold,
    );
}

/// The defensive hold ([rows 11–13](../../../docs/runtime-hardening.md#2-the-inventory)):
/// set by [`report`] whenever it funnels a
/// failure whose posture is [`Posture::Defensive`], released by a load. The play
/// path's run condition reads it, so a held runner stops until a load restores a
/// position it can move from.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct DefensiveHold(pub bool);

impl DefensiveHold {
    /// Engage the hold — the runner is parked where a failure left it.
    pub fn engage(&mut self) {
        self.0 = true;
    }

    /// Release the hold — a load restored a position the runner can move from.
    pub fn release(&mut self) {
        self.0 = false;
    }

    pub fn is_held(&self) -> bool {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::captures::{install_capture, report_once};
    use super::super::LOG_TARGET;
    use unmei_engine::DecisionError;
    /// The acceptance table: every one of the 26 rows logs exactly once at its
    /// level, a Notice appears exactly for the rows the inventory marks, and the
    /// defensive rows engage the hold — all decided by the table, not the caller.
    #[test]
    fn every_inventory_row_logs_once_at_its_level_and_notices_exactly_where_marked() {
        install_capture();

        for row in FailureRow::ALL {
            let mut notice = Notice::default();
            let mut hold = DefensiveHold::default();
            let records = report_once(row, &mut notice, &mut hold);
            let expected_notice = row.notices();

            match row.level() {
                Some(level) => {
                    assert_eq!(records.len(), 1, "{row:?} must log exactly once");
                    let (got_level, target, line) = &records[0];
                    assert_eq!(*got_level, level, "{row:?} logged at the wrong level");
                    assert_eq!(target, LOG_TARGET, "{row:?} logged under the wrong target");
                    if let Some(text) = row.short_text() {
                        assert!(
                            line.starts_with(text),
                            "{row:?}: the short string must prefix the log line ({line:?})"
                        );
                    }
                }
                None => assert!(records.is_empty(), "{row:?} must log nothing"),
            }

            assert_eq!(
                notice.is_showing(),
                expected_notice,
                "{row:?} Notice presence does not match the inventory"
            );
            assert_eq!(
                hold.is_held(),
                row.posture() == Some(Posture::Defensive),
                "{row:?} hold presence does not match the inventory"
            );
        }
    }
    /// Rows 2–10, 19, 21, 24, 25 and 26 are the silent degradations: never a Notice.
    #[test]
    fn the_silent_rows_never_show_a_notice() {
        install_capture();
        for row in [
            FailureRow::PackWarning,
            FailureRow::BackgroundAsset,
            FailureRow::SpriteAsset,
            FailureRow::BgmAsset,
            FailureRow::SfxAsset,
            FailureRow::AudioDevice,
            FailureRow::DecisionTransport,
            FailureRow::DecisionAnswer,
            FailureRow::DecisionConfigAbsent,
            FailureRow::SettingsMalformed,
            FailureRow::LogUnwritable,
            FailureRow::CredentialRead,
            FailureRow::TitleBackdrop,
            FailureRow::ReadLog,
        ] {
            let mut notice = Notice::default();
            let mut hold = DefensiveHold::default();
            report_once(row, &mut notice, &mut hold);
            assert!(!notice.is_showing(), "{row:?} must not notice");
            assert!(!hold.is_held(), "{row:?} must not hold");
        }
    }
    /// Row 16 logs its string Notice-less: the slot card is its surface.
    #[test]
    fn an_unreadable_slot_logs_its_reason_without_a_notice() {
        install_capture();
        let mut notice = Notice::default();
        let mut hold = DefensiveHold::default();
        let records = report_once(FailureRow::UnreadableSlot, &mut notice, &mut hold);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, log::Level::Error);
        assert!(records[0].2.starts_with("the slot is unreadable"));
        assert!(!notice.is_showing(), "row 16 must not notice");
        assert!(!hold.is_held(), "row 16 must not hold");
    }
    /// Row 22 is not a failure: no log, no notice, no hold.
    #[test]
    fn the_end_of_script_is_not_a_failure() {
        install_capture();
        let mut notice = Notice::default();
        let mut hold = DefensiveHold::default();
        let records = report_once(FailureRow::EndOfScript, &mut notice, &mut hold);
        assert!(records.is_empty());
        assert!(!notice.is_showing());
        assert!(!hold.is_held());
    }
    /// Pin every [`DecisionError`] variant to its failure row. The mapping is an
    /// exhaustive match, so a new engine variant fails to compile in
    /// [`FailureRow::for_decision_reason`]; this table fails loudly if a variant
    /// is mapped to the wrong row.
    #[test]
    fn decision_rows_follow_the_engine_error_variants() {
        let cases = [
            (DecisionError::Http, FailureRow::DecisionTransport),
            (DecisionError::Task, FailureRow::DecisionTransport),
            (
                DecisionError::Status { status: 500 },
                FailureRow::DecisionTransport,
            ),
            (
                DecisionError::Status { status: 301 },
                FailureRow::DecisionTransport,
            ),
            (DecisionError::Decode, FailureRow::DecisionAnswer),
            (
                DecisionError::MissingAnswer("q".into()),
                FailureRow::DecisionAnswer,
            ),
            (DecisionError::UnknownAction, FailureRow::DecisionAnswer),
            (
                DecisionError::UnusableAnswer {
                    question: "q".into(),
                },
                FailureRow::DecisionAnswer,
            ),
            (
                DecisionError::WrongType {
                    question: "q".into(),
                    expected: "choice".into(),
                    answer_type: "noul".into(),
                },
                FailureRow::DecisionAnswer,
            ),
            (
                DecisionError::Config("no key".into()),
                FailureRow::DecisionConfigAbsent,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(
                FailureRow::for_decision_reason(&error),
                expected,
                "{error:?} classified wrongly"
            );
        }
    }
    /// The one-string rule holds even for the silent degradations: rows 3–7 each
    /// carry a short string that prefixes their log line, and none of them
    /// notices (T49 acceptance 3).
    #[test]
    fn the_silent_asset_rows_still_carry_their_one_string() {
        install_capture();
        for row in [
            FailureRow::BackgroundAsset,
            FailureRow::SpriteAsset,
            FailureRow::BgmAsset,
            FailureRow::SfxAsset,
            FailureRow::AudioDevice,
        ] {
            assert!(
                row.short_text().is_some(),
                "{row:?} must carry a short string"
            );
            assert!(!row.notices(), "{row:?} must not notice");

            let mut notice = Notice::default();
            let mut hold = DefensiveHold::default();
            let records = report_once(row, &mut notice, &mut hold);
            assert_eq!(records.len(), 1, "{row:?} must log exactly once");
            assert_eq!(records[0].0, log::Level::Warn, "{row:?} logs at warn");
            assert_eq!(records[0].1, LOG_TARGET);
            assert!(
                records[0].2.starts_with(row.short_text().unwrap()),
                "{row:?}: the short string must prefix the log line ({:?})",
                records[0].2
            );
            assert!(!notice.is_showing(), "{row:?} must not notice");
        }
    }
}
