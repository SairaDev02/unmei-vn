//! The startup error screen: why startup stopped, and its input.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use unmei_engine::LoadError;

use super::session_log::session_log_file_name;
use crate::launch::NO_EXECUTABLE_MESSAGE;

/// The error screen shows at most this many `file:line: message` lines.
pub const MAX_SCREEN_LINES: usize = 20;

/// The one hint a player sees when the missing pack came from the default rule,
/// so the path they never typed is explained. The pack name
/// comes from the rule itself, so a rename cannot leave the hint behind.
pub fn default_pack_hint() -> String {
    format!(
        "No PACK_DIR given — expected {} beside unmei-play.",
        crate::launch::DEFAULT_PACK_NAME
    )
}

/// Why startup stopped before play, recorded instead of printed-and-exited.
#[derive(Resource, Debug, Clone)]
pub enum StartupFailure {
    /// A pack that would not load: the canonicalised path, the whole error list,
    /// and whether the default rule chose that path.
    Pack {
        pack_root: PathBuf,
        errors: Vec<LoadError>,
        /// `true` when no `PACK_DIR` was given and the default rule supplied the
        /// path, which is the only case that shows [`default_pack_hint`].
        defaulted: bool,
    },
    /// No `PACK_DIR` was given and the executable's folder is unknown, so
    /// `demo-pack` beside it cannot be derived. There is no pack, so there is no
    /// sibling log to point at.
    NoExecutable,
}

impl StartupFailure {
    /// The sans intro above the machine block.
    pub fn intro(&self) -> &'static str {
        match self {
            StartupFailure::Pack { .. } => "unmei could not load the pack:",
            StartupFailure::NoExecutable => "unmei-play could not start:",
        }
    }

    /// The pack path drawn above the detail lines; `None` when startup stopped
    /// before any pack path existed.
    pub fn pack_root(&self) -> Option<&Path> {
        match self {
            StartupFailure::Pack { pack_root, .. } => Some(pack_root),
            StartupFailure::NoExecutable => None,
        }
    }

    /// The lines the screen draws under the pack path (or as its whole body):
    /// the default-rule hint, then up to [`MAX_SCREEN_LINES`] `file:line:
    /// message` lines, then the log pointer when there are more.
    pub fn screen_lines(&self) -> Vec<String> {
        let (errors, defaulted, pack_root) = match self {
            StartupFailure::Pack {
                pack_root,
                errors,
                defaulted,
            } => (errors, *defaulted, pack_root),
            StartupFailure::NoExecutable => {
                return vec![NO_EXECUTABLE_MESSAGE.to_string()];
            }
        };

        let mut lines = Vec::with_capacity(errors.len().min(MAX_SCREEN_LINES) + 2);
        if defaulted {
            lines.push(default_pack_hint());
        }
        lines.extend(
            errors
                .iter()
                .take(MAX_SCREEN_LINES)
                .map(ToString::to_string),
        );
        if errors.len() > MAX_SCREEN_LINES {
            lines.push(format!(
                "… and {} more — see {}",
                errors.len() - MAX_SCREEN_LINES,
                session_log_file_name(pack_root)
            ));
        }
        lines
    }
}

/// `Esc`, `Enter`/`Space` or a click on the error screen quits with a non-zero
/// code. Runs only while a [`StartupFailure`] is recorded.
pub fn error_screen_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut exit: MessageWriter<AppExit>,
) {
    let quit = keys.just_pressed(KeyCode::Escape)
        || keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space)
        || mouse.just_pressed(MouseButton::Left);
    if quit {
        exit.write(AppExit::error());
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    use super::super::captures::{capture_records, install_capture};
    use super::super::funnel::{report, DefensiveHold, Failure};
    use super::super::notice::Notice;
    use super::super::rows::FailureRow;
    #[test]
    fn the_error_screen_shows_twenty_lines_and_then_a_pointer() {
        let errors = (0..25)
            .map(|i| LoadError::new(format!("scenes/s{i}.unmei"), Some(i), "boom"))
            .collect();
        let failure = StartupFailure::Pack {
            pack_root: PathBuf::from("/games/pack"),
            errors,
            defaulted: false,
        };
        let lines = failure.screen_lines();
        assert_eq!(lines.len(), MAX_SCREEN_LINES + 1);
        assert_eq!(lines[0], "scenes/s0.unmei:0: boom");
        assert_eq!(lines[MAX_SCREEN_LINES], "… and 5 more — see pack.log");
    }
    #[test]
    fn the_error_screen_pointer_names_the_pack_sibling_log() {
        let errors = (0..25)
            .map(|i| LoadError::new(format!("scenes/s{i}.unmei"), Some(i), "boom"))
            .collect();
        let failure = StartupFailure::Pack {
            pack_root: PathBuf::from("/games/demo-pack"),
            errors,
            defaulted: false,
        };
        let lines = failure.screen_lines();
        assert_eq!(lines[MAX_SCREEN_LINES], "… and 5 more — see demo-pack.log");
    }

    /// The hint explains the default rule, so it shows only when the default was
    /// used: an explicit `PACK_DIR` never sees it.
    #[test]
    fn the_default_pack_hint_shows_only_when_the_default_was_used() {
        const HINT: &str = "No PACK_DIR given — expected demo-pack beside unmei-play.";
        let errors = vec![LoadError::new("pack.ron", None, "missing")];

        let defaulted = StartupFailure::Pack {
            pack_root: PathBuf::from("/games/demo-pack"),
            errors: errors.clone(),
            defaulted: true,
        };
        assert!(
            defaulted.screen_lines().iter().any(|line| line == HINT),
            "the defaulted path explains itself: {:?}",
            defaulted.screen_lines()
        );

        let explicit = StartupFailure::Pack {
            pack_root: PathBuf::from("/games/my-pack"),
            errors,
            defaulted: false,
        };
        assert!(
            !explicit
                .screen_lines()
                .iter()
                .any(|line| line.contains("PACK_DIR")),
            "an explicit pack shows no hint: {:?}",
            explicit.screen_lines()
        );
    }

    /// The no-executable screen states the one message `stderr` also prints, and
    /// points at no log because no pack exists for a log to sit beside.
    #[test]
    fn the_no_executable_screen_states_the_shared_message_and_names_no_log() {
        let lines = StartupFailure::NoExecutable.screen_lines();
        assert_eq!(
            lines,
            vec![crate::launch::NO_EXECUTABLE_MESSAGE.to_string()]
        );
        assert!(lines[0].contains("PACK_DIR"), "{}", lines[0]);
        assert!(lines[0].contains("executable"), "{}", lines[0]);
        assert!(
            !lines.iter().any(|line| line.contains(".log")),
            "no log pointer: {lines:?}"
        );
        assert_eq!(
            StartupFailure::NoExecutable.pack_root(),
            None,
            "there is no pack path to draw"
        );
    }
    #[test]
    fn the_log_pointer_keeps_a_dotted_pack_name_whole() {
        assert_eq!(
            session_log_file_name(Path::new("/games/my.pack")),
            "my.pack.log"
        );
    }
    #[test]
    fn twenty_errors_show_without_a_pointer() {
        let errors = (0..MAX_SCREEN_LINES)
            .map(|i| LoadError::new(format!("scenes/s{i}.unmei"), None, "boom"))
            .collect();
        let failure = StartupFailure::Pack {
            pack_root: PathBuf::from("/games/pack"),
            errors,
            defaulted: false,
        };
        let lines = failure.screen_lines();
        assert_eq!(lines.len(), MAX_SCREEN_LINES);
        assert!(
            !lines.iter().any(|line| line.contains("more")),
            "no pointer at exactly the cap: {lines:?}"
        );
    }
    #[test]
    fn a_pack_load_failure_carries_every_error_into_one_log_line() {
        install_capture();
        let errors = (0..25)
            .map(|i| LoadError::new(format!("scenes/s{i}.unmei"), None, "boom"))
            .collect::<Vec<_>>();
        let detail = errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let mut notice = Notice::default();
        let mut hold = DefensiveHold::default();
        capture_records();
        report(
            Failure::new(FailureRow::PackLoad, detail),
            &mut notice,
            &mut hold,
        );

        let records = capture_records();
        assert_eq!(records.len(), 1, "row 1 is one log record");
        for i in 0..25 {
            assert!(
                records[0].2.contains(&format!("scenes/s{i}.unmei")),
                "every LoadError must reach the log line"
            );
        }
        assert!(!notice.is_showing());
    }
    /// F49: a real bad-pack run keeps the whole error list for the screen cap.
    #[test]
    fn a_broken_pack_run_keeps_every_error_and_caps_the_screen() {
        use bevy::prelude::{ButtonInput, KeyCode, MouseButton};
        use bevy::MinimalPlugins;

        let dir = tempfile::tempdir().expect("temp dir");
        let pack_root = dir.path().join("broken-pack");
        std::fs::create_dir_all(pack_root.join("npcs")).expect("npcs dir");
        std::fs::create_dir_all(pack_root.join("scenes")).expect("scenes dir");
        std::fs::write(
            pack_root.join("pack.ron"),
            "(name: \"broken\", entry: \"scene\")",
        )
        .expect("manifest");
        std::fs::write(pack_root.join("state.ron"), "{}").expect("state");
        std::fs::write(pack_root.join("scenes/scene.unmei"), "Narrator: hi\n").expect("scene");
        // 21 NPC files whose ids do not match their filename stems.
        for i in 0..21 {
            std::fs::write(
                pack_root.join(format!("npcs/npc{i}.ron")),
                format!("(id: \"other{i}\", name: \"N{i}\", decisions: {{}})"),
            )
            .expect("npc");
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(crate::RuntimeConfig::new(pack_root, None))
            .add_plugins(crate::RuntimeCorePlugin);
        app.update();

        let failure = app
            .world()
            .get_resource::<StartupFailure>()
            .expect("the failed load is recorded");
        let StartupFailure::Pack {
            errors, defaulted, ..
        } = failure
        else {
            panic!("a broken pack is a pack failure: {failure:?}");
        };
        assert_eq!(errors.len(), 21, "the error list is kept whole");
        assert!(!defaulted, "the test world passes an explicit pack path");
        assert_eq!(
            failure.screen_lines().len(),
            MAX_SCREEN_LINES + 1,
            "20 lines plus the pointer"
        );
        assert!(
            app.world().get_resource::<crate::Runtime>().is_none(),
            "play stays shut behind the screen"
        );
    }
}
