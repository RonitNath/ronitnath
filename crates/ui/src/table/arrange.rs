//! Sorting, filtering and pagination, over cell text rather than over rows.
//!
//! The component renders a generic row type, but every one of these decisions
//! is made on the strings the row's columns produced. Doing it that way keeps
//! the rules here — pure, and tested without a browser or a row type — and
//! keeps one filter box able to search a column whose value is computed.

/// Which way a column is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// A → Z, 1 → 9, oldest first.
    Asc,
    /// The reverse.
    Desc,
}

impl Dir {
    /// The other one. A header click flips the column it is already on.
    pub fn flipped(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    /// The `aria-sort` value a header carries.
    pub fn aria(self) -> &'static str {
        match self {
            Self::Asc => "ascending",
            Self::Desc => "descending",
        }
    }
}

/// The column being sorted on, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    /// Index into the column list.
    pub column: usize,
    /// Which way.
    pub dir: Dir,
}

/// What to show: which rows, in which order, and where in the set they sit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Arrangement {
    /// Indices into the original row slice, in display order, one page's worth.
    pub visible: Vec<usize>,
    /// How many rows survived the filter.
    pub matched: usize,
    /// How many pages that is. Always at least one, so "page 1 of 1" is what
    /// an empty table says.
    pub pages: usize,
    /// The page actually shown, clamped into range.
    pub page: usize,
}

/// Arrange `cells` — one `Vec<String>` per row, one entry per column.
///
/// `filter` matches case-insensitively against any cell. `page` is zero-based
/// and clamped: deleting the last row of the last page moves you back a page
/// rather than showing nothing.
pub fn arrange(
    cells: &[Vec<String>],
    filter: &str,
    sort: Option<Sort>,
    page: usize,
    per_page: usize,
) -> Arrangement {
    let needle = filter.trim().to_lowercase();
    let mut visible: Vec<usize> = (0..cells.len())
        .filter(|&row| {
            needle.is_empty()
                || cells[row]
                    .iter()
                    .any(|cell| cell.to_lowercase().contains(&needle))
        })
        .collect();

    if let Some(Sort { column, dir }) = sort {
        // A stable sort, so rows the sort cannot tell apart keep the order the
        // query gave them instead of shuffling on every diff.
        visible.sort_by(|&a, &b| {
            let ordering = compare(cell(cells, a, column), cell(cells, b, column));
            match dir {
                Dir::Asc => ordering,
                Dir::Desc => ordering.reverse(),
            }
        });
    }

    let matched = visible.len();
    let per_page = per_page.max(1);
    let pages = matched.div_ceil(per_page).max(1);
    let page = page.min(pages - 1);
    let start = page * per_page;
    let end = (start + per_page).min(matched);
    visible = visible[start..end].to_vec();

    Arrangement {
        visible,
        matched,
        pages,
        page,
    }
}

fn cell(cells: &[Vec<String>], row: usize, column: usize) -> &str {
    cells[row].get(column).map(String::as_str).unwrap_or("")
}

/// Compare two cells: as numbers when both are numbers, case-insensitively
/// otherwise. A count column sorts 2 before 10, and a name column ignores the
/// capital letter it happens to start with.
fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(a), Ok(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
        _ => a
            .to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Vec<String>> {
        [
            ["Kernel report", "2"],
            ["design brief", "10"],
            ["Rebuild plan", "1"],
            ["audit trail", "1"],
        ]
        .iter()
        .map(|row| row.iter().map(|c| (*c).to_owned()).collect())
        .collect()
    }

    #[test]
    fn an_unsorted_unfiltered_table_is_the_query_order() {
        let arranged = arrange(&rows(), "", None, 0, 10);
        assert_eq!(arranged.visible, vec![0, 1, 2, 3]);
        assert_eq!(arranged.matched, 4);
        assert_eq!(arranged.pages, 1);
    }

    #[test]
    fn sorting_text_ignores_case() {
        let sort = Some(Sort {
            column: 0,
            dir: Dir::Asc,
        });
        let arranged = arrange(&rows(), "", sort, 0, 10);
        assert_eq!(arranged.visible, vec![3, 1, 0, 2]);
    }

    #[test]
    fn sorting_numbers_compares_them_as_numbers() {
        let sort = Some(Sort {
            column: 1,
            dir: Dir::Asc,
        });
        let arranged = arrange(&rows(), "", sort, 0, 10);
        assert_eq!(
            arranged.visible,
            vec![2, 3, 0, 1],
            "10 sorts last, and the tie keeps query order"
        );
    }

    #[test]
    fn descending_is_the_reverse_order_of_values() {
        let rows = rows();
        let values = |dir| {
            arrange(&rows, "", Some(Sort { column: 1, dir }), 0, 10)
                .visible
                .into_iter()
                .map(|row| rows[row][1].clone())
                .collect::<Vec<_>>()
        };
        let mut descending = values(Dir::Desc);
        descending.reverse();
        assert_eq!(values(Dir::Asc), descending);
        // Rows the sort cannot tell apart keep query order in both
        // directions, which is why this compares values and not indices.
        assert_eq!(values(Dir::Asc), ["1", "1", "2", "10"]);
    }

    #[test]
    fn a_filter_matches_any_cell_case_insensitively() {
        let arranged = arrange(&rows(), "REPORT", None, 0, 10);
        assert_eq!(arranged.visible, vec![0]);
        assert_eq!(arranged.matched, 1);

        let arranged = arrange(&rows(), "  1 ", None, 0, 10);
        assert_eq!(
            arranged.visible,
            vec![1, 2, 3],
            "the count column matches too"
        );
    }

    #[test]
    fn a_filter_that_matches_nothing_leaves_one_empty_page() {
        let arranged = arrange(&rows(), "zzz", None, 3, 10);
        assert!(arranged.visible.is_empty());
        assert_eq!(arranged.matched, 0);
        assert_eq!(arranged.pages, 1);
        assert_eq!(arranged.page, 0);
    }

    #[test]
    fn pagination_slices_and_reports_the_page_count() {
        let first = arrange(&rows(), "", None, 0, 3);
        assert_eq!(first.visible, vec![0, 1, 2]);
        assert_eq!(first.pages, 2);

        let second = arrange(&rows(), "", None, 1, 3);
        assert_eq!(second.visible, vec![3]);
        assert_eq!(second.page, 1);
    }

    #[test]
    fn a_page_past_the_end_clamps_to_the_last_one() {
        let arranged = arrange(&rows(), "", None, 9, 3);
        assert_eq!(arranged.page, 1);
        assert_eq!(arranged.visible, vec![3]);
    }

    #[test]
    fn filter_and_sort_and_page_compose() {
        let sort = Some(Sort {
            column: 0,
            dir: Dir::Desc,
        });
        let arranged = arrange(&rows(), "e", sort, 0, 2);
        // "Kernel report", "design brief", "Rebuild plan" match; descending by
        // title that is Rebuild, Kernel, design — first page is the first two.
        assert_eq!(arranged.matched, 3);
        assert_eq!(arranged.visible, vec![2, 0]);
        assert_eq!(arranged.pages, 2);
    }

    #[test]
    fn a_zero_page_size_is_treated_as_one() {
        let arranged = arrange(&rows(), "", None, 0, 0);
        assert_eq!(arranged.visible, vec![0]);
        assert_eq!(arranged.pages, 4);
    }
}
