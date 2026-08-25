use gitcomet_state::session::HistoryGraphStylePreset;

/// All geometry that must change as one unit when switching graph styles.
/// Keeping this value copyable lets the hot row renderer capture it without
/// consulting session state or allocating per row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::view) struct HistoryGraphMetrics {
    pub(in crate::view) row_height: f32,
    pub(in crate::view) lane_pitch: f32,
    pub(in crate::view) margin_x: f32,
    pub(in crate::view) elbow_radius: f32,
    pub(in crate::view) stroke_width: f32,
    pub(in crate::view) node_radius: f32,
    pub(in crate::view) node_corner_radius: f32,
}

pub(in crate::view) const SOURCETREE_GRAPH_METRICS: HistoryGraphMetrics = HistoryGraphMetrics {
    row_height: 20.0,
    lane_pitch: 11.0,
    margin_x: 11.0,
    elbow_radius: 5.0,
    stroke_width: 2.0,
    node_radius: 3.5,
    node_corner_radius: 3.5,
};

pub(in crate::view) const GITCOMET_GRAPH_METRICS: HistoryGraphMetrics = HistoryGraphMetrics {
    row_height: 28.0,
    lane_pitch: 16.0,
    margin_x: 10.0,
    elbow_radius: 6.0,
    stroke_width: 1.6,
    node_radius: 3.4,
    node_corner_radius: 2.0,
};

pub(in crate::view) fn history_graph_metrics(
    style: HistoryGraphStylePreset,
) -> HistoryGraphMetrics {
    match style {
        HistoryGraphStylePreset::SourceTree => SOURCETREE_GRAPH_METRICS,
        HistoryGraphStylePreset::GitComet => GITCOMET_GRAPH_METRICS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_keep_their_measured_geometry_together() {
        let source = history_graph_metrics(HistoryGraphStylePreset::SourceTree);
        assert_eq!(source.row_height, 20.0);
        assert_eq!(source.lane_pitch, 11.0);
        assert_eq!(source.stroke_width, 2.0);
        assert_eq!(source.node_radius * 2.0, 7.0);

        let classic = history_graph_metrics(HistoryGraphStylePreset::GitComet);
        assert_eq!(classic.row_height, 28.0);
        assert_eq!(classic.lane_pitch, 16.0);
        assert_eq!(classic.stroke_width, 1.6);
        assert_eq!(classic.node_corner_radius, 2.0);
    }
}
