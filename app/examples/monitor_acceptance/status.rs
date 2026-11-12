//! The status footer's readouts and the runner wait the plans compare.

use crate::prelude::*;

pub(crate) struct StatusCounters {
    pub(crate) retained: u64,
    pub(crate) shown: u64,
    pub(crate) evicted: u64,
    pub(crate) lost: u64,
    pub(crate) truncated: u64,
    pub(crate) updating: bool,
    pub(crate) expired: bool,
    pub(crate) following: bool,
    pub(crate) text: String,
}

pub(crate) fn status_counters(snap: &Snap) -> Option<StatusCounters> {
    let status = snap.status()?;
    let number_after = |key: &str| -> Option<u64> {
        let start = status.find(key)? + key.len();
        let digits: String = status[start..]
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    };
    Some(StatusCounters {
        retained: number_after("retained")?,
        shown: number_after("shown")?,
        evicted: number_after("evicted")?,
        lost: number_after("lost")?,
        truncated: number_after("truncated")?,
        updating: status.contains("updating view"),
        expired: status.contains("older lines expired"),
        following: status.contains("following latest"),
        text: status,
    })
}

pub(crate) fn status_of(world: &mut World) -> (Option<StatusCounters>, Snap) {
    let snap = crate::harness::snap(world);
    (status_counters(&snap), snap)
}

pub(crate) fn wait_of(world: &mut World) -> Option<String> {
    world
        .get_resource::<Runtime>()
        .map(|runtime| format!("{:?}", runtime.runner.wait()))
}

pub(crate) fn wait_kind(world: &mut World) -> Option<Wait> {
    world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.runner.wait().cloned())
}
