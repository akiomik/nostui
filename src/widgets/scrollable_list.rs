pub trait ScrollableList<T> {
    fn select(&mut self, index: Option<usize>);

    fn selected(&self) -> Option<usize>;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool;

    fn scroll_up(&mut self) {
        let selection = match self.selected() {
            _ if self.is_empty() => None,
            Some(i) if i > 1 => Some(i - 1),
            _ => Some(0),
        };
        self.select(selection);
    }

    fn scroll_down(&mut self) {
        let selection = match self.selected() {
            _ if self.is_empty() => None,
            Some(i) if i < self.len() - 1 => Some(i + 1),
            Some(_) => Some(self.len() - 1),
            None if self.len() > 1 => Some(1),
            None => Some(0),
        };
        self.select(selection);
    }

    fn scroll_to_top(&mut self) {
        let selection = match self.selected() {
            _ if self.is_empty() => None,
            _ => Some(0),
        };
        self.select(selection);
    }

    fn scroll_to_bottom(&mut self) {
        let selection = match self.selected() {
            _ if self.is_empty() => None,
            _ => Some(self.len() - 1),
        };
        self.select(selection);
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use pretty_assertions::assert_eq;

    use super::*;

    #[derive(Default)]
    struct TestScrollableList {
        items: Vec<usize>,
        index: Option<usize>,
    }

    impl TestScrollableList {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl ScrollableList<usize> for TestScrollableList {
        fn len(&self) -> usize {
            self.items.len()
        }

        fn is_empty(&self) -> bool {
            self.items.is_empty()
        }

        fn select(&mut self, index: Option<usize>) {
            self.index = index;
        }

        fn selected(&self) -> Option<usize> {
            self.index
        }
    }

    #[test]
    fn test_scroll_up_empty() {
        let mut list = TestScrollableList::new();
        list.scroll_up();
        assert_eq!(list.selected(), None)
    }

    #[test]
    fn test_scroll_up_normal() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2];
        list.select(Some(1));
        assert_eq!(list.selected(), Some(1));
        list.scroll_up();
        assert_eq!(list.selected(), Some(0));
    }

    #[test]
    fn test_scroll_up_unselected() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2];
        list.scroll_up();
        assert_eq!(list.selected(), Some(0));
    }

    #[test]
    fn test_scroll_up_saturated_top() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2];
        list.scroll_up();
        assert_eq!(list.selected(), Some(0));
        list.scroll_up();
        assert_eq!(list.selected(), Some(0));
    }

    #[test]
    fn test_scroll_down_empty() {
        let mut list = TestScrollableList::new();
        list.scroll_down();
        assert_eq!(list.selected(), None)
    }

    #[test]
    fn test_scroll_down_normal() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2];
        list.select(Some(0));
        assert_eq!(list.selected(), Some(0));
        list.scroll_down();
        assert_eq!(list.selected(), Some(1));
    }

    #[test]
    fn test_scroll_down_unselected_1() {
        let mut list = TestScrollableList::new();
        list.items = vec![1];
        list.scroll_down();
        assert_eq!(list.selected(), Some(0));
    }

    #[test]
    fn test_scroll_down_unselected_2() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2];
        list.scroll_down();
        assert_eq!(list.selected(), Some(1));
    }

    #[test]
    fn test_scroll_down_unselected_3() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2, 3];
        list.scroll_down();
        assert_eq!(list.selected(), Some(1));
    }

    #[test]
    fn test_scroll_down_saturated_bottom() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2];
        list.scroll_down();
        assert_eq!(list.selected(), Some(1));
        list.scroll_down();
        assert_eq!(list.selected(), Some(1));
    }

    #[test]
    fn test_scroll_to_top_empty() {
        let mut list = TestScrollableList::new();
        list.scroll_to_top();
        assert_eq!(list.selected(), None);
    }

    #[test]
    fn test_scroll_to_top_unselected() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2, 3];
        list.scroll_to_top();
        assert_eq!(list.selected(), Some(0));
    }

    #[test]
    fn test_scroll_to_top_selected() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2, 3];
        list.select(Some(2));
        assert_eq!(list.selected(), Some(2));
        list.scroll_to_top();
        assert_eq!(list.selected(), Some(0));
    }

    #[test]
    fn test_scroll_to_top_saturated_top() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2, 3];
        list.select(Some(0));
        assert_eq!(list.selected(), Some(0));
        list.scroll_to_top();
        assert_eq!(list.selected(), Some(0));
    }

    #[test]
    fn test_scroll_to_bottom_empty() {
        let mut list = TestScrollableList::new();
        list.scroll_to_bottom();
        assert_eq!(list.selected(), None);
    }

    #[test]
    fn test_scroll_to_bottom_unselected() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2, 3];
        list.scroll_to_bottom();
        assert_eq!(list.selected(), Some(2));
    }

    #[test]
    fn test_scroll_to_bottom_selected() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2, 3];
        list.select(Some(0));
        assert_eq!(list.selected(), Some(0));
        list.scroll_to_bottom();
        assert_eq!(list.selected(), Some(2));
    }

    #[test]
    fn test_scroll_to_bottom_saturated_bottom() {
        let mut list = TestScrollableList::new();
        list.items = vec![1, 2, 3];
        list.select(Some(2));
        assert_eq!(list.selected(), Some(2));
        list.scroll_to_bottom();
        assert_eq!(list.selected(), Some(2));
    }
}
