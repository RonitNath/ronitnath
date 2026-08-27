//! The table.
//!
//! One component, generic over the row type, doing what an operator expects a
//! table to do: sort by any column, filter across all of them, page, drop the
//! columns that matter least when the viewport shrinks, and drill into a row.
//! Columns declare their own priority, so narrowing is an information decision
//! the caller made, not a squeeze the browser improvised.

mod arrange;

use std::sync::Arc;

use leptos::prelude::*;

pub use arrange::{Arrangement, Dir, Sort, arrange};

#[cfg(test)]
mod tests;

/// How long a column survives a narrowing viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Always shown. The column the row is identified by.
    Always,
    /// Dropped on a phone.
    Secondary,
    /// Dropped on a tablet, and on a phone.
    Tertiary,
}

impl Priority {
    fn class(self) -> &'static str {
        match self {
            Self::Always => "p1",
            Self::Secondary => "p2",
            Self::Tertiary => "p3",
        }
    }
}

/// How a cell is drawn, once the column has said what it says.
///
/// The text comes first and the view second, deliberately: sorting, filtering
/// and the empty check are all decided on the string, so a renderer can change
/// what a cell *looks* like without changing what the table knows.
pub type Render = Arc<dyn Fn(&str) -> AnyView + Send + Sync>;

/// The machine value behind a cell, for `title`. Absent means the cell is the
/// whole of what the column has to say.
pub type Titled<R> = Option<Arc<dyn Fn(&R) -> String + Send + Sync>>;

/// Whether a row admits a control. Absent means every row does.
pub type Offered<R> = Option<Arc<dyn Fn(&R) -> bool + Send + Sync>>;

/// One column: its header, how it reads a row, and how it behaves.
pub struct Column<R> {
    label: &'static str,
    value: Arc<dyn Fn(&R) -> String + Send + Sync>,
    priority: Priority,
    mono: bool,
    render: Option<Render>,
    title: Titled<R>,
}

impl<R> Clone for Column<R> {
    fn clone(&self) -> Self {
        Self {
            label: self.label,
            value: Arc::clone(&self.value),
            priority: self.priority,
            mono: self.mono,
            render: self.render.clone(),
            title: self.title.clone(),
        }
    }
}

impl<R> Column<R> {
    /// A column headed `label`, reading each row with `value`.
    ///
    /// The header is a human word — "Created", "Role" — never a schema name.
    pub fn new(label: &'static str, value: impl Fn(&R) -> String + Send + Sync + 'static) -> Self {
        Self {
            label,
            value: Arc::new(value),
            priority: Priority::Always,
            mono: false,
            render: None,
            title: None,
        }
    }

    /// Machine-given values — public ids, hashes, timestamps — set in mono
    /// with tabular figures, so columns of them line up.
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// What this column gives up first.
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// The machine value behind a cell, on hover and to a screen reader.
    ///
    /// For the column that shows a *name* where the row also carries an *id*:
    /// the name is what the reader recognises, the id is what they quote, and
    /// only one of them belongs in the cell. It is a `title`, not a second
    /// column, because it is never what the table is read for.
    pub fn titled(mut self, title: impl Fn(&R) -> String + Send + Sync + 'static) -> Self {
        self.title = Some(Arc::new(title));
        self
    }

    /// Draw this column's cells with `render` instead of as bare text.
    ///
    /// The string the column produced is still what the table sorts, filters
    /// and searches on, so a rendered cell is never a cell the filter box
    /// cannot find.
    pub fn render(mut self, render: impl Fn(&str) -> AnyView + Send + Sync + 'static) -> Self {
        self.render = Some(Arc::new(render));
        self
    }

    /// A status column: the word the schema uses, in its own colour.
    ///
    /// Never a pill, never a dot, never a tint behind it — `active`,
    /// `disabled`, `draft`, `published` are read as vocabulary, and the colour
    /// is the ink (`tokens.css` `.state`). A reader who cannot separate the
    /// hues still has the word.
    pub fn state(self) -> Self {
        self.render(|text| {
            let word = text.to_owned();
            view! {
                <span class="state" data-state=text.to_owned()>
                    {word}
                </span>
            }
            .into_any()
        })
    }
}

/// One per-row control: the verb, what it runs, and when it is offered.
///
/// Rare and destructive verbs still belong behind a side affordance rather
/// than in every row (`design/interface-taste.md`); these are for the ones the
/// list exists to run — withdrawing a link, ending a session — where a panel
/// per row would be a click each to reach the same button.
pub struct RowAction<R: 'static> {
    label: &'static str,
    run: Callback<R>,
    offered: Offered<R>,
    undo: bool,
}

impl<R> Clone for RowAction<R> {
    fn clone(&self) -> Self {
        Self {
            label: self.label,
            run: self.run,
            offered: self.offered.clone(),
            undo: self.undo,
        }
    }
}

impl<R> RowAction<R>
where
    R: Send + Sync + 'static,
{
    /// A control called `label` that runs `run` on the row it sits in.
    pub fn new(label: &'static str, run: impl Into<Callback<R>>) -> Self {
        Self {
            label,
            run: run.into(),
            offered: None,
            undo: false,
        }
    }

    /// Offer it only on the rows `offered` admits.
    ///
    /// Absent rather than present-and-refused: a control that lies until you
    /// use it is worse than no control.
    pub fn when(mut self, offered: impl Fn(&R) -> bool + Send + Sync + 'static) -> Self {
        self.offered = Some(Arc::new(offered));
        self
    }

    /// This one takes something away, which the ink says.
    pub fn undo(mut self) -> Self {
        self.undo = true;
        self
    }
}

/// A sortable, filterable, paginated table.
///
/// `empty` is the sentence shown instead of rows: say what the table is and
/// why it might be empty, because "no data" tells a reader nothing.
#[component]
pub fn Table<R>(
    /// The rows, live.
    #[prop(into)]
    rows: Signal<Vec<R>>,
    /// The columns, left to right.
    columns: Vec<Column<R>>,
    /// What to say when there is nothing to show.
    #[prop(into)]
    empty: String,
    /// Rows per page.
    #[prop(default = 25)]
    per_page: usize,
    /// Called with the row a reader drilled into. Without it, rows are not
    /// clickable and carry no affordance saying they are.
    #[prop(optional, into)]
    on_row: Option<Callback<R>>,
    /// Per-row controls, in a trailing column of their own. It has no header
    /// and does not sort: it holds verbs, not a value.
    #[prop(optional)]
    actions: Vec<RowAction<R>>,
) -> impl IntoView
where
    R: Clone + Send + Sync + 'static,
{
    let has_actions = !actions.is_empty();
    let actions = StoredValue::new(actions);
    let columns = StoredValue::new(columns);
    let filter = RwSignal::new(String::new());
    let sort = RwSignal::new(None::<Sort>);
    let page = RwSignal::new(0usize);

    // Every decision below is made on the text the columns produced.
    let cells = Memo::new(move |_| {
        columns.with_value(|columns| {
            rows.get()
                .iter()
                .map(|row| columns.iter().map(|c| (c.value)(row)).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        })
    });
    let arranged = Memo::new(move |_| {
        cells.with(|cells| arrange(cells, &filter.get(), sort.get(), page.get(), per_page))
    });

    let head = move || {
        columns.with_value(|columns| {
            columns
                .iter()
                .enumerate()
                .map(|(index, column)| {
                    let label = column.label;
                    let class = column.priority.class();
                    let aria = move || {
                        sort.get()
                            .filter(|s| s.column == index)
                            .map(|s| s.dir.aria().to_owned())
                    };
                    view! {
                        <th
                            class=class
                            aria-sort=aria
                            scope="col"
                            on:click=move |_| {
                                sort.update(|current| {
                                    *current = Some(match *current {
                                        Some(s) if s.column == index => Sort {
                                            column: index,
                                            dir: s.dir.flipped(),
                                        },
                                        _ => Sort { column: index, dir: Dir::Asc },
                                    });
                                });
                                page.set(0);
                            }
                        >
                            {label}
                        </th>
                    }
                })
                .collect_view()
        })
    };

    let body = move || {
        let arrangement = arranged.get();
        if arrangement.visible.is_empty() {
            let span = columns.with_value(Vec::len) + usize::from(has_actions);
            let empty = empty.clone();
            return view! {
                <tr>
                    <td class="empty" colspan=span>
                        {empty}
                    </td>
                </tr>
            }
            .into_any();
        }
        let source = rows.get();
        cells
            .with(|cells| {
                arrangement
                    .visible
                    .iter()
                    .map(|&index| {
                        let row = source[index].clone();
                        let drill = on_row.map(|callback| move || callback.run(row.clone()));
                        let cells = columns.with_value(|columns| {
                            columns
                                .iter()
                                .zip(&cells[index])
                                .map(|(column, text)| {
                                    let class = if column.mono {
                                        format!("{} mono num", column.priority.class())
                                    } else {
                                        column.priority.class().to_owned()
                                    };
                                    let body = match &column.render {
                                        Some(render) => render(text),
                                        None => text.clone().into_any(),
                                    };
                                    let title = column
                                        .title
                                        .as_ref()
                                        .map(|title| title(&source[index]))
                                        .filter(|title| !title.is_empty());
                                    view! { <td class=class title=title>{body}</td> }
                                })
                                .collect_view()
                        });
                        let verbs = has_actions.then(|| {
                            let row = source[index].clone();
                            actions.with_value(|actions| {
                                actions
                                    .iter()
                                    .filter(|action| {
                                        action.offered.as_ref().is_none_or(|offered| offered(&row))
                                    })
                                    .cloned()
                                    .map(|action| {
                                        let row = row.clone();
                                        view! {
                                            <button
                                                type="button"
                                                class="act"
                                                data-weight=action.undo.then_some("undo")
                                                // The row is a drill-in; a verb
                                                // inside it is not that verb's
                                                // way of opening the row.
                                                on:click=move |event| {
                                                    event.stop_propagation();
                                                    action.run.run(row.clone());
                                                }
                                            >
                                                {action.label}
                                            </button>
                                        }
                                    })
                                    .collect_view()
                            })
                        });
                        let tabindex = drill.is_some().then_some("0");
                        let on_click = drill.clone();
                        let on_key = drill;
                        view! {
                            <tr
                                tabindex=tabindex
                                on:click=move |_| {
                                    if let Some(drill) = on_click.as_ref() {
                                        drill();
                                    }
                                }
                                on:keydown=move |event: leptos::ev::KeyboardEvent| {
                                    if event.key() == "Enter"
                                        && let Some(drill) = on_key.as_ref()
                                    {
                                        drill();
                                    }
                                }
                            >
                                {cells}
                                {verbs.map(|verbs| view! { <td class="p1 does">{verbs}</td> })}
                            </tr>
                        }
                    })
                    .collect_view()
            })
            .into_any()
    };

    view! {
        <div class="table-tools">
            <input
                type="search"
                aria-label="Filter rows"
                placeholder="Filter"
                prop:value=move || filter.get()
                on:input=move |event| {
                    filter.set(event_target_value(&event));
                    page.set(0);
                }
            />
            <span class="count">
                {move || {
                    let matched = arranged.get().matched;
                    format!("{matched} {}", if matched == 1 { "row" } else { "rows" })
                }}
            </span>
            <span class="pager">
                <button
                    type="button"
                    aria-label="Previous page"
                    disabled=move || arranged.get().page == 0
                    on:click=move |_| page.update(|p| *p = p.saturating_sub(1))
                >
                    "\u{2039}"
                </button>
                <span>
                    {move || arranged.get().page + 1} " / " {move || arranged.get().pages}
                </span>
                <button
                    type="button"
                    aria-label="Next page"
                    disabled=move || {
                        let a = arranged.get();
                        a.page + 1 >= a.pages
                    }
                    on:click=move |_| {
                        let last = arranged.get().pages - 1;
                        page.update(|p| *p = (*p + 1).min(last));
                    }
                >
                    "\u{203a}"
                </button>
            </span>
        </div>
        <table class="tbl">
            <thead>
                <tr>
                    {head} {has_actions.then(|| view! { <th class="p1 does" scope="col"></th> })}
                </tr>
            </thead>
            <tbody>{body}</tbody>
        </table>
    }
}
