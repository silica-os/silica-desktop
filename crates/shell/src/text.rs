//! Text helpers shared by the shell's views.

use std::rc::Rc;

use vetro_lib::amalfi::foundation::Label;
use vetro_lib::text::{Align, TextLayout, TextStyle};

/// Lays `text` out with label `i` of `labels` (made when missing): a view keeps one label per
/// text it draws, so unchanged texts are not laid out again.
pub fn lay(
    labels: &mut Vec<Label>,
    i: usize,
    text: &str,
    style: &TextStyle,
    align: Align,
    width: f32,
    lines: Option<usize>,
) -> Rc<TextLayout> {
    if labels.len() <= i {
        labels.resize_with(i + 1, Label::default);
    }
    match labels.get_mut(i) {
        Some(l) => l.layout(text, style, align, width, lines),
        None => Label::default().layout(text, style, align, width, lines),
    }
}
