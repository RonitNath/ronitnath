//! Named-star labels drawn over the sky.
//!
//! They name what is overhead right now — the catalog is IAU names with SIMBAD
//! classifications and distances, so every line is something the sky actually
//! contains, which is why a label carries the constellation and the distance
//! rather than a caption about the sky.
//!
//! Each one is also the way into the atlas: a label points at a star, so
//! clicking it opens the atlas already tracking that star. That is why they are
//! buttons carrying the star's catalog position, and not text.

use web_sys::Element;

use crate::annotate::Placement;
use crate::catalog::NamedStar;
use crate::dom;

/// The attribute a label carries its J2000 position in, so the click handler
/// can aim the atlas without a second lookup into the catalog.
pub const POSITION_ATTRIBUTE: &str = "data-position";

/// How many labels the frame carries at once. Three is what fits down one side
/// of a phone without the sky becoming a list.
pub const MAX_LABELS: usize = 3;

/// The second line of a label: constellation, spectral class, distance.
#[must_use]
pub fn detail(star: &NamedStar) -> String {
    format!(
        "{} · {} · {} ly",
        star.constellation,
        star.classification,
        star.distance_ly.round().max(1.0) as u64
    )
}

/// Bring the container's labels in line with the current placements.
///
/// The label set is rebuilt only when *which* stars are labelled changes;
/// otherwise the existing elements are moved. Rebuilding unconditionally
/// restarts the entrance animation on every refresh, which leaves every label
/// permanently mid-fade — the labels then read as a flicker rather than as
/// text, and the effect is invisible in a static screenshot.
pub fn render(container: &Element, stars: &[NamedStar], placements: &[Placement]) {
    if !same_stars(container, placements) {
        container.set_inner_html("");
        let Some(document) = dom::document() else {
            return;
        };
        for placement in placements {
            let Some(star) = stars.get(placement.star) else {
                continue;
            };
            let Ok(label) = document.create_element("button") else {
                continue;
            };
            let _ = label.set_attribute("type", "button");
            let _ = label.set_attribute("data-star", &placement.star.to_string());
            let _ = label.set_attribute("data-name", &star.name);
            let _ = label.set_attribute(POSITION_ATTRIBUTE, &position_attribute(placement));
            let _ = label.set_attribute(
                "aria-label",
                &format!("Open the atlas on {}, {}", star.name, detail(star)),
            );
            label.set_inner_html(&format!(
                "<strong>{}</strong><span>{}</span>",
                escape(&star.name),
                escape(&detail(star))
            ));
            let _ = container.append_child(&label);
        }
    }

    let children = container.children();
    for (index, placement) in placements.iter().enumerate() {
        let Some(label) = children.item(u32::try_from(index).unwrap_or(u32::MAX)) else {
            continue;
        };
        let _ = label.set_attribute(
            "class",
            if placement.forced {
                "star-callout is-forced"
            } else {
                "star-callout"
            },
        );
        let (left, top) = css_position(placement);
        let _ = label.set_attribute(
            "style",
            &format!("left:{left:.2}%;top:{top:.2}%;transform:translate(-50%,-50%)"),
        );
    }
}

/// A placement's sky direction, as the three numbers the atlas opens on.
fn position_attribute(placement: &Placement) -> String {
    let [x, y, z] = placement.position;
    format!("{x},{y},{z}")
}

/// Read a position back off a label. Anything that is not three finite numbers
/// is no position at all: the atlas would otherwise open on a NaN direction and
/// draw an empty sky with no way to tell why.
#[must_use]
pub fn parse_position(value: &str) -> Option<[f64; 3]> {
    let mut parts = value.split(',').map(str::trim).map(str::parse::<f64>);
    let mut next = || parts.next()?.ok().filter(|value| value.is_finite());
    let position = [next()?, next()?, next()?];
    if parts.next().is_some() {
        return None;
    }
    let norm = position[0].hypot(position[1]).hypot(position[2]);
    (norm > 0.5).then_some(position)
}

/// Whether the container already labels exactly these stars, in this order.
fn same_stars(container: &Element, placements: &[Placement]) -> bool {
    let children = container.children();
    if children.length() as usize != placements.len() {
        return false;
    }
    placements.iter().enumerate().all(|(index, placement)| {
        children
            .item(u32::try_from(index).unwrap_or(u32::MAX))
            .and_then(|label| label.get_attribute("data-star"))
            .and_then(|value| value.parse::<usize>().ok())
            == Some(placement.star)
    })
}

/// Normalised device coordinates as CSS percentages of the viewport. The y axis
/// flips: NDC counts up from the bottom, CSS counts down from the top.
#[must_use]
pub fn css_position(placement: &Placement) -> (f64, f64) {
    ((placement.x + 1.0) * 50.0, (1.0 - placement.y) * 50.0)
}

/// The catalog is a static asset we ship, but it reaches this function as
/// fetched text, and text that arrives over the network is never markup.
fn escape(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            _ => ch.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::NamedCatalog;

    #[test]
    fn normalised_coordinates_become_css_percentages_with_the_y_axis_flipped() {
        let at = |x, y| {
            css_position(&Placement {
                star: 0,
                position: [0.0, 0.0, 1.0],
                x,
                y,
                forced: false,
            })
        };
        assert_eq!(at(0.0, 0.0), (50.0, 50.0));
        assert_eq!(at(-1.0, 1.0), (0.0, 0.0));
        assert_eq!(at(1.0, -1.0), (100.0, 100.0));
    }

    #[test]
    fn the_detail_line_states_the_catalog_and_nothing_more() {
        let star = &NamedCatalog::parse(include_str!("../../../static/stars/named.json"))
            .expect("named.json")
            .stars[0];
        let detail = detail(star);
        assert!(detail.contains(&star.constellation));
        assert!(detail.ends_with(" ly"));
        assert_eq!(detail.matches(" · ").count(), 2);
    }

    #[test]
    fn a_label_carries_the_direction_the_atlas_would_open_on() {
        let placement = Placement {
            star: 4,
            position: [0.5, -0.5, std::f64::consts::FRAC_1_SQRT_2],
            x: 0.4,
            y: -0.2,
            forced: false,
        };
        assert_eq!(
            parse_position(&position_attribute(&placement)),
            Some(placement.position)
        );
    }

    #[test]
    fn a_direction_that_is_not_a_direction_opens_nothing() {
        for junk in ["", "0,0,0", "1,2", "NaN,0,1", "a,b,c", "1,2,3,4"] {
            assert_eq!(parse_position(junk), None, "{junk}");
        }
    }

    #[test]
    fn fetched_text_can_never_become_markup() {
        assert_eq!(
            escape(r#"<img src=x onerror="alert(1)">"#),
            "&lt;img src=x onerror=&quot;alert(1)&quot;&gt;"
        );
        assert_eq!(escape("Alpha & Beta"), "Alpha &amp; Beta");
    }
}
