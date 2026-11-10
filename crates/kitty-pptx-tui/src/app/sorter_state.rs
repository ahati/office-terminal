use crate::app::mode::AppMode;
use crate::app::state::PresentationState;

/// State and 2D cursor navigation logic for the Slide Sorter Grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SorterState {
    pub selected_index: usize,
    pub cols: usize,
    pub wrap: bool,
}

impl Default for SorterState {
    fn default() -> Self {
        Self {
            selected_index: 0,
            cols: 4,
            wrap: false,
        }
    }
}

impl SorterState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn set_selected_index(&mut self, index: usize, total_slides: usize) -> bool {
        if total_slides == 0 {
            self.selected_index = 0;
            return false;
        }
        let clamped = index.min(total_slides - 1);
        let changed = self.selected_index != clamped;
        self.selected_index = clamped;
        changed
    }

    pub fn cols(&self) -> usize {
        self.cols.max(1)
    }

    pub fn set_cols(&mut self, cols: usize) {
        self.cols = cols.max(1);
    }

    pub fn wrap(&self) -> bool {
        self.wrap
    }

    pub fn set_wrap(&mut self, wrap: bool) {
        self.wrap = wrap;
    }

    pub fn left(&mut self, total_slides: usize) -> bool {
        if total_slides == 0 {
            return false;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
            true
        } else if self.wrap {
            self.selected_index = total_slides - 1;
            true
        } else {
            false
        }
    }

    pub fn right(&mut self, total_slides: usize) -> bool {
        if total_slides == 0 {
            return false;
        }
        let last = total_slides - 1;
        if self.selected_index < last {
            self.selected_index += 1;
            true
        } else if self.wrap {
            self.selected_index = 0;
            true
        } else {
            false
        }
    }

    pub fn up(&mut self, total_slides: usize) -> bool {
        if total_slides == 0 {
            return false;
        }
        let cols = self.cols.max(1);
        if self.selected_index >= cols {
            self.selected_index -= cols;
            true
        } else if self.wrap {
            let col = self.selected_index % cols;
            let rows = total_slides.div_ceil(cols);
            let target = ((rows - 1) * cols + col).min(total_slides - 1);
            if target != self.selected_index {
                self.selected_index = target;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn down(&mut self, total_slides: usize) -> bool {
        if total_slides == 0 {
            return false;
        }
        let cols = self.cols.max(1);
        let last = total_slides - 1;
        let next = self.selected_index + cols;
        if next <= last {
            self.selected_index = next;
            true
        } else if self.selected_index / cols < last / cols {
            self.selected_index = last;
            true
        } else if self.wrap {
            let col = self.selected_index % cols;
            if col != self.selected_index {
                self.selected_index = col;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn page_up(&mut self, total_slides: usize) -> bool {
        if total_slides == 0 {
            return false;
        }
        let step = self.cols.max(1) * 2;
        let target = self.selected_index.saturating_sub(step);
        let changed = self.selected_index != target;
        self.selected_index = target;
        changed
    }

    pub fn page_down(&mut self, total_slides: usize) -> bool {
        if total_slides == 0 {
            return false;
        }
        let step = self.cols.max(1) * 2;
        let last = total_slides.saturating_sub(1);
        let target = (self.selected_index + step).min(last);
        let changed = self.selected_index != target;
        self.selected_index = target;
        changed
    }

    pub fn home(&mut self, total_slides: usize) -> bool {
        if total_slides == 0 {
            return false;
        }
        let changed = self.selected_index != 0;
        self.selected_index = 0;
        changed
    }

    pub fn end(&mut self, total_slides: usize) -> bool {
        if total_slides == 0 {
            return false;
        }
        let last = total_slides - 1;
        let changed = self.selected_index != last;
        self.selected_index = last;
        changed
    }

    pub fn update_cols_for_width(&mut self, width: u16) {
        self.cols = if width < 60 {
            2
        } else if width < 100 {
            3
        } else if width < 140 {
            4
        } else {
            5
        };
    }
}

/// PresentationState extensions for Sorter Grid.
impl PresentationState {
    pub fn selected_sorter_index(&self) -> usize {
        self.sorter.selected_index()
    }

    pub fn set_selected_sorter_index(&mut self, index: usize) -> bool {
        self.sorter.set_selected_index(index, self.total_slides())
    }

    pub fn sorter_cols(&self) -> usize {
        self.sorter.cols()
    }

    pub fn set_sorter_cols(&mut self, cols: usize) {
        self.sorter.set_cols(cols);
    }

    pub fn sorter_wrap(&self) -> bool {
        self.sorter.wrap()
    }

    pub fn set_sorter_wrap(&mut self, wrap: bool) {
        self.sorter.set_wrap(wrap);
    }

    pub fn toggle_sorter(&mut self) {
        if self.mode() == AppMode::Sorter {
            self.exit_sorter(false);
        } else {
            self.enter_overlay(AppMode::Sorter);
            self.sorter.selected_index = self.current_slide();
        }
    }

    pub fn open_sorter(&mut self) {
        if self.mode() != AppMode::Sorter {
            self.enter_overlay(AppMode::Sorter);
            self.sorter.selected_index = self.current_slide();
        }
    }

    pub fn exit_sorter(&mut self, apply_selection: bool) -> bool {
        if self.mode() != AppMode::Sorter {
            return false;
        }
        let mut slide_changed = false;
        if apply_selection && self.total_slides() > 0 {
            let clamped = self.sorter.selected_index.min(self.total_slides() - 1);
            slide_changed = self.set_current_slide(clamped);
        }
        self.exit_overlay();
        slide_changed
    }

    pub fn sorter_left(&mut self) -> bool {
        let total = self.total_slides();
        self.sorter.left(total)
    }

    pub fn sorter_right(&mut self) -> bool {
        let total = self.total_slides();
        self.sorter.right(total)
    }

    pub fn sorter_up(&mut self) -> bool {
        let total = self.total_slides();
        self.sorter.up(total)
    }

    pub fn sorter_down(&mut self) -> bool {
        let total = self.total_slides();
        self.sorter.down(total)
    }

    pub fn sorter_page_up(&mut self) -> bool {
        let total = self.total_slides();
        self.sorter.page_up(total)
    }

    pub fn sorter_page_down(&mut self) -> bool {
        let total = self.total_slides();
        self.sorter.page_down(total)
    }

    pub fn sorter_home(&mut self) -> bool {
        let total = self.total_slides();
        self.sorter.home(total)
    }

    pub fn sorter_end(&mut self) -> bool {
        let total = self.total_slides();
        self.sorter.end(total)
    }

    pub fn update_layout_dimensions(&mut self, width: u16, height: u16) {
        self.sorter.update_cols_for_width(width);
        self.set_layout_dimensions(width, height);
    }
}
