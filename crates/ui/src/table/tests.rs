//! What the table decides before it draws anything.
//!
//! The component itself needs a document; these are the decisions that do not.
//! A cell renderer and a row action are both predicates over a row, and both
//! have a rule about them worth holding: a rendered cell is still findable by
//! the filter box, and a control is absent on the rows it would be refused on.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    name: &'static str,
    status: &'static str,
    mine: bool,
}

const ROWS: [Row; 3] = [
    Row {
        name: "Founders",
        status: "active",
        mine: true,
    },
    Row {
        name: "Archive",
        status: "disabled",
        mine: false,
    },
    Row {
        name: "Drafts",
        status: "draft",
        mine: true,
    },
];

fn columns() -> Vec<Column<Row>> {
    vec![
        Column::new("Name", |row: &Row| row.name.to_owned()),
        Column::new("Status", |row: &Row| row.status.to_owned()).state(),
    ]
}

fn cells(columns: &[Column<Row>]) -> Vec<Vec<String>> {
    ROWS.iter()
        .map(|row| columns.iter().map(|column| (column.value)(row)).collect())
        .collect()
}

#[test]
fn a_rendered_column_still_produces_the_text_the_filter_searches() {
    let columns = columns();
    let cells = cells(&columns);
    assert_eq!(cells[1], vec!["Archive".to_owned(), "disabled".to_owned()]);

    // The filter reads the same string the renderer will be handed, so a
    // status a reader can see is a status they can also search for.
    let found = arrange(&cells, "disabled", None, 0, 25);
    assert_eq!(found.visible, vec![1]);
    assert_eq!(found.matched, 1);
}

#[test]
fn only_the_columns_that_asked_for_one_carry_a_renderer() {
    let columns = columns();
    assert!(columns[0].render.is_none(), "a plain column draws its text");
    assert!(columns[1].render.is_some(), "a state column draws a word");
    // And a renderer survives the clone the component makes.
    assert!(columns[1].clone().render.is_some());
}

#[test]
fn a_row_action_is_absent_where_it_would_be_refused() {
    let action = RowAction::new("Leave", Callback::new(|_row: Row| ())).when(|row: &Row| row.mine);
    let offered = action.offered.expect("the predicate was kept");
    assert!(offered(&ROWS[0]));
    assert!(!offered(&ROWS[1]), "not this reader's row");
    assert!(offered(&ROWS[2]));
}

#[test]
fn an_action_with_no_predicate_is_offered_on_every_row() {
    let action = RowAction::new("Revoke", Callback::new(|_row: Row| ())).undo();
    assert!(action.offered.is_none());
    assert!(action.undo, "a verb that takes something away says so");
    assert_eq!(action.label, "Revoke");
}
