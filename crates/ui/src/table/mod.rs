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

/// One column: its header, how it reads a row, and how it behaves.
pub struct Column<R> {
    label: &'static str,
    value: Arc<dyn Fn(&R) -> String + Send + Sync>,
    priority: Priority,
    mono: bool,
}

impl<R> Clone for Column<R> {
    fn clone(&self) -> Self {
        Self {
            label: self.label,
            value: Arc::clone(&self.value),
            priority: self.priority,
            mono: self.mono,
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
) -> impl IntoView
where
    R: Clone + Send + Sync + 'static,
{
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
            let span = columns.with_value(Vec::len);
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
                                    view! { <td class=class>{text.clone()}</td> }
                                })
                                .collect_view()
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
            <span class="count">{move || arranged.get().matched} " rows"</span>
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
                <tr>{head}</tr>
            </thead>
            <tbody>{body}</tbody>
        </table>
    }
}
