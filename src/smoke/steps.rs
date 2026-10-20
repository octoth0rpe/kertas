//! The steps of the `1` and `write` smoke modes, shared with the unit
//! tests that take them through a document headless
//! (crates/kertas-editor/tests/layout.rs).

use kertas_editor::actions::{
    Backspace, Delete, DeleteWordLeft, IndentLines, IndentTab, InsertTab, LineBreak, MoveBlockDown,
    MoveBlockUp, MoveDocumentEnd, MoveDocumentStart, MoveDown, MoveLeft, MoveRight, MoveUp,
    Newline, OutdentTab, Redo, SelectAll, SelectBlockDown, SelectDown, SelectUp, Undo, Unreveal,
};
use kertas_editor::gpui::Action;

pub enum Step {
    Action(Box<dyn Action>),
    Type(&'static str),
}

fn repeat(steps: &mut Vec<Step>, action: &dyn Action, count: usize) {
    steps.extend((0..count).map(|_| Step::Action(action.boxed_clone())));
}

/// Moving around a document of `items` items, by character, line and
/// block. Lines and characters are moved through near the start; blocks
/// all the way to the end and back, though no further than 150, which
/// reaches well past the measured part of a long document.
pub fn movement_steps(steps: &mut Vec<Step>, items: usize) {
    let blocks = (items + 5).min(150);
    // With no caret, ↑ and ↓ only scroll; → starts editing (#99).
    repeat(steps, &MoveRight, 1);
    repeat(steps, &MoveDocumentStart, 1);
    repeat(steps, &MoveDown, 60);
    repeat(steps, &MoveUp, 60);
    repeat(steps, &MoveRight, 40);
    repeat(steps, &MoveLeft, 40);
    repeat(steps, &MoveDocumentEnd, 1);
    repeat(steps, &SelectUp, 30);
    repeat(steps, &SelectDown, 30);
    repeat(steps, &Unreveal, 1);
    repeat(steps, &MoveBlockDown, blocks);
    repeat(steps, &MoveBlockUp, blocks);
    repeat(steps, &SelectBlockDown, 5);
}

/// Writing at the end: paragraphs through the gap, a list, a quote and a
/// code block with indentation.
pub fn writing_steps(steps: &mut Vec<Step>) {
    // With no caret, ⌘↓ only scrolls; → starts editing (#99).
    repeat(steps, &MoveRight, 1);
    repeat(steps, &MoveDocumentEnd, 1);
    repeat(steps, &Newline, 2);
    repeat(steps, &MoveUp, 3);
    repeat(steps, &MoveDown, 3);
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("New paragraph"));
    repeat(steps, &LineBreak, 1);
    steps.push(Step::Type("second line"));
    repeat(steps, &Newline, 2);
    steps.push(Step::Type("- item"));
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("[ ] task"));
    repeat(steps, &IndentTab, 1);
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("sub"));
    repeat(steps, &OutdentTab, 1);
    repeat(steps, &Newline, 2);
    steps.push(Step::Type("> quote"));
    repeat(steps, &LineBreak, 1);
    repeat(steps, &Newline, 3);
    repeat(steps, &MoveBlockUp, 4);
    repeat(steps, &MoveBlockDown, 4);
    repeat(steps, &MoveDocumentEnd, 1);
    steps.push(Step::Type("```"));
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("fn"));
    repeat(steps, &IndentTab, 1);
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("    x"));
    repeat(steps, &IndentLines, 1);
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("y"));
    repeat(steps, &OutdentTab, 1);
    repeat(steps, &InsertTab, 1);
    repeat(steps, &Undo, 1);
}

/// Edits all over the document, undo and redo, and replacing everything.
pub fn edit_steps(steps: &mut Vec<Step>) {
    // Editing at the start: a new heading and paragraph, then undo/redo.
    repeat(steps, &MoveDocumentStart, 1);
    steps.push(Step::Type("# Smoke"));
    repeat(steps, &Newline, 2);
    steps.push(Step::Type("Typed **bold** text"));
    repeat(steps, &Newline, 2);
    repeat(steps, &Backspace, 5);
    repeat(steps, &Undo, 4);
    repeat(steps, &Redo, 2);
    // Edits sprinkled through the document, including joining blocks.
    for _ in 0..12 {
        repeat(steps, &MoveDown, 3);
        steps.push(Step::Type("x"));
        repeat(steps, &Backspace, 2);
        repeat(steps, &Delete, 1);
        repeat(steps, &DeleteWordLeft, 1);
        repeat(steps, &Newline, 1);
    }
    repeat(steps, &Undo, 20);
    // Replace everything, then empty the document entirely.
    repeat(steps, &SelectAll, 1);
    steps.push(Step::Type("Replaced"));
    repeat(steps, &SelectAll, 1);
    repeat(steps, &Backspace, 1);
    steps.push(Step::Type("a"));
    repeat(steps, &Backspace, 3);
    repeat(steps, &Undo, 3);
}
