pub use blit_layout::{Align, Justify};

pub type Length = f32;
pub type Padding = blit_layout::Padding<Length>;
pub type Sizing = blit_layout::Sizing<Length>;

blit_layout::layout_ext!(Length);

pub mod absolute {
    pub use blit_layout::absolute::Anchor;
    pub type Layout<L> = blit_layout::absolute::Layout<L, super::Length>;
}

pub mod flex {
    pub type Layout = blit_layout::flex::Layout<super::Length>;
    pub type Item = blit_layout::flex::Item<super::Length>;

    pub const fn layout(axis: blit::Axis) -> Layout {
        Layout::new(axis)
    }
    pub const fn row() -> Layout {
        layout(blit::Axis::Horizontal)
    }
    pub const fn column() -> Layout {
        layout(blit::Axis::Vertical)
    }
    pub const fn item() -> Item {
        Item::new()
    }
}

pub mod wrap {
    pub type Layout = blit_layout::wrap::Layout<super::Length>;
    pub type Item = blit_layout::wrap::Item<super::Length>;

    pub const fn layout(axis: blit::Axis) -> Layout {
        Layout::new(axis)
    }
    pub const fn horizontal() -> Layout {
        layout(blit::Axis::Horizontal)
    }
    pub const fn vertical() -> Layout {
        layout(blit::Axis::Vertical)
    }
    pub const fn item() -> Item {
        Item::new()
    }
}

pub mod single {
    pub type Layout = blit_layout::single::Layout<super::Length>;
    pub type Item = blit_layout::single::Item<super::Length>;

    pub const fn layout() -> Layout {
        Layout::new()
    }
    pub const fn item() -> Item {
        Item::new()
    }
}

pub mod grid {
    pub type Layout<const N: usize = 64> = blit_layout::grid::Layout<super::Length, N>;
    pub type Item = blit_layout::grid::Item<super::Length>;

    pub const fn columns(columns: u16) -> Layout {
        Layout::new(columns)
    }
    pub const fn item() -> Item {
        Item::new()
    }
}
