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
