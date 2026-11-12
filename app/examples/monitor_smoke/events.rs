//! The real diagnostics the walk admits into the dev capture.

/// Emit the walk's fixtures through the production `log` path: forty numbered
/// events (some with a long wrapped body), the bidi short fixture, the long
/// wrapped message and the long target that must wrap inside its column.
pub(crate) fn emit_smoke_events() {
    for i in 0..40 {
        let filler = if i % 7 == 0 {
            "x".repeat(180)
        } else {
            String::new()
        };
        log::info!(target: "unmei::smoke::seq", "event {i} {filler}");
    }
    log::info!(target: "unmei::smoke\u{202E}", "monitor smoke fixture");
    log::info!(
        target: "unmei::smoke",
        "{}",
        "SystemInfo { os: \"windows\", kernel: \"26300\", cpu: \"Intel(R) Core(TM) i7-14650HX\", core_count: 24, physical_core_count: 24, memory: 31.8 GB, dpi: 96, window: 1280x720, backend: \"Vulkan\", driver: \"NVIDIA 617.42\", frame: 1234, uptime: \"12m\", profile: \"dev\", marker: \"monitor-smoke-wrapped-message\" }"
    );
    log::info!(
        target: "unmei::smoke::a_deliberately_long_module_path_that_does_not_fit_the_column::the_target_column_tail",
        "short body"
    );
}
