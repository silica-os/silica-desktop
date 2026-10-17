//! Compiles one silica-icons app folder (`icon.json` format 1 or 2) into the bytes of an
//! `icon.assets` ([`crate::appicon`]). Deterministic. Defaults and dark overrides are resolved
//! here (docs/icon-format.md, Format 2); the reader applies no rules of its own.

use std::path::Path;

use serde_json::{Map, Value};

use super::CompileError;
use crate::appicon::{
    AppIcon, Background, Blend, Dynamic, DynamicKind, Form, HandUnit, Layer, Mode, ModeSpec, Role,
    Shadow,
};

const GRAPHITE: ([u8; 3], [u8; 3]) = ([0x3a, 0x3a, 0x3c], [0x14, 0x14, 0x16]);
/// Layers per icon (format 2).
const MAX_LAYERS: usize = 9;

fn fail<T>(message: impl Into<String>) -> Result<T, CompileError> {
    Err(CompileError::new(message))
}

/// Collects the SVG files into the blob table, once each.
struct Blobs {
    dir: std::path::PathBuf,
    blobs: Vec<Vec<u8>>,
}

impl Blobs {
    fn add(&mut self, rel: &str) -> Result<u32, CompileError> {
        if rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
            return fail(format!("{rel}: path leaves the icon folder"));
        }
        let path = self.dir.join(rel);
        let data = std::fs::read(&path)
            .map_err(|e| CompileError::new(format!("{}: {e}", path.display())))?;
        if let Some(i) = self.blobs.iter().position(|b| *b == data) {
            return Ok(i as u32);
        }
        self.blobs.push(data);
        Ok((self.blobs.len() - 1) as u32)
    }
}

fn hex(text: &str) -> Result<[u8; 3], CompileError> {
    let t = text.trim_start_matches('#');
    let part = |i: usize| {
        t.get(i..i + 2)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .ok_or_else(|| CompileError::new(format!("{text}: not a #rrggbb colour")))
    };
    if t.len() != 6 {
        return fail(format!("{text}: not a #rrggbb colour"));
    }
    Ok([part(0)?, part(2)?, part(4)?])
}

fn background(spec: Option<&Value>, blobs: &mut Blobs) -> Result<Option<Background>, CompileError> {
    let Some(spec) = spec else { return Ok(None) };
    if let Some(file) = spec.get("file").and_then(Value::as_str) {
        return Ok(Some(Background::Svg(blobs.add(file)?)));
    }
    let stops = spec.get("gradient").and_then(Value::as_array);
    match stops.map(Vec::as_slice) {
        Some([top, bottom]) => {
            let c = |v: &Value| hex(v.as_str().unwrap_or(""));
            Ok(Some(Background::Gradient(c(top)?, c(bottom)?)))
        }
        _ => fail("background needs `gradient: [top, bottom]` or `file`"),
    }
}

fn role_defaults(role: Role) -> (Mode, Mode, f32) {
    match role {
        Role::Body | Role::Detail | Role::Dynamic => (Mode::Ramp, Mode::Ramp, 1.0),
        Role::Highlight => (Mode::Keep, Mode::Keep, 0.6),
    }
}

fn num(v: Option<&Value>) -> Option<f32> {
    v.and_then(Value::as_f64).map(|n| n as f32)
}

fn mode_spec(
    spec: Option<&Value>,
    default: Mode,
    factor: f32,
    clear: bool,
) -> Result<ModeSpec, CompileError> {
    let mode = match spec.and_then(|s| s.get("mode")).and_then(Value::as_str) {
        Some(name) => {
            Mode::parse(name).ok_or_else(|| CompileError::new(format!("unknown mode {name}")))?
        }
        None => default,
    };
    if clear && mode == Mode::Accent {
        return fail("mode accent is not allowed in clear");
    }
    if !clear && mode == Mode::Glass {
        return fail("mode glass is not allowed in tinted");
    }
    let has_mode = spec.and_then(|s| s.get("mode")).is_some();
    let opacity =
        num(spec.and_then(|s| s.get("opacity"))).unwrap_or(if has_mode { 1.0 } else { factor });
    let luma = match spec
        .and_then(|s| s.get("luma"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    {
        Some([lo, hi]) => match (lo.as_f64(), hi.as_f64()) {
            (Some(lo), Some(hi)) => Some([lo as f32, hi as f32]),
            _ => return fail("luma needs two numbers"),
        },
        Some(_) => return fail("luma needs two numbers"),
        None => None,
    };
    Ok(ModeSpec {
        mode,
        opacity,
        luma,
    })
}

/// The layer with its dark overrides merged in (missing keys inherit).
fn merged(layer: &Map<String, Value>, dark: bool) -> Map<String, Value> {
    let mut out = layer.clone();
    out.remove("dark");
    if !dark {
        return out;
    }
    if let Some(over) = layer.get("dark").and_then(Value::as_object) {
        for (k, v) in over {
            match (
                k.as_str(),
                out.get(k).and_then(Value::as_object),
                v.as_object(),
            ) {
                ("clear" | "tinted", Some(base), Some(add)) => {
                    let mut m = base.clone();
                    m.extend(add.clone());
                    out.insert(k.clone(), Value::Object(m));
                }
                _ => {
                    out.insert(k.clone(), v.clone());
                }
            }
        }
    }
    out
}

fn form(
    layer: &Map<String, Value>,
    id: &str,
    dark: bool,
    blobs: &mut Blobs,
) -> Result<(Role, Form), CompileError> {
    let l = merged(layer, dark);
    let role = l
        .get("role")
        .and_then(Value::as_str)
        .and_then(Role::parse)
        .ok_or_else(|| {
            CompileError::new(format!(
                "layer {id}: role must be body, detail, highlight or dynamic"
            ))
        })?;
    let (clear_mode, tinted_mode, factor) = role_defaults(role);
    let blend = match l.get("blend").and_then(Value::as_str) {
        Some(b) => Blend::parse(b)
            .ok_or_else(|| CompileError::new(format!("layer {id}: unknown blend {b}")))?,
        None => Blend::Normal,
    };
    let shadow = match l.get("shadow").and_then(Value::as_str) {
        Some(s) => Shadow::parse(s)
            .ok_or_else(|| CompileError::new(format!("layer {id}: unknown shadow {s}")))?,
        None if matches!(role, Role::Body | Role::Dynamic) => Shadow::Neutral,
        None => Shadow::None,
    };
    let file = match l.get("file").and_then(Value::as_str) {
        Some(f) => f.to_owned(),
        None => {
            let dark_file = format!("layers/{id}-dark.svg");
            if dark && blobs.dir.join(&dark_file).is_file() {
                dark_file
            } else {
                format!("layers/{id}.svg")
            }
        }
    };
    let f = Form {
        svg: blobs.add(&file)?,
        opacity: num(l.get("opacity")).unwrap_or(1.0),
        blend,
        shadow,
        min_size: num(l.get("min_size")).unwrap_or(0.0),
        clear: mode_spec(l.get("clear"), clear_mode, factor, true)
            .map_err(|e| CompileError::new(format!("layer {id}: {e}")))?,
        tinted: mode_spec(l.get("tinted"), tinted_mode, factor, false)
            .map_err(|e| CompileError::new(format!("layer {id}: {e}")))?,
    };
    Ok((role, f))
}

fn dynamic(layer: &Map<String, Value>, id: &str) -> Result<Option<Dynamic>, CompileError> {
    let Some(d) = layer.get("dynamic").and_then(Value::as_object) else {
        return Ok(None);
    };
    let kind = d
        .get("kind")
        .and_then(Value::as_str)
        .and_then(DynamicKind::parse)
        .ok_or_else(|| CompileError::new(format!("layer {id}: unknown dynamic kind")))?;
    let unit = d
        .get("unit")
        .and_then(Value::as_str)
        .and_then(HandUnit::parse);
    if kind == DynamicKind::ClockHand && unit.is_none() {
        return fail(format!(
            "layer {id}: a clock hand needs unit hour, minute or second"
        ));
    }
    let pivot = match d.get("pivot").and_then(Value::as_array).map(Vec::as_slice) {
        Some([x, y]) => [
            x.as_f64().unwrap_or(512.0) as f32,
            y.as_f64().unwrap_or(512.0) as f32,
        ],
        _ => [512.0, 512.0],
    };
    Ok(Some(Dynamic { kind, unit, pivot }))
}

/// Compiles the icon folder `dir` into the bytes of an `icon.assets`.
pub fn app_icon(dir: &Path) -> Result<Vec<u8>, CompileError> {
    let path = dir.join("icon.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| CompileError::new(format!("{}: {e}", path.display())))?;
    let json: Value = serde_json::from_str(&text)
        .map_err(|e| CompileError::new(format!("{}: {e}", path.display())))?;
    let mut blobs = Blobs {
        dir: dir.to_path_buf(),
        blobs: Vec::new(),
    };
    let icon = match json.get("format").and_then(Value::as_u64) {
        Some(2) => format2(&json, &mut blobs)?,
        Some(1) => format1(&json, &mut blobs)?,
        _ => return fail(format!("{}: unknown icon format", path.display())),
    };
    let icon = AppIcon {
        blobs: blobs.blobs,
        ..icon
    };
    Ok(icon.encode())
}

fn empty() -> AppIcon {
    AppIcon {
        background: [Background::Gradient([255; 3], [255; 3]); 2],
        glass: [0.85, 0.55, 12.0],
        layers: Vec::new(),
        blobs: Vec::new(),
    }
}

fn format2(json: &Value, blobs: &mut Blobs) -> Result<AppIcon, CompileError> {
    let mut icon = empty();
    let spec = json.get("background");
    let light = background(spec.and_then(|b| b.get("light")), blobs)?
        .ok_or_else(|| CompileError::new("background.light is required"))?;
    let dark = background(spec.and_then(|b| b.get("dark")), blobs)?
        .unwrap_or(Background::Gradient(GRAPHITE.0, GRAPHITE.1));
    icon.background = [light, dark];
    if let Some(g) = json.get("glass") {
        let top = num(g.get("top")).unwrap_or(0.85);
        let bottom = num(g.get("bottom")).unwrap_or(0.55);
        let blur = num(g.get("blur")).unwrap_or(12.0);
        icon.glass = [top, bottom, blur];
    }
    let layers = json
        .get("layers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if layers.is_empty() || layers.len() > MAX_LAYERS {
        return fail(format!("an icon has 1 to {MAX_LAYERS} layers"));
    }
    let mut seen: Vec<&str> = Vec::new();
    for layer in layers {
        let Some(obj) = layer.as_object() else {
            return fail("a layer must be an object");
        };
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CompileError::new("a layer needs an id"))?;
        if seen.contains(&id) || id == "background" || id == "fallback" {
            return fail(format!("layer id {id} is reserved or repeated"));
        }
        seen.push(id);
        let (role, light) = form(obj, id, false, blobs)?;
        let (_, dark) = form(obj, id, true, blobs)?;
        icon.layers.push(Layer {
            id: id.to_owned(),
            role,
            dynamic: dynamic(obj, id)?,
            forms: [light, dark],
        });
    }
    let glass = icon
        .layers
        .iter()
        .filter(|l| l.forms.iter().any(|f| f.clear.mode == Mode::Glass))
        .count();
    if glass > 1 {
        return fail("at most one glass layer per icon");
    }
    Ok(icon)
}

/// Format 1: the background file and one body layer, the foreground.
fn format1(json: &Value, blobs: &mut Blobs) -> Result<AppIcon, CompileError> {
    let mut icon = empty();
    let file_of = |role: &str| {
        json.get("layers")
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .find(|l| l.get("role").and_then(Value::as_str) == Some(role))
            })
            .and_then(|l| l.get("file").and_then(Value::as_str).map(str::to_owned))
    };
    let variant = |key: &str| {
        json.get("variants")
            .and_then(|v| v.get("dark"))
            .and_then(|d| d.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let bg =
        file_of("background").ok_or_else(|| CompileError::new("format 1: no background layer"))?;
    let fg =
        file_of("foreground").ok_or_else(|| CompileError::new("format 1: no foreground layer"))?;
    let bg_dark = variant("background").unwrap_or_else(|| bg.clone());
    let fg_dark = variant("foreground").unwrap_or_else(|| fg.clone());
    icon.background = [
        Background::Svg(blobs.add(&bg)?),
        Background::Svg(blobs.add(&bg_dark)?),
    ];
    let make = |svg: u32| Form {
        svg,
        opacity: 1.0,
        blend: Blend::Normal,
        shadow: Shadow::Neutral,
        min_size: 0.0,
        clear: ModeSpec {
            mode: Mode::Ramp,
            opacity: 1.0,
            luma: None,
        },
        tinted: ModeSpec {
            mode: Mode::Ramp,
            opacity: 1.0,
            luma: None,
        },
    };
    icon.layers.push(Layer {
        id: "foreground".to_owned(),
        role: Role::Body,
        dynamic: None,
        forms: [make(blobs.add(&fg)?), make(blobs.add(&fg_dark)?)],
    });
    Ok(icon)
}
