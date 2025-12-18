/// Coalescing helper for render/resize decisions
pub struct Coalescer;

impl Coalescer {
    /// Pure decision function: whether to render this loop based on coalesced inputs
    #[inline]
    pub fn decide_render(queued_render_reqs: usize, saw_tui_render: bool) -> bool {
        queued_render_reqs > 0 || saw_tui_render
    }

    /// Pure decision function: coalesce multiple resizes into last-only
    #[inline]
    pub fn decide_resize(
        last_seen: Option<(u16, u16)>,
        events: &[(u16, u16)],
    ) -> Option<(u16, u16)> {
        if let Some((w, h)) = events.last().cloned() {
            Some((w, h))
        } else {
            last_seen
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Coalescer;

    #[test]
    fn decide_render_table_tests() {
        assert!(!Coalescer::decide_render(0, false));
        assert!(Coalescer::decide_render(1, false));
        assert!(Coalescer::decide_render(2, false));
        assert!(Coalescer::decide_render(0, true));
        assert!(Coalescer::decide_render(3, true));
    }

    #[test]
    fn decide_resize_last_only_table_tests() {
        assert_eq!(Coalescer::decide_resize(None, &[]), None);
        assert_eq!(Coalescer::decide_resize(None, &[(10, 10)]), Some((10, 10)));
        assert_eq!(
            Coalescer::decide_resize(None, &[(10, 10), (20, 30)]),
            Some((20, 30))
        );
        assert_eq!(
            Coalescer::decide_resize(Some((5, 5)), &[(100, 200)]),
            Some((100, 200))
        );
        assert_eq!(Coalescer::decide_resize(Some((5, 5)), &[]), Some((5, 5)));
    }
}
