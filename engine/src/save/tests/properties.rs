//! The save round trip over generated saves: any state, position, wait,
//! staging and timestamp survives `to_ron` then `from_ron`.

use proptest::prelude::*;

use super::*;
use crate::pack_ron::strategies::{config, same_state, state_map, text};

fn wait() -> impl Strategy<Value = Wait> {
    prop_oneof![
        (text(), text()).prop_map(|(speaker, text)| Wait::Say { speaker, text }),
        prop::collection::vec((text(), text()), 0..4).prop_map(|options| Wait::Choice { options }),
        (text(), text(), text()).prop_map(|(npc_id, question, prefix)| Wait::Decision {
            npc_id,
            question,
            prefix,
        }),
    ]
}

fn staging() -> impl Strategy<Value = StagingState> {
    (
        prop::option::of(text()),
        prop::collection::btree_map(text(), text(), 0..4),
        prop::option::of(text()),
    )
        .prop_map(|(background, sprites, bgm)| StagingState {
            background,
            sprites,
            bgm,
        })
}

fn save_file() -> impl Strategy<Value = SaveFile> {
    (
        state_map(),
        text(),
        any::<usize>(),
        prop::option::of(wait()),
        staging(),
        prop::option::of(any::<u64>()),
    )
        .prop_map(|(state, scene_id, pc, wait, staging, saved_at)| SaveFile {
            version: SAVE_VERSION,
            state,
            scene_id,
            pc,
            wait,
            staging,
            saved_at,
        })
}

proptest! {
    #![proptest_config(config())]

    /// Any save reads back as exactly the save that was written.
    #[test]
    fn a_save_reads_back_exactly(save in save_file()) {
        let text = save.to_ron().expect("a generated save encodes");
        let decoded = SaveFile::from_ron(&text);
        prop_assert!(
            decoded
                .as_ref()
                .is_ok_and(|decoded| decoded == &save && same_state(&decoded.state, &save.state)),
            "{save:?} encoded as {text} decoded as {decoded:?}"
        );
    }
}
