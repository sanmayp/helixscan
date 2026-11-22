use helixscan_core::{Panel, scan::Progress};

#[test]
fn concurrent_progress_sums_worker_votes_without_loss() {
    let panel: Panel = serde_json::from_reader(
        std::fs::File::open(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/panel.json"),
        )
        .expect("open verified panel"),
    )
    .expect("parse verified panel");
    let progress = Progress::new(panel.variants.len());
    let counts = vec![[7, 11, 13, 17]; panel.variants.len()];
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let progress = &progress;
            let counts = &counts;
            scope.spawn(move || progress.update_votes(counts));
        }
    });
    for live in progress.votes.iter() {
        for (actual, expected) in live.iter().zip([7, 11, 13, 17]) {
            assert_eq!(
                actual.load(std::sync::atomic::Ordering::Relaxed),
                expected * 4
            );
        }
    }
    progress.reset();
    assert!(
        progress
            .votes
            .iter()
            .flatten()
            .all(|count| { count.load(std::sync::atomic::Ordering::Relaxed) == 0 })
    );
}
