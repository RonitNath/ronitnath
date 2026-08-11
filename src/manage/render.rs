//! HTML rendering for the data browser.
//!
//! Standalone server-rendered documents, styled per `docs/design.md`: the
//! night-sky token values are inlined here (same numbers as
//! `public/css/starscape.css`, both themes authored), the starscape itself
//! stays outside — internal surfaces are glass and hairlines, no sky.
//!
//! Everything row-derived passes through [`escape`]; the only unescaped
//! strings in a page are the `'static` titles and descriptions the enum owns.

use super::queries::{Cell, DataModel, ROW_LIMIT, TableData, fmt_utc};

/// Escape a value for interpolation into HTML text or attribute position.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn cell_html(cell: &Cell) -> String {
    match cell {
        Cell::Mono(v) => format!("<code>{}</code>", escape(v)),
        // The label reads; the uuid is there on hover for anyone who needs to
        // correlate rows. Resolution happened server-side, in the row's query.
        Cell::Ref { label, public_id } => format!(
            "<span class=\"ref\" title=\"{}\">{}</span>",
            escape(public_id),
            escape(label)
        ),
        Cell::Text(v) => escape(v),
        Cell::Tag(v) => format!("<span class=\"tag\">{}</span>", escape(v)),
        Cell::Time(ms) => format!("<span class=\"time\">{}</span>", fmt_utc(*ms)),
        Cell::None => "<span class=\"none\">—</span>".to_string(),
    }
}

/// Shared document shell: head, tokens, header bar, model nav, footer.
fn shell(title: &str, active: Option<DataModel>, main: &str) -> String {
    let nav: String = DataModel::ALL
        .iter()
        .map(|model| {
            let class = if Some(*model) == active {
                " class=\"active\""
            } else {
                ""
            };
            format!(
                "<a href=\"/manage/{}\"{class}>{}</a>",
                model.slug(),
                model.title()
            )
        })
        .collect();

    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"/>\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>\
         <title>{title} — Manage</title>\
         <style>{STYLE}</style></head>\
         <body>\
         <header class=\"bar\">\
           <a class=\"brand\" href=\"/manage\">Manage</a>\
           <span class=\"bar-note\">times UTC · latest {ROW_LIMIT} rows per model</span>\
           <nav class=\"bar-links\">\
             <a href=\"/protected\">Session</a>\
             <a href=\"/\">ronitnath.com</a>\
           </nav>\
         </header>\
         <div class=\"frame\">\
           <nav class=\"rail\" aria-label=\"Data models\">{nav}</nav>\
           <main>{main}</main>\
         </div>\
         </body></html>"
    )
}

/// `/manage`: one card per model, with its live row count.
pub fn index_page(counts: &[(DataModel, i64)]) -> String {
    let cards: String = counts
        .iter()
        .map(|(model, count)| {
            format!(
                "<a class=\"card\" href=\"/manage/{slug}\">\
                 <span class=\"card-head\"><span class=\"card-title\">{title}</span>\
                 <span class=\"count\">{count}</span></span>\
                 <span class=\"card-desc\">{desc}</span></a>",
                slug = model.slug(),
                title = model.title(),
                desc = model.description(),
            )
        })
        .collect();
    let main = format!(
        "<h1>Data models</h1>\
         <p class=\"lede\">Every durable table in the application, read-only. \
         Select a model to inspect its rows.</p>\
         <div class=\"cards\">{cards}</div>"
    );
    shell("Data models", None, &main)
}

/// `/manage/{model}`: the rows table, or an empty state that explains itself.
pub fn model_page(model: DataModel, total: i64, table: &TableData) -> String {
    let shown = table.rows.len() as i64;
    let count_line = if total == 0 {
        "No rows yet".to_string()
    } else if total > shown {
        format!("Showing the latest {shown} of {total} rows")
    } else if total == 1 {
        "1 row".to_string()
    } else {
        format!("{total} rows")
    };

    let body = if table.rows.is_empty() {
        format!(
            "<div class=\"empty\"><p>{}</p>\
             <p>Nothing has written to this table yet.</p></div>",
            model.description()
        )
    } else {
        let head: String = table
            .columns
            .iter()
            .map(|c| format!("<th>{c}</th>"))
            .collect();
        let rows: String = table
            .rows
            .iter()
            .map(|row| {
                let cells: String = row
                    .iter()
                    .map(|cell| format!("<td>{}</td>", cell_html(cell)))
                    .collect();
                format!("<tr>{cells}</tr>")
            })
            .collect();
        format!(
            "<div class=\"table-scroll\"><table>\
             <thead><tr>{head}</tr></thead><tbody>{rows}</tbody>\
             </table></div>"
        )
    };

    let main = format!(
        "<h1>{title}</h1>\
         <p class=\"lede\">{desc}</p>\
         <p class=\"count-line\">{count_line}</p>\
         {body}",
        title = model.title(),
        desc = model.description(),
    );
    shell(model.title(), Some(model), &main)
}

/// 404 for a slug that names no model.
pub fn not_found_page(slug: &str) -> String {
    let main = format!(
        "<h1>No such model</h1>\
         <p class=\"lede\">Nothing here is called <code>{}</code>. \
         The models this browser knows are listed on the left.</p>\
         <p><a href=\"/manage\">Back to the index</a></p>",
        escape(slug)
    );
    shell("No such model", None, &main)
}

/// 503 when the database cannot answer.
pub fn unavailable_page() -> String {
    let main = "<h1>Temporarily unavailable</h1>\
        <p class=\"lede\">The data store could not be reached. \
        Nothing is wrong with your session — try again shortly.</p>";
    shell("Temporarily unavailable", None, main)
}

/// Values mirror `public/css/starscape.css` — see docs/design.md (Color).
const STYLE: &str = r#"
:root {
  color-scheme: dark;
  --bg: oklch(0.06 0.005 240);
  --bg-muted: oklch(0.10 0.008 240);
  --bg-subtle: oklch(0.14 0.010 240);
  --fg: oklch(0.96 0.002 80);
  --fg-muted: oklch(0.75 0.005 80);
  --fg-subtle: oklch(0.55 0.005 80);
  --border: oklch(0.22 0.010 240);
  --accent: oklch(0.65 0.15 210);
  --accent-hover: oklch(0.72 0.15 210);
  --surface-glass: color-mix(in oklab, var(--bg-muted) 60%, transparent);
  --radius: 0.25rem;
  --font-display: "Agency Bold", system-ui, sans-serif;
  --font-body: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, monospace;
}
@media (prefers-color-scheme: light) {
  :root {
    color-scheme: light;
    --bg: oklch(0.97 0.003 240);
    --bg-muted: oklch(0.93 0.005 240);
    --bg-subtle: oklch(0.89 0.007 240);
    --fg: oklch(0.15 0.010 240);
    --fg-muted: oklch(0.30 0.012 255);
    --fg-subtle: oklch(0.40 0.012 255);
    --border: oklch(0.82 0.015 250);
    --accent: oklch(0.34 0.14 215);
    --accent-hover: oklch(0.24 0.12 215);
    --surface-glass: color-mix(in oklab, var(--bg-muted) 60%, transparent);
  }
}
@font-face {
  font-family: "Agency Bold";
  src: url("/fonts/agency-bold.ttf") format("truetype");
  font-weight: 400 700;
  font-display: swap;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--fg);
  font: 15px/1.5 var(--font-body);
}
a { color: var(--accent); text-decoration: none; }
a:hover { color: var(--accent-hover); text-decoration: underline; }
a:focus-visible, .card:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
  border-radius: var(--radius);
}
code { font-family: var(--font-mono); font-size: 0.86em; }

.bar {
  display: flex;
  align-items: baseline;
  gap: 1rem;
  padding: 0.85rem 1.25rem;
  border-bottom: 1px solid var(--border);
  background: var(--surface-glass);
}
.brand {
  font-family: var(--font-display);
  font-size: 1.35rem;
  letter-spacing: 0.04em;
  color: var(--fg);
}
.brand:hover { color: var(--fg); text-decoration: none; }
.bar-note { color: var(--fg-subtle); font-size: 0.8rem; }
.bar-links { margin-left: auto; display: flex; gap: 1rem; font-size: 0.9rem; }

.frame {
  display: grid;
  grid-template-columns: 15rem minmax(0, 1fr);
  gap: 2rem;
  max-width: 76rem;
  margin: 0 auto;
  padding: 2rem 1.25rem 4rem;
}
.rail { display: flex; flex-direction: column; gap: 0.15rem; align-self: start; }
.rail a {
  color: var(--fg-muted);
  padding: 0.4rem 0.6rem;
  border-left: 2px solid transparent;
  border-radius: 0 var(--radius) var(--radius) 0;
}
.rail a:hover { color: var(--fg); background: var(--bg-muted); text-decoration: none; }
.rail a.active { color: var(--fg); border-left-color: var(--accent); background: var(--bg-muted); }

h1 {
  font-family: var(--font-display);
  font-size: 1.9rem;
  letter-spacing: 0.03em;
  margin: 0 0 0.35rem;
}
.lede { color: var(--fg-muted); margin: 0 0 1.5rem; max-width: 46rem; }
.count-line { color: var(--fg-subtle); font-size: 0.85rem; margin: 0 0 0.75rem; }

.cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(19rem, 1fr)); gap: 1rem; }
.card {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  padding: 1rem 1.1rem;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface-glass);
  color: var(--fg);
  transition: border-color 150ms ease-out;
}
.card:hover { border-color: var(--accent); text-decoration: none; color: var(--fg); }
.card-head { display: flex; align-items: baseline; justify-content: space-between; gap: 0.75rem; }
.card-title { font-weight: 600; }
.count {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  font-size: 0.85rem;
  color: var(--accent);
}
.card-desc { color: var(--fg-muted); font-size: 0.85rem; line-height: 1.45; }

.table-scroll {
  overflow-x: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface-glass);
}
table { border-collapse: collapse; width: 100%; font-size: 0.86rem; }
th, td {
  text-align: left;
  padding: 0.5rem 0.75rem;
  border-bottom: 1px solid var(--border);
  white-space: nowrap;
  vertical-align: baseline;
}
tbody tr:last-child td { border-bottom: none; }
th {
  color: var(--fg-subtle);
  font-weight: 500;
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  background: var(--bg-muted);
}
tbody tr:hover td { background: var(--bg-muted); }
.time { font-family: var(--font-mono); font-variant-numeric: tabular-nums; color: var(--fg-muted); }
.tag { color: var(--fg-muted); }
.none { color: var(--fg-subtle); }
.ref { border-bottom: 1px dotted var(--fg-subtle); cursor: help; }

.empty {
  border: 1px dashed var(--border);
  border-radius: var(--radius);
  padding: 2rem;
  color: var(--fg-muted);
  max-width: 46rem;
}
.empty p { margin: 0 0 0.5rem; }
.empty p:last-child { margin: 0; color: var(--fg-subtle); }

@media (max-width: 768px) {
  /* minmax(0,1fr), not 1fr: the nowrap table must overflow its own
     scroll container, never widen the page. */
  .frame { grid-template-columns: minmax(0, 1fr); gap: 1.25rem; padding-top: 1.25rem; }
  .rail { flex-direction: row; overflow-x: auto; gap: 0.35rem; padding-bottom: 0.25rem; }
  .rail a { border-left: none; border-bottom: 2px solid transparent; border-radius: var(--radius) var(--radius) 0 0; white-space: nowrap; }
  .rail a.active { border-left: none; border-bottom-color: var(--accent); }
  .bar-note { display: none; }
}
@media (prefers-reduced-motion: reduce) {
  .card { transition: none; }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_neutralizes_markup() {
        assert_eq!(
            escape("<script>\"a\"&'b'</script>"),
            "&lt;script&gt;&quot;a&quot;&amp;&#39;b&#39;&lt;/script&gt;"
        );
    }

    #[test]
    fn row_values_are_escaped_into_the_page() {
        let table = TableData {
            columns: &["Email"],
            rows: vec![vec![Cell::Text("<img onerror=x>@example.test".into())]],
        };
        let html = model_page(DataModel::IdentityEmails, 1, &table);
        assert!(html.contains("&lt;img onerror=x&gt;@example.test"));
        assert!(!html.contains("<img onerror"));
    }
}
