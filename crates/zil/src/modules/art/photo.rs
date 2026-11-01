//! Photography: depth of field.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "photo",
    about: "camera math like how much of a shot is in focus",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Camera calculations. Sensors are named by their type, like `\"full frame\"` or `\"aps-c\"`."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("photo", &[
            ("how much is sharp for a portrait at 3 m", "depth_of_field(85 mm, 1.8, 3 m)"),
            ("the same on a crop sensor", r#"depth_of_field(50 mm, 2.8, 5 m, "aps-c")"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

/// Sensor diagonals in mm; the circle of confusion is the diagonal / 1500.
const SENSORS: &[(&str, f64)] =
    &[("full frame", 43.27), ("aps-c", 28.2), ("aps-h", 34.5), ("micro four thirds", 21.6), ("1-inch", 15.86), ("medium format", 67.0)];

#[zil_macros::builtins]
mod fns {
    use super::SENSORS;
    use crate::modules::Fail;
    use crate::modules::units::{expect_in, terms};
    use crate::signature::{As, Map, Qty};
    use crate::value::Value;
    use indexmap::IndexMap;

    /// the zone that looks sharp: {near, far, total, hyperfocal}, for lens focal length at aperture f focused at dist
    ///
    /// sensor is full frame, aps-c, aps-h, micro four thirds, 1-inch or medium format, and defaults to full frame. far is inf at or beyond the hyperfocal distance
    #[builtin(examples = ["depth_of_field(85 mm, 1.8, 3 m)", r#"depth_of_field(50 mm, 2.8, 5 m, "aps-c")"#])]
    fn depth_of_field(focal: Qty, f: f64, dist: Qty, sensor: Option<&str>) -> Result<As<Map>, Fail> {
        let name = sensor.unwrap_or("full frame").to_lowercase();
        let Some((_, diag)) = SENSORS.iter().find(|s| s.0 == name) else {
            let names: Vec<_> = SENSORS.iter().map(|s| s.0).collect();
            return Err(Fail::Arg(3, format!("unknown sensor {name:?}\nnote: known: {}", names.join(", "))));
        };
        let mm = terms("mm")?;
        let (fx, fu) = focal.get();
        let (dx, du) = dist.get();
        let fl = expect_in(fx, fu, "mm").map_err(|e| Fail::Arg(0, e))?;
        let s = expect_in(dx, du, "mm").map_err(|e| Fail::Arg(2, e))?;
        if fl <= 0.0 || f <= 0.0 || s <= 0.0 {
            return Err(Fail::Msg("focal length, aperture and distance must be positive".into()));
        }
        let h = fl * fl / (f * diag / 1500.0) + fl;
        let near = s * (h - fl) / (h + s - 2.0 * fl);
        let far = if s >= h { f64::INFINITY } else { s * (h - fl) / (h - s) };
        let q = |x: f64| if x.is_infinite() { Value::Float(x) } else { Value::qty(x, mm.clone()) };
        let out = IndexMap::from([
            ("near".to_string(), q(near)),
            ("far".to_string(), q(far)),
            ("total".to_string(), q(far - near)),
            ("hyperfocal".to_string(), q(h)),
        ]);
        Ok(Value::map(out).into())
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn photo() {
        assert_eq!(show("depth_of_field(50 mm, 8, 3 m).hyperfocal.round"), "10883 mm");
        assert_eq!(show("depth_of_field(50 mm, 8, 20 m).far"), "∞");
        assert!(try_eval(r#"depth_of_field(50 mm, 8, 3 m, "x")"#).is_err());
    }
}
