//! The SVG as Sketch draws it, for resvg: the port of `prepare_svg` of `tools/compose.py`
//! (silica-icons).
//!
//! Sketch draws shadows without a filter region, but its SVG export writes `objectBoundingBox`
//! regions computed from the shape before boolean operations, so on a flattened shape resvg, like a
//! browser, cuts the shadow. Each filter gets a `userSpaceOnUse` region of the box of the shape it
//! is applied to, grown by the filter's reach (offsets + 3 sigma); filters on text and on shapes
//! of unknown extent keep their region.

use std::collections::HashMap;

use roxmltree::{Document, Node};
use svgtypes::{SimplePathSegment, SimplifyingPathParser};

const XLINK: &str = "http://www.w3.org/1999/xlink";

type Bounds = (f64, f64, f64, f64);

/// The leading number of an attribute value (`12.5px` is 12.5), 0 when there is none.
fn lead(value: &str) -> f64 {
    let v = value.trim();
    let mut end = v
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E')))
        .unwrap_or(v.len());
    while end > 0 {
        if let Some(n) = v.get(..end).and_then(|s| s.parse::<f64>().ok()) {
            return n;
        }
        end -= 1;
    }
    0.0
}

fn num(el: Node, name: &str) -> f64 {
    el.attribute(name).map_or(0.0, lead)
}

fn grow(b: &mut Option<Bounds>, x: f64, y: f64) {
    *b = Some(match *b {
        Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        None => (x, y, x, y),
    });
}

/// The bounding box of a shape in its own user space (a superset for curves: end and control
/// points); None when unknown (text) or empty.
fn shape_box(el: Node, ids: &HashMap<&str, Node>, depth: u32) -> Option<Bounds> {
    if depth > 16 {
        return None;
    }
    let mut b: Option<Bounds> = None;
    match el.tag_name().name() {
        "use" => {
            let href = el
                .attribute((XLINK, "href"))
                .or_else(|| el.attribute("href"))?;
            let r = shape_box(*ids.get(href.trim_start_matches('#'))?, ids, depth + 1)?;
            let (dx, dy) = (num(el, "x"), num(el, "y"));
            return Some((r.0 + dx, r.1 + dy, r.2 + dx, r.3 + dy));
        }
        "path" => {
            for seg in SimplifyingPathParser::from(el.attribute("d").unwrap_or("")).flatten() {
                match seg {
                    SimplePathSegment::MoveTo { x, y } | SimplePathSegment::LineTo { x, y } => {
                        grow(&mut b, x, y);
                    }
                    SimplePathSegment::CurveTo {
                        x1,
                        y1,
                        x2,
                        y2,
                        x,
                        y,
                    } => {
                        grow(&mut b, x1, y1);
                        grow(&mut b, x2, y2);
                        grow(&mut b, x, y);
                    }
                    SimplePathSegment::Quadratic { x1, y1, x, y } => {
                        grow(&mut b, x1, y1);
                        grow(&mut b, x, y);
                    }
                    SimplePathSegment::ClosePath => {}
                }
            }
        }
        "rect" => {
            let (x, y) = (num(el, "x"), num(el, "y"));
            grow(&mut b, x, y);
            grow(&mut b, x + num(el, "width"), y + num(el, "height"));
        }
        tag @ ("circle" | "ellipse") => {
            let (rx, ry) = if tag == "circle" {
                (num(el, "r"), num(el, "r"))
            } else {
                (num(el, "rx"), num(el, "ry"))
            };
            let (cx, cy) = (num(el, "cx"), num(el, "cy"));
            grow(&mut b, cx - rx, cy - ry);
            grow(&mut b, cx + rx, cy + ry);
        }
        "g" if el.attribute("transform").is_none() => {
            for c in el.children().filter(Node::is_element) {
                let r = shape_box(c, ids, depth + 1)?;
                grow(&mut b, r.0, r.1);
                grow(&mut b, r.2, r.3);
            }
        }
        _ => return None,
    }
    b
}

/// How far (user units) a filter's result can reach past the shape: offsets plus 3 sigma.
fn filter_reach(flt: Node) -> f64 {
    let mut reach = 0.0;
    for p in flt.descendants().filter(Node::is_element) {
        let list = |name: &str| -> Vec<f64> {
            p.attribute(name)
                .map(|v| v.split_whitespace().map(lead).collect())
                .unwrap_or_default()
        };
        match p.tag_name().name() {
            "feOffset" => reach += num(p, "dx").abs() + num(p, "dy").abs(),
            "feGaussianBlur" => {
                reach += 3.0 * list("stdDeviation").into_iter().fold(0.0, f64::max);
            }
            "feMorphology" => reach += list("radius").into_iter().fold(0.0, f64::max),
            _ => {}
        }
    }
    reach + 2.0
}

/// `data` with every filter region widened; `data` itself when nothing changes or it is not
/// something this can read (resvg then renders it as it is).
pub fn widen_filters(data: &[u8]) -> Vec<u8> {
    widen(data).unwrap_or_else(|| data.to_vec())
}

fn widen(data: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(data).ok()?;
    let doc = Document::parse(text).ok()?;
    let ids: HashMap<&str, Node> = doc
        .descendants()
        .filter_map(|n| n.attribute("id").map(|id| (id, n)))
        .collect();
    // The region of each filter, by the node's start in the text (the last user wins, as in
    // compose.py).
    let mut regions: HashMap<usize, (Node, Bounds)> = HashMap::new();
    for el in doc.descendants().filter(Node::is_element) {
        let Some(id) = el
            .attribute("filter")
            .and_then(|f| f.trim().strip_prefix("url(#"))
            .and_then(|f| f.strip_suffix(')'))
        else {
            continue;
        };
        let (Some(flt), Some(b)) = (ids.get(id).copied(), shape_box(el, &ids, 0)) else {
            continue;
        };
        let r = filter_reach(flt);
        regions.insert(
            flt.range().start,
            (
                flt,
                (b.0 - r, b.1 - r, b.2 - b.0 + 2.0 * r, b.3 - b.1 + 2.0 * r),
            ),
        );
    }
    // (start, end, replacement): attributes dropped, the new ones inserted after `<filter`.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for (flt, (x, y, w, h)) in regions.values() {
        let at = flt.range().start;
        if !text.get(at..)?.starts_with("<filter") || flt.tag_name().name() != "filter" {
            continue;
        }
        for a in flt.attributes() {
            if matches!(a.name(), "filterUnits" | "x" | "y" | "width" | "height") {
                edits.push((a.range().start, a.range().end, String::new()));
            }
        }
        edits.push((
            at + "<filter".len(),
            at + "<filter".len(),
            format!(" filterUnits=\"userSpaceOnUse\" x=\"{x:.3}\" y=\"{y:.3}\" width=\"{w:.3}\" height=\"{h:.3}\""),
        ));
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|e| (e.0, e.1));
    let mut out = String::with_capacity(text.len() + 128 * edits.len());
    let mut pos = 0;
    for (s, e, rep) in &edits {
        out.push_str(text.get(pos..*s)?);
        out.push_str(rep);
        pos = *e;
    }
    out.push_str(text.get(pos..)?);
    Some(out.into_bytes())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_region_grows_to_the_reach_of_its_shadow() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<filter id="f" x="-1%" y="-1%" width="102%" height="102%"><feOffset dx="0" dy="4"/><feGaussianBlur stdDeviation="3"/></filter>
<rect x="10" y="20" width="30" height="40" filter="url(#f)"/></svg>"##;
        let out = String::from_utf8(widen_filters(svg.as_bytes())).unwrap();
        assert!(out.contains("filterUnits=\"userSpaceOnUse\""), "{out}");
        // reach 4 + 9 + 2 = 15: x 10 - 15, width 30 + 30.
        assert!(
            out.contains("x=\"-5.000\"") && out.contains("width=\"60.000\""),
            "{out}"
        );
        assert!(!out.contains("-1%"), "{out}");
    }

    #[test]
    fn what_is_not_xml_or_has_no_filter_is_returned_as_it_is() {
        assert_eq!(widen_filters(b"not xml"), b"not xml");
        let plain =
            br#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1"/></svg>"#;
        assert_eq!(widen_filters(plain), plain);
    }
}
