//! Areas and volumes of common shapes, carrying units through.

use crate::ast::BinOp::{self, Mul};
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Fail, Module, builtin};
use crate::ops::binary;
use crate::value::Value;

pub const MODULE: Module = Module {
    name: "geometry",
    about: "areas, volumes and surface areas of circles, spheres, boxes, triangles and other shapes",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Areas and volumes of common shapes, from their measurements."),
        ("good to know", "\
Give lengths with units and the answer has the right units: `circle_area(3 cm)` is in cm². Plain numbers give plain numbers."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("geometry", &[
            ("the area of a circle of radius 3 cm", "circle_area(3 cm)"),
            ("how much a 10 cm sphere holds, in liters", "sphere_volume(10 cm) to L"),
            ("the area of a 3-4-5 triangle", "triangle_area(3, 4, 5)"),
            ("the area of a polygon from its corners", "polygon_area([[0, 0], [4, 0], [4, 3]])"),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("circles", &["circle_area", "circumference"]),
        ("solids", &["sphere_volume", "sphere_area", "cylinder_volume", "cylinder_area", "cone_volume", "cone_area", "box_volume", "box_area", "pyramid_volume", "tetrahedron_volume"]),
        ("polygons", &["triangle_area", "trapezoid_area", "parallelogram_area", "polygon_area"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

fn op(o: BinOp, a: &Value, b: &Value) -> Result<Value, Fail> {
    Ok(binary(o, a, b)?)
}

/// The product of every factor, left to right.
fn prod(xs: &[&Value]) -> Result<Value, Fail> {
    xs[1..].iter().try_fold(xs[0].clone(), |acc, x| op(Mul, &acc, x))
}

fn k(x: f64) -> Value {
    Value::Float(x)
}

fn call1(it: &mut Interp, f: &str, x: Value, span: &Span) -> Result<Value, Fail> {
    Ok(it.call(&builtin(f), vec![x], span)?)
}

fn pi_r2(r: &Value) -> Result<Value, Fail> {
    prod(&[&k(std::f64::consts::PI), r, r])
}

#[zil_macros::builtins]
mod fns {
    use super::{call1, k, op, pi_r2, prod};
    use crate::ast::BinOp::{Add, Div, Mul, Sub};
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::{Fail, builtin};
    use crate::signature::List;
    use crate::value::{Value, num};
    use std::f64::consts::PI;

    /// the area of a circle of radius r
    #[builtin(examples = ["circle_area(3 cm)", "circle_area(1)"], see = ["circumference", "sphere_area"])]
    fn circle_area(r: &Value) -> Result<Value, Fail> {
        pi_r2(r)
    }

    /// the distance around a circle of radius r
    #[builtin(examples = ["circumference(3 cm)"], see = ["circle_area"])]
    fn circumference(r: &Value) -> Result<Value, Fail> {
        prod(&[&k(2.0 * PI), r])
    }

    /// the volume of a sphere of radius r
    #[builtin(examples = ["sphere_volume(2 m)"], see = ["sphere_area"])]
    fn sphere_volume(r: &Value) -> Result<Value, Fail> {
        prod(&[&k(4.0 / 3.0 * PI), r, r, r])
    }

    /// the surface area of a sphere of radius r
    #[builtin(examples = ["sphere_area(2 m)"], see = ["sphere_volume", "circle_area"])]
    fn sphere_area(r: &Value) -> Result<Value, Fail> {
        prod(&[&k(4.0 * PI), r, r])
    }

    /// the volume of a cylinder with base radius r and height h
    #[builtin(examples = ["cylinder_volume(3 cm, 10 cm)"], see = ["cylinder_area", "cone_volume"])]
    fn cylinder_volume(r: &Value, h: &Value) -> Result<Value, Fail> {
        Ok(op(Mul, &pi_r2(r)?, h)?)
    }

    /// the total surface area of a closed cylinder with base radius r and height h
    #[builtin(examples = ["cylinder_area(3 cm, 10 cm)"], see = ["cylinder_volume"])]
    fn cylinder_area(r: &Value, h: &Value) -> Result<Value, Fail> {
        prod(&[&k(2.0 * PI), r, &op(Add, r, h)?])
    }

    /// the volume of a cone with base radius r and height h
    #[builtin(examples = ["cone_volume(3 cm, 10 cm)"], see = ["cone_area", "cylinder_volume"])]
    fn cone_volume(r: &Value, h: &Value) -> Result<Value, Fail> {
        prod(&[&k(PI / 3.0), r, r, h])
    }

    /// the total surface area of a cone with base radius r and height h, base included
    #[builtin(examples = ["cone_area(3 cm, 4 cm)"], see = ["cone_volume"])]
    fn cone_area(it: &mut Interp, r: &Value, h: &Value, span: &Span) -> Result<Value, Fail> {
        let slant = it.call(&builtin("hypot"), vec![r.clone(), h.clone()], span)?;
        prod(&[&k(PI), r, &op(Add, r, &slant)?])
    }

    /// the volume of a box with sides a, b and c
    #[builtin(examples = ["box_volume(2 m, 3 m, 4 m)"], see = ["box_area"])]
    fn box_volume(a: &Value, b: &Value, c: &Value) -> Result<Value, Fail> {
        prod(&[a, b, c])
    }

    /// the surface area of a box with sides a, b and c
    #[builtin(examples = ["box_area(2 m, 3 m, 4 m)"], see = ["box_volume"])]
    fn box_area(a: &Value, b: &Value, c: &Value) -> Result<Value, Fail> {
        let sum = op(Add, &op(Add, &prod(&[a, b])?, &prod(&[b, c])?)?, &prod(&[a, c])?)?;
        Ok(op(Mul, &k(2.0), &sum)?)
    }

    /// the volume of a pyramid with base area base and height h
    #[builtin(examples = ["pyramid_volume(25 m^2, 6 m)"], see = ["cone_volume"])]
    fn pyramid_volume(base: &Value, h: &Value) -> Result<Value, Fail> {
        Ok(op(Div, &prod(&[base, h])?, &Value::int(3))?)
    }

    /// the volume of a regular tetrahedron with edge length a
    #[builtin(examples = ["tetrahedron_volume(3 cm)"], see = ["pyramid_volume"])]
    fn tetrahedron_volume(a: &Value) -> Result<Value, Fail> {
        prod(&[&k(2f64.sqrt() / 12.0), a, a, a])
    }

    /// the area of a triangle from its base and height
    ///
    /// Given three side lengths instead, uses Heron's formula
    #[builtin(examples = ["triangle_area(6 cm, 4 cm)"], see = ["trapezoid_area", "parallelogram_area"])]
    fn triangle_area(b: &Value, h: &Value) -> Result<Value, Fail> {
        Ok(op(Div, &prod(&[b, h])?, &Value::int(2))?)
    }

    #[builtin(name = "triangle_area", examples = ["triangle_area(3, 4, 5)"])]
    fn triangle_area_sides(a: &Value, b: &Value, c: &Value) -> Result<Value, Fail> {
        let s = op(Div, &op(Add, &op(Add, a, b)?, c)?, &Value::int(2))?;
        let area2 = prod(&[&s, &op(Sub, &s, a)?, &op(Sub, &s, b)?, &op(Sub, &s, c)?])?;
        // The root of a quantity isn't available, so take it off a unit-free ratio and put the unit back.
        let side = [a, b, c].into_iter().max_by(|x, y| num(x).unwrap_or(0.0).abs().total_cmp(&num(y).unwrap_or(0.0).abs())).unwrap_or(a);
        let side2 = prod(&[side, side])?;
        let t = num(&op(Div, &area2, &prod(&[&side2, &side2])?)?).ok_or("sides must be numbers or lengths")?;
        if t < 0.0 || t.is_nan() {
            return Err("these sides can not make a triangle".into());
        }
        Ok(op(Mul, &side2, &k(t.sqrt()))?)
    }

    /// the area of a trapezoid with parallel sides a and b, and height h
    #[builtin(examples = ["trapezoid_area(3 cm, 5 cm, 4 cm)"], see = ["triangle_area"])]
    fn trapezoid_area(a: &Value, b: &Value, h: &Value) -> Result<Value, Fail> {
        Ok(op(Div, &prod(&[&op(Add, a, b)?, h])?, &Value::int(2))?)
    }

    /// the area of a parallelogram from its base and height
    #[builtin(examples = ["parallelogram_area(6 cm, 4 cm)"], see = ["triangle_area"])]
    fn parallelogram_area(b: &Value, h: &Value) -> Result<Value, Fail> {
        prod(&[b, h])
    }

    /// the area of a polygon from its corners in order, each a list [x, y]
    #[builtin(examples = ["polygon_area([[0, 0], [4, 0], [4, 3]])", "polygon_area([[0 m, 0 m], [2 m, 0 m], [2 m, 2 m], [0 m, 2 m]])"], see = ["triangle_area"])]
    fn polygon_area(it: &mut Interp, points: &List, span: &Span) -> Result<Value, Fail> {
        let pts: Vec<(Value, Value)> = points
            .borrow()
            .iter()
            .map(|p| match p {
                Value::List(l) if l.borrow().len() == 2 => Ok((l.borrow()[0].clone(), l.borrow()[1].clone())),
                _ => Err(Fail::Arg(0, "expected corners like [x, y]".into())),
            })
            .collect::<Result<_, _>>()?;
        if pts.len() < 3 {
            return Err(Fail::Arg(0, "a polygon needs at least 3 corners".into()));
        }
        let mut twice = prod(&[&pts[0].0, &pts[0].1])?;
        twice = op(Sub, &twice, &twice)?;
        for (i, (x1, y1)) in pts.iter().enumerate() {
            let (x2, y2) = &pts[(i + 1) % pts.len()];
            twice = op(Add, &twice, &op(Sub, &prod(&[x1, y2])?, &prod(&[x2, y1])?)?)?;
        }
        let half = op(Div, &twice, &Value::int(2))?;
        call1(it, "abs", half, span)
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn geometry() {
        assert_eq!(
            show("[round(circle_area(1), 4), round(circumference(1), 4), round(sphere_volume(1), 4), round(sphere_area(1), 4)]"),
            "[3.1416, 6.2832, 4.1888, 12.5664]"
        );
        assert_eq!(
            show("[round(cylinder_volume(1, 2), 4), round(cone_volume(1, 3), 4), round(cone_area(3, 4), 4), box_volume(2, 3, 4), box_area(2, 3, 4)]"),
            "[6.2832, 3.1416, 75.3982, 24, 52]"
        );
        assert_eq!(
            show("[triangle_area(6, 4), triangle_area(3, 4, 5), trapezoid_area(3, 5, 4), parallelogram_area(6, 4), pyramid_volume(3, 3)]"),
            "[12, 6, 16, 24, 3]"
        );
        assert_eq!(show("[polygon_area([[0, 0], [4, 0], [4, 3]]), polygon_area([[0 m, 0 m], [2 m, 0 m], [2 m, 2 m], [0 m, 2 m]])]"), "[6, 4 m^2]");
        assert_eq!(show("round(circle_area(3 cm), 3)"), "28.274 cm^2");
    }
}
