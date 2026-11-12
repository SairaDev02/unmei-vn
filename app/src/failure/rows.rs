//! The failure inventory: one variant per row, and the posture, level, surface
//! and short string the table — not the call site — decides.

use unmei_engine::DecisionError;

/// What the player gets and what they can do next — the class *is* the surface
/// and the recovery. The five classes from the plan's §1 table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    /// A pack that does not load: the startup error screen, then quit.
    QuitWithMessage,
    /// An action fails and the player is told: the Notice.
    ContinueWithMessage,
    /// A silent degradation: the log only.
    ContinueSilently,
    /// Any Decisions failure: the authored `fallback` branch plays.
    ContinueWithFallback,
    /// A branch the loader's validation makes unreachable: `error!` + Notice.
    Defensive,
}

/// One variant per row of the plan's 26-row inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailureRow {
    // 1
    PackLoad,
    // 2
    PackWarning,
    // 3
    BackgroundAsset,
    // 4
    SpriteAsset,
    // 5
    BgmAsset,
    // 6
    SfxAsset,
    // 7
    AudioDevice,
    // 8
    DecisionTransport,
    // 9
    DecisionAnswer,
    // 10
    DecisionConfigAbsent,
    // 11
    DecidePointMissing,
    // 12
    DecisionLabelMissing,
    // 13
    RunnerCall,
    // 14
    ManualSave,
    // 15
    Autosave,
    // 16
    UnreadableSlot,
    // 17
    LoadRefused,
    // 18
    LegacyImport,
    // 19
    SettingsMalformed,
    // 20
    SettingsFlush,
    // 21
    LogUnwritable,
    // 22 — not a failure: the end-of-script marker is the existing surface.
    EndOfScript,
    // 23
    CredentialsWrite,
    // 24
    CredentialRead,
    // 25
    TitleBackdrop,
    // 26 — the read log's IO: a present file that will not decode, carries
    // another version or cannot be read, or a flush that cannot be written.
    ReadLog,
}

impl FailureRow {
    /// Every row, in inventory order. The table-driven test walks this.
    pub const ALL: [FailureRow; 26] = [
        FailureRow::PackLoad,
        FailureRow::PackWarning,
        FailureRow::BackgroundAsset,
        FailureRow::SpriteAsset,
        FailureRow::BgmAsset,
        FailureRow::SfxAsset,
        FailureRow::AudioDevice,
        FailureRow::DecisionTransport,
        FailureRow::DecisionAnswer,
        FailureRow::DecisionConfigAbsent,
        FailureRow::DecidePointMissing,
        FailureRow::DecisionLabelMissing,
        FailureRow::RunnerCall,
        FailureRow::ManualSave,
        FailureRow::Autosave,
        FailureRow::UnreadableSlot,
        FailureRow::LoadRefused,
        FailureRow::LegacyImport,
        FailureRow::SettingsMalformed,
        FailureRow::SettingsFlush,
        FailureRow::LogUnwritable,
        FailureRow::EndOfScript,
        FailureRow::CredentialsWrite,
        FailureRow::CredentialRead,
        FailureRow::TitleBackdrop,
        FailureRow::ReadLog,
    ];

    /// The row's posture. Row 22 is not a failure, so it has none.
    pub fn posture(self) -> Option<Posture> {
        use FailureRow::*;
        match self {
            PackLoad => Some(Posture::QuitWithMessage),
            PackWarning | BackgroundAsset | SpriteAsset | BgmAsset | SfxAsset | AudioDevice
            | SettingsMalformed | LogUnwritable | CredentialRead | TitleBackdrop | ReadLog => {
                Some(Posture::ContinueSilently)
            }
            DecisionTransport | DecisionAnswer | DecisionConfigAbsent => {
                Some(Posture::ContinueWithFallback)
            }
            DecidePointMissing | DecisionLabelMissing | RunnerCall => Some(Posture::Defensive),
            ManualSave | Autosave | UnreadableSlot | LoadRefused | LegacyImport | SettingsFlush
            | CredentialsWrite => Some(Posture::ContinueWithMessage),
            EndOfScript => None,
        }
    }

    /// The exactly-one log level for the row; `None` means nothing is logged.
    pub fn level(self) -> Option<log::Level> {
        match self.posture()? {
            Posture::QuitWithMessage | Posture::ContinueWithMessage | Posture::Defensive => {
                Some(log::Level::Error)
            }
            Posture::ContinueSilently | Posture::ContinueWithFallback => Some(log::Level::Warn),
        }
    }

    /// Whether the row's surface is the Notice. Row 16 is the one
    /// continue-with-message row whose message the slot card already owns.
    pub fn notices(self) -> bool {
        use FailureRow::*;
        matches!(
            self,
            DecidePointMissing
                | DecisionLabelMissing
                | RunnerCall
                | ManualSave
                | Autosave
                | LoadRefused
                | LegacyImport
                | SettingsFlush
                | CredentialsWrite
        )
    }

    /// The row's one short string: the prefix of its log line, and the text of
    /// its surface when that surface is the Notice. The silent rows carry one
    /// too, so "one string per failure" holds even when the player sees
    /// nothing. Row 16's string is the slot card's own reason and rides
    /// [`super::Failure::with_short`].
    pub fn short_text(self) -> Option<&'static str> {
        use FailureRow::*;
        match self {
            BackgroundAsset => Some("couldn't load a background"),
            TitleBackdrop => Some("couldn't load the title backdrop"),
            SpriteAsset => Some("couldn't load a sprite"),
            BgmAsset => Some("couldn't load the music"),
            SfxAsset => Some("couldn't load the choice sound"),
            AudioDevice => Some("couldn't start audio"),
            DecidePointMissing | DecisionLabelMissing | RunnerCall => {
                Some("the story can't continue — Esc for the menu")
            }
            ManualSave => Some("couldn't save — the slot is unchanged"),
            Autosave => Some("couldn't autosave"),
            LoadRefused => Some("couldn't load that save"),
            LegacyImport => Some("couldn't import the legacy save"),
            SettingsFlush => Some("settings not saved"),
            ReadLog => Some("couldn't use the read log"),
            CredentialsWrite => Some("Could not save the key"),
            _ => None,
        }
    }

    /// Classify a Decisions fallback from [`unmei_engine::ResolvedDecision`]'s
    /// `fallback_reason` — the [`DecisionError`] itself, not its `Display` text.
    ///
    /// The engine carries the typed error across the resolver seam precisely so
    /// this mapping can match the variant: a new engine variant fails to compile
    /// here instead of silently reclassifying a row. Every variant's payload is
    /// authored or fixed (a question id, one of the wire's type names, a status
    /// code), so the row's text stays categorical — no response body, URL,
    /// decoder or model text can reach it.
    pub fn for_decision_reason(reason: &DecisionError) -> Self {
        match reason {
            DecisionError::Config(_) => FailureRow::DecisionConfigAbsent,
            DecisionError::Http | DecisionError::Task | DecisionError::Status { .. } => {
                FailureRow::DecisionTransport
            }
            DecisionError::Decode
            | DecisionError::MissingAnswer(_)
            | DecisionError::WrongType { .. }
            | DecisionError::UnknownAction
            | DecisionError::UnusableAnswer { .. } => FailureRow::DecisionAnswer,
        }
    }
}
